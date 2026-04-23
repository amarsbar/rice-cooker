//! Symlink install — `ln -sfnT <clone>/<symlink_src> <symlink_dst>`.
//! ~30 LOC. No git-status, no fallbacks. If a user customized files
//! inside the symlinked clone, the uninstall path `cp -rT`s the whole
//! clone to rcsave before removing it.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::catalog::RiceEntry;

use super::env::expand_home;

#[derive(Debug, Clone, PartialEq)]
pub struct SymlinkPaths {
    pub symlink_path: PathBuf,
    pub symlink_target: PathBuf,
}

/// Create `symlink_dst` pointing at `<clone>/<symlink_src>`. Creates
/// parent dirs. Overwrites stale symlinks. Refuses to replace a real
/// directory at the target (protects the user's existing config).
pub fn create_symlink(clone_dir: &Path, entry: &RiceEntry, home: &Path) -> Result<SymlinkPaths> {
    let src = clone_dir.join(&entry.symlink_src);
    let dst = expand_home(&entry.symlink_dst, home);

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating parent of {}", dst.display()))?;
    }

    match fs::symlink_metadata(&dst) {
        Ok(md) => {
            let ft = md.file_type();
            if ft.is_dir() {
                return Err(anyhow!(
                    "{} exists as a directory; refusing to replace with a symlink",
                    dst.display()
                ));
            }
            if !ft.is_symlink() {
                // Real file (not a symlink). Could be user-created config.
                // Refuse rather than silently clobber.
                return Err(anyhow!(
                    "{} exists as a regular file; refusing to replace with a symlink (move or remove it first)",
                    dst.display()
                ));
            }
            // Stale symlink — will be replaced atomically by the rename below.
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(anyhow!("reading {}: {e}", dst.display())),
    }

    // Create the new symlink at a sibling temp path, then atomically
    // rename over dst. Skipping this and doing remove+symlink leaves a
    // window where dst doesn't exist — any reader in that window gets
    // ENOENT, and a racing process could mkdir the path and make the
    // symlink call fail with EEXIST. rename handles both.
    let parent = dst
        .parent()
        .ok_or_else(|| anyhow!("{}: no parent dir", dst.display()))?;
    let file_name = dst
        .file_name()
        .ok_or_else(|| anyhow!("{}: no file name", dst.display()))?;
    let mut tmp_name = file_name.to_os_string();
    tmp_name.push(".rctmp");
    let tmp = parent.join(tmp_name);
    // Clean up any stray tmp from a previous failed run. Swallow
    // NotFound (expected — tmp usually doesn't exist) but surface
    // unexpected errors so a root-owned `.rctmp` or similar doesn't
    // get silently masked by the subsequent symlink-EEXIST failure.
    // If a prior run (or a user) left a directory at the .rctmp path,
    // remove_file fails with EISDIR — fall back to remove_dir_all so
    // install isn't wedged by a stale dir we created ourselves.
    match fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) if e.kind() == std::io::ErrorKind::IsADirectory => {
            fs::remove_dir_all(&tmp)
                .with_context(|| format!("clearing stale temp dir {}", tmp.display()))?;
        }
        Err(e) => {
            return Err(anyhow!("clearing stale temp {}: {e}", tmp.display()));
        }
    }
    symlink(&src, &tmp)
        .with_context(|| format!("symlink {} -> {}", tmp.display(), src.display()))?;
    if let Err(e) = fs::rename(&tmp, &dst) {
        // Best-effort cleanup of the temp symlink on rename failure;
        // swallow anything here — the real error is the rename failure.
        let _ = fs::remove_file(&tmp);
        return Err(anyhow!(
            "renaming {} -> {}: {e}",
            tmp.display(),
            dst.display()
        ));
    }

    Ok(SymlinkPaths {
        symlink_path: dst,
        symlink_target: src,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::EntryPoint;
    use tempfile::tempdir;

    fn mk_entry(src: &str, dst: &str) -> RiceEntry {
        RiceEntry {
            display_name: "X".into(),
            description: "".into(),
            repo: "https://x".into(),
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            symlink_src: src.into(),
            symlink_dst: dst.into(),
            aur_deps: vec![],
            pacman_deps: vec![],
            interactive: false,
            entry: EntryPoint::default(),
            documented_system_effects: vec![],
        }
    }

    #[test]
    fn writes_symlink_and_records_paths() {
        let t = tempdir().unwrap();
        let home = t.path();
        let clone = home.join("clone");
        fs::create_dir_all(&clone).unwrap();
        let paths = create_symlink(&clone, &mk_entry(".", "~/.config/qs/x"), home).unwrap();
        assert_eq!(paths.symlink_path, home.join(".config/qs/x"));
        assert_eq!(fs::read_link(&paths.symlink_path).unwrap(), clone);
    }

    #[test]
    fn overwrites_stale_symlink() {
        let t = tempdir().unwrap();
        let home = t.path();
        let clone = home.join("clone");
        fs::create_dir_all(&clone).unwrap();
        fs::create_dir_all(home.join(".config/qs")).unwrap();
        symlink("/nonexistent", home.join(".config/qs/x")).unwrap();
        create_symlink(&clone, &mk_entry(".", "~/.config/qs/x"), home).unwrap();
        assert_eq!(fs::read_link(home.join(".config/qs/x")).unwrap(), clone);
    }

    #[test]
    fn refuses_to_replace_directory() {
        let t = tempdir().unwrap();
        let home = t.path();
        let clone = home.join("clone");
        fs::create_dir_all(&clone).unwrap();
        let user_dir = home.join(".config/qs/x");
        fs::create_dir_all(&user_dir).unwrap();
        fs::write(user_dir.join("mine.conf"), "mine").unwrap();
        let err = create_symlink(&clone, &mk_entry(".", "~/.config/qs/x"), home).unwrap_err();
        assert!(err.to_string().contains("exists as a directory"));
        assert!(user_dir.join("mine.conf").exists());
    }

    #[test]
    fn refuses_to_replace_regular_file() {
        let t = tempdir().unwrap();
        let home = t.path();
        let clone = home.join("clone");
        fs::create_dir_all(&clone).unwrap();
        let user_file = home.join(".config/qs/x");
        fs::create_dir_all(user_file.parent().unwrap()).unwrap();
        fs::write(&user_file, "mine").unwrap();
        let err = create_symlink(&clone, &mk_entry(".", "~/.config/qs/x"), home).unwrap_err();
        assert!(err.to_string().contains("exists as a regular file"));
        assert_eq!(fs::read_to_string(&user_file).unwrap(), "mine");
    }

    #[test]
    fn creates_parent_dirs() {
        let t = tempdir().unwrap();
        let home = t.path();
        let clone = home.join("clone");
        fs::create_dir_all(&clone).unwrap();
        create_symlink(&clone, &mk_entry(".", "~/.config/deeply/nested/x"), home).unwrap();
        assert!(home.join(".config/deeply/nested/x").is_symlink());
    }
}
