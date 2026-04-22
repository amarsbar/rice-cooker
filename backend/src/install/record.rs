//! Install record: the JSON file at
//! `~/.local/share/rice-cooker/installs/<name>.json` plus the `current.json`
//! "what's installed now?" pointer.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::catalog::Shape;

use super::diff::FsDiff;
use super::env::Dirs;

pub const SCHEMA_VERSION: u32 = 1;

fn default_shape() -> Shape {
    Shape::Dotfiles
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallRecord {
    pub schema_version: u32,
    pub name: String,
    /// Pinned catalog commit.
    pub commit: String,
    /// Install shape this record was written with. Defaults to `Dotfiles`
    /// for backwards compatibility with pre-shape records.
    #[serde(default = "default_shape")]
    pub shape: Shape,
    /// `Symlink` shape only: the `ln -sfnT` destination we created. Empty
    /// for dotfiles. Uninstall removes this symlink.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symlink_path: Option<PathBuf>,
    /// `Symlink` shape only: what the symlink points at. Informational.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<PathBuf>,
    /// BLAKE3 of the catalog entry's TOML, so `status` can tell if the
    /// catalog changed since this rice was installed.
    pub catalog_entry_hash: String,
    /// RFC3339 timestamp.
    pub installed_at: String,
    pub install_cmd: String,
    pub exit_code: i32,
    /// True if install.sh exited non-zero; we still wrote this record so
    /// uninstall has the state to reverse.
    pub partial: bool,
    pub fs_diff: FsDiff,
    pub pacman_diff: PacmanDiff,
    /// Catalog's `partial_ownership` and `runtime_regenerated` paths, HOME-
    /// expanded at record time. Uninstall reads these to apply per-path
    /// reversal rules without re-reading the catalog (catalog may have
    /// changed by then).
    #[serde(default)]
    pub partial_ownership_paths: Vec<PathBuf>,
    #[serde(default)]
    pub runtime_regenerated_paths: Vec<PathBuf>,
    /// Systemd user units enabled during this install (detected via
    /// `~/.config/systemd/user/**/*.target.wants/*.service` symlinks in the
    /// fs_diff).
    #[serde(default)]
    pub systemd_units_enabled: Vec<String>,
    /// Paths whose pre-install content couldn't be trusted for restore
    /// (TOCTOU race during pre-content capture, permission denied, etc).
    /// Uninstall SKIPS restore for these paths and leaves a message so the
    /// user knows. Empty in the common case.
    #[serde(default)]
    pub unrestorable_paths: Vec<PathBuf>,
    /// True when this record was written by `write_partial_crashed_record`
    /// — i.e. install_cmd ran but the pipeline crashed before we could
    /// take the post-snapshot, so `fs_diff` is empty-by-omission rather
    /// than empty-because-install-did-nothing. Uninstall consults this to
    /// surface a loud "files may remain in HOME" warning and preserve
    /// snapshot_dir for manual recovery. Defaults to false for backwards
    /// compatibility with records written before this field existed.
    #[serde(default)]
    pub crash_recovery: bool,
    /// Mid-uninstall phase tracker. Populated by the dotfiles uninstall
    /// path after each phase completes. On retry, skip past completed
    /// phases (all phases are individually idempotent, but skipping
    /// avoids unnecessary work and wrong-state surprises). `None`
    /// when the record has never been through uninstall.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uninstall_phase: Option<UninstallPhase>,
    /// Path to the log file that captured install.sh stdout+stderr.
    pub log_path: PathBuf,
}

/// Phases of dotfiles uninstall, stamped into the record at each
/// completion so retry-after-crash can skip finished work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UninstallPhase {
    /// `pacman -Rns` complete; continue with systemd disable.
    Pacman,
    /// `systemctl --user disable` complete; continue with fs-diff reversal.
    Systemd,
    /// `fs_diff` reversed; continue with cache-dir cleanup.
    FsDiff,
    /// Cache dirs cleaned; continue with record retirement (last step).
    Cleanup,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PacmanDiff {
    #[serde(default)]
    pub added_explicit: Vec<String>,
    #[serde(default)]
    pub removed_explicit: Vec<String>,
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

/// Atomically mark a rice as the currently-installed one. Writes
/// `current.json` with the record's name inside — uninstall / status read
/// this to find the active rice without scanning the installs dir.
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

/// In-progress marker schema: written at install start, deleted on
/// successful record write. Its presence at startup means a prior install
/// crashed mid-way; the user must run `rice-cooker cleanup` first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InProgress {
    pub name: String,
    pub shape: crate::catalog::Shape,
    pub started_at: String,
}

pub fn write_in_progress(dirs: &Dirs, name: &str, shape: crate::catalog::Shape) -> Result<()> {
    let marker = InProgress {
        name: name.to_string(),
        shape,
        started_at: InstallRecord::now_rfc3339(),
    };
    let body = serde_json::to_string_pretty(&marker).context("serializing in-progress marker")?;
    let path = dirs.in_progress_json();
    fs::create_dir_all(dirs.installs_dir())
        .with_context(|| format!("creating {}", dirs.installs_dir().display()))?;
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, body).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, &path)
        .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

pub fn read_in_progress(dirs: &Dirs) -> Result<Option<InProgress>> {
    match fs::read_to_string(dirs.in_progress_json()) {
        Ok(s) => {
            let m: InProgress = serde_json::from_str(&s).context("parsing .in-progress.json")?;
            Ok(Some(m))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("reading .in-progress.json"),
    }
}

pub fn clear_in_progress(dirs: &Dirs) -> Result<()> {
    match fs::remove_file(dirs.in_progress_json()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).context("removing .in-progress.json"),
    }
}

/// Move `<name>.json` to `previous.json`, overwriting any existing one.
/// Called from uninstall to retire the record.
pub fn retire_to_previous(dirs: &Dirs, name: &str) -> Result<()> {
    let from = dirs.record_json(name);
    let to = dirs.previous_json();
    // If the record is missing entirely, nothing to retire.
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

    fn sample_record() -> InstallRecord {
        InstallRecord {
            schema_version: SCHEMA_VERSION,
            name: "caelestia".into(),
            commit: "a3f4b2c9e1d7".into(),
            shape: Shape::Dotfiles,
            symlink_path: None,
            symlink_target: None,
            catalog_entry_hash: "HASH".into(),
            installed_at: InstallRecord::now_rfc3339(),
            install_cmd: "./install.fish".into(),
            exit_code: 0,
            partial: false,
            fs_diff: FsDiff::default(),
            pacman_diff: PacmanDiff::default(),
            partial_ownership_paths: vec![],
            runtime_regenerated_paths: vec![],
            systemd_units_enabled: vec![],
            unrestorable_paths: vec![],
            crash_recovery: false,
            uninstall_phase: None,
            log_path: PathBuf::from("/tmp/log"),
        }
    }

    #[test]
    fn record_round_trips_through_json() {
        let (_t, d) = tmp_dirs();
        let r = sample_record();
        let path = d.record_json(&r.name);
        save_record(&path, &r).unwrap();
        let back = load_record(&path).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn current_json_roundtrip() {
        let (_t, d) = tmp_dirs();
        assert_eq!(read_current(&d).unwrap(), None);
        write_current(&d, "caelestia").unwrap();
        assert_eq!(read_current(&d).unwrap().as_deref(), Some("caelestia"));
        clear_current(&d).unwrap();
        assert_eq!(read_current(&d).unwrap(), None);
        // Idempotent.
        clear_current(&d).unwrap();
    }

    #[test]
    fn retire_moves_to_previous() {
        let (_t, d) = tmp_dirs();
        let r = sample_record();
        save_record(&d.record_json(&r.name), &r).unwrap();
        assert!(d.record_json("caelestia").exists());
        retire_to_previous(&d, "caelestia").unwrap();
        assert!(!d.record_json("caelestia").exists());
        assert!(d.previous_json().exists());
    }

    #[test]
    fn load_rejects_future_schema_version() {
        let (_t, d) = tmp_dirs();
        let path = d.record_json("x");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"schema_version":99,"name":"x","commit":"a","catalog_entry_hash":"","installed_at":"","install_cmd":"","exit_code":0,"partial":false,"fs_diff":{},"pacman_diff":{},"log_path":"/"}"#,
        )
        .unwrap();
        assert!(load_record(&path).is_err());
    }
}
