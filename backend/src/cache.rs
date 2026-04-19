use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

pub struct Cache {
    root: PathBuf,
}

impl Cache {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn from_env() -> Result<Self> {
        Ok(Self::at(resolve_root()?))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn rice_dir(&self, name: &str) -> PathBuf {
        self.root.join("rices").join(name)
    }

    pub fn last_run_log(&self) -> PathBuf {
        self.root.join("last-run.log")
    }

    pub fn apply_lock(&self) -> PathBuf {
        self.root.join("apply.lock")
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(self.root.join("rices"))
            .with_context(|| format!("creating cache root at {}", self.root.display()))?;
        Ok(())
    }

    pub fn active(&self) -> Result<Option<String>> {
        read_line_file(&self.root.join("active"))
    }

    pub fn previous(&self) -> Result<Option<String>> {
        read_line_file(&self.root.join("previous"))
    }

    pub fn original(&self) -> Result<Option<String>> {
        read_line_file(&self.root.join("original"))
    }

    pub fn original_is_recorded(&self) -> bool {
        self.root.join("original").is_file()
    }

    pub fn set_active(&self, name: &str) -> Result<()> {
        write_line_file(&self.root.join("active"), name)
    }

    pub fn set_previous(&self, name: &str) -> Result<()> {
        write_line_file(&self.root.join("previous"), name)
    }

    pub fn set_original(&self, entry: Option<&str>) -> Result<()> {
        write_line_file(&self.root.join("original"), entry.unwrap_or(""))
    }

    pub fn swap_active_previous(&self) -> Result<()> {
        let a = self.active()?;
        let p = self.previous()?;
        match (a, p) {
            (Some(a), Some(p)) => {
                self.set_active(&p)?;
                self.set_previous(&a)?;
            }
            (Some(_), None) | (None, Some(_)) | (None, None) => {
                return Err(anyhow!("cannot swap: missing active or previous"));
            }
        }
        Ok(())
    }

    pub fn clear_active_previous(&self) -> Result<()> {
        for name in ["active", "previous"] {
            let p = self.root.join(name);
            match fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e).with_context(|| format!("removing {}", p.display())),
            }
        }
        Ok(())
    }
}

fn read_line_file(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => {
            let trimmed = s.trim_end_matches(['\n', '\r']).to_string();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

fn write_line_file(path: &Path, contents: &str) -> Result<()> {
    // Append `.tmp` to the full filename instead of replacing the extension —
    // `with_extension("tmp")` would turn `foo.bar` into `foo.tmp`, a potential collision.
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    if let Err(e) = fs::write(&tmp, format!("{contents}\n")) {
        let _ = fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("writing {}", tmp.display()));
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()));
    }
    Ok(())
}

pub fn resolve_root_with(
    rice_cooker_cache_dir: Option<&str>,
    xdg_cache_home: Option<&str>,
    home: Option<&str>,
) -> Result<PathBuf> {
    if let Some(dir) = non_empty(rice_cooker_cache_dir) {
        return Ok(PathBuf::from(dir));
    }
    if let Some(dir) = non_empty(xdg_cache_home) {
        return Ok(PathBuf::from(dir).join("rice-cooker"));
    }
    if let Some(dir) = non_empty(home) {
        return Ok(PathBuf::from(dir).join(".cache").join("rice-cooker"));
    }
    Err(anyhow!(
        "cannot resolve cache root: set RICE_COOKER_CACHE_DIR, XDG_CACHE_HOME, or HOME"
    ))
}

pub fn resolve_root() -> Result<PathBuf> {
    let rc = std::env::var("RICE_COOKER_CACHE_DIR").ok();
    let xdg = std::env::var("XDG_CACHE_HOME").ok();
    let home = std::env::var("HOME").ok();
    resolve_root_with(rc.as_deref(), xdg.as_deref(), home.as_deref())
}

fn non_empty(s: Option<&str>) -> Option<&str> {
    s.filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_root_prefers_rice_cooker_cache_dir() {
        let root = resolve_root_with(Some("/a"), Some("/b/cache"), Some("/home/x")).unwrap();
        assert_eq!(root, std::path::PathBuf::from("/a"));
    }

    #[test]
    fn resolve_root_falls_back_to_xdg_cache_home() {
        let root = resolve_root_with(None, Some("/b/cache"), Some("/home/x")).unwrap();
        assert_eq!(root, std::path::PathBuf::from("/b/cache/rice-cooker"));
    }

    #[test]
    fn resolve_root_falls_back_to_home_dot_cache() {
        let root = resolve_root_with(None, None, Some("/home/x")).unwrap();
        assert_eq!(root, std::path::PathBuf::from("/home/x/.cache/rice-cooker"));
    }

    #[test]
    fn resolve_root_treats_empty_env_as_unset() {
        let root = resolve_root_with(Some(""), Some(""), Some("/home/x")).unwrap();
        assert_eq!(root, std::path::PathBuf::from("/home/x/.cache/rice-cooker"));
    }

    #[test]
    fn resolve_root_errors_when_nothing_set() {
        assert!(resolve_root_with(None, None, None).is_err());
    }

    fn tmp_cache() -> (tempfile::TempDir, Cache) {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        cache.ensure_dirs().unwrap();
        (dir, cache)
    }

    #[test]
    fn ensure_dirs_creates_root_and_rices_subdir() {
        let (_dir, cache) = tmp_cache();
        assert!(cache.root().is_dir());
        assert!(cache.root().join("rices").is_dir());
    }

    #[test]
    fn active_returns_none_when_missing() {
        let (_dir, cache) = tmp_cache();
        assert!(cache.active().unwrap().is_none());
    }

    #[test]
    fn active_returns_none_when_empty() {
        let (_dir, cache) = tmp_cache();
        std::fs::write(cache.root().join("active"), "").unwrap();
        assert!(cache.active().unwrap().is_none());
    }

    #[test]
    fn set_active_and_read_roundtrip() {
        let (_dir, cache) = tmp_cache();
        cache.set_active("caelestia").unwrap();
        assert_eq!(cache.active().unwrap().as_deref(), Some("caelestia"));
    }

    #[test]
    fn set_active_overwrites_with_trailing_newline_trimmed() {
        let (_dir, cache) = tmp_cache();
        cache.set_active("first").unwrap();
        cache.set_active("second").unwrap();
        assert_eq!(cache.active().unwrap().as_deref(), Some("second"));
    }

    #[test]
    fn set_original_none_writes_empty_sentinel() {
        let (_dir, cache) = tmp_cache();
        cache.set_original(None).unwrap();
        assert!(cache.root().join("original").is_file());
        assert!(cache.original().unwrap().is_none());
    }

    #[test]
    fn original_is_recorded_distinguishes_presence_from_emptiness() {
        let (_dir, cache) = tmp_cache();
        assert!(!cache.original_is_recorded());
        cache.set_original(None).unwrap();
        assert!(cache.original_is_recorded());
    }

    #[test]
    fn swap_exchanges_active_and_previous() {
        let (_dir, cache) = tmp_cache();
        cache.set_active("b").unwrap();
        cache.set_previous("a").unwrap();
        cache.swap_active_previous().unwrap();
        assert_eq!(cache.active().unwrap().as_deref(), Some("a"));
        assert_eq!(cache.previous().unwrap().as_deref(), Some("b"));
    }

    #[test]
    fn clear_active_and_previous_removes_both_files() {
        let (_dir, cache) = tmp_cache();
        cache.set_active("x").unwrap();
        cache.set_previous("y").unwrap();
        cache.clear_active_previous().unwrap();
        assert!(cache.active().unwrap().is_none());
        assert!(cache.previous().unwrap().is_none());
    }
}
