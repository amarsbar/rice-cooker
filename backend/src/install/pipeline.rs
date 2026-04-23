//! try / uninstall / list / status pipeline.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use serde::Serialize;

use crate::catalog::{Catalog, RiceEntry, is_placeholder_commit};
use crate::deps;
use crate::events::{Event, EventWriter, SCHEMA_VERSION as EVENT_SCHEMA_VERSION, Step, StepState};
use crate::git;
use crate::lock::{ApplyLock, LockError};
use crate::paths::{OriginalShell, Paths, expand_home};
use crate::process::{self, VerifyResult};

use super::record::{
    InstallRecord, PacmanDiff, SCHEMA_VERSION, clear_current, load_record, read_current,
    save_record, write_current,
};
use super::symlink as symlink_shape;

#[derive(Debug, Clone, Copy, Default)]
pub struct Flags {
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ListRow {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub installed: bool,
    pub documented_system_effects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusRow {
    pub installed: Option<InstallRecord>,
}

/// Unwrap or emit a Fail event and return Ok(false). Post-hello contract.
macro_rules! try_stage {
    ($events:expr, $stage:literal, $expr:expr $(,)?) => {
        match $expr {
            Ok(v) => v,
            Err(e) => {
                emit_fail($events, $stage, &format!("{e:#}"), None)?;
                return Ok(false);
            }
        }
    };
    ($events:expr, $stage:literal, $reason_prefix:literal, $expr:expr $(,)?) => {
        match $expr {
            Ok(v) => v,
            Err(e) => {
                emit_fail(
                    $events,
                    $stage,
                    &format!("{}: {:#}", $reason_prefix, e),
                    None,
                )?;
                return Ok(false);
            }
        }
    };
}

pub fn run_try<W: Write>(
    cat: &Catalog,
    paths: &Paths,
    name: &str,
    events: &mut EventWriter<W>,
) -> Result<bool> {
    hello(events, "try")?;
    try_stage!(events, "init", paths.ensure_rices());
    try_stage!(events, "init", paths.ensure_installs());
    let _lock = match acquire_lock(paths, events)? {
        Some(l) => l,
        None => return Ok(false),
    };

    let entry = match cat.get(name) {
        Some(e) if !is_placeholder_commit(&e.commit) => e,
        Some(e) => {
            emit_fail(
                events,
                "preflight",
                &format!(
                    "{name}: commit is a placeholder ({}); pin a real SHA in catalog.toml",
                    e.commit
                ),
                None,
            )?;
            return Ok(false);
        }
        None => {
            emit_fail(
                events,
                "preflight",
                &format!("{name}: not in catalog"),
                None,
            )?;
            return Ok(false);
        }
    };
    let all_deps: Vec<String> = [entry.pacman_deps.clone(), entry.aur_deps.clone()].concat();

    step(events, Step::Preflight, StepState::Start)?;
    try_stage!(events, "preflight", "git", git::preflight());
    if !all_deps.is_empty() {
        try_stage!(
            events,
            "preflight",
            "polkit_agent",
            deps::check_polkit_agent()
        );
    }
    let current = try_stage!(events, "preflight", read_current(paths));
    if current.is_none() && !paths.original_is_recorded() {
        try_stage!(
            events,
            "preflight",
            "record_original",
            record_original(paths)
        );
    }
    step(events, Step::Preflight, StepState::Done)?;

    if current.as_deref() == Some(name) {
        events.emit(&Event::Success {
            active: Some(name.to_string()),
        })?;
        return Ok(true);
    }

    // Evict outgoing rice (replay=false — we launch a new one next).
    if let Some(outgoing) = current.as_deref() {
        step(events, Step::Evict, StepState::Start)?;
        if !uninstall_locked(paths, Flags::default(), events, outgoing, false)? {
            return Ok(false);
        }
        step(events, Step::Evict, StepState::Done)?;
    }

    step(events, Step::Clone, StepState::Start)?;
    try_stage!(events, "clone", do_clone(paths, name, entry));
    step(events, Step::Clone, StepState::Done)?;

    step(events, Step::Deps, StepState::Start)?;
    let added_explicit = try_stage!(events, "deps", do_deps(&all_deps));
    step(events, Step::Deps, StepState::Done)?;

    // Record + current.json persisted BEFORE symlink so a symlink failure
    // still leaves a record uninstall can use to roll back the packages.
    step(events, Step::Record, StepState::Start)?;
    try_stage!(
        events,
        "record",
        do_record(paths, name, entry, added_explicit)
    );
    step(events, Step::Record, StepState::Done)?;

    step(events, Step::Symlink, StepState::Start)?;
    try_stage!(events, "symlink", do_symlink(paths, name, entry));
    step(events, Step::Symlink, StepState::Done)?;

    step(events, Step::Notifiers, StepState::Start)?;
    try_stage!(events, "notifiers", process::kill_notif_daemons());
    step(events, Step::Notifiers, StepState::Done)?;

    step(events, Step::KillQuickshell, StepState::Start)?;
    try_stage!(events, "kill_quickshell", process::kill_quickshell());
    step(events, Step::KillQuickshell, StepState::Done)?;

    let log_file = paths.last_run_log();
    step(events, Step::Launch, StepState::Start)?;
    if let Err(e) = process::launch_detached_by_name(name, &log_file, &paths.home) {
        let tail = read_tail(&log_file);
        emit_fail(events, "launch", &format!("{e:#}"), Some(tail))?;
        return Ok(false);
    }
    step(events, Step::Launch, StepState::Done)?;

    step(events, Step::Verify, StepState::Start)?;
    let verify_result = match process::verify_by_name(name, &log_file) {
        Ok(r) => r,
        Err(e) => {
            let tail = read_tail(&log_file);
            emit_fail(events, "verify", &format!("{e:#}"), Some(tail))?;
            return Ok(false);
        }
    };
    match verify_result {
        VerifyResult::Ok => step(events, Step::Verify, StepState::Done)?,
        VerifyResult::Dead { log_tail } => {
            emit_fail(events, "verify", "qs_exited", Some(log_tail))?;
            return Ok(false);
        }
    }

    events.emit(&Event::Success {
        active: Some(name.to_string()),
    })?;
    Ok(true)
}

pub fn run_uninstall<W: Write>(
    paths: &Paths,
    flags: Flags,
    events: &mut EventWriter<W>,
) -> Result<bool> {
    hello(events, "uninstall")?;
    try_stage!(events, "init", paths.ensure_rices());
    try_stage!(events, "init", paths.ensure_installs());
    let _lock = match acquire_lock(paths, events)? {
        Some(l) => l,
        None => return Ok(false),
    };

    step(events, Step::Preflight, StepState::Start)?;
    let current = try_stage!(events, "preflight", read_current(paths));
    step(events, Step::Preflight, StepState::Done)?;

    let Some(name) = current else {
        events.emit(&Event::Success { active: None })?;
        return Ok(true);
    };

    if !uninstall_locked(paths, flags, events, &name, true)? {
        return Ok(false);
    }
    events.emit(&Event::Success { active: None })?;
    Ok(true)
}

/// Kill qs, clear state, and (if replay=true) replay the pre-rice shell.
fn uninstall_locked<W: Write>(
    paths: &Paths,
    flags: Flags,
    events: &mut EventWriter<W>,
    name: &str,
    replay: bool,
) -> Result<bool> {
    // record_json validates name; route through try_stage! so a tampered
    // current.json surfaces as a Fail, not bare Err past the post-hello contract.
    let record_path = try_stage!(events, "record", "path", paths.record_json(name));
    let record = try_stage!(events, "record", "load", load_record(&record_path));

    step(events, Step::KillQuickshell, StepState::Start)?;
    try_stage!(events, "kill_quickshell", process::kill_quickshell());
    step(events, Step::KillQuickshell, StepState::Done)?;

    // Pre-filter via pacman -Q so retries don't abort on "target not found".
    step(events, Step::Deps, StepState::Start)?;
    if !record.pacman_diff.added_explicit.is_empty() {
        let still = try_stage!(
            events,
            "deps",
            "filter_installed",
            deps::installed(&record.pacman_diff.added_explicit)
        );
        if !still.is_empty() {
            try_stage!(
                events,
                "deps",
                "remove_packages",
                deps::remove_packages(&still)
            );
        }
    }
    step(events, Step::Deps, StepState::Done)?;

    // Remove symlink only if it still points where we left it. User could
    // have retargeted or replaced it; we don't clobber that.
    step(events, Step::Symlink, StepState::Start)?;
    try_stage!(events, "symlink", {
        match fs::symlink_metadata(&record.symlink_path) {
            Ok(md) if md.file_type().is_symlink() => match fs::read_link(&record.symlink_path) {
                Ok(t) if t == record.symlink_target => fs::remove_file(&record.symlink_path)
                    .with_context(|| format!("removing symlink {}", record.symlink_path.display())),
                Ok(t) => {
                    eprintln!(
                        "rice-cooker: skipping {}: target is {t:?}, expected {:?} (user-retargeted?)",
                        record.symlink_path.display(),
                        record.symlink_target
                    );
                    Ok(())
                }
                Err(e) => Err(anyhow!("read_link {}: {e}", record.symlink_path.display())),
            },
            Ok(_) => {
                eprintln!(
                    "rice-cooker: skipping {}: not a symlink anymore",
                    record.symlink_path.display()
                );
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(anyhow!("reading {}: {e}", record.symlink_path.display())),
        }
    });
    step(events, Step::Symlink, StepState::Done)?;

    // Clear current.json (the pointer) BEFORE removing the record (the target).
    // If the record delete subsequently fails, status still reports None sanely.
    step(events, Step::Record, StepState::Start)?;
    try_stage!(events, "record", "clear_current", clear_current(paths));
    match fs::remove_file(&record_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) if flags.force => eprintln!(
            "rice-cooker: warn: --force: could not remove record {}: {e}",
            record_path.display()
        ),
        Err(e) => {
            emit_fail(
                events,
                "record",
                &format!("removing record {}: {e}", record_path.display()),
                None,
            )?;
            return Ok(false);
        }
    }
    step(events, Step::Record, StepState::Done)?;

    if !replay {
        return Ok(true);
    }

    // Replay the captured pre-rice shell. Always clear `original` afterward —
    // even if replay fails, the captured argv is stale relative to whatever
    // the user has launched since. When both fail, the replay error wins.
    let original = try_stage!(events, "replay", "read_original", paths.original());
    let replay_err = if let Some(shell) = original
        && !shell.argv.is_empty()
    {
        step(events, Step::Replay, StepState::Start)?;
        let cwd = shell
            .cwd
            .as_deref()
            .map(Path::new)
            .unwrap_or_else(|| Path::new("/"));
        let log = paths.last_run_log();
        match process::launch_argv(&shell.argv, cwd, &log) {
            Ok(()) => {
                step(events, Step::Replay, StepState::Done)?;
                None
            }
            Err(e) => Some((format!("{e:#}"), read_tail(&log))),
        }
    } else {
        None
    };
    let clear_err = paths.clear_original().err();
    match (replay_err, clear_err) {
        (Some((reason, tail)), None) => {
            emit_fail(events, "replay", &reason, Some(tail))?;
            Ok(false)
        }
        (Some((reason, tail)), Some(ce)) => {
            // Both failed: include the clear_original error in the Fail so the
            // user sees that `original` on disk is stale and will mis-replay
            // on the next try. Dropping `ce` would leave a silent landmine.
            emit_fail(
                events,
                "replay",
                &format!("{reason}; and clear_original also failed: {ce:#}"),
                Some(tail),
            )?;
            Ok(false)
        }
        (None, Some(ce)) => {
            emit_fail(events, "replay", &format!("clear_original: {ce:#}"), None)?;
            Ok(false)
        }
        (None, None) => Ok(true),
    }
}

pub fn list(cat: &Catalog, paths: &Paths) -> Result<Vec<ListRow>> {
    let current = read_current(paths)?;
    Ok(cat
        .rices
        .iter()
        .map(|(name, entry)| ListRow {
            name: name.clone(),
            display_name: entry.display_name.clone(),
            description: entry.description.clone(),
            installed: current.as_deref() == Some(name.as_str()),
            documented_system_effects: entry.documented_system_effects.clone(),
        })
        .collect())
}

pub fn status(paths: &Paths) -> Result<StatusRow> {
    let Some(name) = read_current(paths)? else {
        return Ok(StatusRow { installed: None });
    };
    Ok(StatusRow {
        installed: Some(load_record(&paths.record_json(&name)?)?),
    })
}

// ── install step helpers ──────────────────────────────────────────────────────

/// HEAD matches `commit` (catalog allows short-SHA prefixes).
pub(crate) fn clone_cache_hit(clone_dir: &Path, commit: &str) -> bool {
    if !clone_dir.join(".git").exists() {
        return false;
    }
    let Ok(out) = Command::new("git")
        .arg("-C")
        .arg(clone_dir)
        .args(["rev-parse", "HEAD"])
        .output()
    else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let head = String::from_utf8_lossy(&out.stdout).trim().to_string();
    head.starts_with(commit) || commit.starts_with(&head)
}

fn do_clone(paths: &Paths, name: &str, entry: &RiceEntry) -> Result<()> {
    let clone = paths.clone_dir(name)?;
    if clone_cache_hit(&clone, &entry.commit) {
        return Ok(());
    }
    if clone.exists() {
        remove_dir_all_forceful(&clone)
            .with_context(|| format!("removing stale clone {}", clone.display()))?;
    }
    git::clone_at_commit(&entry.repo, &entry.commit, &clone)
}

fn do_deps(all_deps: &[String]) -> Result<Vec<String>> {
    let missing = deps::missing(all_deps)?;
    // Fast path: nothing to install → skip both pacman -Qqe snapshots.
    if missing.is_empty() {
        return Ok(Vec::new());
    }
    let pre = pacman_explicit().context("pacman -Qqe pre-snapshot")?;
    deps::install_packages(&missing)?;
    let post = pacman_explicit().context("pacman -Qqe post-snapshot")?;
    Ok(diff_explicit(&pre, &post))
}

fn do_record(paths: &Paths, name: &str, entry: &RiceEntry, added: Vec<String>) -> Result<()> {
    let clone = paths.clone_dir(name)?;
    let record = InstallRecord {
        schema_version: SCHEMA_VERSION,
        name: name.to_string(),
        commit: entry.commit.clone(),
        installed_at: InstallRecord::now_rfc3339(),
        symlink_path: expand_home(&entry.symlink_dst, &paths.home),
        symlink_target: clone.join(&entry.symlink_src),
        pacman_diff: PacmanDiff {
            added_explicit: added,
        },
    };
    save_record(&paths.record_json(name)?, &record)?;
    write_current(paths, name)
}

fn do_symlink(paths: &Paths, name: &str, entry: &RiceEntry) -> Result<()> {
    let clone = paths.clone_dir(name)?;
    symlink_shape::create_symlink(&clone, entry, &paths.home)
        .context("run `rice-cooker-backend uninstall` to roll back")
}

// ── NDJSON + misc helpers ─────────────────────────────────────────────────────

fn hello<W: Write>(events: &mut EventWriter<W>, subcommand: &str) -> Result<()> {
    events.emit(&Event::Hello {
        version: EVENT_SCHEMA_VERSION,
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
    log_tail: Option<String>,
) -> Result<()> {
    events.emit(&Event::Fail {
        stage: stage.to_string(),
        reason: reason.to_string(),
        plugins: None,
        log_tail,
    })?;
    Ok(())
}

fn acquire_lock<W: Write>(paths: &Paths, events: &mut EventWriter<W>) -> Result<Option<ApplyLock>> {
    match ApplyLock::try_acquire(&paths.lock()) {
        Ok(l) => Ok(Some(l)),
        Err(LockError::AlreadyHeld) => {
            emit_fail(events, "lock", "already_held", None)?;
            Ok(None)
        }
        Err(LockError::Io(e)) => {
            emit_fail(events, "lock", "io", Some(format!("{e:#}")))?;
            Ok(None)
        }
    }
}

fn read_tail(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(c) => process::tail_lines(&c, 20),
        Err(e) => format!("<log unreadable at {}: {}>", path.display(), e),
    }
}

fn record_original(paths: &Paths) -> Result<()> {
    match process::find_running_quickshell()? {
        Some(proc) => paths.set_original(Some(&OriginalShell {
            argv: proc.cmdline,
            cwd: proc.cwd.map(|p| p.to_string_lossy().into_owned()),
        })),
        None => paths.set_original(None),
    }
}

fn pacman_explicit() -> Result<Vec<String>> {
    let out = Command::new("pacman")
        .args(["-Qqe"])
        .output()
        .context("running pacman -Qqe")?;
    if !out.status.success() {
        return Err(anyhow!("pacman -Qqe exited {:?}", out.status.code()));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

fn diff_explicit(pre: &[String], post: &[String]) -> Vec<String> {
    use std::collections::HashSet;
    let pre_set: HashSet<&str> = pre.iter().map(String::as_str).collect();
    let mut added: Vec<String> = post
        .iter()
        .filter(|p| !pre_set.contains(p.as_str()))
        .cloned()
        .collect();
    added.sort();
    added
}

/// Try std::fs first, fall back to `rm -rf --` on PermissionDenied etc.
/// Covers makepkg's 0111-perm `pkg/` dir and whatever other surprises a
/// rice's hooks leave behind.
fn remove_dir_all_forceful(path: &Path) -> Result<()> {
    let fs_err = match fs::remove_dir_all(path) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => e,
    };
    let status = Command::new("rm")
        .arg("-rf")
        .arg("--")
        .arg(path)
        .status()
        .map_err(|rm_err| {
            anyhow!(
                "spawning rm -rf {}: {rm_err} (after std::fs::remove_dir_all failed: {fs_err})",
                path.display()
            )
        })?;
    if !status.success() {
        return Err(anyhow!(
            "rm -rf {} exited {:?} (after std::fs::remove_dir_all failed: {fs_err})",
            path.display(),
            status.code()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

    #[test]
    fn diff_explicit_finds_added() {
        let pre = vec!["a".into(), "b".into()];
        let post = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        assert_eq!(diff_explicit(&pre, &post), vec!["c", "d"]);
    }

    #[test]
    fn diff_explicit_ignores_removed_and_unchanged() {
        let pre: Vec<String> = vec!["a".into(), "b".into()];
        let shrunk: Vec<String> = vec!["a".into()];
        assert!(diff_explicit(&pre, &shrunk).is_empty());
        assert!(diff_explicit(&pre, &pre).is_empty());
    }

    fn init_repo_at(dir: &Path) -> String {
        let run = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .expect("git spawn")
                .success()
                .then_some(())
                .expect("git ok");
        };
        fs::create_dir_all(dir).unwrap();
        run(&["init"]);
        run(&["config", "user.email", "t@example.com"]);
        run(&["config", "user.name", "T"]);
        fs::write(dir.join("README"), b"rice").unwrap();
        run(&["add", "."]);
        run(&["commit", "-m", "init"]);
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn clone_cache_hit_for_matching_head() {
        let t = tempfile::tempdir().unwrap();
        let sha = init_repo_at(t.path());
        assert!(clone_cache_hit(t.path(), &sha));
        // Short-SHA catalog entries still hit when clone is at the full SHA.
        assert!(clone_cache_hit(t.path(), &sha[..7]));
    }

    #[test]
    fn clone_cache_invalidated_for_differing_head() {
        let t = tempfile::tempdir().unwrap();
        let _sha = init_repo_at(t.path());
        assert!(!clone_cache_hit(t.path(), "deadbeef00000000"));
        // Missing dir + dir without .git both invalidate.
        let t2 = tempfile::tempdir().unwrap();
        assert!(!clone_cache_hit(t2.path(), "deadbeef"));
        assert!(!clone_cache_hit(&t2.path().join("not-there"), "deadbeef"));
    }

    fn tmp_paths() -> (tempfile::TempDir, Paths) {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().to_path_buf();
        let cache = home.join(".cache/rice-cooker");
        let data = home.join(".local/share/rice-cooker");
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir_all(&data).unwrap();
        let p = Paths::at_roots(home, cache, data);
        p.ensure_rices().unwrap();
        p.ensure_installs().unwrap();
        (t, p)
    }

    #[test]
    fn run_uninstall_is_idempotent_when_nothing_is_installed() {
        let (_t, paths) = tmp_paths();
        let mut buf = Vec::new();
        {
            let mut events = EventWriter::new(&mut buf);
            assert!(run_uninstall(&paths, Flags::default(), &mut events).unwrap());
        }
        let out = std::str::from_utf8(&buf).unwrap();
        assert!(out.contains(r#""type":"success""#));
        assert!(!out.contains(r#""type":"fail""#));
        // No destructive steps fired.
        assert!(!out.contains(r#""step":"kill_quickshell""#));
        assert!(!out.contains(r#""step":"deps""#));
    }

    #[test]
    fn run_try_emits_fail_for_missing_catalog_entry() {
        let (_t, paths) = tmp_paths();
        let cat = Catalog::default();
        let mut buf = Vec::new();
        {
            let mut events = EventWriter::new(&mut buf);
            assert!(!run_try(&cat, &paths, "x", &mut events).unwrap());
        }
        let out = std::str::from_utf8(&buf).unwrap();
        assert!(out.contains(r#""stage":"preflight""#));
        assert!(out.contains("not in catalog"));
    }
}
