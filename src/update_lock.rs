use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// A shared lock preventing an update from replacing files used by this process.
///
/// The lock is released by the kernel when this value is dropped or the process
/// exits. The lock file is intentionally left in place so every participant
/// continues to address the same inode.
pub struct RuntimeLock {
    _file: File,
}

impl RuntimeLock {
    /// Acquires the runtime side of the update handshake without waiting.
    pub fn acquire() -> io::Result<Self> {
        lock_at(&path(), false)
    }
}

/// Returns the stable lock-file path for the current XDG data profile.
pub fn path() -> PathBuf {
    crate::store::xdg_path("XDG_DATA_HOME", ".local/share").join("gnome-clip-notes/update.lock")
}

fn lock_at(path: &Path, exclusive: bool) -> io::Result<RuntimeLock> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "update lock has no parent"))?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "update lock is not a regular file",
        ));
    }
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "update lock is owned by another user",
        ));
    }
    if metadata.nlink() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "update lock has multiple hard links",
        ));
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))?;

    let operation = if exclusive {
        libc::LOCK_EX
    } else {
        libc::LOCK_SH
    } | libc::LOCK_NB;
    loop {
        let result = unsafe { libc::flock(file.as_raw_fd(), operation) };
        if result == 0 {
            return Ok(RuntimeLock { _file: file });
        }
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            continue;
        }
        if error.raw_os_error() == Some(libc::EWOULDBLOCK)
            || error.raw_os_error() == Some(libc::EAGAIN)
        {
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        return Err(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn test_path(label: &str) -> PathBuf {
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!("gcn-update-lock-{}-{unique}", std::process::id()))
            .join(label)
            .join("update.lock")
    }

    fn error_kind(result: io::Result<RuntimeLock>) -> io::ErrorKind {
        match result {
            Ok(_) => panic!("lock unexpectedly succeeded"),
            Err(error) => error.kind(),
        }
    }

    #[test]
    fn readers_coexist_and_exclude_writer_until_release() {
        let path = test_path("shared readers");
        let first = lock_at(&path, false).unwrap();
        let second = lock_at(&path, false).unwrap();

        assert_eq!(error_kind(lock_at(&path, true)), io::ErrorKind::WouldBlock);
        drop(first);
        assert_eq!(error_kind(lock_at(&path, true)), io::ErrorKind::WouldBlock);
        drop(second);

        let writer = lock_at(&path, true).unwrap();
        assert_eq!(error_kind(lock_at(&path, false)), io::ErrorKind::WouldBlock);
        drop(writer);
        lock_at(&path, false).unwrap();
    }

    #[test]
    fn rejects_symlink() {
        let path = test_path("symlink");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let target = path.with_file_name("target");
        File::create(&target).unwrap();
        symlink(&target, &path).unwrap();

        assert!(lock_at(&path, false).is_err());
    }

    #[test]
    fn rejects_non_regular_path_without_blocking() {
        let path = test_path("fifo");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let path_bytes = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path_bytes.as_ptr(), 0o600) }, 0);

        assert_eq!(
            error_kind(lock_at(&path, false)),
            io::ErrorKind::InvalidData
        );
    }
}
