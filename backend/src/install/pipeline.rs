//! Glue: install / uninstall / switch / list / status pipelines.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::catalog::{Catalog, RiceEntry};
use crate::git;
use crate::lock::ApplyLock;

use super::diff::{self, FsDiff};
use super::env::{Dirs, expand_home};
use super::pacman::{self, ExplicitSet};
use super::record::{
    InstallRecord, PacmanDiff, SCHEMA_VERSION, clear_current, load_record, read_current,
    retire_to_previous, save_record, write_current,
};
use super::run;
use super::snapshot::{self, Manifest, WalkOpts, path_key};
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
pub fn install(
    cat: &Catalog,
    dirs: &Dirs,
    name: &str,
    flags: Flags,
) -> Result<InstallOutcome> {
    dirs.ensure()?;
    let _lock = ApplyLock::try_acquire(&dirs.lock_file())
        .map_err(|e| anyhow!("lock: {e}"))?;
    install_locked(cat, dirs, name, flags)
}

/// Install pipeline WITHOUT acquiring the lock. Caller must already hold
/// it. Exists so `switch` can hold the lock across uninstall+install.
/// Cleans up the per-rice `snapshot_dir` on error so a retry starts fresh
/// instead of re-using stale pre-content backups keyed against a different
/// pre-snapshot.
fn install_locked(
    cat: &Catalog,
    dirs: &Dirs,
    name: &str,
    flags: Flags,
) -> Result<InstallOutcome> {
    let result = install_locked_inner(cat, dirs, name, flags);
    if result.is_err() {
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

    // Clone / re-clone.
    let clone = dirs.clone_dir(name);
    if clone.exists() {
        fs::remove_dir_all(&clone)
            .with_context(|| format!("removing stale clone {}", clone.display()))?;
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
    let snap_dir = dirs.snapshot_dir(name);
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

    // Post-snapshot.
    log_verbose(flags, "post-snapshot");
    let post = snapshot::take_snapshot(&walk_opts)?;
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
        log_path: log_path.clone(),
    };
    save_record(&dirs.record_json(name), &record)?;
    write_current(dirs, name)?;

    Ok(InstallOutcome {
        name: name.to_string(),
        partial,
        fs_diff: diff,
        pacman_diff,
        log_path,
    })
}

/// Uninstall the currently-installed rice. Acquires the lock.
pub fn uninstall(
    dirs: &Dirs,
    flags: Flags,
) -> Result<UninstallOutcome> {
    dirs.ensure()?;
    let _lock = ApplyLock::try_acquire(&dirs.lock_file())
        .map_err(|e| anyhow!("lock: {e}"))?;
    uninstall_locked(dirs, flags)
}

/// Uninstall WITHOUT acquiring the lock. Caller must already hold it.
fn uninstall_locked(dirs: &Dirs, flags: Flags) -> Result<UninstallOutcome> {

    let name = read_current(dirs)?
        .ok_or_else(|| anyhow!("no rice installed"))?;
    let record = load_record(&dirs.record_json(&name))?;

    if record.partial && !flags.force {
        return Err(anyhow!(
            "{name} was installed partially (install script exit {}); re-run with --force to proceed",
            record.exit_code
        ));
    }

    if flags.dry_run {
        println!("would reverse {} operations", diff_op_count(&record.fs_diff));
        return Ok(UninstallOutcome {
            name,
            rcsave_paths: vec![],
        });
    }

    // 1. Reverse pacman.
    if !flags.skip_pacman && !record.pacman_diff.added_explicit.is_empty() {
        pacman::remove_added(&record.pacman_diff.added_explicit, flags.no_confirm)
            .context("sudo pacman -Rns")?;
    }

    // 2. Disable systemd units.
    systemd::disable_units(&record.systemd_units_enabled)?;

    // 3. Reverse fs diff. `rcsave_paths` is passed mutably so it's
    //    populated in place — a mid-reversal Err still returns the
    //    partial list on the error path via `.map_err` below.
    let mut rcsave_paths: Vec<PathBuf> = Vec::new();
    if let Err(e) = reverse_fs_diff(dirs, &name, &record, flags, &mut rcsave_paths) {
        if !rcsave_paths.is_empty() {
            eprintln!(
                "uninstall reversed partially before failure; preserved user content at:"
            );
            for p in &rcsave_paths {
                eprintln!("  {}", p.display());
            }
        }
        return Err(e);
    }

    // 4. Clean up cache dirs for this rice.
    let _ = fs::remove_dir_all(dirs.clone_dir(&name));
    let _ = fs::remove_dir_all(dirs.snapshot_dir(&name));

    // 5. Retire record.
    retire_to_previous(dirs, &name)?;
    clear_current(dirs)?;

    Ok(UninstallOutcome { name, rcsave_paths })
}

/// Atomic switch: uninstall the current rice and install <to> under a
/// single lock acquisition so another rice-cooker process can't slip an
/// install/apply between the two halves.
pub fn switch(
    cat: &Catalog,
    dirs: &Dirs,
    to: &str,
    flags: Flags,
) -> Result<SwitchOutcome> {
    dirs.ensure()?;
    let _lock = ApplyLock::try_acquire(&dirs.lock_file())
        .map_err(|e| anyhow!("lock: {e}"))?;
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
            fs::remove_file(&a.path)
                .with_context(|| format!("removing {}", a.path.display()))?;
            log_verbose(flags, &format!("added→removed (runtime_regen) {}", a.path.display()));
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
            fs::remove_file(&a.path)
                .with_context(|| format!("removing {}", a.path.display()))?;
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
            fs::remove_file(&s.path)
                .with_context(|| format!("unlinking {}", s.path.display()))?;
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
        fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::copy(src, dest)
        .with_context(|| format!("copy {} -> {}", src.display(), dest.display()))?;
    let mut perms = fs::metadata(dest)
        .with_context(|| format!("stat {} to set mode", dest.display()))?
        .permissions();
    perms.set_mode(mode);
    fs::set_permissions(dest, perms)
        .with_context(|| format!("chmod {} to {:o}", dest.display(), mode))?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{RiceEntry, ShellType};

    fn mk_entry() -> RiceEntry {
        RiceEntry {
            display_name: "X".into(),
            description: "".into(),
            repo: "https://example/x".into(),
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            install_cmd: "true".into(),
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
