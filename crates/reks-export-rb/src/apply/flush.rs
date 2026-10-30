//! Platform flush calls for apply step 6 ([`super::sync_written`]).
//!
//! Unix opens files read-only: a write-open, even one that writes nothing,
//! makes macOS stamp `com.apple.provenance`, which its FAT driver stores as a
//! `._<name>` file beside every flushed file. On Apple `File::sync_all` is
//! `F_FULLFSYNC`, a whole-drive cache flush; per file that is ~25x slower
//! than `fsync` (docs/research/perf-audit-2026-10-30.md, P1-1/P1-2), so files
//! get a plain `fsync` and the drive cache is flushed once by
//! [`flush_volume`].

#![allow(unsafe_code)] // libc FFI; each call's result is checked.

use std::fs::File;
use std::io;
use std::path::Path;

/// Pushes one file's data and metadata to the device.
#[cfg(unix)]
pub(super) fn sync_file(path: &Path) -> io::Result<()> {
    fsync(&File::open(path)?)
}

/// Pushes one file's data to the device. FlushFileBuffers needs write
/// access; nothing is written.
#[cfg(not(unix))]
pub(super) fn sync_file(path: &Path) -> io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)?
        .sync_all()
}

/// Pushes a directory's entries to the device. Some file systems refuse.
#[cfg(unix)]
pub(super) fn sync_dir(path: &Path) -> io::Result<()> {
    fsync(&File::open(path)?)
}

/// Pushes a directory's entries to the device. Some file systems refuse.
#[cfg(not(unix))]
pub(super) fn sync_dir(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

/// Flushes the drive's write cache for the volume holding `anchor`, a file
/// on it. Call after every [`sync_file`]: only then is all of it on the media.
#[cfg(target_vendor = "apple")]
pub(super) fn flush_volume(anchor: &Path) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let file = File::open(anchor)?;
    // SAFETY: the descriptor is open for the call; F_FULLFSYNC takes no
    // argument and touches no memory of ours.
    retry_eintr(|| unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) })
}

/// Elsewhere [`sync_file`] already reaches the device: Linux `fsync` and
/// Windows FlushFileBuffers flush the drive cache for the file.
#[cfg(not(target_vendor = "apple"))]
pub(super) fn flush_volume(_anchor: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn fsync(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: the descriptor is open for the call.
    retry_eintr(|| unsafe { libc::fsync(file.as_raw_fd()) })
}

#[cfg(unix)]
fn retry_eintr(mut call: impl FnMut() -> libc::c_int) -> io::Result<()> {
    loop {
        if call() != -1 {
            return Ok(());
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn read_only_files_sync_without_a_write_open() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("export.pdb");
        std::fs::write(&path, b"pdb").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
        // A write-open would be refused (unless running as root).
        sync_file(&path).unwrap();
        sync_dir(dir.path()).unwrap();
        flush_volume(&path).unwrap();
    }

    #[test]
    fn a_missing_file_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let err = sync_file(&dir.path().join("gone")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}
