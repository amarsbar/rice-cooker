//! Atomic symlink install — `ln -sfnT <clone>/<symlink_src> <symlink_dst>`.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use anyhow::{Context, Result, anyhow};

use crate::catalog::RiceEntry;
use crate::paths::expand_home;

/// Create `symlink_dst` pointing at `<clone>/<symlink_src>`. Creates parent
/// dirs. Overwrites stale rice-cooker symlinks. Refuses to replace a
/// user-owned directory or regular file.
pub fn create_symlink(clone_dir: &Path, entry: &RiceEntry, home: &Path) -> Result<()> {
    let src = clone_dir.join(&entry.symlink_src);
    let dst = expand_home(&entry.symlink_dst, home);
    let parent = dst
        .parent()
        .ok_or_else(|| anyhow!("{}: no parent dir", dst.display()))?;
    let file_name = dst
        .file_name()
        .ok_or_else(|| anyhow!("{}: no file name", dst.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("creating parent of {}", dst.display()))?;

    match fs::symlink_metadata(&dst) {
        Ok(md) if md.file_type().is_dir() => {
            return Err(anyhow!("{} exists as a directory", dst.display()));
        }
        Ok(md) if !md.file_type().is_symlink() => {
            return Err(anyhow!(
                "{} exists as a regular file (move or remove it first)",
                dst.display()
            ));
        }
        Ok(_) => {} // stale symlink — replaced atomically by the rename below
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(anyhow!("reading {}: {e}", dst.display())),
    }

    // Atomic-rename dance: create at sibling `.rctmp`, then rename over dst.
    // Plain remove+symlink leaves an ENOENT window + EEXIST race.
    let mut tmp_name = file_name.to_os_string();
    tmp_name.push(".rctmp");
    let tmp = parent.join(tmp_name);
    match fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) if e.kind() == std::io::ErrorKind::IsADirectory => {
            fs::remove_dir_all(&tmp).with_context(|| format!("clearing stale {}", tmp.display()))?
        }
        Err(e) => return Err(anyhow!("clearing stale {}: {e}", tmp.display())),
    }
    symlink(&src, &tmp)
        .with_context(|| format!("symlink {} -> {}", tmp.display(), src.display()))?;
    fs::rename(&tmp, &dst).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        anyhow!("renaming {} -> {}: {e}", tmp.display(), dst.display())
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
            documented_system_effects: vec![],
        }
    }

    #[test]
    fn writes_symlink_into_expanded_home() {
        let t = tempdir().unwrap();
        let home = t.path();
        let clone = home.join("clone");
        fs::create_dir_all(&clone).unwrap();
        create_symlink(&clone, &mk_entry(".", "~/.config/qs/x"), home).unwrap();
        assert_eq!(fs::read_link(home.join(".config/qs/x")).unwrap(), clone);
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
