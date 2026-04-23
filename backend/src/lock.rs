use std::fs::{File, OpenOptions};
use std::path::Path;

use fs4::fs_std::FileExt;

/// Process-held advisory lock. The lockfile persists on disk — it's a
/// rendezvous point, not a flag; stale content has no meaning.
pub struct ApplyLock {
    file: File,
}

impl std::fmt::Debug for ApplyLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApplyLock").finish_non_exhaustive()
    }
}

/// Distinguishes contention from real IO failure at the type level.
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
    /// Non-blocking. Parent directory must already exist.
    pub fn try_acquire(path: &Path) -> Result<Self, LockError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        match file.try_lock_exclusive() {
            Ok(true) => Ok(ApplyLock { file }),
            Ok(false) => Err(LockError::AlreadyHeld),
            Err(e) => Err(LockError::Io(e)),
        }
    }
}

impl Drop for ApplyLock {
    fn drop(&mut self) {
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
        let first = ApplyLock::try_acquire(&lock_path).expect("first acquire");
        let held = ApplyLock::try_acquire(&lock_path);
        assert!(
            matches!(held, Err(LockError::AlreadyHeld)),
            "expected AlreadyHeld, got {held:?}"
        );
        drop(first);
        assert!(ApplyLock::try_acquire(&lock_path).is_ok());
    }
}
