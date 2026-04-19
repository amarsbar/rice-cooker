use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use crate::cache::Cache;
use crate::events::{Event, EventWriter, SCHEMA_VERSION, Step, StepState};
use crate::git;
use crate::lock::{ApplyLock, LockError};
use crate::process::{self, VerifyResult, extract_entry_arg, find_running_quickshell};

/// Turn a `Result<T, E>` into its Ok value, or emit a stage fail event and early-return
/// `Ok(false)` from the enclosing function. Enforces the post-hello contract: never
/// surface exit-code-2 from a pipeline function without a matching fail event first.
///
/// Uses `{e:#}` so anyhow context chains surface in full (bare `{e}` drops inner sources).
macro_rules! try_stage {
    ($events:expr, $stage:literal, $expr:expr $(,)?) => {
        match $expr {
            Ok(v) => v,
            Err(e) => {
                emit_fail($events, $stage, &format!("{e:#}"), None, None)?;
                return Ok(false);
            }
        }
    };
    // With a prefix: multiple sites share a stage but want distinct reasons.
    ($events:expr, $stage:literal, $reason_prefix:literal, $expr:expr $(,)?) => {
        match $expr {
            Ok(v) => v,
            Err(e) => {
                emit_fail(
                    $events,
                    $stage,
                    &format!("{}: {:#}", $reason_prefix, e),
                    None,
                    None,
                )?;
                return Ok(false);
            }
        }
    };
}

pub struct ApplyParams<'a> {
    pub name: &'a str,
    pub repo: &'a str,
    /// Path to the rice's `shell.qml` relative to the repo root, as declared by the
    /// curated catalog. Defaults to `"shell.qml"` from the CLI.
    pub entry: &'a str,
    pub dry_run: bool,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub active: Option<String>,
    pub original: Option<String>,
    pub quickshell_running: bool,
    pub cache_dir: String,
}

pub fn run_apply<W: Write>(
    cache: &Cache,
    params: &ApplyParams,
    events: &mut EventWriter<W>,
) -> Result<bool> {
    hello(events, "apply")?;

    try_stage!(events, "init", cache.ensure_dirs());
    let _lock = match acquire_lock(cache, events)? {
        Some(l) => l,
        None => return Ok(false),
    };

    if !preflight(cache, events)? {
        return Ok(false);
    }

    step(events, Step::Clone, StepState::Start)?;
    let rice_dir = try_stage!(events, "input", cache.rice_dir(params.name));
    let log_file = cache.last_run_log();
    if let Err(e) = git::clone_or_update(params.repo, &rice_dir, &log_file) {
        let tail = read_tail(&log_file);
        emit_fail(events, "clone", &format!("{e:#}"), None, Some(tail))?;
        return Ok(false);
    }
    step(events, Step::Clone, StepState::Done)?;

    let entry_rel = PathBuf::from(params.entry);
    if !rice_dir.join(&entry_rel).is_file() {
        emit_fail(events, "entry", "entry_missing", None, None)?;
        return Ok(false);
    }

    if params.dry_run {
        events.emit(&Event::Success {
            active: None,
            dry_run: true,
        })?;
        return Ok(true);
    }

    if !wet_pipeline(&rice_dir, &entry_rel, &log_file, events)? {
        return Ok(false);
    }

    // Post-verify: a cache write failure here must still emit a fail event — the UI
    // has already been told the pipeline succeeded through step/verify/done, so
    // exit-code-2 with no fail event would break the contract.
    try_stage!(
        events,
        "commit",
        "set_active",
        cache.set_active(params.name)
    );
    events.emit(&Event::Success {
        active: Some(params.name.to_string()),
        dry_run: false,
    })?;
    Ok(true)
}

pub fn run_exit<W: Write>(cache: &Cache, events: &mut EventWriter<W>) -> Result<bool> {
    hello(events, "exit")?;
    try_stage!(events, "init", cache.ensure_dirs());
    let _lock = match acquire_lock(cache, events)? {
        Some(l) => l,
        None => return Ok(false),
    };

    step(events, Step::KillQuickshell, StepState::Start)?;
    try_stage!(events, "kill_quickshell", process::kill_quickshell());
    step(events, Step::KillQuickshell, StepState::Done)?;

    let original = try_stage!(events, "commit", "read_original", cache.original());
    if let Some(entry_path) = original {
        let entry_pb = PathBuf::from(&entry_path);
        let (rice_dir, entry_rel) = split_launch_path(&entry_pb);
        let log_file = cache.last_run_log();
        step(events, Step::Launch, StepState::Start)?;
        if let Err(e) = process::launch_detached(&rice_dir, &entry_rel, &log_file) {
            let tail = read_tail(&log_file);
            emit_fail(events, "launch", &format!("{e:#}"), None, Some(tail))?;
            return Ok(false);
        }
        step(events, Step::Launch, StepState::Done)?;
    }

    try_stage!(events, "commit", "clear", cache.clear_active());
    events.emit(&Event::Success {
        active: None,
        dry_run: false,
    })?;
    Ok(true)
}

pub fn get_status(cache: &Cache) -> Result<Status> {
    Ok(Status {
        active: cache.active()?,
        original: cache.original()?,
        quickshell_running: find_running_quickshell()?.is_some(),
        cache_dir: cache.root().display().to_string(),
    })
}

fn hello<W: Write>(events: &mut EventWriter<W>, subcommand: &str) -> Result<()> {
    events.emit(&Event::Hello {
        version: SCHEMA_VERSION,
        subcommand: subcommand.to_string(),
    })?;
    Ok(())
}

fn step<W: Write>(events: &mut EventWriter<W>, step: Step, state: StepState) -> Result<()> {
    events.emit(&Event::Step { step, state })?;
    Ok(())
}

fn emit_fail<W: Write>(
    events: &mut EventWriter<W>,
    stage: &str,
    reason: &str,
    plugins: Option<Vec<String>>,
    log_tail: Option<String>,
) -> Result<()> {
    events.emit(&Event::Fail {
        stage: stage.to_string(),
        reason: reason.to_string(),
        plugins,
        log_tail,
    })?;
    Ok(())
}

fn acquire_lock<W: Write>(cache: &Cache, events: &mut EventWriter<W>) -> Result<Option<ApplyLock>> {
    match ApplyLock::try_acquire(&cache.apply_lock()) {
        Ok(l) => Ok(Some(l)),
        Err(LockError::AlreadyHeld) => {
            emit_fail(events, "lock", "already_held", None, None)?;
            Ok(None)
        }
        Err(LockError::Io(e)) => Err(e.into()),
    }
}

fn preflight<W: Write>(cache: &Cache, events: &mut EventWriter<W>) -> Result<bool> {
    step(events, Step::Preflight, StepState::Start)?;
    if git::preflight().is_err() {
        emit_fail(events, "preflight", "git_missing", None, None)?;
        return Ok(false);
    }
    if !cache.original_is_recorded() {
        try_stage!(
            events,
            "preflight",
            "record_original",
            record_original(cache)
        );
    }
    step(events, Step::Preflight, StepState::Done)?;
    Ok(true)
}

fn record_original(cache: &Cache) -> Result<()> {
    match find_running_quickshell()? {
        Some(proc) => {
            let entry = extract_entry_arg(&proc.cmdline);
            // A relative `-p` path is resolved against the process's cwd at scan time
            // so later `exit` can relaunch from the right directory. Bare names (no `/`)
            // stay as-is — they aren't paths.
            let resolved = entry.map(|e| {
                let p = PathBuf::from(&e);
                if p.is_absolute() || !e.contains('/') {
                    e
                } else if let Some(cwd) = &proc.cwd {
                    cwd.join(&p).to_string_lossy().into_owned()
                } else {
                    e
                }
            });
            cache.set_original(resolved.as_deref())?;
        }
        None => cache.set_original(None)?,
    }
    Ok(())
}

fn wet_pipeline<W: Write>(
    rice_dir: &Path,
    entry_rel: &Path,
    log_file: &Path,
    events: &mut EventWriter<W>,
) -> Result<bool> {
    step(events, Step::Notifiers, StepState::Start)?;
    try_stage!(events, "notifiers", process::kill_notif_daemons());
    step(events, Step::Notifiers, StepState::Done)?;

    step(events, Step::KillQuickshell, StepState::Start)?;
    try_stage!(events, "kill_quickshell", process::kill_quickshell());
    step(events, Step::KillQuickshell, StepState::Done)?;

    step(events, Step::Launch, StepState::Start)?;
    if let Err(e) = process::launch_detached(rice_dir, entry_rel, log_file) {
        let tail = read_tail(log_file);
        emit_fail(events, "launch", &format!("{e:#}"), None, Some(tail))?;
        return Ok(false);
    }
    step(events, Step::Launch, StepState::Done)?;

    step(events, Step::Verify, StepState::Start)?;
    match process::verify(entry_rel, log_file)? {
        VerifyResult::Ok => {
            step(events, Step::Verify, StepState::Done)?;
            Ok(true)
        }
        VerifyResult::Dead { log_tail } => {
            emit_fail(events, "verify", "qs_exited", None, Some(log_tail))?;
            Ok(false)
        }
    }
}

fn split_launch_path(entry: &Path) -> (PathBuf, PathBuf) {
    match entry.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => (
            parent.to_path_buf(),
            entry
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| entry.to_path_buf()),
        ),
        _ => (PathBuf::from("."), entry.to_path_buf()),
    }
}

fn read_tail(path: &Path) -> String {
    match std::fs::read_to_string(path) {
        Ok(c) => process::tail_lines(&c, 20),
        Err(e) => format!("<log unreadable at {}: {}>", path.display(), e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_launch_path_with_parent() {
        let (dir, name) = split_launch_path(Path::new("/foo/bar/shell.qml"));
        assert_eq!(dir, PathBuf::from("/foo/bar"));
        assert_eq!(name, PathBuf::from("shell.qml"));
    }

    #[test]
    fn split_launch_path_without_parent() {
        let (dir, name) = split_launch_path(Path::new("shell.qml"));
        assert_eq!(dir, PathBuf::from("."));
        assert_eq!(name, PathBuf::from("shell.qml"));
    }
}
