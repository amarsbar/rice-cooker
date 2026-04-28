//! Unified filesystem layout for rice-cooker state.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use xdg::BaseDirectories;

/// Full argv (not a bare `-p` path) preserves forms like `qs -c <name>`.
/// cwd matters when argv contains a relative `-p` path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OriginalShell {
    pub argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// `home` is the raw $HOME (used for `~/` expansion in catalog `symlink_dst`),
/// not the rice-cooker-prefixed XDG data dir.
pub struct Paths {
    pub home: PathBuf,
    pub cache_home: PathBuf,
    pub data_home: PathBuf,
    xdg: Option<BaseDirectories>,
}

impl Paths {
    pub fn from_env() -> Result<Self> {
        let home = resolve_home_from(std::env::var("HOME").ok().as_deref())?;
        let xdg = BaseDirectories::with_prefix("rice-cooker");
        // RICE_COOKER_CACHE_DIR redirects the whole cache root without touching
        // XDG env vars — convenient for tests against the built binary.
        let cache_home = match std::env::var("RICE_COOKER_CACHE_DIR") {
            Ok(s) if !s.is_empty() => PathBuf::from(s),
            _ => xdg.get_cache_home().ok_or_else(|| {
                anyhow!("cannot resolve cache home — set XDG_CACHE_HOME or ensure HOME is absolute")
            })?,
        };
        let data_home = xdg.get_data_home().ok_or_else(|| {
            anyhow!("cannot resolve data home — set XDG_DATA_HOME or ensure HOME is absolute")
        })?;
        Ok(Self {
            home,
            cache_home,
            data_home,
            xdg: Some(xdg),
        })
    }

    /// Test-only. `find_catalog` and `searched_catalog_paths` return empty.
    pub fn at_roots(home: PathBuf, cache_home: PathBuf, data_home: PathBuf) -> Self {
        Self {
            home,
            cache_home,
            data_home,
            xdg: None,
        }
    }

    pub fn lock(&self) -> PathBuf {
        self.cache_home.join("lock")
    }
    pub fn rices_dir(&self) -> PathBuf {
        self.cache_home.join("rices")
    }
    pub fn clone_dir(&self, name: &str) -> Result<PathBuf> {
        validate_name(name)?;
        Ok(self.rices_dir().join(name))
    }
    pub fn last_run_log(&self) -> PathBuf {
        self.cache_home.join("last-run.log")
    }

    pub fn installs_dir(&self) -> PathBuf {
        self.data_home.join("installs")
    }
    pub fn current_json(&self) -> PathBuf {
        self.installs_dir().join("current.json")
    }
    pub fn record_json(&self, name: &str) -> Result<PathBuf> {
        validate_name(name)?;
        Ok(self.installs_dir().join(format!("{name}.json")))
    }

    pub fn original_file(&self) -> PathBuf {
        self.cache_home.join("original")
    }

    pub fn find_catalog(&self) -> Option<PathBuf> {
        self.xdg.as_ref()?.find_data_file("catalog.toml")
    }

    /// Search order for `find_catalog`; feeds the "not found" error message.
    pub fn searched_catalog_paths(&self) -> Vec<PathBuf> {
        let Some(xdg) = self.xdg.as_ref() else {
            return Vec::new();
        };
        xdg.get_data_home()
            .into_iter()
            .chain(xdg.get_data_dirs())
            .map(|d| d.join("catalog.toml"))
            .collect()
    }

    pub fn ensure_rices(&self) -> Result<()> {
        fs::create_dir_all(self.rices_dir())
            .with_context(|| format!("creating {}", self.rices_dir().display()))
    }
    pub fn ensure_installs(&self) -> Result<()> {
        fs::create_dir_all(self.installs_dir())
            .with_context(|| format!("creating {}", self.installs_dir().display()))
    }

    pub fn original(&self) -> Result<Option<OriginalShell>> {
        let path = self.original_file();
        let s = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        // Malformed content (empty file from older backend, plain path, typo
        // from a hand-edit) reads as None; next install preflight re-captures.
        Ok(serde_json::from_str::<Option<OriginalShell>>(s.trim()).unwrap_or(None))
    }

    pub fn original_is_recorded(&self) -> bool {
        let Ok(s) = fs::read_to_string(self.original_file()) else {
            return false;
        };
        serde_json::from_str::<Option<OriginalShell>>(s.trim()).is_ok()
    }

    pub fn set_original(&self, shell: Option<&OriginalShell>) -> Result<()> {
        let body = serde_json::to_string(&shell).context("serializing original shell")?;
        write_line_file(&self.original_file(), &body)
    }

    pub fn clear_original(&self) -> Result<()> {
        remove_if_exists(&self.original_file())
    }
}

/// Pure form so tests don't have to mutate process-global `HOME`.
pub fn resolve_home_from(home: Option<&str>) -> Result<PathBuf> {
    let home = home.filter(|s| !s.is_empty()).ok_or_else(|| {
        anyhow!(
            "HOME must be set — rice-cooker needs it to expand `~/` in catalog \
             symlink_dst entries (XDG_*_HOME vars are not a substitute)"
        )
    })?;
    let home = PathBuf::from(home);
    if home.as_os_str() == "/" {
        return Err(anyhow!("HOME is '/' — refusing"));
    }
    Ok(home)
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.chars().any(|c| matches!(c, '/' | '\\' | '\0'))
    {
        return Err(anyhow!("invalid rice name: {name:?}"));
    }
    Ok(())
}

pub fn expand_home(raw: &str, home: &Path) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        home.join(rest)
    } else if raw == "~" {
        home.to_path_buf()
    } else if let Some(rest) = raw.strip_prefix("$HOME/") {
        home.join(rest)
    } else if raw == "$HOME" {
        home.to_path_buf()
    } else {
        PathBuf::from(raw)
    }
}

fn remove_if_exists(p: &Path) -> Result<()> {
    match fs::remove_file(p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("removing {}", p.display())),
    }
}

// No parent-dir fsync: `original` is cache; next install preflight re-captures.
fn write_line_file(path: &Path, contents: &str) -> Result<()> {
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut f = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&tmp)
        .with_context(|| format!("opening {}", tmp.display()))?;
    let res = f
        .write_all(contents.as_bytes())
        .and_then(|_| f.write_all(b"\n"))
        .and_then(|_| f.sync_all());
    if let Err(e) = res {
        let _ = fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("writing {}", tmp.display()));
    }
    drop(f);
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_paths() -> (tempfile::TempDir, Paths) {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().to_path_buf();
        let cache_home = home.join(".cache/rice-cooker");
        let data_home = home.join(".local/share/rice-cooker");
        fs::create_dir_all(&cache_home).unwrap();
        fs::create_dir_all(&data_home).unwrap();
        let paths = Paths::at_roots(home, cache_home, data_home);
        paths.ensure_rices().unwrap();
        paths.ensure_installs().unwrap();
        (t, paths)
    }

    #[test]
    fn expand_home_cases() {
        let h = Path::new("/h");
        assert_eq!(expand_home("~/x", h), PathBuf::from("/h/x"));
        assert_eq!(expand_home("~", h), PathBuf::from("/h"));
        assert_eq!(expand_home("$HOME/y", h), PathBuf::from("/h/y"));
        assert_eq!(expand_home("/etc/hypr", h), PathBuf::from("/etc/hypr"));
    }

    #[test]
    fn resolve_home_rejects_none_empty_and_root() {
        assert!(resolve_home_from(None).is_err());
        assert!(resolve_home_from(Some("")).is_err());
        assert!(resolve_home_from(Some("/")).is_err());
        assert_eq!(
            resolve_home_from(Some("/home/x")).unwrap(),
            PathBuf::from("/home/x")
        );
    }

    #[test]
    fn clone_and_record_reject_traversal() {
        let (_t, p) = tmp_paths();
        for good in ["caelestia", "noctalia-2", "rice_v1", "Foo.Bar"] {
            assert_eq!(p.clone_dir(good).unwrap(), p.rices_dir().join(good));
            assert_eq!(
                p.record_json(good).unwrap(),
                p.installs_dir().join(format!("{good}.json"))
            );
        }
        for bad in ["", ".", "..", "a/b", "a\\b", "a\0b"] {
            assert!(p.clone_dir(bad).is_err(), "clone_dir accepted: {bad:?}");
            assert!(p.record_json(bad).is_err(), "record_json accepted: {bad:?}");
        }
    }

    #[test]
    fn original_none_recorded_flag_and_argv_roundtrip() {
        let (_t, p) = tmp_paths();
        assert!(!p.original_is_recorded());
        assert!(p.original().unwrap().is_none());
        p.set_original(None).unwrap();
        assert!(p.original_is_recorded());
        assert!(p.original().unwrap().is_none());
        let shell = OriginalShell {
            argv: vec!["qs".into(), "-c".into(), "clock".into()],
            cwd: Some("/home/x".into()),
        };
        p.set_original(Some(&shell)).unwrap();
        assert_eq!(p.original().unwrap(), Some(shell));
    }

    #[test]
    fn original_stale_plain_text_reads_as_unrecorded() {
        // Older backend versions wrote a bare path; migration contract is that
        // such a file reads as None and re-records on the next install.
        let (_t, p) = tmp_paths();
        fs::write(p.original_file(), "shell.qml\n").unwrap();
        assert!(p.original().unwrap().is_none());
        assert!(!p.original_is_recorded());
    }

    #[test]
    fn clear_original_is_idempotent() {
        let (_t, p) = tmp_paths();
        p.set_original(None).unwrap();
        assert!(p.original_is_recorded());
        p.clear_original().unwrap();
        assert!(!p.original_is_recorded());
        p.clear_original().unwrap();
    }

    #[test]
    fn find_catalog_and_searched_paths_empty_under_at_roots() {
        let (_t, p) = tmp_paths();
        assert!(p.find_catalog().is_none());
        assert!(p.searched_catalog_paths().is_empty());
    }

    #[test]
    fn lock_path_is_cache_home_slash_lock() {
        let p = Paths::at_roots(
            PathBuf::from("/h"),
            PathBuf::from("/c"),
            PathBuf::from("/d"),
        );
        assert_eq!(p.lock(), PathBuf::from("/c/lock"));
    }
}
