use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use crate::cache::Cache;
use crate::detect::detect_missing_plugins;
use crate::entry::find_shell_qml;
use crate::events::{Event, EventWriter, Step, StepState, SCHEMA_VERSION};
use crate::git;
use crate::lock::{ApplyLock, LockError};
use crate::proc_info::{extract_entry_arg, find_running_quickshell};
use crate::process::{self, VerifyResult};

pub struct ApplyParams<'a> {
    pub name: &'a str,
    pub repo: &'a str,
    pub dry_run: bool,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub active: Option<String>,
    pub previous: Option<String>,
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
    if let Err(reason) = validate_rice_name(params.name) {
        emit_fail(events, "input", reason, None, None)?;
        return Ok(false);
    }

    cache.ensure_dirs()?;
    let _lock = match acquire_lock(cache, events)? {
        Some(l) => l,
        None => return Ok(false),
    };

    if !preflight(cache, events)? {
        return Ok(false);
    }

    step(events, Step::Clone, StepState::Start)?;
    let rice_dir = cache.rice_dir(params.name);
    let log_file = cache.last_run_log();
    if let Err(e) = git::clone_or_update(params.repo, &rice_dir, &log_file) {
        let tail = read_tail(&log_file);
        emit_fail(events, "clone", &format!("{e}"), None, Some(tail))?;
        return Ok(false);
    }
    step(events, Step::Clone, StepState::Done)?;

    let Some(entry_rel) = locate_entry(&rice_dir, events)? else {
        return Ok(false);
    };

    if !precheck(&rice_dir, events)? {
        return Ok(false);
    }

    if params.dry_run {
        events.emit(&Event::Success {
            active: None,
            previous: None,
            dry_run: true,
        })?;
        return Ok(true);
    }

    if !wet_pipeline(&rice_dir, &entry_rel, &log_file, events)? {
        return Ok(false);
    }

    // Post-verify: shell is running the new rice. If a cache write fails here the user
    // shouldn't see exit-code-2 without a fail event — the UI has already been told the
    // pipeline succeeded through step/verify/done. Translate to a commit-stage fail.
    let prior = match cache.active() {
        Ok(p) => p,
        Err(e) => {
            emit_fail(events, "commit", &format!("read_active: {e}"), None, None)?;
            return Ok(false);
        }
    };
    if let Err(e) = cache.set_active(params.name) {
        emit_fail(events, "commit", &format!("set_active: {e}"), None, None)?;
        return Ok(false);
    }
    if let Some(p) = &prior {
        if p != params.name {
            if let Err(e) = cache.set_previous(p) {
                emit_fail(events, "commit", &format!("set_previous: {e}"), None, None)?;
                return Ok(false);
            }
        }
    }
    events.emit(&Event::Success {
        active: Some(params.name.to_string()),
        previous: prior,
        dry_run: false,
    })?;
    Ok(true)
}

/// Rejects names that would break out of the rice cache or trip git argv parsing.
fn validate_rice_name(name: &str) -> std::result::Result<(), &'static str> {
    if name.is_empty() {
        return Err("empty_name");
    }
    if name.len() > 64 {
        return Err("name_too_long");
    }
    if name == "." || name == ".." {
        return Err("reserved_name");
    }
    if name.starts_with('-') {
        return Err("leading_dash");
    }
    if name.starts_with('.') {
        return Err("leading_dot");
    }
    if name.chars().any(|c| matches!(c, '/' | '\\' | '\0')) {
        return Err("invalid_char");
    }
    Ok(())
}

pub fn run_revert<W: Write>(cache: &Cache, events: &mut EventWriter<W>) -> Result<bool> {
    hello(events, "revert")?;
    cache.ensure_dirs()?;
    let _lock = match acquire_lock(cache, events)? {
        Some(l) => l,
        None => return Ok(false),
    };

    if !preflight(cache, events)? {
        return Ok(false);
    }

    let Some(previous_name) = cache.previous()? else {
        emit_fail(events, "revert", "no_previous", None, None)?;
        return Ok(false);
    };
    let rice_dir = cache.rice_dir(&previous_name);
    if !rice_dir.is_dir() {
        emit_fail(events, "revert", "previous_missing_from_cache", None, None)?;
        return Ok(false);
    }

    let Some(entry_rel) = locate_entry(&rice_dir, events)? else {
        return Ok(false);
    };

    if !precheck(&rice_dir, events)? {
        return Ok(false);
    }

    let log_file = cache.last_run_log();
    if !wet_pipeline(&rice_dir, &entry_rel, &log_file, events)? {
        return Ok(false);
    }

    if let Err(e) = cache.swap_active_previous() {
        emit_fail(events, "commit", &format!("swap: {e}"), None, None)?;
        return Ok(false);
    }
    let active = cache.active()?;
    let previous = cache.previous()?;
    events.emit(&Event::Success {
        active,
        previous,
        dry_run: false,
    })?;
    Ok(true)
}

pub fn run_exit<W: Write>(cache: &Cache, events: &mut EventWriter<W>) -> Result<bool> {
    hello(events, "exit")?;
    cache.ensure_dirs()?;
    let _lock = match acquire_lock(cache, events)? {
        Some(l) => l,
        None => return Ok(false),
    };

    step(events, Step::KillQuickshell, StepState::Start)?;
    process::kill_quickshell()?;
    step(events, Step::KillQuickshell, StepState::Done)?;

    let original = cache.original()?;
    if let Some(entry_path) = original {
        let entry_pb = PathBuf::from(&entry_path);
        let (rice_dir, entry_rel) = split_launch_path(&entry_pb);
        let log_file = cache.last_run_log();
        step(events, Step::Launch, StepState::Start)?;
        if let Err(e) = process::launch_detached(&rice_dir, &entry_rel, &log_file) {
            let tail = read_tail(&log_file);
            emit_fail(events, "launch", &format!("{e}"), None, Some(tail))?;
            return Ok(false);
        }
        step(events, Step::Launch, StepState::Done)?;
    }

    cache.clear_active_previous()?;
    events.emit(&Event::Success {
        active: None,
        previous: None,
        dry_run: false,
    })?;
    Ok(true)
}

pub fn get_status(cache: &Cache) -> Result<Status> {
    Ok(Status {
        active: cache.active()?,
        previous: cache.previous()?,
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
        record_original(cache)?;
    }
    step(events, Step::Preflight, StepState::Done)?;
    Ok(true)
}

fn record_original(cache: &Cache) -> Result<()> {
    match find_running_quickshell()? {
        Some(proc) => {
            let entry = extract_entry_arg(&proc.cmdline);
            cache.set_original(entry.as_deref())?;
        }
        None => cache.set_original(None)?,
    }
    Ok(())
}

fn locate_entry<W: Write>(rice_dir: &Path, events: &mut EventWriter<W>) -> Result<Option<PathBuf>> {
    step(events, Step::Entry, StepState::Start)?;
    match find_shell_qml(rice_dir)? {
        Some(p) => {
            step(events, Step::Entry, StepState::Done)?;
            Ok(Some(p))
        }
        None => {
            emit_fail(events, "entry", "no_shell_qml", None, None)?;
            Ok(None)
        }
    }
}

fn precheck<W: Write>(rice_dir: &Path, events: &mut EventWriter<W>) -> Result<bool> {
    step(events, Step::Precheck, StepState::Start)?;
    let missing = detect_missing_plugins(rice_dir)?;
    if !missing.is_empty() {
        emit_fail(events, "precheck", "missing_plugins", Some(missing), None)?;
        return Ok(false);
    }
    step(events, Step::Precheck, StepState::Done)?;
    Ok(true)
}

fn wet_pipeline<W: Write>(
    rice_dir: &Path,
    entry_rel: &Path,
    log_file: &Path,
    events: &mut EventWriter<W>,
) -> Result<bool> {
    step(events, Step::Notifiers, StepState::Start)?;
    process::kill_notif_daemons()?;
    step(events, Step::Notifiers, StepState::Done)?;

    step(events, Step::KillQuickshell, StepState::Start)?;
    process::kill_quickshell()?;
    step(events, Step::KillQuickshell, StepState::Done)?;

    step(events, Step::Launch, StepState::Start)?;
    if let Err(e) = process::launch_detached(rice_dir, entry_rel, log_file) {
        let tail = read_tail(log_file);
        emit_fail(events, "launch", &format!("{e}"), None, Some(tail))?;
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
        VerifyResult::LogErrors { log_tail } => {
            emit_fail(events, "verify", "qs_error", None, Some(log_tail))?;
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
