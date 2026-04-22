//! Glue: install / uninstall / switch / list / status pipelines.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::catalog::{Catalog, RiceEntry, Shape};
use crate::git;
use crate::lock::ApplyLock;

use super::diff::{self, FsDiff};
use super::env::{Dirs, expand_home};
use super::pacman::{self, ExplicitSet};
use super::record::{
    InstallRecord, PacmanDiff, SCHEMA_VERSION, UninstallPhase, clear_current, clear_in_progress,
    load_record, read_current, read_in_progress, retire_to_previous, save_record, write_current,
    write_in_progress,
};
use super::run;
use super::snapshot::{self, Manifest, WalkOpts, path_key};
use super::symlink as symlink_shape;
use super::systemd;

/// Flags shared across the mutating subcommands.
#[derive(Debug, Clone, Copy, Default)]
pub struct Flags {
    pub dry_run: bool,
    pub force: bool,
    pub no_confirm: bool,
    pub verbose: bool,
    /// If true, run the pacman `-Rns` step on uninstall. If false, skip
    /// pacman work (tests that don't have a real pacman on PATH, or the
    /// user passes `--skip-pacman`).
    pub skip_pacman: bool,
}

/// Install <name> from the catalog. Acquires the lock.
pub fn install(cat: &Catalog, dirs: &Dirs, name: &str, flags: Flags) -> Result<InstallOutcome> {
    dirs.ensure()?;
    let _lock = ApplyLock::try_acquire(&dirs.lock_file()).map_err(|e| anyhow!("lock: {e}"))?;
    install_locked(cat, dirs, name, flags)
}

/// Install pipeline WITHOUT acquiring the lock. Caller must already hold
/// it. Exists so `switch` can hold the lock across uninstall+install.
///
/// Error-cleanup semantics have two phases:
/// - PRE-install_cmd (clone, pre-snapshot, pre-content backup): on error,
///   nuke `snapshot_dir` and `clone_dir` so a retry starts from scratch.
///   Nothing is on the user's filesystem yet except git clone state.
/// - POST-install_cmd (post-snapshot, diff compute, record write): on
///   error, DO NOT clean up. The rice has written files to HOME. We need
///   the clone + snapshot dir alive so `uninstall` can reverse whatever
///   did land. The inner path attempts a best-effort record write so the
///   user has something to uninstall even if later steps failed.
///
/// The phase boundary is tracked via an in-memory `AtomicBool` passed
/// through as `cmd_ran`. Using an in-memory flag (rather than the
/// previous on-disk `.cmd-ran` sentinel) closes R6-I-2: a sentinel-create
/// failure can't silently put us on the wrong side of the phase.
fn install_locked(cat: &Catalog, dirs: &Dirs, name: &str, flags: Flags) -> Result<InstallOutcome> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let cmd_ran = AtomicBool::new(false);
    let result = install_locked_inner(cat, dirs, name, flags, &cmd_ran);
    if result.is_err() && !cmd_ran.load(Ordering::SeqCst) {
        let _ = fs::remove_dir_all(dirs.snapshot_dir(name));
        let _ = fs::remove_dir_all(dirs.clone_dir(name));
    }
    result
}

fn install_locked_inner(
    cat: &Catalog,
    dirs: &Dirs,
    name: &str,
    flags: Flags,
    cmd_ran: &std::sync::atomic::AtomicBool,
) -> Result<InstallOutcome> {
    let entry = cat
        .get(name)
        .ok_or_else(|| anyhow!("{name}: not in catalog"))?;

    if crate::catalog::is_placeholder_commit(&entry.commit) {
        return Err(anyhow!(
            "{name}: catalog commit is a placeholder ({}). Pin a real SHA in catalog.toml before installing.",
            entry.commit
        ));
    }

    if let Some(cur) = read_current(dirs)? {
        return Err(anyhow!(
            "{cur} is already installed — run uninstall or switch first"
        ));
    }
    // In-progress marker from a crashed prior install → refuse. The user
    // must run `rice-cooker cleanup` to restore pre-install state before
    // retrying. `--dry-run` is allowed so users can still inspect what
    // would happen without committing.
    if !flags.dry_run && read_in_progress(dirs)?.is_some() {
        return Err(anyhow!(
            "a previous install was interrupted — run `rice-cooker cleanup` to reset, then retry"
        ));
    }

    // Shape dispatch: symlink-shape rices take a fast path (clone +
    // `ln -sfnT`, no filesystem snapshot). Dotfiles rices continue
    // through the existing pre/post-snapshot pipeline.
    if entry.shape == Shape::Symlink {
        return install_symlink_locked(dirs, name, entry, flags, cmd_ran);
    }

    // Write the in-progress marker BEFORE any mutating work. Deleted at
    // the end in save_record, or left in place on crash so cleanup can
    // see the interrupted state.
    if !flags.dry_run {
        write_in_progress(dirs, name, entry.shape)?;
    }

    // Clone / re-clone.
    let clone = dirs.clone_dir(name);
    if clone.exists() {
        fs::remove_dir_all(&clone)
            .with_context(|| format!("removing stale clone {}", clone.display()))?;
    }
    // Nuke any stale snapshot-dir too. R5 finding: a prior install that
    // crashed after install_cmd leaves snapshot_dir in place (so the user
    // can uninstall). If the user instead retries install, we MUST discard
    // the stale content/ + manifest.json + .cmd-ran — they describe a
    // different pre-snapshot than the one we're about to take, and
    // trusting them would silently restore wrong content on the next
    // uninstall.
    let snap_dir = dirs.snapshot_dir(name);
    if snap_dir.exists() {
        fs::remove_dir_all(&snap_dir)
            .with_context(|| format!("removing stale snapshot {}", snap_dir.display()))?;
    }
    log_verbose(flags, &format!("cloning {} @ {}", entry.repo, entry.commit));
    git::clone_at_commit(&entry.repo, &entry.commit, &clone)?;

    let walk_opts = WalkOpts::for_home_with_extras(
        &dirs.home,
        &entry.extra_watched_roots,
        &entry.partial_ownership,
        &entry.runtime_regenerated,
    )?;

    // Pre-snapshot.
    log_verbose(flags, "pre-snapshot");
    let pre = snapshot::take_snapshot(&walk_opts)?;
    fs::create_dir_all(snap_dir.join("content"))
        .with_context(|| format!("creating {}", snap_dir.display()))?;
    snapshot::save_manifest(&snap_dir.join("manifest.json"), &pre)?;

    // Back up pre-install content (hardlink-first, copy fallback). Any
    // path that couldn't be trusted (TOCTOU race between snapshot and
    // backup, permission denied) is returned and threaded into the record
    // so uninstall can SKIP restore rather than restoring wrong content.
    log_verbose(flags, "pre-content backup");
    let unrestorable = capture_pre_content(&snap_dir, &pre)?;

    // Pre-pacman state (skippable for tests).
    let pre_pacman = if flags.skip_pacman {
        ExplicitSet::default()
    } else {
        pacman::snapshot_explicit().unwrap_or_else(|e| {
            eprintln!("pacman pre-snapshot failed ({e}); skipping pacman diff");
            ExplicitSet::default()
        })
    };

    if flags.dry_run {
        println!("would run: cd {} && {}", clone.display(), entry.install_cmd);
        return Ok(InstallOutcome {
            name: name.to_string(),
            partial: false,
            fs_diff: FsDiff::default(),
            pacman_diff: PacmanDiff::default(),
            log_path: PathBuf::new(),
        });
    }

    // Run install.sh.
    let log_path = run::log_path(&dirs.logs_dir(), name);
    log_verbose(flags, &format!("running install: {}", entry.install_cmd));
    let exit_code = run::run_install_cmd(
        &clone,
        &entry.install_cmd,
        entry.interactive,
        &log_path,
        &[("RICE_COOKER_NAME".into(), name.into())],
    )?;
    let partial = exit_code != 0;

    // Install_cmd has run. From here on, the rice may have deployed files
    // to HOME — the outer install_locked wrapper must NOT clean up cache
    // state on subsequent error. Flip the in-memory flag so that guard
    // doesn't depend on a sentinel file that could fail to create.
    cmd_ran.store(true, std::sync::atomic::Ordering::SeqCst);

    // Post-snapshot. If this fails, best-effort record below still runs.
    log_verbose(flags, "post-snapshot");
    let post = match snapshot::take_snapshot(&walk_opts) {
        Ok(p) => p,
        Err(e) => {
            // We ran install_cmd but can't observe what it did. Write a
            // minimal record so the user has a way to retry uninstall or
            // manually clean up.
            write_partial_crashed_record(
                dirs,
                name,
                entry,
                exit_code,
                &log_path,
                &format!("post_snapshot_failed: {e:#}"),
            );
            return Err(anyhow::anyhow!(
                "install post-snapshot failed: {e:#}. Files may have been deployed to HOME; `status` shows what we have."
            ));
        }
    };
    let diff = diff::compute(&pre, &post);

    // Pre content was backed up wholesale above. Modified/deleted files
    // restore from that store; nothing more to do per-path here.

    // Post-pacman state.
    let post_pacman = if flags.skip_pacman {
        ExplicitSet::default()
    } else {
        pacman::snapshot_explicit().unwrap_or_default()
    };
    let pacman_diff = pacman::diff_sets(&pre_pacman, &post_pacman);

    // Systemd unit detection.
    let units = systemd::detect_enabled_units(&dirs.home, &diff);

    // Compile the record.
    let record = InstallRecord {
        schema_version: SCHEMA_VERSION,
        name: name.to_string(),
        commit: entry.commit.clone(),
        shape: entry.shape,
        symlink_path: None,
        symlink_target: None,
        catalog_entry_hash: hash_catalog_entry(entry),
        installed_at: InstallRecord::now_rfc3339(),
        install_cmd: entry.install_cmd.clone(),
        exit_code,
        partial,
        fs_diff: diff.clone(),
        pacman_diff: pacman_diff.clone(),
        partial_ownership_paths: entry
            .partial_ownership
            .iter()
            .map(|s| expand_home(s, &dirs.home))
            .collect(),
        runtime_regenerated_paths: entry
            .runtime_regenerated
            .iter()
            .map(|s| expand_home(s, &dirs.home))
            .collect(),
        systemd_units_enabled: units,
        unrestorable_paths: {
            let mut v: Vec<PathBuf> = unrestorable.into_iter().collect();
            v.sort();
            v
        },
        crash_recovery: false,
        uninstall_phase: None,
        log_path: log_path.clone(),
    };
    // Past this point, install_cmd has deployed files to HOME. If the
    // record / current.json write fails (disk full, permissions flipped),
    // we MUST surface loud recovery guidance — silently returning Err
    // would orphan the user with no state, no way to uninstall, and no
    // idea what happened.
    if let Err(e) = save_record(&dirs.record_json(name), &record) {
        eprintln!(
            "\n=== CRITICAL: install succeeded but saving the record FAILED ===\n\
             error: {e:#}\n\
             The rice has likely deployed files to $HOME but rice-cooker has no\n\
             record to reverse them. Review the install log at {} and clean up\n\
             HOME manually. The clone + snapshot dirs are preserved at:\n  {}\n  {}\n",
            log_path.display(),
            clone.display(),
            snap_dir.display()
        );
        return Err(e);
    }
    if let Err(e) = write_current(dirs, name) {
        eprintln!(
            "\n=== CRITICAL: record saved but current.json write FAILED ===\n\
             error: {e:#}\n\
             The record exists at {} but rice-cooker won't find it via status/uninstall.\n\
             Manually move it to current.json, or retry `install {name}` after\n\
             resolving the underlying filesystem issue.\n",
            dirs.record_json(name).display()
        );
        return Err(e);
    }
    // Clear the in-progress marker last — presence of the marker + no
    // record means a crash between install_cmd and record save. A
    // non-fatal clear failure still leaves the record valid; the marker
    // just has to be reconciled by the next cleanup run.
    let _ = clear_in_progress(dirs);

    Ok(InstallOutcome {
        name: name.to_string(),
        partial,
        fs_diff: diff,
        pacman_diff,
        log_path,
    })
}

/// Symlink-shape install: clone the repo, create the symlink. No
/// filesystem snapshot, no `install_cmd` (forbidden by catalog
/// validation). Any system-package deps the rice needs will be installed
/// via the privileged helper binary (v2 spec Phase 1), not yet wired;
/// for now, symlink rices only work when their deps are already in the
/// PKGBUILD baseline. The pre/post `pacman -Qqe` snapshots below capture
/// whatever happens to be installed so the record can reverse it if a
/// future helper-binary step later populates `pacman_diff.added_explicit`.
fn install_symlink_locked(
    dirs: &Dirs,
    name: &str,
    entry: &RiceEntry,
    flags: Flags,
    cmd_ran: &std::sync::atomic::AtomicBool,
) -> Result<InstallOutcome> {
    // In-progress marker written before any mutation. Cleanup reads this
    // to know what to reverse if the install is interrupted.
    if !flags.dry_run {
        write_in_progress(dirs, name, Shape::Symlink)?;
    }

    // Clone / re-clone.
    let clone = dirs.clone_dir(name);
    if clone.exists() {
        fs::remove_dir_all(&clone)
            .with_context(|| format!("removing stale clone {}", clone.display()))?;
    }
    log_verbose(flags, &format!("cloning {} @ {}", entry.repo, entry.commit));
    git::clone_at_commit(&entry.repo, &entry.commit, &clone)?;

    // Pre-pacman state: populate so the uninstall record can reverse
    // anything that got installed (wired once the helper binary lands;
    // today a symlink install doesn't run pacman itself).
    let pre_pacman = if flags.skip_pacman {
        ExplicitSet::default()
    } else {
        pacman::snapshot_explicit().unwrap_or_default()
    };

    if flags.dry_run {
        let dst = expand_home(&entry.symlink_dst, &dirs.home);
        let src = clone.join(&entry.symlink_src);
        println!("would symlink: {} -> {}", dst.display(), src.display());
        return Ok(InstallOutcome {
            name: name.to_string(),
            partial: false,
            fs_diff: FsDiff::default(),
            pacman_diff: PacmanDiff::default(),
            log_path: PathBuf::new(),
        });
    }

    // Create the symlink — this is the entire "install" for this shape.
    // After this point, there's user-visible state on disk; do NOT let
    // the outer install_locked cleanup nuke the clone dir if a later
    // record-write fails.
    let paths = symlink_shape::create_symlink(&clone, entry, &dirs.home)?;
    cmd_ran.store(true, std::sync::atomic::Ordering::SeqCst);

    let post_pacman = if flags.skip_pacman {
        ExplicitSet::default()
    } else {
        pacman::snapshot_explicit().unwrap_or_default()
    };
    let pacman_diff = pacman::diff_sets(&pre_pacman, &post_pacman);

    // Log path: symlink installs don't run install.sh but the record
    // requires a log_path field. Point at a placeholder that logs_dir
    // manages so `status` has something readable.
    let log_path = run::log_path(&dirs.logs_dir(), name);
    // Touch the log so it exists (otherwise `status` reports a missing
    // file). Failure is non-fatal.
    let _ = fs::write(
        &log_path,
        b"symlink-shape install: no install.sh executed\n",
    );

    let record = InstallRecord {
        schema_version: SCHEMA_VERSION,
        name: name.to_string(),
        commit: entry.commit.clone(),
        shape: Shape::Symlink,
        symlink_path: Some(paths.symlink_path.clone()),
        symlink_target: Some(paths.symlink_target.clone()),
        catalog_entry_hash: hash_catalog_entry(entry),
        installed_at: InstallRecord::now_rfc3339(),
        install_cmd: String::new(),
        exit_code: 0,
        partial: false,
        fs_diff: FsDiff::default(),
        pacman_diff: pacman_diff.clone(),
        partial_ownership_paths: vec![],
        runtime_regenerated_paths: vec![],
        systemd_units_enabled: vec![],
        unrestorable_paths: vec![],
        crash_recovery: false,
        uninstall_phase: None,
        log_path: log_path.clone(),
    };
    if let Err(e) = save_record(&dirs.record_json(name), &record) {
        eprintln!(
            "\n=== CRITICAL: symlink installed but saving the record FAILED ===\n\
             error: {e:#}\n\
             The symlink exists at {} but rice-cooker can't reverse it via\n\
             uninstall. Remove it manually with:\n  rm {}\n",
            paths.symlink_path.display(),
            paths.symlink_path.display()
        );
        return Err(e);
    }
    if let Err(e) = write_current(dirs, name) {
        eprintln!(
            "\n=== CRITICAL: record saved but current.json write FAILED ===\n\
             error: {e:#}\n\
             Manually move {} to current.json, or retry `install {name}`.\n",
            dirs.record_json(name).display()
        );
        return Err(e);
    }
    // Clear the in-progress marker on success. Non-fatal if it lingers —
    // cleanup would only run once the user explicitly invokes it.
    let _ = clear_in_progress(dirs);
    Ok(InstallOutcome {
        name: name.to_string(),
        partial: false,
        fs_diff: FsDiff::default(),
        pacman_diff,
        log_path,
    })
}

/// Uninstall the currently-installed rice. Acquires the lock.
pub fn uninstall(dirs: &Dirs, flags: Flags) -> Result<UninstallOutcome> {
    dirs.ensure()?;
    let _lock = ApplyLock::try_acquire(&dirs.lock_file()).map_err(|e| anyhow!("lock: {e}"))?;
    uninstall_locked(dirs, flags)
}

/// Symlink-shape uninstall: pacman pre-filter → rcsave user edits →
/// remove symlink → delete clone → retire record. All steps idempotent;
/// safe to retry after a mid-way crash.
fn uninstall_symlink_locked(
    dirs: &Dirs,
    name: String,
    record: InstallRecord,
    flags: Flags,
) -> Result<UninstallOutcome> {
    if flags.dry_run {
        let sym = record
            .symlink_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<unknown>".into());
        println!("would remove symlink {sym}");
        return Ok(UninstallOutcome {
            name,
            rcsave_paths: vec![],
            crash_record: false,
            preserved_snapshot_dir: None,
        });
    }

    // 1. Reverse pacman diff. Pre-filter already-removed packages: pacman
    //    -Rns on a non-installed pkg exits non-zero, which would trigger
    //    abort and block retry after a mid-uninstall crash.
    if !flags.skip_pacman && !record.pacman_diff.added_explicit.is_empty() {
        let still_installed: Vec<String> = record
            .pacman_diff
            .added_explicit
            .iter()
            .filter(|p| pacman::is_installed(p).unwrap_or(true))
            .cloned()
            .collect();
        if !still_installed.is_empty() {
            pacman::remove_added(&still_installed, flags.no_confirm).context("sudo pacman -Rns")?;
        }
    }

    // 2. Disable any systemd units that ended up in the record. Symlink
    //    installs today don't enable units (install_cmd is forbidden),
    //    so the list is expected to be empty — this call is defensive
    //    against future helper-binary wiring. `disable_units` no-ops on
    //    empty input.
    systemd::disable_units(&record.systemd_units_enabled)?;

    // 3. Preserve user edits + remove symlink. The rcsave step runs
    //    BEFORE symlink removal so a rcsave failure doesn't leave the
    //    user without their symlink AND without their edits.
    let clone_dir = dirs.clone_dir(&name);
    let rcsave_root = symlink_shape::rcsave_root(dirs, &name);
    let rcsave_paths = if let Some(sym) = record.symlink_path.as_ref() {
        symlink_shape::remove_symlink_with_preservation(&clone_dir, sym, &rcsave_root)?
    } else {
        // Legacy records without symlink_path shouldn't exist, but don't
        // crash if one does.
        vec![]
    };

    // 4. Delete clone dir.
    let _ = fs::remove_dir_all(&clone_dir);

    // 5. Retire record.
    retire_to_previous(dirs, &name)?;
    clear_current(dirs)?;

    Ok(UninstallOutcome {
        name,
        rcsave_paths,
        crash_record: false,
        preserved_snapshot_dir: None,
    })
}

/// Uninstall WITHOUT acquiring the lock. Caller must already hold it.
fn uninstall_locked(dirs: &Dirs, flags: Flags) -> Result<UninstallOutcome> {
    let name = read_current(dirs)?.ok_or_else(|| anyhow!("no rice installed"))?;
    let record = load_record(&dirs.record_json(&name))?;

    if record.partial && !flags.force {
        return Err(anyhow!(
            "{name} was installed partially (install script exit {}); re-run with --force to proceed",
            record.exit_code
        ));
    }

    // Shape dispatch: symlink records take the minimal fast-path
    // (rcsave user edits, rm symlink, rm clone). Dotfiles records go
    // through the full fs-diff reversal below.
    if record.shape == Shape::Symlink {
        return uninstall_symlink_locked(dirs, name, record, flags);
    }

    if flags.dry_run {
        println!(
            "would reverse {} operations",
            diff_op_count(&record.fs_diff)
        );
        return Ok(UninstallOutcome {
            name,
            rcsave_paths: vec![],
            crash_record: false,
            preserved_snapshot_dir: None,
        });
    }

    // Phase-aware retry: if the record was stamped with a completed
    // phase by a prior uninstall that crashed mid-way, skip past it.
    // All phases are individually idempotent (pacman pre-filter,
    // systemctl-disable on already-disabled, fs-diff current-state
    // check) so skipping avoids unnecessary work and surprising side
    // effects.
    let skip_pacman_phase = matches!(
        record.uninstall_phase,
        Some(
            UninstallPhase::Pacman
                | UninstallPhase::Systemd
                | UninstallPhase::FsDiff
                | UninstallPhase::Cleanup
        )
    );
    let skip_systemd_phase = matches!(
        record.uninstall_phase,
        Some(UninstallPhase::Systemd | UninstallPhase::FsDiff | UninstallPhase::Cleanup)
    );
    let skip_fs_diff_phase = matches!(
        record.uninstall_phase,
        Some(UninstallPhase::FsDiff | UninstallPhase::Cleanup)
    );
    let skip_cleanup_phase = matches!(record.uninstall_phase, Some(UninstallPhase::Cleanup));

    // 1. Reverse pacman. Pre-filter already-removed packages: `pacman
    //    -Rns` on a non-installed pkg exits non-zero, which would abort
    //    uninstall and block retry after a mid-uninstall crash.
    if !skip_pacman_phase && !flags.skip_pacman && !record.pacman_diff.added_explicit.is_empty() {
        let still_installed: Vec<String> = record
            .pacman_diff
            .added_explicit
            .iter()
            .filter(|p| pacman::is_installed(p).unwrap_or(true))
            .cloned()
            .collect();
        if !still_installed.is_empty() {
            pacman::remove_added(&still_installed, flags.no_confirm).context("sudo pacman -Rns")?;
        }
    }
    stamp_uninstall_phase(dirs, &name, &record, UninstallPhase::Pacman);

    // 2. Disable systemd units.
    if !skip_systemd_phase {
        systemd::disable_units(&record.systemd_units_enabled)?;
    }
    stamp_uninstall_phase(dirs, &name, &record, UninstallPhase::Systemd);

    // Crash-record detection: use the explicit field written by
    // `write_partial_crashed_record`. Previous heuristic (partial + empty
    // diffs) misfired on legitimate "install_cmd exits non-zero before
    // deploying anything" records — those are still normal records the
    // user can uninstall cleanly; no bogus warning needed.
    let crash_record = record.crash_recovery;

    // 3. Reverse fs diff. `rcsave_paths` is passed mutably so a mid-
    //    reversal Err still surfaces the partial list.
    let mut rcsave_paths: Vec<PathBuf> = Vec::new();
    if !skip_fs_diff_phase
        && let Err(e) = reverse_fs_diff(dirs, &name, &record, flags, &mut rcsave_paths)
    {
        if !rcsave_paths.is_empty() {
            eprintln!("uninstall reversed partially before failure; preserved user content at:");
            for p in &rcsave_paths {
                eprintln!("  {}", p.display());
            }
        }
        return Err(e);
    }
    stamp_uninstall_phase(dirs, &name, &record, UninstallPhase::FsDiff);

    // 4. Clean up cache dirs. On crash-records, preserve snapshot_dir so
    //    the user can manually recover — it may hold the only copy of
    //    pre-install content for files the rice modified before crashing.
    let preserved_snapshot_dir = if !skip_cleanup_phase {
        let _ = fs::remove_dir_all(dirs.clone_dir(&name));
        let psd = if crash_record && dirs.snapshot_dir(&name).exists() {
            Some(dirs.snapshot_dir(&name))
        } else {
            let _ = fs::remove_dir_all(dirs.snapshot_dir(&name));
            None
        };
        stamp_uninstall_phase(dirs, &name, &record, UninstallPhase::Cleanup);
        psd
    } else {
        None
    };

    // 5. Retire record.
    retire_to_previous(dirs, &name)?;
    clear_current(dirs)?;

    Ok(UninstallOutcome {
        name,
        rcsave_paths,
        crash_record,
        preserved_snapshot_dir,
    })
}

/// Atomic switch: uninstall the current rice and install <to> under a
/// single lock acquisition so another rice-cooker process can't slip an
/// install/apply between the two halves.
pub fn switch(cat: &Catalog, dirs: &Dirs, to: &str, flags: Flags) -> Result<SwitchOutcome> {
    dirs.ensure()?;
    let _lock = ApplyLock::try_acquire(&dirs.lock_file()).map_err(|e| anyhow!("lock: {e}"))?;
    let uninstall_out = uninstall_locked(dirs, flags)?;
    let install_out = install_locked(cat, dirs, to, flags)?;
    Ok(SwitchOutcome {
        from: uninstall_out.name,
        to: install_out.name,
        rcsave_paths: uninstall_out.rcsave_paths,
        partial: install_out.partial,
    })
}

pub fn list(cat: &Catalog, dirs: &Dirs) -> Result<Vec<ListRow>> {
    let current = read_current(dirs)?;
    let mut rows = Vec::new();
    for (name, entry) in &cat.rices {
        rows.push(ListRow {
            name: name.clone(),
            display_name: entry.display_name.clone(),
            description: entry.description.clone(),
            installed: current.as_deref() == Some(name),
            documented_system_effects: entry.documented_system_effects.clone(),
        });
    }
    Ok(rows)
}

pub fn status(dirs: &Dirs) -> Result<StatusRow> {
    dirs.ensure()?;
    match read_current(dirs)? {
        Some(name) => {
            let record = load_record(&dirs.record_json(&name))?;
            Ok(StatusRow {
                installed: Some(record),
            })
        }
        None => Ok(StatusRow { installed: None }),
    }
}

// ── Internals ──────────────────────────────────────────────────────────────

/// Order: deletes first, modifications, re-creations, symlinks.
/// Fills `rcsave_paths` with `.rcsave-<ts>` paths created during reversal.
/// The out-param pattern preserves information on early Err return — a
/// mid-reversal failure leaves rcsave files on disk, and the caller needs
/// to tell the user about them even if a later op blew up.
fn reverse_fs_diff(
    dirs: &Dirs,
    name: &str,
    record: &InstallRecord,
    flags: Flags,
    rcsave_paths: &mut Vec<PathBuf>,
) -> Result<()> {
    let diff = &record.fs_diff;
    let partial_ownership: HashSet<&PathBuf> = record.partial_ownership_paths.iter().collect();
    let runtime_regen: HashSet<&PathBuf> = record.runtime_regenerated_paths.iter().collect();
    let unrestorable: HashSet<&PathBuf> = record.unrestorable_paths.iter().collect();

    // Content backup dir (for modifications + deletions we could restore).
    let content_dir = dirs.snapshot_dir(name).join("content");

    // 3a. Added files: remove iff current hash matches our recorded
    //     post-install hash. Else .rcsave to preserve user edits.
    //     Catalog policies still apply even though pre didn't exist:
    //     runtime_regenerated → unconditional remove (no .rcsave;
    //       the path is expected to drift at runtime and user isn't
    //       authoring it).
    //     partial_ownership    → always .rcsave (user is the co-owner;
    //       their edits are sacred even if hash happens to match).
    for a in &diff.added {
        if !a.path.exists() {
            continue;
        }
        if runtime_regen.contains(&a.path) {
            fs::remove_file(&a.path).with_context(|| format!("removing {}", a.path.display()))?;
            log_verbose(
                flags,
                &format!("added→removed (runtime_regen) {}", a.path.display()),
            );
            continue;
        }
        if partial_ownership.contains(&a.path) {
            let dest = rcsave_path(&a.path);
            fs::rename(&a.path, &dest)
                .with_context(|| format!("rcsave {} -> {}", a.path.display(), dest.display()))?;
            rcsave_paths.push(dest);
            log_verbose(
                flags,
                &format!("added→rcsave (partial_ownership) {}", a.path.display()),
            );
            continue;
        }
        let current_hash = match snapshot::hash_file(&a.path) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("skipping {}: {e}", a.path.display());
                continue;
            }
        };
        if current_hash == a.hash {
            fs::remove_file(&a.path).with_context(|| format!("removing {}", a.path.display()))?;
        } else {
            let dest = rcsave_path(&a.path);
            fs::rename(&a.path, &dest)
                .with_context(|| format!("rcsave {} -> {}", a.path.display(), dest.display()))?;
            rcsave_paths.push(dest);
        }
        log_verbose(flags, &format!("added→removed {}", a.path.display()));
    }

    // 3b. Modified files: per-path policy.
    for m in &diff.modified {
        let backup = content_dir.join(path_key(&m.path));
        let is_partial = partial_ownership.contains(&m.path);
        let is_runtime = runtime_regen.contains(&m.path);
        if unrestorable.contains(&m.path) {
            // We couldn't trust the pre-content backup (race or copy
            // failure at install time). Leave the file alone; user is
            // told what we skipped.
            eprintln!(
                "skipping restore of {}: pre-install content wasn't trustworthy",
                m.path.display()
            );
            continue;
        }
        // Idempotent short-circuit: if the current content already matches
        // pre-install (this can happen on a retry of a partially-completed
        // uninstall, where we already restored this path on the first try),
        // skip both the .rcsave and the restore. Without this, a retry
        // would .rcsave the correctly-restored pre-install content as if
        // it were user-modified.
        if let Ok(current) = snapshot::hash_file(&m.path)
            && current == m.pre_hash
        {
            continue;
        }

        if is_runtime {
            // Runtime-regenerated: restore pre-install content if we have
            // a backup; otherwise just delete the post-install file (it's
            // expected to regen).
            if backup.exists() {
                copy_file(&backup, &m.path, m.pre_mode)?;
            } else if m.path.exists() {
                let _ = fs::remove_file(&m.path);
            }
            continue;
        }
        if is_partial {
            // Always .rcsave, always restore (if we have a backup).
            if m.path.exists() {
                let dest = rcsave_path(&m.path);
                fs::copy(&m.path, &dest)
                    .with_context(|| format!("rcsave copy {}", m.path.display()))?;
                rcsave_paths.push(dest);
            }
            if backup.exists() {
                copy_file(&backup, &m.path, m.pre_mode)?;
            }
            continue;
        }
        // Default: hash-compare current to post.
        let current_hash = snapshot::hash_file(&m.path).unwrap_or_default();
        if current_hash == m.post_hash {
            // Unchanged since install — restore pre content (if available).
            if backup.exists() {
                copy_file(&backup, &m.path, m.pre_mode)?;
            }
        } else {
            // User modified → .rcsave the current, then restore pre.
            if m.path.exists() {
                let dest = rcsave_path(&m.path);
                fs::rename(&m.path, &dest)
                    .with_context(|| format!("rcsave rename {}", m.path.display()))?;
                rcsave_paths.push(dest);
            }
            if backup.exists() {
                copy_file(&backup, &m.path, m.pre_mode)?;
            }
        }
    }

    // 3c. Deleted files: restore pre content from backup.
    for d in &diff.deleted {
        if unrestorable.contains(&d.path) {
            eprintln!(
                "skipping restore of {}: pre-install content wasn't trustworthy",
                d.path.display()
            );
            continue;
        }
        let backup = content_dir.join(path_key(&d.path));
        if backup.exists() {
            copy_file(&backup, &d.path, d.pre_mode)?;
        } else {
            eprintln!(
                "no pre-install backup for deleted file {} — cannot restore",
                d.path.display()
            );
        }
    }

    // 3d. Added symlinks: unlink iff target matches.
    for s in &diff.symlinks_added {
        let meta = match fs::symlink_metadata(&s.path) {
            Ok(m) => m,
            Err(_) => continue, // already gone
        };
        if !meta.file_type().is_symlink() {
            continue;
        }
        let actual = fs::read_link(&s.path).ok();
        if actual.as_deref() == Some(s.target.as_path()) {
            fs::remove_file(&s.path).with_context(|| format!("unlinking {}", s.path.display()))?;
        } else {
            eprintln!(
                "symlink {} now points at {:?}, leaving in place",
                s.path.display(),
                actual
            );
        }
    }

    // 3e. Dirs added: rmdir iff empty (best effort, leaf-first).
    let mut dirs_added: Vec<&PathBuf> = diff.dirs_added.iter().map(|d| &d.path).collect();
    dirs_added.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for d in dirs_added {
        let _ = fs::remove_dir(d); // silently ignore non-empty
    }

    Ok(())
}

fn rcsave_path(original: &Path) -> PathBuf {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut name = original.as_os_str().to_os_string();
    name.push(format!(".rcsave-{ts}"));
    PathBuf::from(name)
}

fn copy_file(src: &Path, dest: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::copy(src, dest).with_context(|| format!("copy {} -> {}", src.display(), dest.display()))?;
    let mut perms = fs::metadata(dest)
        .with_context(|| format!("stat {} to set mode", dest.display()))?
        .permissions();
    perms.set_mode(mode);
    fs::set_permissions(dest, perms)
        .with_context(|| format!("chmod {} to {:o}", dest.display(), mode))?;
    Ok(())
}

/// Best-effort record write when the install pipeline crashed AFTER
/// install_cmd succeeded. The record carries an empty fs_diff (we don't
/// know what changed) but points at the log, so the user has something
/// to read and at least a `current.json` entry pointing at this rice.
/// Errors writing the record are eprintln'd — we're already on the error
/// path and the user is better off with a best-effort write than a
/// silent orphan.
fn write_partial_crashed_record(
    dirs: &Dirs,
    name: &str,
    entry: &RiceEntry,
    exit_code: i32,
    log_path: &Path,
    error: &str,
) {
    let record = InstallRecord {
        schema_version: SCHEMA_VERSION,
        name: name.to_string(),
        commit: entry.commit.clone(),
        shape: entry.shape,
        symlink_path: None,
        symlink_target: None,
        catalog_entry_hash: hash_catalog_entry(entry),
        installed_at: InstallRecord::now_rfc3339(),
        install_cmd: entry.install_cmd.clone(),
        exit_code,
        partial: true,
        fs_diff: FsDiff::default(),
        pacman_diff: PacmanDiff::default(),
        // Populate from the catalog entry for forensic value — even
        // though we don't have per-install state, knowing which paths
        // the rice *declared* as partial / runtime helps the user
        // diagnose what might be out in $HOME.
        partial_ownership_paths: entry
            .partial_ownership
            .iter()
            .map(|s| expand_home(s, &dirs.home))
            .collect(),
        runtime_regenerated_paths: entry
            .runtime_regenerated
            .iter()
            .map(|s| expand_home(s, &dirs.home))
            .collect(),
        systemd_units_enabled: vec![],
        unrestorable_paths: vec![],
        crash_recovery: true,
        uninstall_phase: None,
        log_path: log_path.to_path_buf(),
    };
    if let Err(e) = save_record(&dirs.record_json(name), &record) {
        eprintln!("failed to save crash-record for {name}: {e:#} (original error: {error})");
    }
    if let Err(e) = write_current(dirs, name) {
        eprintln!("failed to write current.json for {name}: {e:#}");
    }
}

fn hash_catalog_entry(entry: &RiceEntry) -> String {
    let s = serde_json::to_string(entry).unwrap_or_default();
    blake3::hash(s.as_bytes()).to_hex().to_string()
}

fn diff_op_count(d: &FsDiff) -> usize {
    d.added.len() + d.modified.len() + d.deleted.len() + d.symlinks_added.len()
}

fn log_verbose(flags: Flags, msg: &str) {
    if flags.verbose {
        eprintln!("rice-cooker: {msg}");
    }
}

// ── Outcome structs ────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct InstallOutcome {
    pub name: String,
    pub partial: bool,
    pub fs_diff: FsDiff,
    pub pacman_diff: PacmanDiff,
    pub log_path: PathBuf,
}

#[derive(Debug)]
pub struct UninstallOutcome {
    pub name: String,
    pub rcsave_paths: Vec<PathBuf>,
    /// Set when the record being uninstalled looked like a crash-record
    /// (partial + empty fs_diff). Caller should warn the user loudly —
    /// HOME may still have files the rice deployed; rice-cooker has no
    /// record of them. The `preserved_snapshot_dir` below points at the
    /// manual-recovery content if we kept it.
    pub crash_record: bool,
    /// When `crash_record` is true, this is the preserved snapshot dir
    /// where pre-install content backups (if any made it to disk) still
    /// live, so the user can manually recover. None when no recovery
    /// data exists.
    pub preserved_snapshot_dir: Option<PathBuf>,
}

#[derive(Debug)]
pub struct SwitchOutcome {
    pub from: String,
    pub to: String,
    pub rcsave_paths: Vec<PathBuf>,
    pub partial: bool,
}

#[derive(Debug)]
pub struct ListRow {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub installed: bool,
    pub documented_system_effects: Vec<String>,
}

#[derive(Debug)]
pub struct StatusRow {
    pub installed: Option<InstallRecord>,
}

/// Back up every file in the pre-snapshot so uninstall can restore any
/// `modified` or `deleted` entry. Uses `fs::copy` for independent inodes —
/// we can't use hardlinks because `> file` shell redirection (the most
/// common install-script write pattern) truncates the shared inode,
/// corrupting the backup along with the target.
///
/// Re-hashes each copy and compares against the pre-snapshot's recorded
/// hash to catch TOCTOU races (a daemon writing to the file between
/// take_snapshot and fs::copy). Mismatches → drop the backup and record
/// the path as unrestorable so uninstall skips it instead of restoring
/// wrong content.
///
/// Returns the set of paths whose backup couldn't be trusted.
pub fn capture_pre_content(
    snap_dir: &Path,
    pre: &Manifest,
) -> Result<std::collections::HashSet<PathBuf>> {
    use std::collections::HashSet;
    let content_dir = snap_dir.join("content");
    fs::create_dir_all(&content_dir)
        .with_context(|| format!("creating {}", content_dir.display()))?;
    let mut unrestorable: HashSet<PathBuf> = HashSet::new();
    for (path, entry) in &pre.entries {
        let snapshot::Entry::File { hash: pre_hash, .. } = entry else {
            continue;
        };
        let dest = content_dir.join(path_key(path));
        if dest.exists() {
            continue;
        }
        if let Err(e) = fs::copy(path, &dest) {
            eprintln!(
                "pre-content backup skip {}: copy failed: {e}",
                path.display()
            );
            unrestorable.insert(path.clone());
            continue;
        }
        // Verify the backup matches the pre-snapshot hash.
        let backup_hash = match snapshot::hash_file(&dest) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("pre-content verify {}: {e}", path.display());
                unrestorable.insert(path.clone());
                let _ = fs::remove_file(&dest);
                continue;
            }
        };
        if &backup_hash != pre_hash {
            eprintln!(
                "pre-content race on {}: pre-snapshot hash {pre_hash} but backup hash {backup_hash}; skipping restore",
                path.display()
            );
            unrestorable.insert(path.clone());
            let _ = fs::remove_file(&dest);
        }
    }
    Ok(unrestorable)
}

/// Outcome of a `rice-cooker cleanup` invocation.
#[derive(Debug, Clone, PartialEq)]
pub struct CleanupOutcome {
    pub name: String,
    pub shape: Shape,
    /// Files restored from the pre-install content backup (dotfiles shape).
    pub restored: usize,
    /// Files deleted from the watched paths because they were added by
    /// install.sh before the crash (dotfiles shape).
    pub deleted_added: usize,
    /// Whether the dangling symlink was removed (symlink shape).
    pub removed_symlink: bool,
}

/// Reset state after a crashed install. Precondition: `.in-progress.json`
/// exists. Restores pre-install filesystem state (dotfiles) or removes
/// the dangling symlink (symlink), then deletes install artifacts.
///
/// See SPEC.md `cleanup` section for full step-by-step. Refuses if:
/// - the global lock is held
/// - `.in-progress.json` is absent (use `uninstall --force` for a failed
///   completed install)
pub fn cleanup(cat: &Catalog, dirs: &Dirs) -> Result<CleanupOutcome> {
    dirs.ensure()?;
    let _lock = ApplyLock::try_acquire(&dirs.lock_file()).map_err(|e| anyhow!("lock: {e}"))?;
    cleanup_locked(cat, dirs)
}

fn cleanup_locked(cat: &Catalog, dirs: &Dirs) -> Result<CleanupOutcome> {
    let marker = read_in_progress(dirs)?.ok_or_else(|| {
        anyhow!("nothing to clean up; for a failed completed install, use `uninstall --force`")
    })?;
    let name = marker.name.clone();
    let shape = marker.shape;

    let mut restored = 0usize;
    let mut deleted_added = 0usize;
    let mut removed_symlink = false;

    match shape {
        Shape::Dotfiles => {
            // Restore pre-install filesystem state from the content backup.
            let snap_dir = dirs.snapshot_dir(&name);
            let manifest_path = snap_dir.join("manifest.json");
            if manifest_path.exists() {
                let manifest = snapshot::load_manifest(&manifest_path)?;
                let content_dir = snap_dir.join("content");
                let (r, d) = cleanup_dotfiles_restore(&manifest, &content_dir, &dirs.home)?;
                restored = r;
                deleted_added = d;
            }
        }
        Shape::Symlink => {
            // Remove dangling symlink (install may have succeeded
            // create_symlink but crashed before record write).
            if let Some(entry) = cat.get(&name) {
                let dst = expand_home(&entry.symlink_dst, &dirs.home);
                if fs::symlink_metadata(&dst).is_ok() {
                    match fs::remove_file(&dst) {
                        Ok(()) => removed_symlink = true,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => {
                            return Err(anyhow!("rm {}: {e}", dst.display()));
                        }
                    }
                }
            }
            // If the catalog no longer has this entry (rice removed mid-
            // crash), we can't know symlink_dst. Best-effort: continue
            // cleanup of clone + marker so the user isn't stuck.
        }
    }

    // Common cleanup: snapshot dir, clone dir, AUR clones, marker.
    let _ = fs::remove_dir_all(dirs.snapshot_dir(&name));
    let _ = fs::remove_dir_all(dirs.clone_dir(&name));
    // AUR clones are ephemeral per-install; wipe them if we know what to
    // wipe. The catalog is consulted rather than the marker (which only
    // carries name/shape) so this is best-effort.
    // Deferred: a dedicated `aur_deps` field on the marker would make
    // this robust to catalog changes between install-start and cleanup.
    clear_in_progress(dirs)?;

    Ok(CleanupOutcome {
        name,
        shape,
        restored,
        deleted_added,
        removed_symlink,
    })
}

/// Dotfiles-cleanup restoration: walk each tracked dir, delete entries
/// install.sh added that aren't in the manifest, then restore each
/// manifest entry from content backup. Order matters — delete-additions
/// before restore-manifest so we don't immediately re-delete what we
/// just restored.
fn cleanup_dotfiles_restore(
    manifest: &Manifest,
    content_dir: &Path,
    home: &Path,
) -> Result<(usize, usize)> {
    use std::collections::HashSet;
    let known: HashSet<&PathBuf> = manifest.entries.keys().collect();
    let mut deleted_added = 0usize;
    let mut restored = 0usize;

    // (a) Walk each watched root; delete anything not in the manifest.
    for root in &manifest.roots {
        if !root.exists() {
            continue;
        }
        let mut stack: Vec<PathBuf> = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let entries = match fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for de in entries.flatten() {
                let path = de.path();
                let ft = match de.file_type() {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                if known.contains(&path) {
                    // Keep — will be handled by the restore pass.
                    if ft.is_dir() {
                        stack.push(path);
                    }
                    continue;
                }
                // Install-sh-added entry. Delete it.
                let res = if ft.is_dir() {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_file(&path)
                };
                if res.is_ok() {
                    deleted_added += 1;
                }
            }
        }
    }

    // (b) Restore each manifest entry from content backup.
    // Sort by depth so dirs are created before files inside them.
    let mut ordered: Vec<(&PathBuf, &snapshot::Entry)> = manifest.entries.iter().collect();
    ordered.sort_by_key(|(p, _)| p.components().count());
    let _ = home; // home not needed — manifest paths are absolute.

    for (path, entry) in ordered {
        // Handle type mismatches: if the current path is the wrong type,
        // remove it before restoring. Symlink/file can `rm -f`; a dir
        // where a file belongs needs `rm -rf`.
        if let Ok(md) = fs::symlink_metadata(path) {
            let cur_is_dir = md.file_type().is_dir();
            let want_is_dir = matches!(entry, snapshot::Entry::Dir { .. });
            if cur_is_dir != want_is_dir {
                let _ = if cur_is_dir {
                    fs::remove_dir_all(path)
                } else {
                    fs::remove_file(path)
                };
            }
        }
        match entry {
            snapshot::Entry::File { mode, .. } => {
                let src = content_dir.join(path_key(path));
                if !src.exists() {
                    // Backup missing (e.g., unrestorable at install) —
                    // skip silently.
                    continue;
                }
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if fs::copy(&src, path).is_ok() {
                    set_mode(path, *mode);
                    restored += 1;
                }
            }
            snapshot::Entry::Symlink { target, .. } => {
                let _ = fs::remove_file(path);
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if std::os::unix::fs::symlink(target, path).is_ok() {
                    restored += 1;
                }
            }
            snapshot::Entry::Dir { mode } => {
                if fs::create_dir_all(path).is_ok() {
                    set_mode(path, *mode);
                    restored += 1;
                }
            }
        }
    }
    Ok((restored, deleted_added))
}

fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
}

/// Stamp the record with the uninstall phase that just completed, so
/// retry after crash can skip past it. Non-fatal if the save fails —
/// next retry simply re-does the phase, which is idempotent.
fn stamp_uninstall_phase(dirs: &Dirs, name: &str, record: &InstallRecord, phase: UninstallPhase) {
    let mut r = record.clone();
    r.uninstall_phase = Some(phase);
    let _ = save_record(&dirs.record_json(name), &r);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{RiceEntry, Shape, ShellType};

    fn mk_entry() -> RiceEntry {
        RiceEntry {
            display_name: "X".into(),
            description: "".into(),
            repo: "https://example/x".into(),
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            shape: Shape::Dotfiles,
            install_cmd: "true".into(),
            symlink_src: String::new(),
            symlink_dst: String::new(),
            interactive: false,
            shell_type: ShellType::None,
            runtime_regenerated: vec![],
            partial_ownership: vec![],
            extra_watched_roots: vec![],
            documented_system_effects: vec![],
        }
    }

    #[test]
    fn rcsave_path_appends_timestamp_suffix() {
        let p = rcsave_path(Path::new("/h/x.conf"));
        let s = p.display().to_string();
        assert!(s.starts_with("/h/x.conf.rcsave-"), "got {s}");
    }

    #[test]
    fn diff_op_count_sums() {
        let mut d = FsDiff::default();
        d.added.push(crate::install::diff::AddedFile {
            path: PathBuf::new(),
            hash: "".into(),
            size: 0,
            mode: 0,
        });
        d.deleted.push(crate::install::diff::DeletedFile {
            path: PathBuf::new(),
            pre_hash: "".into(),
            pre_size: 0,
            pre_mode: 0,
        });
        assert_eq!(diff_op_count(&d), 2);
    }

    #[test]
    fn catalog_entry_hash_stable_across_calls() {
        let e = mk_entry();
        assert_eq!(hash_catalog_entry(&e), hash_catalog_entry(&e));
    }
}
