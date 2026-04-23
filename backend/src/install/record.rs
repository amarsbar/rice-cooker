//! Install record: the JSON file at
//! `~/.local/share/rice-cooker/installs/<name>.json` + the `current.json`
//! pointer to the active rice.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
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
            .expect("RFC3339 formatting of OffsetDateTime::now_utc cannot fail")
    }
}

pub fn save_record(path: &Path, r: &InstallRecord) -> Result<()> {
    let body = serde_json::to_string_pretty(r).context("serializing install record")?;
    atomic_write_fsync(path, body.as_bytes())
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
    atomic_write_fsync(&dirs.current_json(), body.as_bytes())
}

/// Write `body` to `path` atomically and durably: write-to-tmp, fsync
/// the tmp file, rename over `path`, then fsync the parent directory so
/// the rename itself survives power loss.
///
/// Without the file fsync, a crash between rename and kernel writeback
/// can leave a zero-byte record file at `path`; without the directory
/// fsync, the rename may be lost and the content only visible under
/// the `.tmp` name. Parent-dir fsync failure doesn't abort: by that
/// point the file itself is durable at the new name, and returning
/// Err here would desync `save_record` → `write_current` ordering
/// (record on disk, current.json skipped, packages orphaned). We warn
/// to stderr and continue.
///
/// On any earlier error, the `.tmp` file is best-effort unlinked so
/// failures don't litter the installs dir.
fn atomic_write_fsync(path: &Path, body: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{}: no parent directory", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);

    let write_then_rename = || -> Result<()> {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)
            .with_context(|| format!("opening {}", tmp.display()))?;
        f.write_all(body)
            .with_context(|| format!("writing {}", tmp.display()))?;
        f.sync_all()
            .with_context(|| format!("fsync {}", tmp.display()))?;
        drop(f);
        fs::rename(&tmp, path)
            .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()))
    };

    if let Err(e) = write_then_rename() {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    match fs::File::open(parent) {
        Ok(dir) => {
            if let Err(e) = dir.sync_all() {
                eprintln!(
                    "rice-cooker: warn: fsync {}: {e} (file content is durable; rename may not survive power loss)",
                    parent.display()
                );
            }
        }
        Err(e) => {
            eprintln!(
                "rice-cooker: warn: open {} for fsync: {e} (file content is durable; rename may not survive power loss)",
                parent.display()
            );
        }
    }
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
