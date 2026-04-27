use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

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
    /// Preview <name>, installing only catalog preview dependencies.
    Preview { name: String },
    /// Activate <name> (install + launch; evicts any currently-active rice).
    Try { name: String },
    /// Uninstall the active rice and replay the pre-rice shell. Clone stays
    /// cached at `~/.cache/rice-cooker/rices/<name>/`.
    Uninstall {
        #[arg(long)]
        force: bool,
    },
    /// List catalog entries (JSON).
    List,
    /// Print the active rice's install record (JSON).
    Status,
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1), // Fail event already on stdout
        Err(e) => {
            eprintln!("rice-cooker: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    let paths = Paths::from_env()?;
    match &cli.cmd {
        Cmd::Preview { name } => {
            let cat = Catalog::from_file(&catalog_path(&paths, cli.catalog.as_deref())?)?;
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            let mut events = EventWriter::new(&mut lock);
            install::run_preview(&cat, &paths, name, &mut events)
        }
        Cmd::Try { name } => {
            let cat = Catalog::from_file(&catalog_path(&paths, cli.catalog.as_deref())?)?;
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            let mut events = EventWriter::new(&mut lock);
            install::run_try(&cat, &paths, name, &mut events)
        }
        Cmd::Uninstall { force } => {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            let mut events = EventWriter::new(&mut lock);
            install::run_uninstall(&paths, Flags { force: *force }, &mut events)
        }
        Cmd::List => {
            let cat = Catalog::from_file(&catalog_path(&paths, cli.catalog.as_deref())?)?;
            let rows = install::list(&cat, &paths)?;
            serde_json::to_writer_pretty(std::io::stdout(), &rows)?;
            println!();
            Ok(true)
        }
        Cmd::Status => {
            let row = install::status(&paths)?;
            serde_json::to_writer_pretty(std::io::stdout(), &row)?;
            println!();
            Ok(true)
        }
    }
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
