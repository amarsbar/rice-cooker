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
//! - pacman pre-filter handles already-removed packages
//! - `rm -f` / `rm -rf` are idempotent
//! - timestamped rcsave dirs never collide
//!
//! No `uninstall_phase` tracking needed.
//!
//! Target: ~50 LOC of install + ~30 LOC of uninstall + tests.

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
    // existing-symlink branch below.
    if let Ok(md) = fs::symlink_metadata(&dst)
        && md.file_type().is_dir()
    {
        return Err(anyhow!(
            "{} exists as a directory; refusing to replace with a symlink",
            dst.display()
        ));
    }
    // Clear any existing symlink/file at the destination before writing
    // the new one. `symlink()` itself errors on EEXIST.
    let _ = fs::remove_file(&dst);
    symlink(&src, &dst).with_context(|| format!("ln -sfnT {} {}", src.display(), dst.display()))?;

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

    // Step B: remove the symlink. Idempotent — missing-symlink is not an
    // error (uninstall retry after successful symlink removal).
    let _ = fs::remove_file(symlink_path);
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
    // Fast-path bail: no clone dir means nothing to preserve.
    if !clone_dir.exists() {
        return Ok(vec![]);
    }

    // If .git is missing, fall back to preserving the whole clone.
    if !clone_dir.join(".git").exists() {
        return preserve_whole_clone(clone_dir, rcsave_root);
    }

    let out = Command::new("git")
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .current_dir(clone_dir)
        .output();

    let out = match out {
        Ok(o) if o.status.success() => o,
        _ => {
            // git failed — fall back to whole-clone preservation rather
            // than silently drop user edits.
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
        let rel = std::str::from_utf8(path_bytes)
            .map_err(|e| anyhow!("git status path not utf-8: {e}"))?;
        let rel = Path::new(rel);
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
