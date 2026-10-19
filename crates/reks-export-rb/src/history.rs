//! History safeguard (D23, Phase 2 task 12): find the play history players
//! left on a stick and back it up before an export rewrites the stick.
//!
//! baken rebuilds `export.pdb` on every run and removes `exportLibrary.db`
//! unless OneLibrary is on (R§10), so a plain re-export erases the history
//! players wrote. The planner calls [`read_history`] and [`plan_backups`] for
//! the dry run; `apply` calls [`run_backups`] before its first stick write and
//! aborts on any error. Backups only ever read from the stick.
//!
//! Backups go to `<app_data_dir>/stick-backups/<volume serial>/<UTC time>/`
//! with the stick's layout kept (`PIONEER/rekordbox/export.pdb`, ...), so
//! [`read_history`] works on a backup folder as on a stick, plus a
//! `manifest.json` with each file's size and BLAKE3 hash. They live in the data
//! dir, not the cache dir (D19): a backup can be the only copy.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use binrw::{BinRead, Endian};
use rekordcrate::pdb::{Header, Page, PageType, Row, Table};
use serde::{Deserialize, Serialize};

/// Folder under the app data dir that holds stick backups.
pub const BACKUPS_DIR: &str = "stick-backups";
/// Folder name used when the OS gave no volume serial.
pub const UNKNOWN_SERIAL_DIR: &str = "unknown-serial";
/// Written last into each backup folder; its presence marks a complete backup.
pub const BACKUP_MANIFEST: &str = "manifest.json";

/// What players left on a stick (phase-2.md "Data structures").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StickHistory {
    /// From [`crate::device::volume_serial`]; `None` if the OS gave none.
    pub volume_serial: Option<String>,
    /// History playlists in `export.pdb` (table 11), one per player session.
    pub pdb_sessions: u32,
    /// History entries in `export.pdb` (table 12).
    pub pdb_entries: u32,
    /// `export.pdb` exists but could not be read or parsed. Treated as
    /// holding history.
    pub pdb_unreadable: bool,
    /// `PIONEER/rekordbox/exportLibrary.db` (OneLibrary) is present. Players
    /// log history there too, and baken removes it.
    pub onelibrary_db: bool,
}

impl StickHistory {
    /// The stick holds, or may hold, history an export would erase.
    pub fn may_hold_history(&self) -> bool {
        self.pdb_sessions > 0 || self.pdb_entries > 0 || self.pdb_unreadable || self.onelibrary_db
    }
}

/// What an export does with a stick's history. Phase 2.5 adds `Import`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryPolicy {
    /// Copy the stick's database files to the app data dir first (default).
    #[default]
    BackUp,
    /// Skip the backup. Only with an explicit user confirmation.
    Discard,
}

/// Errors from planning or running a backup. Any of them must abort the
/// export before it writes to the stick.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("discarding the stick's play history needs an explicit confirmation")]
    DiscardNotConfirmed,

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// The copy, or a second read of the stick file, hashed differently.
    #[error("backup of {path} does not match the stick (BLAKE3 {expected}, got {actual})")]
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("could not write the backup manifest: {0}")]
    Manifest(#[from] serde_json::Error),
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> HistoryError + '_ {
    move |source| HistoryError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// One stick file to back up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    /// Relative to the device root, `/`-separated, as named on the stick
    /// (`PIONEER/rekordbox/export.pdb`).
    pub rel_path: String,
    /// Size when planned; the backup copies whatever is there when it runs.
    pub bytes: u64,
}

/// The backups an export will make, for the dry run and for [`run_backups`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPlan {
    pub device_root: PathBuf,
    pub volume_serial: Option<String>,
    /// Empty when there is nothing to back up or the user chose `Discard`.
    pub files: Vec<BackupFile>,
}

impl BackupPlan {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }
}

/// One verified file in a finished backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackedUpFile {
    pub rel_path: String,
    pub bytes: u64,
    /// BLAKE3 of the stick file and of the copy (hex).
    pub blake3: String,
}

/// A finished backup; also written as `manifest.json` into `dir`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupReport {
    #[serde(skip)]
    pub dir: PathBuf,
    pub volume_serial: Option<String>,
    /// UTC ISO-8601.
    pub created_at: String,
    pub files: Vec<BackedUpFile>,
}

/// Reads what history `device_root` holds. Never fails: a missing
/// `export.pdb` means no history, an unreadable one sets `pdb_unreadable`.
pub fn read_history(device_root: &Path, volume_serial: Option<String>) -> StickHistory {
    let mut history = StickHistory {
        volume_serial,
        ..StickHistory::default()
    };
    let rb_dir = match rekordbox_dir(device_root) {
        Ok(Some(dir)) => dir,
        Ok(None) => return history,
        Err(e) => {
            tracing::warn!(root = %device_root.display(), error = %e, "cannot list the stick; assuming history");
            history.pdb_unreadable = true;
            return history;
        }
    };

    match find_ci(&rb_dir, "export.pdb") {
        Ok(None) => {}
        Ok(Some(pdb)) => match fs::read(&pdb)
            .map_err(|e| e.to_string())
            .and_then(|bytes| count_history(&bytes))
        {
            Ok(counts) => {
                history.pdb_sessions = counts.sessions;
                history.pdb_entries = counts.entries;
            }
            Err(e) => {
                tracing::warn!(path = %pdb.display(), error = %e, "unreadable export.pdb; assuming history");
                history.pdb_unreadable = true;
            }
        },
        Err(e) => {
            tracing::warn!(dir = %rb_dir.display(), error = %e, "cannot list the stick; assuming history");
            history.pdb_unreadable = true;
        }
    }
    // An error here would also hide a OneLibrary file; assume one is there.
    history.onelibrary_db = find_ci(&rb_dir, "exportLibrary.db").map_or(true, |p| p.is_some());
    history
}

/// Plans the backups for `history` under `policy`. `Discard` returns an
/// empty plan only when `discard_confirmed` is set, and is refused otherwise.
/// `BackUp` lists `PIONEER/rekordbox/*.pdb` and `exportLibrary.db` (with any
/// SQLite `-wal`/`-shm`/`-journal` files) when the stick may hold history.
pub fn plan_backups(
    device_root: &Path,
    history: &StickHistory,
    policy: HistoryPolicy,
    discard_confirmed: bool,
) -> Result<BackupPlan, HistoryError> {
    let mut plan = BackupPlan {
        device_root: device_root.to_path_buf(),
        volume_serial: history.volume_serial.clone(),
        files: Vec::new(),
    };
    match policy {
        HistoryPolicy::Discard if !discard_confirmed => {
            return Err(HistoryError::DiscardNotConfirmed)
        }
        HistoryPolicy::Discard => {
            if history.may_hold_history() {
                tracing::warn!(root = %device_root.display(), ?history, "user confirmed discarding stick history");
            }
            return Ok(plan);
        }
        HistoryPolicy::BackUp if !history.may_hold_history() => return Ok(plan),
        HistoryPolicy::BackUp => {}
    }

    let Some(rb_dir) = rekordbox_dir(device_root).map_err(io_err(device_root))? else {
        return Ok(plan);
    };
    let rb_rel = rel_of(device_root, &rb_dir);
    let entries = fs::read_dir(&rb_dir).map_err(io_err(&rb_dir))?;
    for entry in entries {
        let entry = entry.map_err(io_err(&rb_dir))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_backed_up(&name) {
            continue;
        }
        let meta = entry.metadata().map_err(io_err(&entry.path()))?;
        if meta.is_file() {
            plan.files.push(BackupFile {
                rel_path: format!("{rb_rel}/{name}"),
                bytes: meta.len(),
            });
        }
    }
    plan.files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(plan)
}

/// `<app_data_dir>/stick-backups/<serial>/`, where every backup of one stick goes.
pub fn backup_root(app_data_dir: &Path, volume_serial: Option<&str>) -> PathBuf {
    let serial = match volume_serial {
        Some(s) if !s.is_empty() => s
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect(),
        _ => UNKNOWN_SERIAL_DIR.to_string(),
    };
    app_data_dir.join(BACKUPS_DIR).join(serial)
}

/// Runs `plan`: copies each file into a new folder under [`backup_root`],
/// fsyncs it, and checks that a second read of the stick file and a read of
/// the copy both hash like the bytes copied. Returns `None` for an empty plan.
///
/// On any error the new backup folder is removed and the error returned; the
/// caller must then abort the export. Only reads from the stick.
pub fn run_backups(
    plan: &BackupPlan,
    app_data_dir: &Path,
) -> Result<Option<BackupReport>, HistoryError> {
    run_backups_at(plan, app_data_dir, &reks_core::timefmt::now_utc())
}

fn run_backups_at(
    plan: &BackupPlan,
    app_data_dir: &Path,
    created_at: &str,
) -> Result<Option<BackupReport>, HistoryError> {
    if plan.files.is_empty() {
        return Ok(None);
    }
    let root = backup_root(app_data_dir, plan.volume_serial.as_deref());
    fs::create_dir_all(&root).map_err(io_err(&root))?;
    // `:` is not allowed in Windows file names.
    let dir = create_unique_dir(&root, &created_at.replace(':', "-"))?;

    let result = copy_all(plan, &dir, created_at);
    if result.is_err() {
        if let Err(e) = fs::remove_dir_all(&dir) {
            tracing::warn!(dir = %dir.display(), error = %e, "could not remove a failed backup");
        }
    }
    let report = result?;
    tracing::info!(dir = %dir.display(), files = report.files.len(), "stick history backed up");
    Ok(Some(report))
}

fn copy_all(plan: &BackupPlan, dir: &Path, created_at: &str) -> Result<BackupReport, HistoryError> {
    let mut report = BackupReport {
        dir: dir.to_path_buf(),
        volume_serial: plan.volume_serial.clone(),
        created_at: created_at.to_string(),
        files: Vec::with_capacity(plan.files.len()),
    };
    for file in &plan.files {
        let src = join_rel(&plan.device_root, &file.rel_path);
        let dst = join_rel(dir, &file.rel_path);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(io_err(parent))?;
        }
        let (bytes, copied) = copy_hashing(&src, &dst)?;
        verify(&src, &copied)?;
        verify(&dst, &copied)?;
        report.files.push(BackedUpFile {
            rel_path: file.rel_path.clone(),
            bytes,
            blake3: copied.to_hex().to_string(),
        });
    }
    let manifest = dir.join(BACKUP_MANIFEST);
    let json = serde_json::to_vec_pretty(&report)?;
    write_synced(&manifest, &json)?;
    Ok(report)
}

/// Copies `src` to a new file `dst`, hashing the bytes as they pass, then
/// fsyncs `dst`.
fn copy_hashing(src: &Path, dst: &Path) -> Result<(u64, blake3::Hash), HistoryError> {
    let mut input = File::open(src).map_err(io_err(src))?;
    let mut output = File::options()
        .write(true)
        .create_new(true)
        .open(dst)
        .map_err(io_err(dst))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut total = 0u64;
    loop {
        let n = match input.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(io_err(src)(e)),
        };
        hasher.update(&buf[..n]);
        output.write_all(&buf[..n]).map_err(io_err(dst))?;
        total += n as u64;
    }
    output.sync_all().map_err(io_err(dst))?;
    Ok((total, hasher.finalize()))
}

fn hash_file(path: &Path) -> Result<blake3::Hash, HistoryError> {
    let mut file = File::open(path).map_err(io_err(path))?;
    let mut hasher = blake3::Hasher::new();
    io::copy(&mut file, &mut hasher).map_err(io_err(path))?;
    Ok(hasher.finalize())
}

/// Re-reads `path` and fails unless it hashes to `expected`.
fn verify(path: &Path, expected: &blake3::Hash) -> Result<(), HistoryError> {
    let actual = hash_file(path)?;
    if actual != *expected {
        return Err(HistoryError::HashMismatch {
            path: path.to_path_buf(),
            expected: expected.to_hex().to_string(),
            actual: actual.to_hex().to_string(),
        });
    }
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), HistoryError> {
    let mut file = File::create(path).map_err(io_err(path))?;
    file.write_all(bytes).map_err(io_err(path))?;
    file.sync_all().map_err(io_err(path))
}

fn create_unique_dir(parent: &Path, name: &str) -> Result<PathBuf, HistoryError> {
    for n in 0..100u32 {
        let dir = if n == 0 {
            parent.join(name)
        } else {
            parent.join(format!("{name}-{n}"))
        };
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_err(&dir)(e)),
        }
    }
    Err(io_err(parent)(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "too many backups with the same timestamp",
    )))
}

fn is_backed_up(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    // macOS writes `._*` AppleDouble files on FAT; they are not databases.
    if lower.starts_with("._") {
        return false;
    }
    lower.ends_with(".pdb")
        || matches!(
            lower.as_str(),
            "exportlibrary.db"
                | "exportlibrary.db-wal"
                | "exportlibrary.db-shm"
                | "exportlibrary.db-journal"
        )
}

/// `PIONEER/rekordbox` under `device_root`, matched case-insensitively (FAT
/// is case-insensitive; a temp dir on Linux is not).
fn rekordbox_dir(device_root: &Path) -> io::Result<Option<PathBuf>> {
    let Some(pioneer) = find_ci(device_root, "PIONEER")? else {
        return Ok(None);
    };
    find_ci(&pioneer, "rekordbox")
}

/// The entry of `dir` named `name`, ignoring ASCII case. `Ok(None)` if `dir`
/// or the entry does not exist.
fn find_ci(dir: &Path, name: &str) -> io::Result<Option<PathBuf>> {
    let exact = dir.join(name);
    if exact.try_exists()? {
        return Ok(Some(exact));
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(name)
        {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

fn rel_of(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn join_rel(root: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c))
}

#[derive(Debug, Default, PartialEq, Eq)]
struct HistoryCounts {
    sessions: u32,
    entries: u32,
}

/// Counts history playlist and entry rows (tables 11/12) in an `export.pdb`.
/// Walks each table's page chain itself, bounded by the file size and with a
/// cycle check, because a player- or tool-damaged file must not hang us.
fn count_history(bytes: &[u8]) -> Result<HistoryCounts, String> {
    let mut reader = Cursor::new(bytes);
    let header = Header::read(&mut reader).map_err(|e| format!("PDB header: {e}"))?;
    if !(512..=65_536).contains(&header.page_size) {
        return Err(format!("implausible page size {}", header.page_size));
    }
    let mut counts = HistoryCounts::default();
    for table in &header.tables {
        match table.page_type {
            PageType::HistoryPlaylists => {
                counts.sessions += count_rows(&mut reader, &header, table)?
            }
            PageType::HistoryEntries => counts.entries += count_rows(&mut reader, &header, table)?,
            _ => {}
        }
    }
    Ok(counts)
}

fn count_rows(reader: &mut Cursor<&[u8]>, header: &Header, table: &Table) -> Result<u32, String> {
    let page_size = u64::from(header.page_size);
    let len = reader.get_ref().len() as u64;
    let mut seen = HashSet::new();
    let mut index = table.first_page.clone();
    let mut rows = 0u32;
    loop {
        let offset = index.offset(header.page_size);
        // Page 0 is the file header; a table page past the end is damage.
        if offset < page_size || offset.saturating_add(page_size) > len {
            return Err(format!(
                "{:?} page at byte {offset} is outside the file",
                table.page_type
            ));
        }
        if !seen.insert(offset) {
            return Err(format!(
                "{:?} page chain loops at byte {offset}",
                table.page_type
            ));
        }
        reader
            .seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let page = Page::read_options(reader, Endian::Little, (header.page_size,))
            .map_err(|e| format!("{:?} page at byte {offset}: {e}", table.page_type))?;
        if page.page_index != index || page.page_type != table.page_type {
            return Err(format!(
                "{:?} page at byte {offset} has the wrong index or type",
                table.page_type
            ));
        }
        for group in &page.row_groups {
            rows += group
                .present_rows()
                .filter(|row| matches!(row, Row::HistoryPlaylist(_) | Row::HistoryEntry(_)))
                .count() as u32;
        }
        if page.page_index == table.last_page {
            return Ok(rows);
        }
        index = page.next_page;
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;

    const PAGE: usize = 4096;
    const T_HISTORY_PLAYLISTS: u32 = 11;
    const T_HISTORY_ENTRIES: u32 = 12;
    const T_TRACKS: u32 = 0;

    fn history_playlist(id: u32, name: &str) -> Vec<u8> {
        let mut row = id.to_le_bytes().to_vec();
        // DeviceSQL short ASCII string: ((len + 1) << 1) | 1, then the bytes.
        row.push((((name.len() + 1) << 1) | 1) as u8);
        row.extend_from_slice(name.as_bytes());
        row
    }

    fn history_entry(track_id: u32, playlist_id: u32, index: u32) -> Vec<u8> {
        [track_id, playlist_id, index]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect()
    }

    fn put_u16(buf: &mut [u8], at: usize, v: u16) {
        buf[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn put_u32(buf: &mut [u8], at: usize, v: u32) {
        buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// A minimal `export.pdb`: page 0 is the header, then each table's pages
    /// in order. `tables` = (page type, pages, each page's rows (≤ 16)).
    fn synthetic_pdb(tables: &[(u32, Vec<Vec<Vec<u8>>>)]) -> Vec<u8> {
        let total_pages = 1 + tables.iter().map(|t| t.1.len()).sum::<usize>();
        let mut buf = vec![0u8; total_pages * PAGE];
        put_u32(&mut buf, 4, PAGE as u32);
        put_u32(&mut buf, 8, tables.len() as u32);
        put_u32(&mut buf, 12, total_pages as u32); // next_unused_page
        put_u32(&mut buf, 20, 1); // sequence
        let mut next = 1usize;
        for (t, (page_type, pages)) in tables.iter().enumerate() {
            let at = 28 + 16 * t;
            put_u32(&mut buf, at, *page_type);
            put_u32(&mut buf, at + 8, next as u32);
            put_u32(&mut buf, at + 12, (next + pages.len() - 1) as u32);
            for (p, rows) in pages.iter().enumerate() {
                let index = next + p;
                let base = index * PAGE;
                let next_page = if p + 1 < pages.len() {
                    index + 1
                } else {
                    total_pages
                };
                put_u32(&mut buf, base + 4, index as u32);
                put_u32(&mut buf, base + 8, *page_type);
                put_u32(&mut buf, base + 12, next_page as u32);
                buf[base + 24] = rows.len() as u8; // num_rows_small
                buf[base + 27] = 0x34; // page flags: has data
                put_u16(&mut buf, base + 38, 1); // unknown7: 1 on history pages
                let heap = base + 0x28;
                let mut used = 0usize;
                let mut present = 0u16;
                for (i, row) in rows.iter().enumerate() {
                    buf[heap + used..heap + used + row.len()].copy_from_slice(row);
                    put_u16(&mut buf, base + PAGE - 4 - 2 * (i + 1), used as u16);
                    present |= 1 << i;
                    used += row.len().next_multiple_of(4);
                }
                put_u16(&mut buf, base + 30, used as u16); // used_size
                put_u16(&mut buf, base + PAGE - 4, present);
            }
            next += pages.len();
        }
        buf
    }

    fn played_pdb() -> Vec<u8> {
        synthetic_pdb(&[
            (T_TRACKS, vec![vec![]]),
            (
                T_HISTORY_PLAYLISTS,
                vec![vec![
                    history_playlist(1, "HISTORY 001"),
                    history_playlist(2, "HISTORY 002"),
                ]],
            ),
            (
                T_HISTORY_ENTRIES,
                vec![
                    vec![history_entry(5, 1, 1), history_entry(7, 1, 2)],
                    vec![history_entry(5, 2, 1)],
                ],
            ),
        ])
    }

    fn unplayed_pdb() -> Vec<u8> {
        synthetic_pdb(&[
            (T_TRACKS, vec![vec![]]),
            (T_HISTORY_PLAYLISTS, vec![vec![]]),
            (T_HISTORY_ENTRIES, vec![vec![]]),
        ])
    }

    fn stick(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (rel, bytes) in files {
            let path = join_rel(dir.path(), rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        dir
    }

    /// Every file under `root` with its bytes and mtime.
    fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>, SystemTime)> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let mtime = fs::metadata(&path).unwrap().modified().unwrap();
                    out.push((path.clone(), fs::read(&path).unwrap(), mtime));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn synthetic_pdb_counts_history_rows() {
        assert_eq!(
            count_history(&played_pdb()).unwrap(),
            HistoryCounts {
                sessions: 2,
                entries: 3
            }
        );
        assert_eq!(
            count_history(&unplayed_pdb()).unwrap(),
            HistoryCounts::default()
        );
    }

    #[test]
    fn no_pdb_means_no_history() {
        let empty = tempfile::tempdir().unwrap();
        let h = read_history(empty.path(), Some("4993-1CFB".into()));
        assert_eq!(
            h,
            StickHistory {
                volume_serial: Some("4993-1CFB".into()),
                ..StickHistory::default()
            }
        );
        assert!(!h.may_hold_history());

        let other = stick(&[("Contents/a.mp3", b"x")]);
        assert!(!read_history(other.path(), None).may_hold_history());
    }

    #[test]
    fn played_stick_reports_counts() {
        let s = stick(&[("PIONEER/rekordbox/export.pdb", &played_pdb())]);
        let h = read_history(s.path(), None);
        assert_eq!(
            (h.pdb_sessions, h.pdb_entries, h.pdb_unreadable),
            (2, 3, false)
        );
        assert!(h.may_hold_history());
    }

    #[test]
    fn pdb_without_history_needs_no_backup() {
        let s = stick(&[("PIONEER/rekordbox/export.pdb", &unplayed_pdb())]);
        let h = read_history(s.path(), None);
        assert!(!h.may_hold_history(), "{h:?}");
        let plan = plan_backups(s.path(), &h, HistoryPolicy::BackUp, false).unwrap();
        assert!(plan.files.is_empty());
    }

    #[test]
    fn unreadable_pdb_is_treated_as_history() {
        let mut truncated = played_pdb();
        truncated.truncate(PAGE + 100);
        let mut looping = played_pdb();
        // History playlists table: last page never reached, page 2 links to itself.
        put_u32(&mut looping, 28 + 16 + 12, 9);
        put_u32(&mut looping, 2 * PAGE + 12, 2);
        for bytes in [b"not a pdb at all".to_vec(), truncated, looping, Vec::new()] {
            let s = stick(&[("PIONEER/rekordbox/export.pdb", &bytes)]);
            let h = read_history(s.path(), None);
            assert!(h.pdb_unreadable, "{h:?}");
            assert!(h.may_hold_history());
        }
    }

    #[test]
    fn onelibrary_db_and_case_insensitive_names() {
        let s = stick(&[
            ("pioneer/REKORDBOX/EXPORT.PDB", &unplayed_pdb()),
            ("pioneer/REKORDBOX/exportLibrary.db", b"sqlite"),
        ]);
        let h = read_history(s.path(), None);
        assert!(h.onelibrary_db && !h.pdb_unreadable);
        assert!(h.may_hold_history());
    }

    #[test]
    fn plan_lists_database_files_only() {
        let s = stick(&[
            ("PIONEER/rekordbox/export.pdb", &played_pdb()),
            ("PIONEER/rekordbox/exportExt.pdb", b"ext"),
            ("PIONEER/rekordbox/exportLibrary.db", b"one"),
            ("PIONEER/rekordbox/exportLibrary.db-wal", b"wal"),
            ("PIONEER/rekordbox/._export.pdb", b"appledouble"),
            ("PIONEER/rekordbox/notes.txt", b"no"),
            ("PIONEER/USBANLZ/P000/00000001/ANLZ0000.DAT", b"anlz"),
        ]);
        let h = read_history(s.path(), Some("S".into()));
        let plan = plan_backups(s.path(), &h, HistoryPolicy::BackUp, false).unwrap();
        let rels: Vec<_> = plan.files.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(
            rels,
            [
                "PIONEER/rekordbox/export.pdb",
                "PIONEER/rekordbox/exportExt.pdb",
                "PIONEER/rekordbox/exportLibrary.db",
                "PIONEER/rekordbox/exportLibrary.db-wal",
            ]
        );
        assert_eq!(plan.total_bytes(), played_pdb().len() as u64 + 9);
        assert_eq!(plan.volume_serial.as_deref(), Some("S"));
    }

    #[test]
    fn discard_needs_the_explicit_flag() {
        let s = stick(&[("PIONEER/rekordbox/export.pdb", &played_pdb())]);
        let h = read_history(s.path(), None);
        assert!(matches!(
            plan_backups(s.path(), &h, HistoryPolicy::Discard, false),
            Err(HistoryError::DiscardNotConfirmed)
        ));
        let plan = plan_backups(s.path(), &h, HistoryPolicy::Discard, true).unwrap();
        assert!(plan.files.is_empty());
        assert_eq!(HistoryPolicy::default(), HistoryPolicy::BackUp);
    }

    #[test]
    fn backup_copies_verifies_and_reads_back() {
        let s = stick(&[
            ("PIONEER/rekordbox/export.pdb", &played_pdb()),
            ("PIONEER/rekordbox/exportExt.pdb", b"ext"),
        ]);
        let before = snapshot(s.path());
        let data = tempfile::tempdir().unwrap();
        let h = read_history(s.path(), Some("4993-1CFB".into()));
        let plan = plan_backups(s.path(), &h, HistoryPolicy::BackUp, false).unwrap();

        let ts = "2026-10-19T15:30:12.123Z";
        let report = run_backups_at(&plan, data.path(), ts).unwrap().unwrap();
        let expected_dir = data
            .path()
            .join("stick-backups/4993-1CFB/2026-10-19T15-30-12.123Z");
        assert_eq!(report.dir, expected_dir);
        assert_eq!(report.files.len(), 2);
        for f in &report.files {
            let copy = fs::read(join_rel(&report.dir, &f.rel_path)).unwrap();
            assert_eq!(blake3::hash(&copy).to_hex().as_str(), f.blake3);
            assert_eq!(copy.len() as u64, f.bytes);
        }
        let manifest: BackupReport =
            serde_json::from_slice(&fs::read(report.dir.join(BACKUP_MANIFEST)).unwrap()).unwrap();
        assert_eq!(manifest.files, report.files);
        assert_eq!(manifest.created_at, ts);

        // The backup folder reads like the stick.
        let again = read_history(&report.dir, None);
        assert_eq!((again.pdb_sessions, again.pdb_entries), (2, 3));

        // Same timestamp twice: a second folder, not an overwrite.
        let second = run_backups_at(&plan, data.path(), ts).unwrap().unwrap();
        assert_ne!(second.dir, report.dir);

        assert_eq!(snapshot(s.path()), before, "backup touched the stick");
    }

    #[test]
    fn empty_plan_makes_no_backup() {
        let data = tempfile::tempdir().unwrap();
        let plan = BackupPlan {
            device_root: data.path().join("stick"),
            volume_serial: None,
            files: vec![],
        };
        assert!(run_backups(&plan, data.path()).unwrap().is_none());
        assert!(!data.path().join(BACKUPS_DIR).exists());
    }

    #[test]
    fn failing_backup_leaves_the_stick_untouched() {
        let s = stick(&[
            ("PIONEER/rekordbox/export.pdb", &played_pdb()),
            ("PIONEER/rekordbox/exportExt.pdb", b"ext"),
        ]);
        let h = read_history(s.path(), Some("S".into()));
        let plan = plan_backups(s.path(), &h, HistoryPolicy::BackUp, false).unwrap();
        let before = snapshot(s.path());

        // The data dir is a file: nothing can be created.
        let blocker = tempfile::NamedTempFile::new().unwrap();
        assert!(matches!(
            run_backups(&plan, blocker.path()),
            Err(HistoryError::Io { .. })
        ));

        // A planned file vanished: the partial backup folder is removed.
        let data = tempfile::tempdir().unwrap();
        let mut gone = plan.clone();
        gone.files.push(BackupFile {
            rel_path: "PIONEER/rekordbox/zz-missing.pdb".into(),
            bytes: 1,
        });
        assert!(matches!(
            run_backups(&gone, data.path()),
            Err(HistoryError::Io { .. })
        ));
        let serial_dir = backup_root(data.path(), Some("S"));
        assert_eq!(fs::read_dir(serial_dir).unwrap().count(), 0);

        assert_eq!(snapshot(s.path()), before);
    }

    #[test]
    fn verify_rejects_a_different_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("export.pdb");
        fs::write(&path, b"copy").unwrap();
        assert!(verify(&path, &blake3::hash(b"copy")).is_ok());
        assert!(matches!(
            verify(&path, &blake3::hash(b"stick")),
            Err(HistoryError::HashMismatch { .. })
        ));
    }

    #[test]
    fn backup_root_sanitises_the_serial() {
        let root = Path::new("/data");
        assert_eq!(
            backup_root(root, Some("10DE9FD0-1935-3F77")),
            root.join("stick-backups/10DE9FD0-1935-3F77")
        );
        assert_eq!(
            backup_root(root, Some("a/b:c")),
            root.join("stick-backups/a_b_c")
        );
        assert_eq!(
            backup_root(root, None),
            root.join("stick-backups/unknown-serial")
        );
        assert_eq!(
            backup_root(root, Some("")),
            root.join("stick-backups/unknown-serial")
        );
    }

    /// Needs a real played stick's `export.pdb` (hardware checklist step 7),
    /// sanitised, at `fixtures/rekordbox/played-<ver>/PIONEER/rekordbox/export.pdb`.
    /// Set `REKS_PLAYED_FIXTURE` to that `played-<ver>` folder and the expected
    /// counts as `REKS_PLAYED_COUNTS=<sessions>,<entries>`, then run with
    /// `--ignored`. Make it a plain test once the fixture is checked in.
    #[test]
    #[ignore = "needs fixtures/rekordbox/played-<ver>/ from the hardware checklist (step 7)"]
    fn played_stick_fixture_counts() {
        let root = PathBuf::from(std::env::var("REKS_PLAYED_FIXTURE").unwrap());
        let counts = std::env::var("REKS_PLAYED_COUNTS").unwrap();
        let (sessions, entries) = counts.split_once(',').unwrap();
        let h = read_history(&root, None);
        assert!(!h.pdb_unreadable);
        assert_eq!(h.pdb_sessions, sessions.parse::<u32>().unwrap());
        assert_eq!(h.pdb_entries, entries.parse::<u32>().unwrap());
    }
}
