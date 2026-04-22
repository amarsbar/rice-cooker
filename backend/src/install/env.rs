//! Resolved directory layout for install state.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

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
    pub fn lock_file(&self) -> PathBuf {
        self.cache.join("lock")
    }
    pub fn installs_dir(&self) -> PathBuf {
        self.data.join("installs")
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

    pub fn ensure(&self) -> Result<()> {
        for d in [&self.rices_dir(), &self.installs_dir()] {
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
    let cache = env_override("XDG_CACHE_HOME", &home.join(".cache"));
    let data = env_override("XDG_DATA_HOME", &home.join(".local/share"));
    Ok(Dirs {
        home,
        cache: cache.join("rice-cooker"),
        data: data.join("rice-cooker"),
    })
}

fn env_override(var: &str, default: &Path) -> PathBuf {
    match std::env::var(var) {
        Ok(s) if !s.is_empty() => PathBuf::from(s),
        _ => default.to_path_buf(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_home_cases() {
        let h = Path::new("/h");
        assert_eq!(expand_home("~/x", h), PathBuf::from("/h/x"));
        assert_eq!(expand_home("~", h), PathBuf::from("/h"));
        assert_eq!(expand_home("$HOME/y", h), PathBuf::from("/h/y"));
        assert_eq!(expand_home("/etc/hypr", h), PathBuf::from("/etc/hypr"));
    }
}
