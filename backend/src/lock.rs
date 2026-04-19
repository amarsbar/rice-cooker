use std::fs::{File, OpenOptions};
use std::path::Path;

use fs4::fs_std::FileExt;

/// A process-held exclusive lock tied to a lockfile path.
/// Dropping the `ApplyLock` releases the lock. The lockfile itself persists on disk;
/// that's fine — it's a rendezvous point, not a flag.
pub struct ApplyLock {
    file: File,
}

impl std::fmt::Debug for ApplyLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApplyLock").finish_non_exhaustive()
    }
}

/// Distinct error returned when the lock is held by another process.
/// This lets callers tell "someone else is already applying" apart from IO errors.
#[derive(Debug)]
pub enum LockError {
    AlreadyHeld,
    Io(std::io::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::AlreadyHeld => {
                write!(
                    f,
                    "another rice-cooker-backend process holds the apply lock"
                )
            }
            LockError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LockError::AlreadyHeld => None,
            LockError::Io(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for LockError {
    fn from(e: std::io::Error) -> Self {
        LockError::Io(e)
    }
}

impl ApplyLock {
    /// Try to acquire the lock non-blocking. Returns `LockError::AlreadyHeld` if another
    /// process currently holds it, or `LockError::Io` on underlying IO failure.
    /// Parent directories are NOT auto-created — caller ensures the parent exists.
    pub fn try_acquire(path: &Path) -> Result<Self, LockError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        // fs4 returns Ok(true) on acquired, Ok(false) on contention, Err on real IO failure.
        match file.try_lock_exclusive() {
            Ok(true) => Ok(ApplyLock { file }),
            Ok(false) => Err(LockError::AlreadyHeld),
            Err(e) => Err(LockError::Io(e)),
        }
    }
}

impl Drop for ApplyLock {
    fn drop(&mut self) {
        // Best-effort; ignore errors.
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn acquire_contend_drop_cycle() {
        let dir = tempdir().unwrap();
        let lock_path = dir.path().join("apply.lock");
        // Fresh acquire succeeds.
        let first = ApplyLock::try_acquire(&lock_path).expect("first acquire");
        // Second acquire while first is held → AlreadyHeld.
        let held = ApplyLock::try_acquire(&lock_path);
        assert!(
            matches!(held, Err(LockError::AlreadyHeld)),
            "expected AlreadyHeld, got {held:?}"
        );
        // Drop the first; a subsequent acquire succeeds.
        drop(first);
        assert!(ApplyLock::try_acquire(&lock_path).is_ok());
    }
}
