use std::io::{self, Write};
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use rice_cooker_backend::apply::{self, ApplyParams};
use rice_cooker_backend::cache::Cache;
use rice_cooker_backend::events::EventWriter;

#[derive(Parser)]
#[command(name = "rice-cooker-backend", about = "Quickshell rice apply engine")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Clone + apply a rice. Streams NDJSON progress to stdout.
    Apply {
        #[arg(long)]
        name: String,
        #[arg(long)]
        repo: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Switch back to the previously-applied rice.
    Revert,
    /// Kill the active rice; restore the user's pre-RiceCooker shell if recorded.
    Exit,
    /// Print a single JSON object describing current state.
    Status,
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("rice-cooker-backend: internal error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    let cache = Cache::from_env()?;
    let stdout = io::stdout();
    let mut lock = stdout.lock();

    match cli.cmd {
        Cmd::Apply {
            name,
            repo,
            dry_run,
        } => {
            let mut events = EventWriter::new(&mut lock);
            let params = ApplyParams {
                name: &name,
                repo: &repo,
                dry_run,
            };
            apply::run_apply(&cache, &params, &mut events)
        }
        Cmd::Revert => {
            let mut events = EventWriter::new(&mut lock);
            apply::run_revert(&cache, &mut events)
        }
        Cmd::Exit => {
            let mut events = EventWriter::new(&mut lock);
            apply::run_exit(&cache, &mut events)
        }
        Cmd::Status => {
            let status = apply::get_status(&cache)?;
            let line = serde_json::to_string(&status)?;
            writeln!(lock, "{line}")?;
            Ok(true)
        }
    }
}
