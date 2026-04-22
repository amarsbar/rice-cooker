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
            // Stale symlink — safe to replace.
            fs::remove_file(&dst)
                .map_err(|e| anyhow!("clearing stale symlink {}: {e}", dst.display()))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(anyhow!("reading {}: {e}", dst.display())),
    }
    symlink(&src, &dst)
        .with_context(|| format!("symlink {} -> {}", dst.display(), src.display()))?;

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
    fn creates_parent_dirs() {
        let t = tempdir().unwrap();
        let home = t.path();
        let clone = home.join("clone");
        fs::create_dir_all(&clone).unwrap();
        create_symlink(&clone, &mk_entry(".", "~/.config/deeply/nested/x"), home).unwrap();
        assert!(home.join(".config/deeply/nested/x").is_symlink());
    }
}
