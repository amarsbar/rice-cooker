use std::fs::{File, OpenOptions};
use std::path::Path;

use fs2::FileExt;

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

        match file.try_lock_exclusive() {
            Ok(()) => Ok(ApplyLock { file }),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    return Err(LockError::AlreadyHeld);
                }
                // On Linux EWOULDBLOCK == EAGAIN == 11; cover both regardless of
                // what ErrorKind fs2 surfaces on a given platform/version.
                if let Some(raw) = e.raw_os_error() {
                    if raw == libc::EWOULDBLOCK || raw == libc::EAGAIN {
                        return Err(LockError::AlreadyHeld);
                    }
                }
                Err(LockError::Io(e))
            }
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
    fn acquire_succeeds_on_fresh_path() {
        let dir = tempdir().unwrap();
        let lock_path = dir.path().join("apply.lock");
        let result = ApplyLock::try_acquire(&lock_path);
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[test]
    fn second_acquire_while_held_errors_already_held() {
        let dir = tempdir().unwrap();
        let lock_path = dir.path().join("apply.lock");
        let _first = ApplyLock::try_acquire(&lock_path).expect("first acquire must succeed");
        let second = ApplyLock::try_acquire(&lock_path);
        assert!(
            matches!(second, Err(LockError::AlreadyHeld)),
            "expected AlreadyHeld, got {second:?}"
        );
    }

    #[test]
    fn lock_released_after_drop() {
        let dir = tempdir().unwrap();
        let lock_path = dir.path().join("apply.lock");
        {
            let _lock = ApplyLock::try_acquire(&lock_path).expect("first acquire must succeed");
        } // _lock dropped here
        let second = ApplyLock::try_acquire(&lock_path);
        assert!(second.is_ok(), "expected Ok after drop, got {second:?}");
    }

    #[test]
    fn missing_parent_dir_errors_io_not_alreadyheld() {
        let dir = tempdir().unwrap();
        let lock_path = dir.path().join("nonexistent_dir").join("apply.lock");
        let result = ApplyLock::try_acquire(&lock_path);
        assert!(
            matches!(result, Err(LockError::Io(_))),
            "expected Io error, got {result:?}"
        );
    }
}
