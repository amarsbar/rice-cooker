use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use rice_cooker_backend::apply::{self, ApplyParams};
use rice_cooker_backend::catalog::Catalog;
use rice_cooker_backend::events::EventWriter;
use rice_cooker_backend::install::{self, Flags};
use rice_cooker_backend::paths::Paths;

#[derive(Parser)]
#[command(name = "rice-cooker-backend", about = "Quickshell rice install engine")]
struct Cli {
    /// Alternate catalog file path (default: XDG-data lookup for rice-cooker/catalog.toml).
    #[arg(long, global = true)]
    catalog: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Install a rice from the catalog.
    Install { name: String },
    /// Uninstall the currently-installed rice.
    ///
    /// Deletes the symlink, the clone directory at
    /// `~/.cache/rice-cooker/rices/<name>/`, and the install record.
    /// Any edits you made to files inside the clone are lost — copy
    /// them out first.
    Uninstall {
        #[arg(long)]
        force: bool,
    },
    /// Uninstall current (if any) then install <name>.
    ///
    /// The outgoing rice's clone at `~/.cache/rice-cooker/rices/<from>/`
    /// is deleted along with its symlink and record — any edits you
    /// made to files inside are lost, so copy them out first if you
    /// need them.
    ///
    /// NOT atomic. If uninstall succeeds but install then fails (bad
    /// catalog entry, network flap, dep install failure), nothing is
    /// left installed. Re-run `install <name>` once the underlying
    /// failure is addressed.
    Switch {
        name: String,
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
    let paths = Paths::from_env()?;
    let flags = Flags { force: false };
    match &cli.cmd {
        Cmd::Install { name } => {
            let cat = Catalog::from_file(&catalog_path(&paths, cli.catalog.as_deref())?)?;
            let out = install::install(&cat, &paths, name, flags)?;
            println!("installed: {}", out.name);
            if !out.pacman_diff.added_explicit.is_empty() {
                println!(
                    "pacman: added {} explicit: {:?}",
                    out.pacman_diff.added_explicit.len(),
                    out.pacman_diff.added_explicit
                );
            }
        }
        Cmd::Uninstall { force } => {
            let mut f = flags;
            f.force = *force;
            let out = install::uninstall(&paths, f)?;
            println!("uninstalled: {}", out.name);
        }
        Cmd::Switch { name, force } => {
            let cat = Catalog::from_file(&catalog_path(&paths, cli.catalog.as_deref())?)?;
            let mut f = flags;
            f.force = *force;
            let out = install::switch(&cat, &paths, name, f)?;
            println!("switched: {} -> {}", out.from, out.to);
        }
        Cmd::List => {
            let cat = Catalog::from_file(&catalog_path(&paths, cli.catalog.as_deref())?)?;
            for row in install::list(&cat, &paths)? {
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
            let row = install::status(&paths)?;
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
        Cmd::Apply { name, repo, entry } => {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            let mut events = EventWriter::new(&mut lock);
            let params = ApplyParams { name, repo, entry };
            apply::run_apply(&paths, &params, &mut events)?;
        }
        Cmd::Exit => {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            let mut events = EventWriter::new(&mut lock);
            apply::run_exit(&paths, &mut events)?;
        }
    }
    Ok(())
}

/// Resolve the catalog location in preference order:
/// 1. `--catalog` flag
/// 2. `$RICE_COOKER_CATALOG` env var
/// 3. CWD-relative dev paths (`./backend/catalog.toml`, `./catalog.toml`)
/// 4. `Paths::find_catalog()` — walks `$XDG_DATA_HOME` then `$XDG_DATA_DIRS`
///    looking for `rice-cooker/catalog.toml` (standard XDG Base Directory lookup
///    for read-only application data; the packaged install lands here).
fn catalog_path(paths: &Paths, flag: Option<&std::path::Path>) -> Result<PathBuf> {
    if let Some(p) = flag {
        return Ok(p.to_path_buf());
    }
    if let Ok(p) = std::env::var("RICE_COOKER_CATALOG")
        && !p.is_empty()
    {
        return Ok(PathBuf::from(p));
    }
    let cwd = std::env::current_dir()?;
    for rel in ["backend/catalog.toml", "catalog.toml"] {
        let p = cwd.join(rel);
        if p.exists() {
            return Ok(p);
        }
    }
    if let Some(p) = paths.find_catalog() {
        return Ok(p);
    }
    let xdg_list = paths
        .searched_catalog_paths()
        .into_iter()
        .map(|p| format!("  {}", p.display()))
        .collect::<Vec<_>>()
        .join("\n");
    Err(anyhow::anyhow!(
        "no catalog found. Tried:\n  \
         --catalog flag, $RICE_COOKER_CATALOG\n  \
         ./backend/catalog.toml, ./catalog.toml (cwd: {})\n{}\n\
         Install the rice-cooker package, pass --catalog <path>, or set \
         RICE_COOKER_CATALOG.",
        cwd.display(),
        xdg_list
    ))
}
