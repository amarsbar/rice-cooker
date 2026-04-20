use std::io::Write;
use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use std::fs;

use crate::cache::{Cache, OriginalShell};
use crate::events::{Event, EventWriter, SCHEMA_VERSION, Step, StepState};
use crate::git;
use crate::lock::{ApplyLock, LockError};
use crate::process::{self, VerifyResult, find_running_quickshell};

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

    // Validate the requested entry up front so `--entry /etc/passwd` or
    // `--entry ../escape.qml` gets a dedicated `entry_invalid` reason rather than
    // silently falling through to a fallback (which would mislead catalog debugging).
    let preferred_entry = match validate_entry_path(params.entry) {
        Some(p) => p,
        None => {
            emit_fail(events, "input", "entry_invalid", None, None)?;
            return Ok(false);
        }
    };
    let entry_rel = match resolve_entry(&rice_dir, &preferred_entry) {
        Some(r) => {
            if r != preferred_entry {
                // Visible to the operator but out-of-band from the NDJSON stream, so
                // UIs that only read stdout aren't affected; CLI users see the note.
                eprintln!(
                    "rice-cooker-backend: entry '{}' not found; using '{}' instead",
                    preferred_entry.display(),
                    r.display()
                );
            }
            r
        }
        None => {
            emit_fail(events, "entry", "entry_missing", None, None)?;
            return Ok(false);
        }
    };

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
    if let Some(shell) = original {
        let cwd = shell
            .cwd
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let log_file = cache.last_run_log();
        step(events, Step::Launch, StepState::Start)?;
        if let Err(e) = process::launch_argv(&shell.argv, &cwd, &log_file) {
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
        // Render the saved argv as a human-readable command line for display only;
        // the full argv lives in the cache file for exit-time restoration.
        original: cache.original()?.map(|s| s.argv.join(" ")),
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
            let shell = OriginalShell {
                argv: proc.cmdline,
                cwd: proc.cwd.map(|p| p.to_string_lossy().into_owned()),
            };
            cache.set_original(Some(&shell))?;
        }
        None => cache.set_original(None)?,
    }
    Ok(())
}

/// Reject absolute paths and any component containing `..` so the entry path can't
/// escape the cloned rice directory. The curated-catalog threat model doesn't
/// defend against symlinks *inside* the clone (a malicious maintainer with commit
/// access could point `shell.qml -> /etc/passwd`); quickshell would refuse to load
/// that content, so the blast radius is a failed verify, not data exfiltration.
fn validate_entry_path(entry: &str) -> Option<PathBuf> {
    let pb = PathBuf::from(entry);
    if pb.is_absolute() || pb.components().any(|c| matches!(c, Component::ParentDir)) {
        return None;
    }
    Some(pb)
}

/// Resolve the rice's entry file. Try the requested path first; if missing, walk
/// the rice directory looking for any `shell.qml` and pick the shallowest match
/// (ties broken lexicographically). Only `.git` is skipped during the walk —
/// dotfile-style repos commonly stash their shell under `.config/quickshell/`,
/// so we can't blanket-skip dot-prefixed directories.
fn resolve_entry(rice_dir: &Path, preferred: &Path) -> Option<PathBuf> {
    if rice_dir.join(preferred).is_file() {
        return Some(preferred.to_path_buf());
    }
    let mut matches: Vec<PathBuf> = Vec::new();
    collect_shell_qml(rice_dir, rice_dir, &mut matches);
    matches.sort_by(|a, b| {
        a.components()
            .count()
            .cmp(&b.components().count())
            .then_with(|| a.cmp(b))
    });
    matches.into_iter().next()
}

fn collect_shell_qml(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == ".git" {
            continue;
        }
        let path = entry.path();
        match entry.file_type() {
            // Symlinks fall through both arms (is_dir/is_file false on Linux),
            // so we never descend into or collect a symlinked shell.qml.
            Ok(ft) if ft.is_dir() => collect_shell_qml(root, &path, out),
            Ok(ft) if ft.is_file() && name_str == "shell.qml" => {
                if let Ok(rel) = path.strip_prefix(root) {
                    out.push(rel.to_path_buf());
                }
            }
            _ => {}
        }
    }
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
    match process::verify(rice_dir, entry_rel, log_file)? {
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
    fn validate_entry_path_accepts_relative_rejects_absolute_and_traversal() {
        assert_eq!(
            validate_entry_path("shell.qml"),
            Some(PathBuf::from("shell.qml"))
        );
        assert_eq!(
            validate_entry_path("quickshell/shell.qml"),
            Some(PathBuf::from("quickshell/shell.qml"))
        );
        assert_eq!(validate_entry_path("/etc/passwd"), None);
        assert_eq!(validate_entry_path("../escape.qml"), None);
        assert_eq!(validate_entry_path("nested/../escape.qml"), None);
    }

    #[test]
    fn resolve_entry_prefers_requested_then_walks_for_shell_qml() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // 1. Requested path exists — take it verbatim.
        std::fs::write(root.join("custom.qml"), b"").unwrap();
        assert_eq!(
            resolve_entry(root, Path::new("custom.qml")),
            Some(PathBuf::from("custom.qml"))
        );

        // 2. Requested missing, a deep shell.qml exists — walk finds it.
        std::fs::create_dir_all(root.join("nested/deep")).unwrap();
        std::fs::write(root.join("nested/deep/shell.qml"), b"").unwrap();
        assert_eq!(
            resolve_entry(root, Path::new("missing.qml")),
            Some(PathBuf::from("nested/deep/shell.qml"))
        );

        // 3. Shallower match added — shallowest wins.
        std::fs::create_dir_all(root.join("quickshell")).unwrap();
        std::fs::write(root.join("quickshell/shell.qml"), b"").unwrap();
        assert_eq!(
            resolve_entry(root, Path::new("missing.qml")),
            Some(PathBuf::from("quickshell/shell.qml"))
        );

        // 4. Root shell.qml beats every nested match.
        std::fs::write(root.join("shell.qml"), b"").unwrap();
        assert_eq!(
            resolve_entry(root, Path::new("missing.qml")),
            Some(PathBuf::from("shell.qml"))
        );

        // 5. Lexicographic tie-break between two equally-shallow matches.
        let tied = tempfile::tempdir().unwrap();
        let troot = tied.path();
        std::fs::create_dir_all(troot.join("zeta")).unwrap();
        std::fs::create_dir_all(troot.join("alpha")).unwrap();
        std::fs::write(troot.join("zeta/shell.qml"), b"").unwrap();
        std::fs::write(troot.join("alpha/shell.qml"), b"").unwrap();
        assert_eq!(
            resolve_entry(troot, Path::new("missing.qml")),
            Some(PathBuf::from("alpha/shell.qml"))
        );

        // 6. `.git` is skipped (clone metadata shouldn't win) but other dot-dirs
        //    like `.config/` are walked — that's the common dotfiles-repo layout.
        let mixed = tempfile::tempdir().unwrap();
        let mroot = mixed.path();
        std::fs::create_dir_all(mroot.join(".git")).unwrap();
        std::fs::write(mroot.join(".git/shell.qml"), b"").unwrap();
        std::fs::create_dir_all(mroot.join(".config/quickshell")).unwrap();
        std::fs::write(mroot.join(".config/quickshell/shell.qml"), b"").unwrap();
        assert_eq!(
            resolve_entry(mroot, Path::new("missing.qml")),
            Some(PathBuf::from(".config/quickshell/shell.qml"))
        );
    }
}
