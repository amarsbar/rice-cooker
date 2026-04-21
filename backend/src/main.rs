use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use rice_cooker_backend::apply::{self, ApplyParams};
use rice_cooker_backend::cache::Cache;
use rice_cooker_backend::catalog::Catalog;
use rice_cooker_backend::events::EventWriter;
use rice_cooker_backend::install::{self, Flags};

#[derive(Parser)]
#[command(name = "rice-cooker-backend", about = "Quickshell rice install engine")]
struct Cli {
    /// Skip interactive confirmation prompts that rice-cooker itself would
    /// show. Does NOT pass --noconfirm to pacman or install.sh — those are
    /// catalog-declared.
    #[arg(long, global = true)]
    no_confirm: bool,
    /// Print each file operation as it happens.
    #[arg(long, global = true)]
    verbose: bool,
    /// Alternate catalog file path (default: backend/catalog.toml).
    #[arg(long, global = true)]
    catalog: Option<PathBuf>,
    /// Skip the pacman interaction (no -Qqe snapshot, no -Rns on uninstall).
    /// Useful in sandboxed test environments without a working pacman.
    #[arg(long, global = true)]
    skip_pacman: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Install a rice from the catalog.
    Install {
        name: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Uninstall the currently-installed rice.
    Uninstall {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        force: bool,
    },
    /// Uninstall current (if any) and install <name>.
    Switch {
        name: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        force: bool,
    },
    /// List catalog entries; marks the installed one.
    List,
    /// Print details about the currently-installed rice.
    Status,
    /// v1 preview: clone + launch a rice in-session; `exit` restores.
    Apply {
        #[arg(long)]
        name: String,
        #[arg(long)]
        repo: String,
        #[arg(long, default_value = "shell.qml")]
        entry: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// v1 preview: kill the active rice; restore pre-apply shell.
    Exit,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rice-cooker: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let flags = Flags {
        dry_run: false,
        force: false,
        no_confirm: cli.no_confirm,
        verbose: cli.verbose,
        skip_pacman: cli.skip_pacman,
    };
    match &cli.cmd {
        Cmd::Install { name, dry_run } => {
            let dirs = install::resolve_dirs()?;
            let cat = Catalog::from_file(&catalog_path(cli.catalog.as_deref())?)?;
            let mut f = flags;
            f.dry_run = *dry_run;
            let out = install::install(&cat, &dirs, name, f)?;
            if out.partial {
                eprintln!(
                    "install partial: install_cmd exited non-zero; \
                     uninstall with --force to reverse what happened."
                );
            }
            println!(
                "installed: {} (log: {})",
                out.name,
                out.log_path.display()
            );
            if !out.pacman_diff.added_explicit.is_empty() {
                println!(
                    "pacman: added {} explicit: {:?}",
                    out.pacman_diff.added_explicit.len(),
                    out.pacman_diff.added_explicit
                );
            }
        }
        Cmd::Uninstall { dry_run, force } => {
            let dirs = install::resolve_dirs()?;
            let mut f = flags;
            f.dry_run = *dry_run;
            f.force = *force;
            let out = install::uninstall(&dirs, f)?;
            println!("uninstalled: {}", out.name);
            if !out.rcsave_paths.is_empty() {
                println!("preserved user-modified content at:");
                for p in out.rcsave_paths {
                    println!("  {}", p.display());
                }
            }
        }
        Cmd::Switch { name, dry_run, force } => {
            let dirs = install::resolve_dirs()?;
            let cat = Catalog::from_file(&catalog_path(cli.catalog.as_deref())?)?;
            let mut f = flags;
            f.dry_run = *dry_run;
            f.force = *force;
            let out = install::switch(&cat, &dirs, name, f)?;
            println!("switched: {} -> {}", out.from, out.to);
            if !out.rcsave_paths.is_empty() {
                println!("preserved user-modified content at:");
                for p in out.rcsave_paths {
                    println!("  {}", p.display());
                }
            }
        }
        Cmd::List => {
            let dirs = install::resolve_dirs()?;
            let cat = Catalog::from_file(&catalog_path(cli.catalog.as_deref())?)?;
            for row in install::list(&cat, &dirs)? {
                let marker = if row.installed { "*" } else { " " };
                print!("{marker} {:<24} {}", row.name, row.display_name);
                if !row.description.is_empty() {
                    print!(" — {}", row.description);
                }
                println!();
                for eff in &row.documented_system_effects {
                    println!("    ! {eff}");
                }
            }
        }
        Cmd::Status => {
            let dirs = install::resolve_dirs()?;
            let row = install::status(&dirs)?;
            match row.installed {
                Some(r) => {
                    println!("name:         {}", r.name);
                    println!("installed_at: {}", r.installed_at);
                    println!("commit:       {}", r.commit);
                    println!(
                        "files:        +{} ~{} -{} (symlinks: +{}, dirs: +{})",
                        r.fs_diff.added.len(),
                        r.fs_diff.modified.len(),
                        r.fs_diff.deleted.len(),
                        r.fs_diff.symlinks_added.len(),
                        r.fs_diff.dirs_added.len()
                    );
                    if !r.pacman_diff.added_explicit.is_empty() {
                        println!("pacman:       +{:?}", r.pacman_diff.added_explicit);
                    }
                    if !r.systemd_units_enabled.is_empty() {
                        println!("systemd:      {:?}", r.systemd_units_enabled);
                    }
                    println!("log:          {}", r.log_path.display());
                    if r.partial {
                        println!("PARTIAL INSTALL — install_cmd exited {}", r.exit_code);
                    }
                }
                None => println!("nothing installed"),
            }
        }
        Cmd::Apply {
            name,
            repo,
            entry,
            dry_run,
        } => {
            let cache = Cache::from_env()?;
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            let mut events = EventWriter::new(&mut lock);
            let params = ApplyParams {
                name,
                repo,
                entry,
                dry_run: *dry_run,
            };
            apply::run_apply(&cache, &params, &mut events)?;
        }
        Cmd::Exit => {
            let cache = Cache::from_env()?;
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            let mut events = EventWriter::new(&mut lock);
            apply::run_exit(&cache, &mut events)?;
        }
    }
    Ok(())
}

fn catalog_path(flag: Option<&std::path::Path>) -> Result<PathBuf> {
    if let Some(p) = flag {
        return Ok(p.to_path_buf());
    }
    if let Ok(p) = std::env::var("RICE_COOKER_CATALOG")
        && !p.is_empty()
    {
        return Ok(PathBuf::from(p));
    }
    // Default: catalog.toml relative to cwd or next to the binary.
    let cwd = std::env::current_dir()?.join("catalog.toml");
    if cwd.exists() {
        return Ok(cwd);
    }
    Ok(PathBuf::from("catalog.toml"))
}
