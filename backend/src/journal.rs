//! Install journal: records every filesystem side effect so uninstall can
//! invert them, and survives crashes via atomic temp+rename writes.
//!
//! Design trade-offs (see docs/install-design.md):
//! - Plain JSON, not SQLite — single-user single-process, atomic file swap.
//! - Flat Operation struct with `Option<T>` payload fields — serde-simple
//!   and future-additive without schema breaks.
//! - All state transitions (record/op) go through mutating methods on
//!   `Journal` that return `&mut Op` / `&mut Record` so callers can chain
//!   updates, then the caller persists by calling `save()`. The caller
//!   owns the "save after each step" discipline.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Journal {
    pub schema_version: u32,
    #[serde(default)]
    pub records: Vec<Record>,
}

impl Default for Journal {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            records: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub rice_id: String,
    pub state: RecordState,
    /// UNIX seconds when the record was last transitioned. Repurposed across
    /// installed/partial/uninstalled rather than adding a separate
    /// `uninstalled_at` — one timestamp is enough for v1's "when was this
    /// record last touched?" needs.
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_sha: Option<String>,
    #[serde(default)]
    pub operations: Vec<Op>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordState {
    /// Install is mid-flight — partial ops committed, no success event yet.
    /// A record in this state on next startup means the previous run crashed.
    Partial,
    /// Install completed cleanly.
    Installed,
    /// Uninstall ran; operations are reversed (each marked `RolledBack`).
    Uninstalled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Op {
    pub seq: u32,
    pub kind: OpKind,
    pub state: OpState,
    /// Target path of the side effect. Always set for symlink/copy/backup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abs_path: Option<String>,
    /// For `SymlinkCreate`: the path the symlink points at.
    /// For `BackupMove` when the original was itself a symlink: the symlink's
    /// own target (so restore can recreate it as a symlink, not a copy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<String>,
    /// For `BackupMove`: where the pre-existing dest was moved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    /// sha256 of the file at deploy time. For `CopyFile`, lets uninstall
    /// detect user modifications before deleting. For `BackupMove`, records
    /// the original content's hash (so restore can verify).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Unix file mode (permissions only, not file type bits). Preserved
    /// across backup/restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
    /// For `BackupMove`: whether the pre-existing entry was a symlink.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub was_symlink: Option<bool>,
    /// For `DirCreate`: whether the directory was created by us (so
    /// rollback can remove it). Missing = we didn't create it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<bool>,
    /// Free-form error detail for `Failed` state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    /// Move a pre-existing `abs_path` to `backup_path` so our deploy can take
    /// its place. Inverse: restore.
    BackupMove,
    /// Create a symlink at `abs_path` pointing at `symlink_target`. Inverse:
    /// remove iff readlink still matches `symlink_target`.
    SymlinkCreate,
    /// Copy a file into `abs_path`. Inverse: remove iff sha256 on disk
    /// matches (user hasn't modified).
    CopyFile,
    /// Create a directory. Inverse: rmdir iff empty.
    DirCreate,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpState {
    Started,
    Committed,
    /// Ran, tried, failed. Details in `error`.
    Failed,
    /// Uninstall successfully inverted this op (symlink removed, backup
    /// restored, etc.).
    RolledBack,
    /// Uninstall tried to invert but couldn't (user modified file; symlink
    /// target diverged). Left in place; details in `error`. Distinguished
    /// from `Failed` so a doctor command can surface user-modified files
    /// without confusing them with install-time errors.
    RollbackSkipped,
}

impl Journal {
    /// Load the journal from disk. A missing file is fresh (empty). A
    /// malformed/partially-written file is treated as "journal missing" for
    /// now — the next install will overwrite. This is a deliberate trade-off:
    /// preserving a corrupt journal would block all future installs until the
    /// user edited JSON by hand. Partial-write protection lives in `save()`
    /// via temp+rename, so in practice we should never read a torn file.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(s) if s.trim().is_empty() => Ok(Self::default()),
            Ok(s) => match serde_json::from_str::<Self>(&s) {
                Ok(j) => Ok(j),
                Err(_) => Ok(Self::default()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading journal {}", path.display())),
        }
    }

    /// Atomic temp-file + rename. Parent must exist. Written as
    /// pretty-printed JSON so a human can grep/read it; the tool doesn't
    /// care either way.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut tmp = path.as_os_str().to_os_string();
        tmp.push(".tmp");
        let tmp_path = PathBuf::from(tmp);
        let body = serde_json::to_string_pretty(self).context("serializing journal")?;
        // Write + fsync the file, then rename. We don't fsync the parent
        // directory after rename — a v1 compromise that matches cache.rs's
        // write_line_file. For journal integrity under power loss, upgrade
        // later.
        let mut f = fs::File::create(&tmp_path)
            .with_context(|| format!("creating journal tmp {}", tmp_path.display()))?;
        f.write_all(body.as_bytes())
            .with_context(|| format!("writing journal tmp {}", tmp_path.display()))?;
        f.sync_all().ok(); // Best effort; some FSes no-op.
        drop(f);
        fs::rename(&tmp_path, path).with_context(|| {
            format!("renaming {} -> {}", tmp_path.display(), path.display())
        })?;
        Ok(())
    }

    /// Find the first record for `rice_id` whose state is not `Uninstalled`,
    /// i.e. the currently-active record. Partial records count as active —
    /// they represent a crash that needs either completion or rollback.
    pub fn active_record(&self, rice_id: &str) -> Option<&Record> {
        self.records
            .iter()
            .rev()
            .find(|r| r.rice_id == rice_id && r.state != RecordState::Uninstalled)
    }

    pub fn active_record_mut(&mut self, rice_id: &str) -> Option<&mut Record> {
        self.records
            .iter_mut()
            .rev()
            .find(|r| r.rice_id == rice_id && r.state != RecordState::Uninstalled)
    }

    /// Begin a new record in `Partial` state. Returns a mutable reference so
    /// the caller can append ops. Panics if a non-uninstalled record already
    /// exists for this rice — caller should check via `active_record` first
    /// and decide (error, prompt, or resume).
    pub fn begin_record(&mut self, rice_id: String, source_sha: Option<String>) -> &mut Record {
        assert!(
            self.active_record(&rice_id).is_none(),
            "begin_record called while record for {rice_id:?} is still active; \
             caller must consume/finish the existing record first"
        );
        self.records.push(Record {
            rice_id,
            state: RecordState::Partial,
            updated_at: now_seconds(),
            source_sha,
            operations: Vec::new(),
        });
        self.records.last_mut().unwrap()
    }
}

impl Record {
    /// Append a fresh op in `Started` state and return a mutable reference.
    /// The caller should flip it to `Committed` (or `Failed`) after the
    /// side effect lands.
    pub fn append_op(&mut self, kind: OpKind) -> &mut Op {
        let seq = self.operations.len() as u32;
        self.operations.push(Op {
            seq,
            kind,
            state: OpState::Started,
            abs_path: None,
            symlink_target: None,
            backup_path: None,
            sha256: None,
            mode: None,
            was_symlink: None,
            created: None,
            error: None,
        });
        self.operations.last_mut().unwrap()
    }

    /// Mark the record `Installed`, bumping `updated_at`. Intended as the
    /// last step of a successful install pipeline — asserting that the
    /// record is currently `Partial` catches double-commit bugs.
    pub fn mark_installed(&mut self) {
        assert_eq!(
            self.state,
            RecordState::Partial,
            "mark_installed expected Partial, found {:?}",
            self.state
        );
        self.state = RecordState::Installed;
        self.updated_at = now_seconds();
    }

    pub fn mark_uninstalled(&mut self) {
        self.state = RecordState::Uninstalled;
        self.updated_at = now_seconds();
    }
}

pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Resolve the journal file path. Precedence:
/// 1. `RICE_COOKER_STATE_DIR` (test override / custom install)
/// 2. `$XDG_STATE_HOME/rice-cooker/installs.json`
/// 3. `$HOME/.local/state/rice-cooker/installs.json`
pub fn resolve_journal_path_with(
    rice_cooker_state_dir: Option<&str>,
    xdg_state_home: Option<&str>,
    home: Option<&str>,
) -> Result<PathBuf> {
    const FILE: &str = "installs.json";
    if let Some(dir) = non_empty(rice_cooker_state_dir) {
        return Ok(PathBuf::from(dir).join(FILE));
    }
    if let Some(dir) = non_empty(xdg_state_home) {
        return Ok(PathBuf::from(dir).join("rice-cooker").join(FILE));
    }
    if let Some(dir) = non_empty(home) {
        return Ok(PathBuf::from(dir)
            .join(".local/state/rice-cooker")
            .join(FILE));
    }
    Err(anyhow!(
        "cannot resolve journal path: set RICE_COOKER_STATE_DIR, XDG_STATE_HOME, or HOME"
    ))
}

pub fn resolve_journal_path() -> Result<PathBuf> {
    let rc = std::env::var("RICE_COOKER_STATE_DIR").ok();
    let xdg = std::env::var("XDG_STATE_HOME").ok();
    let home = std::env::var("HOME").ok();
    resolve_journal_path_with(rc.as_deref(), xdg.as_deref(), home.as_deref())
}

fn non_empty(s: Option<&str>) -> Option<&str> {
    s.filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn tmp_journal_path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().unwrap();
        let p = dir.path().join("installs.json");
        (dir, p)
    }

    #[test]
    fn load_missing_and_empty_both_return_default() {
        let (_d, p) = tmp_journal_path();
        // Missing.
        let j = Journal::load(&p).unwrap();
        assert_eq!(j.records.len(), 0);
        assert_eq!(j.schema_version, SCHEMA_VERSION);
        // Empty file.
        std::fs::write(&p, "").unwrap();
        let j = Journal::load(&p).unwrap();
        assert!(j.records.is_empty());
        // Malformed file — graceful degradation.
        std::fs::write(&p, "{{ not json").unwrap();
        let j = Journal::load(&p).unwrap();
        assert!(j.records.is_empty());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let (_d, p) = tmp_journal_path();
        let mut j = Journal::default();
        let rec = j.begin_record("caelestia".into(), Some("abc123".into()));
        let op = rec.append_op(OpKind::SymlinkCreate);
        op.abs_path = Some("/home/x/.config/quickshell/caelestia".into());
        op.symlink_target = Some("/home/x/.cache/rice-cooker/rices/caelestia/quickshell".into());
        op.state = OpState::Committed;
        rec.mark_installed();
        j.save(&p).unwrap();

        let back = Journal::load(&p).unwrap();
        assert_eq!(back.records.len(), 1);
        let r = &back.records[0];
        assert_eq!(r.rice_id, "caelestia");
        assert_eq!(r.state, RecordState::Installed);
        assert_eq!(r.source_sha.as_deref(), Some("abc123"));
        assert_eq!(r.operations.len(), 1);
        assert_eq!(r.operations[0].kind, OpKind::SymlinkCreate);
        assert_eq!(r.operations[0].state, OpState::Committed);
    }

    #[test]
    fn save_is_atomic_no_dot_tmp_leftover_on_success() {
        let (_d, p) = tmp_journal_path();
        let j = Journal::default();
        j.save(&p).unwrap();
        let tmp = p.with_extension("json.tmp");
        assert!(!tmp.exists(), "temp file should have been renamed");
    }

    #[test]
    fn active_record_finds_latest_non_uninstalled() {
        let mut j = Journal::default();
        {
            let r = j.begin_record("A".into(), None);
            r.mark_installed();
        }
        {
            let r = j.begin_record("B".into(), None);
            r.mark_installed();
            r.mark_uninstalled();
        }
        assert_eq!(j.active_record("A").unwrap().state, RecordState::Installed);
        // B was uninstalled — no active record.
        assert!(j.active_record("B").is_none());
        // Now reinstall B.
        {
            let r = j.begin_record("B".into(), None);
            r.mark_installed();
        }
        // The active record is the most recent one (the reinstall).
        let b = j.active_record("B").unwrap();
        assert_eq!(b.state, RecordState::Installed);
        // Two B records total — one uninstalled, one active.
        assert_eq!(j.records.iter().filter(|r| r.rice_id == "B").count(), 2);
    }

    #[test]
    #[should_panic(expected = "active")]
    fn begin_record_panics_on_overlapping_partial() {
        let mut j = Journal::default();
        j.begin_record("A".into(), None);
        // Leaving it Partial — a second begin should panic.
        j.begin_record("A".into(), None);
    }

    #[test]
    fn op_seq_is_monotonic_within_a_record() {
        let mut j = Journal::default();
        let r = j.begin_record("x".into(), None);
        assert_eq!(r.append_op(OpKind::BackupMove).seq, 0);
        assert_eq!(r.append_op(OpKind::SymlinkCreate).seq, 1);
        assert_eq!(r.append_op(OpKind::CopyFile).seq, 2);
    }

    #[test]
    fn resolve_journal_path_respects_precedence() {
        assert_eq!(
            resolve_journal_path_with(Some("/s"), Some("/x"), Some("/h")).unwrap(),
            PathBuf::from("/s/installs.json")
        );
        assert_eq!(
            resolve_journal_path_with(None, Some("/x"), Some("/h")).unwrap(),
            PathBuf::from("/x/rice-cooker/installs.json")
        );
        assert_eq!(
            resolve_journal_path_with(None, None, Some("/h")).unwrap(),
            PathBuf::from("/h/.local/state/rice-cooker/installs.json")
        );
        // Empty env var is treated as unset.
        assert_eq!(
            resolve_journal_path_with(Some(""), None, Some("/h")).unwrap(),
            PathBuf::from("/h/.local/state/rice-cooker/installs.json")
        );
        assert!(resolve_journal_path_with(None, None, None).is_err());
    }

    #[test]
    fn schema_survives_missing_optional_fields() {
        // Minimal on-disk form — only the required fields. Must deserialize
        // successfully (default-filled) so older journals keep working after
        // additive schema bumps.
        let raw = r#"{
          "schema_version": 1,
          "records": [
            {"rice_id":"x","state":"installed","updated_at":0,"operations":[]}
          ]
        }"#;
        let j: Journal = serde_json::from_str(raw).unwrap();
        let r = &j.records[0];
        assert!(r.source_sha.is_none());
        assert!(r.operations.is_empty());
    }
}
