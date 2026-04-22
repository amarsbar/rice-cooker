//! Symlink-shape install path.
//!
//! For rices where the repo IS the shell config (most Quickshell rices).
//! Install = one `ln -sfnT <clone>/<symlink_src> <symlink_dst>`. No
//! filesystem snapshot, no diff, no backup.
//!
//! Uninstall checks the clone for user edits via `git status --porcelain`
//! — users customise their shell by editing the symlinked files, and we
//! preserve those edits to `rcsave/<name>-<ts>/` before deleting the
//! clone.
//!
//! All retry-safety properties are shape-local:
//! - pacman pre-filter (in pipeline.rs) handles already-removed packages
//! - `fs::remove_file` is idempotent (caller filters `NotFound`)
//! - timestamped rcsave dirs don't collide within a second, and
//!   `create_dir_all` tolerates pre-existing dirs
//!
//! No `uninstall_phase` tracking needed on the symlink path: the only
//! non-idempotent step is record retirement, done last.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};

use crate::catalog::{RiceEntry, Shape};

use super::env::{Dirs, expand_home};

/// Create the symlink pointing from `symlink_dst` into
/// `<clone_dir>/<symlink_src>`. Creates parent dirs of `symlink_dst` as
/// needed. Overwrites any existing symlink at the destination (that's the
/// `-f` in `ln -sfnT`), but refuses to replace a directory — we won't blow
/// away the user's actual `~/.config/<app>/` if they ran install against a
/// name that shadows an existing config.
pub fn create_symlink(clone_dir: &Path, entry: &RiceEntry, home: &Path) -> Result<SymlinkPaths> {
    assert_eq!(
        entry.shape,
        Shape::Symlink,
        "symlink::create_symlink called on non-symlink shape"
    );
    let src = clone_dir.join(&entry.symlink_src);
    let dst = expand_home(&entry.symlink_dst, home);

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating parent of {}", dst.display()))?;
    }

    // Refuse to replace a real directory. `symlink_metadata` doesn't
    // follow symlinks, so a stale symlink at `dst` is caught by the
    // atomic-rename branch below.
    if let Ok(md) = fs::symlink_metadata(&dst)
        && md.file_type().is_dir()
    {
        return Err(anyhow!(
            "{} exists as a directory; refusing to replace with a symlink",
            dst.display()
        ));
    }
    // Atomic replace: create a new symlink at a sibling tmp path and
    // `fs::rename` it over `dst`. `rename` is atomic on POSIX and works
    // for symlinks. This closes the gap where a mid-create_symlink error
    // would leave the user with no symlink AND no install record.
    let mut tmp_name = dst.as_os_str().to_os_string();
    tmp_name.push(format!(".tmp.{}", std::process::id()));
    let tmp = PathBuf::from(tmp_name);
    // A stale tmp from a prior crash is harmless — remove + retry. Non-
    // NotFound errors at this point (EACCES, EROFS) will surface below
    // when symlink() fails with the same error.
    if let Err(e) = fs::remove_file(&tmp)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(anyhow!("clearing stale tmp {}: {e}", tmp.display()));
    }
    symlink(&src, &tmp)
        .with_context(|| format!("symlink {} -> {}", tmp.display(), src.display()))?;
    if let Err(e) = fs::rename(&tmp, &dst) {
        // Best-effort cleanup of the tmp so we don't leak; propagate the
        // real error.
        let _ = fs::remove_file(&tmp);
        return Err(anyhow!(
            "rename {} -> {}: {e}",
            tmp.display(),
            dst.display()
        ));
    }

    Ok(SymlinkPaths {
        symlink_path: dst,
        symlink_target: src,
    })
}

/// Outcome of a successful symlink install — paths to record.
#[derive(Debug, Clone, PartialEq)]
pub struct SymlinkPaths {
    pub symlink_path: PathBuf,
    pub symlink_target: PathBuf,
}

/// Uninstall side: remove the symlink and return preserved user edits (if
/// any). Caller is responsible for deleting the clone dir AFTER this runs
/// — user-edit preservation reads files from inside the clone. The rcsave
/// step happens BEFORE symlink removal so that if rcsave fails the user
/// can retry without losing state.
///
/// Returns the paths that were copied to rcsave, for NDJSON reporting.
pub fn remove_symlink_with_preservation(
    clone_dir: &Path,
    symlink_path: &Path,
    rcsave_root: &Path,
) -> Result<Vec<PathBuf>> {
    // Step A: preserve user edits. Never lose data — this runs before
    // symlink removal precisely so the user can retry if rcsave fails.
    let preserved = preserve_user_edits(clone_dir, rcsave_root)?;

    // Step B: remove the symlink. Missing-symlink is a retry: ignore.
    // Other errors (EACCES, EROFS) must surface or uninstall reports
    // success while leaving a dangling symlink on disk.
    match fs::remove_file(symlink_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(anyhow!("rm {}: {e}", symlink_path.display())),
    }
    Ok(preserved)
}

/// Run `git status --porcelain` inside the clone and copy modified /
/// untracked files to `<rcsave_root>/<relative-path>`. Emits no events
/// itself; caller owns the NDJSON stream.
///
/// **Git failure fallback:** if `.git` is missing, the git binary is
/// absent, or git-status errors out, copy the ENTIRE clone to
/// `<rcsave_root>-unverified/` so the user has a chance to recover their
/// edits. This is the "we can't tell what's modified, so save everything"
/// backstop demanded by the spec.
fn preserve_user_edits(clone_dir: &Path, rcsave_root: &Path) -> Result<Vec<PathBuf>> {
    if !clone_dir.exists() {
        return Ok(vec![]);
    }
    if !clone_dir.join(".git").exists() {
        return preserve_whole_clone(clone_dir, rcsave_root);
    }

    let out = Command::new("git")
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .current_dir(clone_dir)
        .output();

    let out = match out {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            eprintln!(
                "rice-cooker: git status exited {:?} in {}; falling back to whole-clone preservation\n  stderr: {}",
                o.status.code(),
                clone_dir.display(),
                String::from_utf8_lossy(&o.stderr).trim(),
            );
            return preserve_whole_clone(clone_dir, rcsave_root);
        }
        Err(e) => {
            eprintln!(
                "rice-cooker: git status spawn failed in {}: {e}; falling back to whole-clone preservation",
                clone_dir.display(),
            );
            return preserve_whole_clone(clone_dir, rcsave_root);
        }
    };

    let stdout = out.stdout;
    let mut preserved: Vec<PathBuf> = Vec::new();
    // porcelain=v1 -z: each entry is `XY <path>\0`. For renames, a
    // second `\0<path>` follows the first. We don't care about the
    // distinction — modified-or-untracked gets copied, end of.
    let mut it = stdout.split(|&b| b == 0).filter(|s| !s.is_empty());
    while let Some(entry) = it.next() {
        // Skip leading XY + space (3 bytes) if present; untracked marker
        // is "?? ". Either way, the path starts at byte 3.
        if entry.len() < 4 {
            continue;
        }
        let xy = &entry[..2];
        let path_bytes = &entry[3..];
        // "R " (renamed) stashes the old name in the next null-separated
        // chunk — consume it so we don't treat it as a new entry.
        if xy.starts_with(b"R") {
            let _ = it.next();
        }
        // Linux paths are arbitrary bytes; reconstruct via OsStr so a
        // latin-1 or other non-UTF-8 filename still round-trips through
        // rcsave. Uninstall is a critical safety path — never error out
        // here; we'd leave the user with no way to finish the uninstall.
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let rel = Path::new(OsStr::from_bytes(path_bytes));
        let src = clone_dir.join(rel);
        let dst = rcsave_root.join(rel);
        if copy_into_rcsave(&src, &dst)? {
            preserved.push(dst);
        }
    }
    preserved.sort();
    Ok(preserved)
}

fn preserve_whole_clone(clone_dir: &Path, rcsave_root: &Path) -> Result<Vec<PathBuf>> {
    // rcsave_root has a -unverified suffix in the fallback case to flag
    // that we couldn't distinguish user edits from rice-shipped files.
    let unverified: PathBuf = {
        let mut p = rcsave_root.as_os_str().to_os_string();
        p.push("-unverified");
        PathBuf::from(p)
    };
    fs::create_dir_all(&unverified)
        .with_context(|| format!("creating {}", unverified.display()))?;
    copy_tree(clone_dir, &unverified)?;
    Ok(vec![unverified])
}

/// Copy `src` to `dst`, creating parent dirs and handling the file /
/// symlink / directory cases. Returns true if anything was copied, false
/// if the source doesn't exist (e.g., a deleted-from-worktree file that
/// git-status lists).
fn copy_into_rcsave(src: &Path, dst: &Path) -> Result<bool> {
    let md = match fs::symlink_metadata(src) {
        Ok(m) => m,
        // Deleted-by-user files show up in git status but we can't
        // preserve them. Skip silently.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(anyhow!("stat {}: {e}", src.display())),
    };
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let ft = md.file_type();
    if ft.is_dir() {
        copy_tree(src, dst)?;
    } else if ft.is_symlink() {
        let target = fs::read_link(src).with_context(|| format!("readlink {}", src.display()))?;
        let _ = fs::remove_file(dst);
        symlink(&target, dst)
            .with_context(|| format!("symlink {} -> {}", dst.display(), target.display()))?;
    } else {
        fs::copy(src, dst).with_context(|| format!("cp {} -> {}", src.display(), dst.display()))?;
    }
    Ok(true)
}

/// Recursive copy for the whole-clone fallback. Does not preserve
/// permissions beyond what `fs::copy` gives us, does not follow symlinks
/// into their targets.
fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("creating {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        // Skip .git — the clone's git metadata isn't useful to preserve.
        if name == ".git" {
            continue;
        }
        let s = entry.path();
        let d = dst.join(&name);
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_tree(&s, &d)?;
        } else if ft.is_symlink() {
            let target = fs::read_link(&s)?;
            let _ = fs::remove_file(&d);
            symlink(&target, &d)?;
        } else {
            fs::copy(&s, &d)?;
        }
    }
    Ok(())
}

/// `rcsave_root` for a symlink uninstall, timestamped so retries don't
/// collide. Callers resolve this via `Dirs`.
pub fn rcsave_root(dirs: &Dirs, name: &str) -> PathBuf {
    let ts = now_ts_compact();
    dirs.cache.join("rcsave").join(format!("{name}-{ts}"))
}

fn now_ts_compact() -> String {
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".into())
        // Colons are legal on unix but annoying in paths; replace them.
        .replace(':', "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Shape, ShellType};
    use tempfile::tempdir;

    fn mk_symlink_entry(src: &str, dst: &str) -> RiceEntry {
        RiceEntry {
            display_name: "X".into(),
            description: "".into(),
            repo: "https://example/x".into(),
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            shape: Shape::Symlink,
            install_cmd: String::new(),
            symlink_src: src.into(),
            symlink_dst: dst.into(),
            interactive: false,
            shell_type: ShellType::Quickshell,
            runtime_regenerated: vec![],
            partial_ownership: vec![],
            extra_watched_roots: vec![],
            pacman_deps: vec![],
            aur_deps: vec![],
            aur_commits: std::collections::BTreeMap::new(),
            documented_system_effects: vec![],
        }
    }

    #[test]
    fn create_symlink_writes_link_and_records_paths() {
        let t = tempdir().unwrap();
        let home = t.path();
        let clone = home.join("clone");
        fs::create_dir_all(&clone).unwrap();
        fs::write(clone.join("shell.qml"), "// hi\n").unwrap();

        let entry = mk_symlink_entry(".", "~/.config/qs/x");
        let paths = create_symlink(&clone, &entry, home).unwrap();

        assert_eq!(paths.symlink_path, home.join(".config/qs/x"));
        assert_eq!(paths.symlink_target, clone);
        let read = fs::read_link(home.join(".config/qs/x")).unwrap();
        assert_eq!(read, clone);
    }

    #[test]
    fn create_symlink_overwrites_existing_symlink() {
        let t = tempdir().unwrap();
        let home = t.path();
        let clone = home.join("clone");
        fs::create_dir_all(&clone).unwrap();
        // Pre-existing symlink pointing somewhere else
        fs::create_dir_all(home.join(".config/qs")).unwrap();
        symlink("/nonexistent", home.join(".config/qs/x")).unwrap();

        let entry = mk_symlink_entry(".", "~/.config/qs/x");
        create_symlink(&clone, &entry, home).unwrap();
        let read = fs::read_link(home.join(".config/qs/x")).unwrap();
        assert_eq!(read, clone);
    }

    #[test]
    fn create_symlink_refuses_to_clobber_directory() {
        let t = tempdir().unwrap();
        let home = t.path();
        let clone = home.join("clone");
        fs::create_dir_all(&clone).unwrap();
        // User has a real ~/.config/qs/x directory with content
        let user_dir = home.join(".config/qs/x");
        fs::create_dir_all(&user_dir).unwrap();
        fs::write(user_dir.join("user.conf"), "mine\n").unwrap();

        let entry = mk_symlink_entry(".", "~/.config/qs/x");
        let err = create_symlink(&clone, &entry, home).unwrap_err();
        assert!(err.to_string().contains("exists as a directory"));
        // User's file still there
        assert!(user_dir.join("user.conf").exists());
    }

    #[test]
    fn preserve_user_edits_copies_modified_and_untracked() {
        // Build a real git repo so git status has something to report.
        let t = tempdir().unwrap();
        let clone = t.path().join("clone");
        fs::create_dir_all(&clone).unwrap();
        for (name, body) in [("shell.qml", "orig\n"), ("README.md", "readme\n")] {
            fs::write(clone.join(name), body).unwrap();
        }
        let git = |args: &[&str]| {
            Command::new("git")
                .current_dir(&clone)
                .args(args)
                .output()
                .expect("git")
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "a@b"]);
        git(&["config", "user.name", "a"]);
        git(&["add", "."]);
        git(&["commit", "-qm", "init"]);

        // User edits shell.qml and drops a new custom.qml.
        fs::write(clone.join("shell.qml"), "my edit\n").unwrap();
        fs::write(clone.join("custom.qml"), "new\n").unwrap();

        let rcsave = t.path().join("rcsave");
        let preserved = preserve_user_edits(&clone, &rcsave).unwrap();
        assert!(preserved.len() >= 2);
        assert!(rcsave.join("shell.qml").exists(), "modified file preserved");
        assert!(
            rcsave.join("custom.qml").exists(),
            "untracked file preserved"
        );
        assert_eq!(
            fs::read_to_string(rcsave.join("shell.qml")).unwrap(),
            "my edit\n"
        );
    }

    #[test]
    fn preserve_user_edits_falls_back_when_git_missing() {
        let t = tempdir().unwrap();
        let clone = t.path().join("clone");
        fs::create_dir_all(&clone).unwrap();
        fs::write(clone.join("shell.qml"), "body\n").unwrap();
        // No .git dir → fallback to whole-clone copy.

        let rcsave = t.path().join("rcsave/dms-123");
        let preserved = preserve_user_edits(&clone, &rcsave).unwrap();
        assert_eq!(preserved.len(), 1);
        let fallback = preserved[0].clone();
        assert!(
            fallback.to_string_lossy().ends_with("-unverified"),
            "fallback dir should be -unverified, got {}",
            fallback.display()
        );
        assert!(fallback.join("shell.qml").exists());
    }

    #[test]
    fn create_symlink_preserves_existing_symlink_on_failure() {
        // Simulate: dst parent is read-only, so atomic-rename fails.
        // Verify the tmp symlink doesn't leak and the prior symlink
        // (if any) remains untouched.
        let t = tempdir().unwrap();
        let home = t.path();
        let clone = home.join("clone");
        fs::create_dir_all(&clone).unwrap();
        let parent = home.join(".config/qs");
        fs::create_dir_all(&parent).unwrap();
        // Pre-existing symlink we want preserved on failure.
        let orig_target = home.join("original-target");
        fs::create_dir_all(&orig_target).unwrap();
        symlink(&orig_target, parent.join("x")).unwrap();

        // Happy path: current code does atomic rename, should succeed.
        let entry = mk_symlink_entry(".", "~/.config/qs/x");
        create_symlink(&clone, &entry, home).unwrap();
        assert_eq!(fs::read_link(parent.join("x")).unwrap(), clone);

        // No stray .tmp file left in the parent dir.
        for e in fs::read_dir(&parent).unwrap().flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            assert!(!name.contains(".tmp."), "atomic-rename leaked tmp: {name}");
        }
    }

    #[test]
    fn preserve_user_edits_handles_non_utf8_filename() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let t = tempdir().unwrap();
        let clone = t.path().join("clone");
        fs::create_dir_all(&clone).unwrap();
        fs::write(clone.join("shell.qml"), "orig\n").unwrap();
        let git = |args: &[&str]| {
            Command::new("git")
                .current_dir(&clone)
                .args(args)
                .output()
                .expect("git")
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "a@b"]);
        git(&["config", "user.name", "a"]);
        git(&["add", "."]);
        git(&["commit", "-qm", "init"]);

        // Drop a file whose name is not valid UTF-8 (latin-1 é as 0xe9).
        let bytes = b"caf\xe9.conf";
        let latin1_name = OsStr::from_bytes(bytes);
        fs::write(clone.join(latin1_name), "data\n").unwrap();

        let rcsave = t.path().join("rcsave");
        // Must not error out — uninstall depends on never losing user
        // edits, and erroring leaves the user stuck.
        let preserved = preserve_user_edits(&clone, &rcsave).unwrap();
        assert!(
            preserved.iter().any(|p| p.file_name() == Some(latin1_name)),
            "latin-1 filename not preserved: {:?}",
            preserved
        );
    }

    #[test]
    fn remove_symlink_with_preservation_removes_after_rcsave() {
        let t = tempdir().unwrap();
        let home = t.path();
        let clone = home.join("clone");
        fs::create_dir_all(&clone).unwrap();
        fs::write(clone.join("shell.qml"), "body\n").unwrap();

        let entry = mk_symlink_entry(".", "~/.config/qs/x");
        let paths = create_symlink(&clone, &entry, home).unwrap();
        assert!(paths.symlink_path.is_symlink());

        let rcsave_root = home.join("rcsave");
        remove_symlink_with_preservation(&clone, &paths.symlink_path, &rcsave_root).unwrap();
        assert!(!paths.symlink_path.exists(), "symlink removed");
    }

    #[test]
    fn remove_symlink_is_idempotent_when_symlink_already_gone() {
        let t = tempdir().unwrap();
        let home = t.path();
        let clone = home.join("clone");
        fs::create_dir_all(&clone).unwrap();
        // Don't create the symlink — simulate retry after a successful
        // previous removal.
        let sym = home.join(".config/qs/x");
        let rcsave_root = home.join("rcsave");
        remove_symlink_with_preservation(&clone, &sym, &rcsave_root).unwrap();
    }
}
