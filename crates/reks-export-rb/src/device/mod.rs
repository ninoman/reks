//! Export targets: mounted volumes, their file system, eject and volume serial.
//!
//! Discovery wraps `sysinfo::Disks` (D16). Everything that turns OS output into
//! our types is pure and lives in [`parse`]; the code that calls the OS lives
//! in [`platform`] and stays thin. See `docs/phases/phase-2.md` task 1.

pub mod parse;
mod platform;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// File system family of a volume, as far as the export cares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FsKind {
    /// FAT family. macOS (`msdos`) and Linux (`vfat`) do not report the FAT
    /// width, so FAT12/16 volumes land here too.
    Fat32,
    ExFat,
    /// HFS/HFS+ (Mac OS Extended), readable by some players only.
    Hfs,
    /// Anything else (APFS, NTFS, ext4, ...), with the OS's name for it.
    Other(String),
}

/// One mounted volume that an export could target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Where the volume is mounted; the export's device root.
    pub mount_point: PathBuf,
    pub fs: FsKind,
    /// The OS's file system string, for messages (`msdos`, `vfat`, `FAT32`).
    pub fs_name: String,
    /// The OS reports removable media. Windows uses `GetDriveTypeW`, so some
    /// USB SSDs report fixed (R§6).
    pub removable: bool,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// User-visible volume name; may be empty.
    pub label: String,
    /// What the OS calls the volume: a device node on Linux (`/dev/sdb1`),
    /// the volume name on macOS and Windows. Used for eject and serial lookup.
    pub os_name: String,
}

/// One entry from the OS volume list, before normalisation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDisk {
    pub name: String,
    pub file_system: String,
    pub mount_point: PathBuf,
    pub removable: bool,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// Operating system, for the parts of normalisation that differ per OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
    Linux,
    Other,
}

impl Os {
    /// The OS this binary was built for.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else if cfg!(target_os = "linux") {
            Os::Linux
        } else {
            Os::Other
        }
    }
}

impl DeviceInfo {
    /// Normalises an OS volume entry.
    pub fn from_raw(raw: RawDisk, os: Os) -> Self {
        let label = parse::label_for(&raw.name, &raw.mount_point, os);
        DeviceInfo {
            fs: parse::normalize_fs(&raw.file_system),
            fs_name: raw.file_system,
            removable: raw.removable,
            total_bytes: raw.total_bytes,
            free_bytes: raw.free_bytes,
            label,
            os_name: raw.name,
            mount_point: raw.mount_point,
        }
    }
}

/// Device discovery and eject errors.
#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("no mounted volume contains {0}")]
    NotFound(PathBuf),
    #[error("{0} is not a removable volume; refusing to eject it")]
    NotRemovable(PathBuf),
    #[error("ejecting is not supported on this operating system")]
    Unsupported,
    #[error("could not run {command}: {source}")]
    Spawn {
        command: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{command} failed ({status}): {message}")]
    CommandFailed {
        command: String,
        status: String,
        message: String,
    },
    #[error("eject of {mount_point} failed: {message}")]
    Os {
        mount_point: PathBuf,
        message: String,
    },
}

/// All mounted volumes, removable or not.
pub fn list_devices() -> Vec<DeviceInfo> {
    let os = Os::current();
    platform::raw_disks()
        .into_iter()
        .map(|raw| DeviceInfo::from_raw(raw, os))
        .collect()
}

/// Mounted volumes the OS reports as removable: the export picker's list.
pub fn list_removable() -> Vec<DeviceInfo> {
    list_devices().into_iter().filter(|d| d.removable).collect()
}

/// The volume that holds `path` (longest mount-point prefix wins).
pub fn device_for_path(path: &Path) -> Result<DeviceInfo, DeviceError> {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    parse::containing_device(&list_devices(), &path)
        .cloned()
        .ok_or(DeviceError::NotFound(path))
}

/// Unmounts and ejects a removable volume so the stick can be pulled.
///
/// macOS: `diskutil eject`; Windows: lock, dismount and eject IOCTLs; Linux:
/// `udisksctl unmount` + `power-off`, falling back to `eject`. Refuses
/// volumes the OS does not report as removable. UNVERIFIED on Windows and
/// Linux hardware.
pub fn eject(device: &DeviceInfo) -> Result<(), DeviceError> {
    if !device.removable {
        return Err(DeviceError::NotRemovable(device.mount_point.clone()));
    }
    tracing::info!(mount_point = %device.mount_point.display(), "ejecting volume");
    platform::eject(device)
}

/// The volume's serial or UUID, used to key stick backups and the export
/// manifest (task 12, D23). `None` when the OS doesn't tell us.
///
/// UNVERIFIED (R§8 item 14). The values differ per OS for the same stick:
/// Windows and Linux give the FAT/exFAT serial (`4993-1CFB`), macOS gives
/// `diskutil`'s `VolumeUUID`, which is derived from it. Reformatting gives a
/// new serial, so a reformatted stick counts as a new one.
pub fn volume_serial(device: &DeviceInfo) -> Option<String> {
    let serial = platform::volume_serial(device);
    if serial.is_none() {
        tracing::warn!(mount_point = %device.mount_point.display(), "no volume serial");
    }
    serial
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_raw_normalises_a_linux_stick() {
        let raw = RawDisk {
            name: "/dev/sdb1".into(),
            file_system: "vfat".into(),
            mount_point: "/media/dj/REKS".into(),
            removable: true,
            total_bytes: 32_000_000_000,
            free_bytes: 31_000_000_000,
        };
        let dev = DeviceInfo::from_raw(raw, Os::Linux);
        assert_eq!(dev.fs, FsKind::Fat32);
        assert_eq!(dev.fs_name, "vfat");
        assert_eq!(dev.label, "REKS");
        assert_eq!(dev.os_name, "/dev/sdb1");
        assert!(dev.removable);
        assert_eq!(dev.free_bytes, 31_000_000_000);
    }

    #[test]
    fn device_info_round_trips_through_json() -> Result<(), serde_json::Error> {
        let dev = DeviceInfo {
            mount_point: "/Volumes/REKS".into(),
            fs: FsKind::Other("apfs".into()),
            fs_name: "apfs".into(),
            removable: false,
            total_bytes: 1,
            free_bytes: 0,
            label: "REKS".into(),
            os_name: "REKS".into(),
        };
        let back: DeviceInfo = serde_json::from_str(&serde_json::to_string(&dev)?)?;
        assert_eq!(back, dev);
        Ok(())
    }

    #[test]
    fn eject_refuses_a_non_removable_volume() {
        let dir = std::env::temp_dir();
        let dev = DeviceInfo {
            mount_point: dir.clone(),
            fs: FsKind::Fat32,
            fs_name: "msdos".into(),
            removable: false,
            total_bytes: 0,
            free_bytes: 0,
            label: String::new(),
            os_name: String::new(),
        };
        assert!(matches!(eject(&dev), Err(DeviceError::NotRemovable(p)) if p == dir));
    }

    #[test]
    fn real_volume_list_contains_the_temp_dir() -> Result<(), DeviceError> {
        // A temp dir stands in for a stick: it must resolve to some mounted
        // volume with sane sizes.
        let dir = tempfile::tempdir().map_err(|_| DeviceError::Unsupported)?;
        let dev = device_for_path(dir.path())?;
        assert!(dir
            .path()
            .canonicalize()
            .map_err(|_| DeviceError::Unsupported)?
            .starts_with(&dev.mount_point));
        assert!(dev.total_bytes >= dev.free_bytes);
        Ok(())
    }

    /// Live check on a real or disk-image volume, which it ejects. macOS:
    /// `hdiutil create -size 64m -fs "MS-DOS FAT32" -volname REKSTEST
    /// -layout MBRSPUD t.dmg && hdiutil attach t.dmg`, then
    /// `REKS_TEST_VOLUME=/Volumes/REKSTEST cargo test -p reks-export-rb -- --ignored`.
    #[test]
    #[ignore = "needs a mounted removable volume in REKS_TEST_VOLUME; ejects it"]
    fn live_volume_discovery_serial_and_eject() -> Result<(), DeviceError> {
        let Some(path) = std::env::var_os("REKS_TEST_VOLUME") else {
            return Err(DeviceError::NotFound("$REKS_TEST_VOLUME".into()));
        };
        let dev = device_for_path(Path::new(&path))?;
        assert_eq!(dev.mount_point, PathBuf::from(&path));
        assert!(list_removable()
            .iter()
            .any(|d| d.mount_point == dev.mount_point));
        assert_eq!(dev.fs, FsKind::Fat32);
        assert!(volume_serial(&dev).is_some());
        eject(&dev)?;
        assert!(!list_devices()
            .iter()
            .any(|d| d.mount_point == dev.mount_point));
        Ok(())
    }
}
