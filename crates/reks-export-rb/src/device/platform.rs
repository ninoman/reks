//! Thin OS calls behind [`super`]. Parsing lives in [`super::parse`].

use std::process::Command;

#[cfg(not(target_os = "windows"))]
use super::DeviceInfo;
use super::{parse, DeviceError, RawDisk};

/// Mounted volumes from `sysinfo::Disks`.
pub(super) fn raw_disks() -> Vec<RawDisk> {
    sysinfo::Disks::new_with_refreshed_list()
        .list()
        .iter()
        .map(|d| RawDisk {
            name: d.name().to_string_lossy().into_owned(),
            file_system: d.file_system().to_string_lossy().into_owned(),
            mount_point: d.mount_point().to_path_buf(),
            removable: d.is_removable(),
            total_bytes: d.total_space(),
            free_bytes: d.available_space(),
        })
        .collect()
}

/// Runs a command to completion and returns its stdout; a non-zero exit is
/// [`DeviceError::CommandFailed`]. GUI apps don't inherit the shell PATH
/// (R§6), so callers pass absolute program paths where they are fixed.
#[cfg_attr(target_os = "windows", allow(dead_code))]
fn run(program: &str, args: &[&str]) -> Result<String, DeviceError> {
    let command = format!("{program} {}", args.join(" "));
    tracing::debug!(%command, "running");
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|source| DeviceError::Spawn {
            command: command.clone(),
            source,
        })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(DeviceError::CommandFailed {
            command,
            status: out.status.to_string(),
            message: parse::command_message(&out.stderr, &out.stdout),
        })
    }
}

#[cfg(target_os = "macos")]
const DISKUTIL: &str = "/usr/sbin/diskutil";

#[cfg(target_os = "macos")]
pub(super) fn eject(device: &DeviceInfo) -> Result<(), DeviceError> {
    let mount = device.mount_point.to_string_lossy();
    run(DISKUTIL, &["eject", &mount]).map(|_| ())
}

#[cfg(target_os = "macos")]
pub(super) fn volume_serial(device: &DeviceInfo) -> Option<String> {
    let mount = device.mount_point.to_string_lossy();
    match run(DISKUTIL, &["info", "-plist", &mount]) {
        Ok(plist) => parse::diskutil_volume_uuid(&plist),
        Err(e) => {
            tracing::warn!(error = %e, "diskutil info failed");
            None
        }
    }
}

#[cfg(target_os = "linux")]
pub(super) fn eject(device: &DeviceInfo) -> Result<(), DeviceError> {
    // sysinfo's name is the block device on Linux. udisksctl works without
    // root on desktop sessions; `eject` is the fallback when it is missing.
    let dev = device.os_name.as_str();
    match run("udisksctl", &["unmount", "-b", dev]) {
        Ok(_) => run("udisksctl", &["power-off", "-b", dev]).map(|_| ()),
        Err(DeviceError::Spawn { .. }) => run("eject", &[dev]).map(|_| ()),
        Err(e) => Err(e),
    }
}

#[cfg(target_os = "linux")]
pub(super) fn volume_serial(device: &DeviceInfo) -> Option<String> {
    // /dev/disk/by-uuid needs no root; blkid's cache is the fallback.
    let dev = std::path::Path::new(&device.os_name);
    let dev = std::fs::canonicalize(dev).unwrap_or_else(|_| dev.to_path_buf());
    let entries: Vec<_> = std::fs::read_dir("/dev/disk/by-uuid")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let target = std::fs::canonicalize(e.path()).ok()?;
            Some((e.file_name().to_string_lossy().into_owned(), target))
        })
        .collect();
    parse::uuid_from_by_uuid(&entries, &dev).or_else(|| {
        let out = run("blkid", &["-s", "UUID", "-o", "value", &device.os_name]).ok()?;
        parse::first_line(&out)
    })
}

#[cfg(target_os = "windows")]
mod windows {
    #![allow(unsafe_code)] // Win32 FFI; each call is checked for failure.

    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetVolumeInformationW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Ioctl::{
        FSCTL_DISMOUNT_VOLUME, FSCTL_LOCK_VOLUME, IOCTL_STORAGE_EJECT_MEDIA,
        IOCTL_STORAGE_MEDIA_REMOVAL, PREVENT_MEDIA_REMOVAL,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    use super::super::{parse, DeviceError, DeviceInfo};

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    /// `E:\` → `E:`; `None` for mount points that are not drive letters.
    fn drive(device: &DeviceInfo) -> Option<String> {
        let s = device.mount_point.to_string_lossy();
        let mut chars = s.chars();
        match (chars.next(), chars.next()) {
            (Some(c), Some(':')) if c.is_ascii_alphabetic() => Some(format!("{c}:")),
            _ => None,
        }
    }

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: the handle came from a successful CreateFileW.
            unsafe { CloseHandle(self.0) };
        }
    }

    fn ioctl(h: &Handle, code: u32, input: *const core::ffi::c_void, len: u32) -> bool {
        let mut returned = 0u32;
        // SAFETY: `input` points to `len` readable bytes (or is null with
        // len 0); no output buffer is used.
        unsafe {
            DeviceIoControl(
                h.0,
                code,
                input,
                len,
                ptr::null_mut(),
                0,
                &mut returned,
                ptr::null_mut(),
            ) != 0
        }
    }

    /// Lock, dismount, allow removal, eject (the sequence Microsoft documents
    /// for ejecting removable media). UNVERIFIED on hardware.
    pub(in super::super) fn eject(device: &DeviceInfo) -> Result<(), DeviceError> {
        let fail = |step: &str| DeviceError::Os {
            mount_point: device.mount_point.clone(),
            // SAFETY: reads the calling thread's last-error value.
            message: format!("{step} failed (error {})", unsafe { GetLastError() }),
        };
        let drive = drive(device).ok_or(DeviceError::Unsupported)?;
        let path = wide(&format!(r"\\.\{drive}"));
        // SAFETY: `path` is NUL-terminated; other pointers are null as allowed.
        let raw = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if raw == INVALID_HANDLE_VALUE {
            return Err(fail("opening the volume"));
        }
        let h = Handle(raw);
        if !ioctl(&h, FSCTL_LOCK_VOLUME, ptr::null(), 0) {
            return Err(fail("locking the volume (files still open?)"));
        }
        if !ioctl(&h, FSCTL_DISMOUNT_VOLUME, ptr::null(), 0) {
            return Err(fail("dismounting the volume"));
        }
        let allow = PREVENT_MEDIA_REMOVAL {
            PreventMediaRemoval: false,
        };
        let size = std::mem::size_of::<PREVENT_MEDIA_REMOVAL>() as u32;
        if !ioctl(
            &h,
            IOCTL_STORAGE_MEDIA_REMOVAL,
            (&allow as *const PREVENT_MEDIA_REMOVAL).cast(),
            size,
        ) {
            return Err(fail("allowing media removal"));
        }
        if !ioctl(&h, IOCTL_STORAGE_EJECT_MEDIA, ptr::null(), 0) {
            return Err(fail("ejecting the media"));
        }
        Ok(())
    }

    pub(in super::super) fn volume_serial(device: &DeviceInfo) -> Option<String> {
        let root = wide(&format!(r"{}\", drive(device)?));
        let mut serial = 0u32;
        // SAFETY: `root` is NUL-terminated; we only ask for the serial, all
        // other out-pointers are null with zero sizes.
        let ok = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                ptr::null_mut(),
                0,
                &mut serial,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                0,
            )
        };
        (ok != 0).then(|| parse::format_serial32(serial))
    }
}

#[cfg(target_os = "windows")]
pub(super) use windows::{eject, volume_serial};

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(super) fn eject(_device: &DeviceInfo) -> Result<(), DeviceError> {
    Err(DeviceError::Unsupported)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(super) fn volume_serial(_device: &DeviceInfo) -> Option<String> {
    None
}
