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
    /// Alternate catalog file path (default: backend/catalog.toml).
    #[arg(long, global = true)]
    catalog: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Install a rice from the catalog.
    Install {
        name: String,
        /// Print planned actions without doing anything.
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
    /// Uninstall current (if any) then install <name>.
    ///
    /// NOT atomic. If uninstall succeeds but install then fails (bad
    /// catalog entry, network flap, dep install failure), nothing is
    /// left installed. Re-run `install <name>` once the underlying
    /// failure is addressed.
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
    };
    match &cli.cmd {
        Cmd::Install { name, dry_run } => {
            let dirs = install::resolve_dirs()?;
            let cat = Catalog::from_file(&catalog_path(cli.catalog.as_deref())?)?;
            let mut f = flags;
            f.dry_run = *dry_run;
            let out = install::install(&cat, &dirs, name, f)?;
            if out.dry_run {
                return Ok(());
            }
            println!("installed: {}", out.name);
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
            if f.dry_run {
                return Ok(());
            }
            println!("uninstalled: {}", out.name);
        }
        Cmd::Switch {
            name,
            dry_run,
            force,
        } => {
            let dirs = install::resolve_dirs()?;
            let cat = Catalog::from_file(&catalog_path(cli.catalog.as_deref())?)?;
            let mut f = flags;
            f.dry_run = *dry_run;
            f.force = *force;
            let out = install::switch(&cat, &dirs, name, f)?;
            if f.dry_run {
                return Ok(());
            }
            println!("switched: {} -> {}", out.from, out.to);
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
                        "symlink:      {} -> {}",
                        r.symlink_path.display(),
                        r.symlink_target.display()
                    );
                    if !r.pacman_diff.added_explicit.is_empty() {
                        println!("pacman:       +{:?}", r.pacman_diff.added_explicit);
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
    // Resolve against CWD. Running from the repo root is the typical
    // dev invocation (`backend/catalog.toml`), running from `backend/`
    // is the typical script invocation (`catalog.toml`). Check both.
    let cwd = std::env::current_dir()?;
    for rel in ["backend/catalog.toml", "catalog.toml"] {
        let p = cwd.join(rel);
        if p.exists() {
            return Ok(p);
        }
    }
    Ok(PathBuf::from("backend/catalog.toml"))
}
