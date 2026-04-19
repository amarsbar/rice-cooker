use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

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

    /// Resolve the on-disk directory for a rice by name. Rejects names that could
    /// escape the `rices/` subdir — even under a curated catalog, a typo'd entry
    /// (`..`, `/`, `\`, NUL) would otherwise write outside the cache root. The name
    /// is part of a filesystem path, not a URL, so this check matters even when
    /// the URL-side is trusted.
    pub fn rice_dir(&self, name: &str) -> Result<PathBuf> {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.chars().any(|c| matches!(c, '/' | '\\' | '\0'))
        {
            return Err(anyhow!("invalid rice name: {name:?}"));
        }
        Ok(self.root.join("rices").join(name))
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

    pub fn original(&self) -> Result<Option<String>> {
        read_line_file(&self.root.join("original"))
    }

    pub fn original_is_recorded(&self) -> bool {
        self.root.join("original").is_file()
    }

    pub fn set_active(&self, name: &str) -> Result<()> {
        write_line_file(&self.root.join("active"), name)
    }

    pub fn set_original(&self, entry: Option<&str>) -> Result<()> {
        write_line_file(&self.root.join("original"), entry.unwrap_or(""))
    }

    pub fn clear_active(&self) -> Result<()> {
        let p = self.root.join("active");
        match fs::remove_file(&p) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).with_context(|| format!("removing {}", p.display())),
        }
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
    fn resolve_root_respects_env_precedence() {
        // Each row is (rc, xdg, home) inputs and the expected root. `None` root = Err.
        type Row<'a> = (
            Option<&'a str>,
            Option<&'a str>,
            Option<&'a str>,
            Option<&'a str>,
        );
        let cases: &[Row] = &[
            (Some("/a"), Some("/b/c"), Some("/home/x"), Some("/a")),
            (
                None,
                Some("/b/c"),
                Some("/home/x"),
                Some("/b/c/rice-cooker"),
            ),
            (
                None,
                None,
                Some("/home/x"),
                Some("/home/x/.cache/rice-cooker"),
            ),
            // Empty env vars are treated like unset.
            (
                Some(""),
                Some(""),
                Some("/home/x"),
                Some("/home/x/.cache/rice-cooker"),
            ),
            (None, None, None, None),
        ];
        for (rc, xdg, home, expected) in cases {
            let got = resolve_root_with(*rc, *xdg, *home);
            match expected {
                Some(want) => assert_eq!(got.unwrap(), PathBuf::from(want)),
                None => assert!(got.is_err()),
            }
        }
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
    fn active_reads_none_when_missing_or_empty_and_writes_roundtrip() {
        let (_dir, cache) = tmp_cache();
        // Missing and empty both read as None (two distinct IO paths in read_line_file).
        assert!(cache.active().unwrap().is_none());
        std::fs::write(cache.root().join("active"), "").unwrap();
        assert!(cache.active().unwrap().is_none());
        // Write + overwrite both round-trip; trailing newline is stripped on read.
        cache.set_active("first").unwrap();
        cache.set_active("second").unwrap();
        assert_eq!(cache.active().unwrap().as_deref(), Some("second"));
    }

    #[test]
    fn original_none_and_original_is_recorded_flag() {
        let (_dir, cache) = tmp_cache();
        // Not yet recorded.
        assert!(!cache.original_is_recorded());
        assert!(cache.original().unwrap().is_none());
        // set_original(None) writes the empty-sentinel form, which is "recorded as empty"
        // (distinct from "never recorded at all").
        cache.set_original(None).unwrap();
        assert!(cache.original_is_recorded());
        assert!(cache.original().unwrap().is_none());
    }

    #[test]
    fn rice_dir_accepts_clean_names_rejects_traversal() {
        let (_dir, cache) = tmp_cache();
        for good in ["caelestia", "noctalia-2", "rice_v1", "Foo.Bar"] {
            let p = cache.rice_dir(good).unwrap();
            assert_eq!(p, cache.root().join("rices").join(good));
        }
        for bad in ["", ".", "..", "a/b", "a\\b", "a\0b"] {
            assert!(cache.rice_dir(bad).is_err(), "accepted: {bad:?}");
        }
    }

    #[test]
    fn clear_active_removes_the_file() {
        let (_dir, cache) = tmp_cache();
        cache.set_active("x").unwrap();
        cache.clear_active().unwrap();
        assert!(cache.active().unwrap().is_none());
        // Idempotent: clearing a missing file is fine.
        cache.clear_active().unwrap();
    }
}
