// Cross-process write lock. Several processes (the desktop app, the native
// messaging host, the CLI, MCP servers) can open the same vault; every
// mutating operation and every reindex runs under one exclusive advisory lock
// on `<vault>/.herbarium/lock`, so their read-modify-write cycles never
// interleave. The lock is per open file handle, so it also serialises two
// `Host`s inside one process.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// How long `acquire` waits for another process before giving up.
const WAIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(5);

/// Exclusive lock on a vault, released when dropped (or when the process dies).
#[derive(Debug)]
pub struct VaultLock {
    file: File,
}

impl VaultLock {
    /// Wait (bounded) for the vault's lock and hold it.
    pub fn acquire(vault: &Path) -> Result<VaultLock, String> {
        Self::acquire_within(vault, WAIT)
    }

    pub(crate) fn acquire_within(vault: &Path, wait: Duration) -> Result<VaultLock, String> {
        let dir = vault.join(".herbarium");
        fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let path = dir.join("lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        let deadline = Instant::now() + wait;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(VaultLock { file }),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => sleep(POLL),
                Err(TryLockError::WouldBlock) => {
                    return Err(
                        "the vault is busy: another Herbarium process holds its lock".into(),
                    );
                }
                Err(TryLockError::Error(e)) => {
                    return Err(format!("cannot lock {}: {e}", path.display()));
                }
            }
        }
    }
}

impl Drop for VaultLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_holder_waits_then_times_out_until_release() {
        let dir = std::env::temp_dir().join(format!("herbarium-lock-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let first = VaultLock::acquire(&dir).unwrap();
        let err = VaultLock::acquire_within(&dir, Duration::from_millis(30)).unwrap_err();
        assert!(err.contains("busy"), "{err}");
        drop(first);
        VaultLock::acquire_within(&dir, Duration::from_millis(30)).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }
}
