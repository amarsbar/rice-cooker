use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use crate::cache::Cache;
use crate::detect::detect_missing_plugins;
use crate::events::{Event, EventWriter, Step, StepState, SCHEMA_VERSION};
use crate::git;
use crate::lock::{ApplyLock, LockError};
use crate::process::{self, extract_entry_arg, find_running_quickshell, VerifyResult};

const SHELL_QML_CANDIDATES: &[&str] = &[
    "shell.qml",
    "ii/shell.qml",
    "quickshell/shell.qml",
    ".config/quickshell/shell.qml",
];

fn find_shell_qml(rice_root: &Path) -> Result<Option<PathBuf>> {
    for &candidate in SHELL_QML_CANDIDATES {
        if rice_root.join(candidate).is_file() {
            return Ok(Some(PathBuf::from(candidate)));
        }
    }
    Ok(None)
}

/// Turn a `Result<T, E>` into its Ok value, or emit a stage fail event and early-return
/// `Ok(false)` from the enclosing function. Collapses the "step X failed, report it
/// and bail cleanly" boilerplate that otherwise repeats at every pipeline gate.
///
/// Enforces the post-hello contract: any early return on the apply/revert/exit path
/// must emit a fail event before surfacing Ok(false) — never exit-code-2 without one.
///
/// Errors are formatted with `{e:#}` so anyhow context chains (e.g. "spawning git fetch:
/// No such file or directory") surface whole; bare `{e}` would drop inner sources.
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
    // With a prefix: useful when multiple sites share a stage but want distinct reasons.
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

    try_stage!(events, "init", cache.ensure_dirs());
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

    // Post-verify: shell is running the new rice. A cache write failure here must
    // still emit a fail event — the UI has already been told the pipeline succeeded
    // through step/verify/done; exit-code-2 without an event breaks the contract.
    let prior = try_stage!(events, "commit", "read_active", cache.active());
    try_stage!(
        events,
        "commit",
        "set_active",
        cache.set_active(params.name)
    );
    if let Some(p) = &prior {
        if p != params.name {
            if let Err(e) = cache.set_previous(p) {
                // Roll back active to the prior value so state stays coherent. If the
                // rollback itself fails, surface both errors in the event so an operator
                // can see that state is now genuinely wedged (active points to the
                // new rice, previous missing) rather than merely "set_previous failed".
                let reason = match cache.set_active(p) {
                    Ok(()) => format!("set_previous: {e}"),
                    Err(rb) => format!("set_previous: {e}; rollback_failed: {rb}"),
                };
                emit_fail(events, "commit", &reason, None, None)?;
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

/// Names must match `^[A-Za-z0-9_][A-Za-z0-9._-]{0,63}$`. This one regex covers every
/// class of reject the old per-case validator enumerated: empty, too-long, `.`/`..`,
/// leading dash, leading dot, path separators, NUL, whitespace, control chars.
fn validate_rice_name(name: &str) -> std::result::Result<(), &'static str> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"^[A-Za-z0-9_][A-Za-z0-9._-]{0,63}$").unwrap());
    if re.is_match(name) {
        Ok(())
    } else {
        Err("invalid_name")
    }
}

pub fn run_revert<W: Write>(cache: &Cache, events: &mut EventWriter<W>) -> Result<bool> {
    hello(events, "revert")?;
    try_stage!(events, "init", cache.ensure_dirs());
    let _lock = match acquire_lock(cache, events)? {
        Some(l) => l,
        None => return Ok(false),
    };

    if !preflight(cache, events)? {
        return Ok(false);
    }

    let previous_name = match try_stage!(events, "revert", "read_previous", cache.previous()) {
        Some(n) => n,
        None => {
            emit_fail(events, "revert", "no_previous", None, None)?;
            return Ok(false);
        }
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

    try_stage!(events, "commit", "swap", cache.swap_active_previous());
    let active = try_stage!(events, "commit", "read_active", cache.active());
    let previous = try_stage!(events, "commit", "read_previous", cache.previous());
    events.emit(&Event::Success {
        active,
        previous,
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
            emit_fail(events, "launch", &format!("{e}"), None, Some(tail))?;
            return Ok(false);
        }
        step(events, Step::Launch, StepState::Done)?;
    }

    try_stage!(events, "commit", "clear", cache.clear_active_previous());
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
            // If the recorded entry is a relative path, resolve it against the process's
            // cwd at scan time so later `exit` can relaunch from the right directory.
            // Bare names (e.g. from `-c mybar`) stay as-is — they aren't paths.
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
    try_stage!(events, "notifiers", process::kill_notif_daemons());
    step(events, Step::Notifiers, StepState::Done)?;

    step(events, Step::KillQuickshell, StepState::Start)?;
    try_stage!(events, "kill_quickshell", process::kill_quickshell());
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
    fn find_shell_qml_walks_candidates_in_order() {
        use std::fs;
        let make = |rel: &str| {
            let tmp = tempfile::tempdir().unwrap();
            let p = tmp.path().join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, b"").unwrap();
            tmp
        };
        // Empty dir → None.
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(find_shell_qml(empty.path()).unwrap(), None);
        // Each candidate found in isolation, relative to rice root.
        for candidate in SHELL_QML_CANDIDATES {
            let tmp = make(candidate);
            assert_eq!(
                find_shell_qml(tmp.path()).unwrap().as_deref(),
                Some(Path::new(candidate)),
                "candidate: {candidate}"
            );
        }
        // Earlier candidate wins when two are present.
        let tmp = tempfile::tempdir().unwrap();
        for rel in ["shell.qml", "ii/shell.qml"] {
            let p = tmp.path().join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, b"").unwrap();
        }
        assert_eq!(
            find_shell_qml(tmp.path()).unwrap(),
            Some(PathBuf::from("shell.qml"))
        );
    }

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

    #[test]
    fn validate_rice_name_accepts_typical_and_rejects_bad() {
        for good in ["caelestia", "noctalia-2", "rice_v1", "Foo.Bar", "a1b2c3"] {
            assert_eq!(validate_rice_name(good), Ok(()), "rejected: {good:?}");
        }
        let bad: &[&str] = &[
            "",
            ".",
            "..",
            "-foo",
            ".hidden",
            "a/b",
            "a\\b",
            "a\0b",
            "with space",
            "with\ttab",
            "with\nnewline",
            "with\rcr",
            "with\x07bell",
        ];
        for name in bad {
            assert_eq!(
                validate_rice_name(name),
                Err("invalid_name"),
                "accepted: {name:?}"
            );
        }
        assert_eq!(
            validate_rice_name(&"a".repeat(65)),
            Err("invalid_name"),
            "65-char name should exceed the 64-char cap"
        );
    }
}
