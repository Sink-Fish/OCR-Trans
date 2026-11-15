//! Keep login launch and manual launch from owning the same hotkeys at once.
//! The OS releases this lock even after a crash. Never unlink the lock file:
//! a second inode would let two processes both acquire it.
use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

pub struct InstanceGuard {
    _file: File,
}

pub fn acquire(dir: &Path) -> std::io::Result<Option<InstanceGuard>> {
    std::fs::create_dir_all(dir)?;
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(dir.join("instance.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(InstanceGuard { _file: file })),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_owner_and_exit_releases_lock() {
        let dir = std::env::temp_dir().join(format!("ocrtrans-instance-test-{}", std::process::id()));
        let first = acquire(&dir).unwrap().unwrap();
        assert!(acquire(&dir).unwrap().is_none());
        drop(first);
        let next = acquire(&dir).unwrap().unwrap();
        drop(next);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
