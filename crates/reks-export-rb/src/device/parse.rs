//! Pure parsing and normalisation of OS volume information. No I/O here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{DeviceInfo, FsKind, Os};

/// Maps an OS file system string to [`FsKind`].
///
/// Covers sysinfo's names on each OS: macOS `msdos`/`exfat`/`hfs`/`apfs`,
/// Linux `vfat`/`exfat`/`hfsplus`, Windows `FAT32`/`exFAT`/`NTFS`.
pub fn normalize_fs(name: &str) -> FsKind {
    match name.trim().to_ascii_lowercase().as_str() {
        "msdos" | "vfat" | "fat" | "fat12" | "fat16" | "fat32" | "ms-dos fat32" => FsKind::Fat32,
        "exfat" => FsKind::ExFat,
        "hfs" | "hfsplus" | "hfs+" => FsKind::Hfs,
        _ => FsKind::Other(name.trim().to_owned()),
    }
}

/// The user-visible label. sysinfo's disk name is the volume name on macOS
/// and Windows but the device node on Linux, where desktop automounters use
/// the label as the mount directory name (`/media/<user>/<label>`).
pub fn label_for(name: &str, mount_point: &Path, os: Os) -> String {
    let from_mount = || {
        mount_point
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    match os {
        Os::Linux | Os::Other => from_mount(),
        Os::MacOs | Os::Windows if name.is_empty() => from_mount(),
        Os::MacOs | Os::Windows => name.to_owned(),
    }
}

/// The device whose mount point is the longest prefix of `path`.
pub fn containing_device<'a>(devices: &'a [DeviceInfo], path: &Path) -> Option<&'a DeviceInfo> {
    let path = strip_verbatim(path);
    devices
        .iter()
        .filter(|d| path.starts_with(&d.mount_point))
        .max_by_key(|d| d.mount_point.components().count())
}

/// Drops Windows' `\\?\` prefix that `canonicalize` adds, so the path compares
/// with mount points like `E:\`. Other paths pass through.
pub fn strip_verbatim(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// Top-level `<key>` → `<string>` pairs of a property list such as
/// `diskutil info -plist` prints. Nested dicts and arrays and non-string
/// values are skipped; `None` if the text is not a plist dict.
pub fn plist_strings(xml: &str) -> Option<HashMap<String, String>> {
    let mut out = HashMap::new();
    let mut depth = 0usize; // containers (<dict>/<array>) open, the top dict = 1
    let mut seen_dict = false;
    let mut pending_key: Option<String> = None;
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        rest = &rest[start..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |i| &after[i + 3..]);
            continue;
        }
        let end = rest.find('>')?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        let self_closing = tag.ends_with('/');
        let name = tag
            .trim_end_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or("");
        match name {
            "dict" | "array" if !self_closing => {
                depth += 1;
                seen_dict |= name == "dict";
                pending_key = None;
            }
            "/dict" | "/array" => {
                depth = depth.checked_sub(1)?;
                pending_key = None;
            }
            "key" | "string" if !self_closing => {
                let close = format!("</{name}>");
                let text_end = rest.find(&close)?;
                let text = unescape(&rest[..text_end]);
                rest = &rest[text_end + close.len()..];
                if depth != 1 {
                    continue;
                }
                if name == "key" {
                    pending_key = Some(text);
                } else if let Some(key) = pending_key.take() {
                    out.insert(key, text);
                }
            }
            _ => {
                // Any other value (<true/>, <integer>, ...) consumes the key.
                if depth == 1 && !name.starts_with('/') {
                    pending_key = None;
                }
            }
        }
    }
    seen_dict.then_some(out)
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The volume UUID from `diskutil info -plist` output.
pub fn diskutil_volume_uuid(plist: &str) -> Option<String> {
    plist_strings(plist)?
        .remove("VolumeUUID")
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// First non-empty line of a command's output (e.g. `blkid -o value`).
pub fn first_line(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_owned)
}

/// A 32-bit volume serial as Windows and `blkid` print a FAT one: `XXXX-XXXX`.
pub fn format_serial32(serial: u32) -> String {
    format!("{:04X}-{:04X}", serial >> 16, serial & 0xFFFF)
}

/// Finds the UUID for `device` in a `/dev/disk/by-uuid` listing of
/// (link name, resolved target) pairs.
pub fn uuid_from_by_uuid(entries: &[(String, PathBuf)], device: &Path) -> Option<String> {
    entries
        .iter()
        .find(|(_, target)| target == device)
        .map(|(uuid, _)| uuid.clone())
}

/// A short message from a failed command's stderr/stdout.
pub fn command_message(stderr: &[u8], stdout: &[u8]) -> String {
    let pick = |b: &[u8]| String::from_utf8_lossy(b).trim().to_owned();
    let msg = pick(stderr);
    if msg.is_empty() {
        pick(stdout)
    } else {
        msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISKUTIL_FAT32: &str =
        include_str!("../../../../fixtures/device/diskutil-info-fat32.plist");

    fn dev(mount: &str) -> DeviceInfo {
        DeviceInfo {
            mount_point: mount.into(),
            fs: FsKind::Fat32,
            fs_name: "msdos".into(),
            removable: true,
            total_bytes: 0,
            free_bytes: 0,
            label: String::new(),
            os_name: String::new(),
        }
    }

    #[test]
    fn normalizes_file_system_names() {
        for fat in ["msdos", "vfat", "FAT32", "FAT", "fat16"] {
            assert_eq!(normalize_fs(fat), FsKind::Fat32, "{fat}");
        }
        for exfat in ["exfat", "exFAT"] {
            assert_eq!(normalize_fs(exfat), FsKind::ExFat, "{exfat}");
        }
        for hfs in ["hfs", "hfsplus"] {
            assert_eq!(normalize_fs(hfs), FsKind::Hfs, "{hfs}");
        }
        assert_eq!(normalize_fs("apfs"), FsKind::Other("apfs".into()));
        assert_eq!(normalize_fs("NTFS"), FsKind::Other("NTFS".into()));
        assert_eq!(normalize_fs(""), FsKind::Other(String::new()));
    }

    #[test]
    fn labels_per_os() {
        let mount = Path::new("/media/dj/MY STICK");
        assert_eq!(label_for("/dev/sdb1", mount, Os::Linux), "MY STICK");
        assert_eq!(
            label_for("REKS", Path::new("/Volumes/REKS 1"), Os::MacOs),
            "REKS"
        );
        assert_eq!(label_for("", Path::new("/Volumes/REKS"), Os::MacOs), "REKS");
        assert_eq!(label_for("USB", Path::new(r"E:\"), Os::Windows), "USB");
        assert_eq!(label_for("", Path::new("/"), Os::Linux), "");
    }

    #[test]
    fn longest_mount_point_wins() {
        let devices = [dev("/"), dev("/Volumes/REKS"), dev("/Volumes")];
        let found = containing_device(&devices, Path::new("/Volumes/REKS/Contents/a.mp3"));
        assert_eq!(
            found.map(|d| d.mount_point.clone()),
            Some("/Volumes/REKS".into())
        );
        let found = containing_device(&devices, Path::new("/Users/dj"));
        assert_eq!(found.map(|d| d.mount_point.clone()), Some("/".into()));
        // Prefix match is per component, not per character.
        let found = containing_device(&devices, Path::new("/Volumes/REKS2/x"));
        assert_eq!(
            found.map(|d| d.mount_point.clone()),
            Some("/Volumes".into())
        );
        assert!(containing_device(&devices[1..], Path::new("/Users")).is_none());
    }

    #[test]
    fn fake_device_in_a_temp_dir() -> std::io::Result<()> {
        // A temp dir shaped like a stick, registered as a mounted volume.
        let root = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join("PIONEER/rekordbox"))?;
        let devices = [dev("/"), dev(&root.path().to_string_lossy())];
        let pdb = root.path().join("PIONEER/rekordbox/export.pdb");
        let found = containing_device(&devices, &pdb);
        assert_eq!(found.map(|d| d.mount_point.as_path()), Some(root.path()));
        Ok(())
    }

    #[test]
    fn strips_windows_verbatim_prefix() {
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\E:\Contents")),
            PathBuf::from(r"E:\Contents")
        );
        let unc = Path::new(r"\\?\UNC\server\share");
        assert_eq!(strip_verbatim(unc), unc.to_path_buf());
        assert_eq!(strip_verbatim(Path::new("/tmp/x")), PathBuf::from("/tmp/x"));
    }

    #[test]
    fn parses_captured_diskutil_plist() -> Result<(), String> {
        // Captured from a FAT32 disk image (hdiutil, MBR, volume REKSTEST)
        // on macOS 26; see fixtures/README.md.
        let map = plist_strings(DISKUTIL_FAT32).ok_or("not a plist")?;
        assert_eq!(map.get("FilesystemType").map(String::as_str), Some("msdos"));
        assert_eq!(map.get("VolumeName").map(String::as_str), Some("REKSTEST"));
        assert_eq!(
            map.get("MountPoint").map(String::as_str),
            Some("/Volumes/REKSTEST")
        );
        assert_eq!(map.get("MediaName").map(String::as_str), Some(""));
        // Booleans and integers are not strings.
        assert!(!map.contains_key("Removable"));
        assert!(!map.contains_key("TotalSize"));
        assert_eq!(
            diskutil_volume_uuid(DISKUTIL_FAT32).as_deref(),
            Some("10DE9FD0-1935-3F77-BDA8-01229E4D1714")
        );
        Ok(())
    }

    #[test]
    fn plist_skips_nested_keys_and_comments() {
        let xml = r#"<?xml version="1.0"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "x">
<plist version="1.0"><dict>
  <key>Stores</key><array><dict><key>VolumeUUID</key><string>NESTED</string></dict></array>
  <!-- <key>VolumeUUID</key><string>COMMENT</string> -->
  <key>Empty</key><dict/>
  <key>Flag</key><true/>
  <key>Orphan</key>
  <key>Name</key><string>A &amp; B</string>
</dict></plist>"#;
        let map = plist_strings(xml).unwrap_or_default();
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("Name").map(String::as_str), Some("A & B"));
        assert_eq!(diskutil_volume_uuid(xml), None);
    }

    #[test]
    fn plist_rejects_garbage() {
        assert_eq!(plist_strings("Could not find disk: /nope"), None);
        assert_eq!(plist_strings("<plist><dict><key>A"), None);
        assert_eq!(plist_strings(""), None);
        assert_eq!(
            diskutil_volume_uuid("<dict><key>VolumeUUID</key><string> </string></dict>"),
            None
        );
    }

    #[test]
    fn formats_32_bit_serials() {
        assert_eq!(format_serial32(0x4993_1CFB), "4993-1CFB");
        assert_eq!(format_serial32(0x0000_00AB), "0000-00AB");
    }

    #[test]
    fn first_line_and_by_uuid() {
        assert_eq!(first_line("\n  4993-1CFB\n").as_deref(), Some("4993-1CFB"));
        assert_eq!(first_line(" \n"), None);
        let entries = vec![
            ("1234-ABCD".to_owned(), PathBuf::from("/dev/sda1")),
            ("4993-1CFB".to_owned(), PathBuf::from("/dev/sdb1")),
        ];
        assert_eq!(
            uuid_from_by_uuid(&entries, Path::new("/dev/sdb1")).as_deref(),
            Some("4993-1CFB")
        );
        assert_eq!(uuid_from_by_uuid(&entries, Path::new("/dev/sdc1")), None);
    }

    #[test]
    fn command_message_prefers_stderr() {
        assert_eq!(command_message(b" busy \n", b"out"), "busy");
        assert_eq!(
            command_message(b"", b"Volume REKS on disk4 failed\n"),
            "Volume REKS on disk4 failed"
        );
    }
}
