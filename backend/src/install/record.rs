//! Install record: the JSON file at
//! `~/.local/share/rice-cooker/installs/<name>.json` + the `current.json`
//! pointer to the active rice.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::env::Dirs;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallRecord {
    pub schema_version: u32,
    pub name: String,
    pub commit: String,
    pub installed_at: String,
    pub symlink_path: PathBuf,
    pub symlink_target: PathBuf,
    pub pacman_diff: PacmanDiff,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PacmanDiff {
    #[serde(default)]
    pub added_explicit: Vec<String>,
}

impl InstallRecord {
    pub fn now_rfc3339() -> String {
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "0000-00-00T00:00:00Z".into())
    }
}

pub fn save_record(path: &Path, r: &InstallRecord) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let body = serde_json::to_string_pretty(r).context("serializing install record")?;
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, body.as_bytes()).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

pub fn load_record(path: &Path) -> Result<InstallRecord> {
    let body = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let r: InstallRecord = serde_json::from_str(&body)
        .with_context(|| format!("parsing install record at {}", path.display()))?;
    if r.schema_version != SCHEMA_VERSION {
        return Err(anyhow::anyhow!(
            "install record at {} is schema_version {}, tool supports {}",
            path.display(),
            r.schema_version,
            SCHEMA_VERSION
        ));
    }
    Ok(r)
}

pub fn write_current(dirs: &Dirs, name: &str) -> Result<()> {
    let body = serde_json::json!({ "name": name }).to_string();
    let mut tmp = dirs.current_json().as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::create_dir_all(dirs.installs_dir())
        .with_context(|| format!("creating {}", dirs.installs_dir().display()))?;
    fs::write(&tmp, body).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, dirs.current_json()).context("renaming current.json")?;
    Ok(())
}

pub fn read_current(dirs: &Dirs) -> Result<Option<String>> {
    match fs::read_to_string(dirs.current_json()) {
        Ok(s) => {
            #[derive(Deserialize)]
            struct Cur {
                name: String,
            }
            let c: Cur = serde_json::from_str(&s).context("parsing current.json")?;
            Ok(Some(c.name))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("reading current.json"),
    }
}

pub fn clear_current(dirs: &Dirs) -> Result<()> {
    match fs::remove_file(dirs.current_json()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).context("removing current.json"),
    }
}

pub fn retire_to_previous(dirs: &Dirs, name: &str) -> Result<()> {
    let from = dirs.record_json(name);
    let to = dirs.previous_json();
    if !from.exists() {
        return Ok(());
    }
    fs::rename(&from, &to)
        .with_context(|| format!("retiring {} -> {}", from.display(), to.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn tmp_dirs() -> (tempfile::TempDir, Dirs) {
        let t = tempdir().unwrap();
        let d = Dirs {
            home: t.path().to_path_buf(),
            cache: t.path().join("cache"),
            data: t.path().join("data"),
        };
        d.ensure().unwrap();
        (t, d)
    }

    fn sample() -> InstallRecord {
        InstallRecord {
            schema_version: SCHEMA_VERSION,
            name: "dms".into(),
            commit: "abc123".into(),
            installed_at: InstallRecord::now_rfc3339(),
            symlink_path: PathBuf::from("/home/x/.config/quickshell/dms"),
            symlink_target: PathBuf::from("/home/x/.cache/rice-cooker/rices/dms"),
            pacman_diff: PacmanDiff {
                added_explicit: vec!["caelestia-shell-git".into()],
            },
        }
    }

    #[test]
    fn record_round_trips_through_json() {
        let (_t, d) = tmp_dirs();
        let r = sample();
        let path = d.record_json(&r.name);
        save_record(&path, &r).unwrap();
        let back = load_record(&path).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn current_json_roundtrip() {
        let (_t, d) = tmp_dirs();
        assert_eq!(read_current(&d).unwrap(), None);
        write_current(&d, "dms").unwrap();
        assert_eq!(read_current(&d).unwrap().as_deref(), Some("dms"));
        clear_current(&d).unwrap();
        assert_eq!(read_current(&d).unwrap(), None);
        clear_current(&d).unwrap();
    }

    #[test]
    fn retire_moves_to_previous() {
        let (_t, d) = tmp_dirs();
        let r = sample();
        save_record(&d.record_json(&r.name), &r).unwrap();
        retire_to_previous(&d, "dms").unwrap();
        assert!(!d.record_json("dms").exists());
        assert!(d.previous_json().exists());
    }

    #[test]
    fn load_rejects_future_schema_version() {
        let (_t, d) = tmp_dirs();
        let path = d.record_json("x");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"schema_version":99,"name":"x","commit":"a","installed_at":"","symlink_path":"/","symlink_target":"/","pacman_diff":{}}"#,
        )
        .unwrap();
        assert!(load_record(&path).is_err());
    }
}
