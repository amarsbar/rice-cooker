//! Resolved directory layout for install state.
//!
//! All state-dir paths route through a single `Dirs` struct so tests can
//! point them at a tempdir without touching `std::env`. Production entry
//! point is `resolve_dirs()` which reads HOME/XDG from the environment.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

/// Resolved directory layout for one invocation of rice-cooker.
#[derive(Debug, Clone)]
pub struct Dirs {
    pub home: PathBuf,
    /// `$XDG_CACHE_HOME/rice-cooker`, default `$HOME/.cache/rice-cooker`.
    pub cache: PathBuf,
    /// `$XDG_DATA_HOME/rice-cooker`, default `$HOME/.local/share/rice-cooker`.
    pub data: PathBuf,
}

impl Dirs {
    pub fn rices_dir(&self) -> PathBuf {
        self.cache.join("rices")
    }
    pub fn snapshots_dir(&self) -> PathBuf {
        self.cache.join("snapshots")
    }
    pub fn lock_file(&self) -> PathBuf {
        self.cache.join("lock")
    }
    pub fn installs_dir(&self) -> PathBuf {
        self.data.join("installs")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.data.join("logs")
    }
    pub fn current_json(&self) -> PathBuf {
        self.installs_dir().join("current.json")
    }
    pub fn previous_json(&self) -> PathBuf {
        self.installs_dir().join("previous.json")
    }
    pub fn record_json(&self, name: &str) -> PathBuf {
        self.installs_dir().join(format!("{name}.json"))
    }
    pub fn clone_dir(&self, name: &str) -> PathBuf {
        self.rices_dir().join(name)
    }
    pub fn snapshot_dir(&self, name: &str) -> PathBuf {
        self.snapshots_dir().join(name)
    }
    /// Marker file written at install start, deleted on successful record
    /// write. Its presence means an install was interrupted mid-way; the
    /// user must run `rice-cooker cleanup` before retrying.
    pub fn in_progress_json(&self) -> PathBuf {
        self.installs_dir().join(".in-progress.json")
    }
    /// Rcsave dir for symlink-shape user-edit preservation. Timestamped
    /// so concurrent/retry invocations don't collide.
    pub fn rcsave_dir(&self, name: &str, ts: &str) -> PathBuf {
        self.cache.join("rcsave").join(format!("{name}-{ts}"))
    }

    /// Create every directory that install/uninstall/switch relies on being
    /// present. Idempotent.
    pub fn ensure(&self) -> Result<()> {
        for d in [
            &self.rices_dir(),
            &self.snapshots_dir(),
            &self.installs_dir(),
            &self.logs_dir(),
        ] {
            std::fs::create_dir_all(d).map_err(|e| anyhow!("creating {}: {e}", d.display()))?;
        }
        Ok(())
    }
}

pub fn resolve_dirs() -> Result<Dirs> {
    let home = std::env::var("HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("HOME is not set"))?;
    let home = PathBuf::from(home);
    if home.as_os_str() == "/" {
        return Err(anyhow!("HOME is '/' — refusing"));
    }
    let cache = env_override_dir("XDG_CACHE_HOME", &home.join(".cache"))?;
    let data = env_override_dir("XDG_DATA_HOME", &home.join(".local/share"))?;
    Ok(Dirs {
        home,
        cache: cache.join("rice-cooker"),
        data: data.join("rice-cooker"),
    })
}

fn env_override_dir(var: &str, default: &Path) -> Result<PathBuf> {
    match std::env::var(var) {
        Ok(s) if !s.is_empty() => Ok(PathBuf::from(s)),
        _ => Ok(default.to_path_buf()),
    }
}

/// Expand a catalog/config `~/...` or `$HOME/...` path against a given home.
/// Keeps absolute paths untouched. Used for watched-root expansion and the
/// catalog's `runtime_regenerated` / `partial_ownership` / `extra_watched_roots`.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_home_cases() {
        let h = Path::new("/h");
        assert_eq!(expand_home("~/x", h), PathBuf::from("/h/x"));
        assert_eq!(expand_home("~", h), PathBuf::from("/h"));
        assert_eq!(expand_home("$HOME/y", h), PathBuf::from("/h/y"));
        assert_eq!(expand_home("$HOME", h), PathBuf::from("/h"));
        assert_eq!(expand_home("/etc/hypr", h), PathBuf::from("/etc/hypr"));
        // Intermediate ~ is not a home-expansion — that's a path segment.
        assert_eq!(expand_home("foo/~/bar", h), PathBuf::from("foo/~/bar"));
    }

    #[test]
    fn dirs_paths_are_rooted_correctly() {
        let d = Dirs {
            home: PathBuf::from("/home/x"),
            cache: PathBuf::from("/home/x/.cache/rice-cooker"),
            data: PathBuf::from("/home/x/.local/share/rice-cooker"),
        };
        assert_eq!(
            d.rices_dir(),
            PathBuf::from("/home/x/.cache/rice-cooker/rices")
        );
        assert_eq!(
            d.current_json(),
            PathBuf::from("/home/x/.local/share/rice-cooker/installs/current.json")
        );
        assert_eq!(
            d.clone_dir("caelestia"),
            PathBuf::from("/home/x/.cache/rice-cooker/rices/caelestia")
        );
    }
}
