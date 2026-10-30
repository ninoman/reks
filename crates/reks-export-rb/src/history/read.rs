//! History reader (Phase 2.5 task 5): the history playlists players wrote to
//! a stick's `export.pdb`, each entry resolved to its PDB track row.
//!
//! Reads a stick or a Phase 2 backup folder ([`super::run_backups`] keeps the
//! stick layout). Read-only: the stick is opened for reading only, once.
//!
//! Tables: 11 `history_playlists` and 12 `history_entries` through
//! [`crate::pdb`] (R§10); 0 `tracks` and 2 `artists` for the entry's stick path, title,
//! artist and duration; 19 `history` for dates. Table 19's layout is undecoded
//! (HIST1, R§10: rows "with ascending ids and ASCII `YYYY-MM-DD`"), so date
//! decoding is a heuristic, **UNVERIFIED**, and off unless
//! [`HistoryReadOptions::hist1_dates`] is set: each table-19 row must hold
//! exactly one valid `YYYY-MM-DD`, there must be one row per history playlist,
//! and row order paired with ascending playlist id must give non-decreasing
//! dates. Anything else leaves every date `None`.
//!
//! reks-core can't depend on this crate (this crate depends on it), so
//! `reks-core::history::scan` (task 6) takes a [`PlayHistory`] read here,
//! converted by [`PlayHistory::to_source_history`].

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use reks_core::history::{HistoryLibrary, SourceEntry, SourceHistory, SourceSession};
use serde::{Deserialize, Serialize};

use crate::pdb::{rows as pdb_rows, Pdb, Table};

use super::{find_ci, io_err, rekordbox_dir, rel_of, BackupReport, HistoryError, BACKUP_MANIFEST};

/// Where to read history from (phase-2.5.md "Interfaces").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "path")]
pub enum HistorySource {
    /// A mounted stick's root.
    Stick(PathBuf),
    /// A Phase 2 `stick-backups/<serial>/<UTC time>/` folder.
    Backup(PathBuf),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryReadOptions {
    /// Volume serial of a [`HistorySource::Stick`] (from
    /// [`crate::device::volume_serial`]). A backup's comes from its manifest.
    pub stick_serial: Option<String>,
    /// Decode dates from table 19 (HIST1, **UNVERIFIED**; see module docs).
    /// Off by default: dates stay `None` until a played-stick fixture confirms it.
    pub hist1_dates: bool,
}

/// The history in one `export.pdb`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayHistory {
    pub volume_serial: Option<String>,
    /// By history playlist id.
    pub sessions: Vec<PdbSession>,
    pub dates: DatesStatus,
}

/// What became of table 19.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status", content = "reason")]
pub enum DatesStatus {
    /// [`HistoryReadOptions::hist1_dates`] was off.
    #[default]
    NotRead,
    /// No `export.pdb`, or no history playlists to date.
    NoSessions,
    /// The header lists no table 19.
    NoTable,
    /// Every named session got a date.
    Decoded,
    /// Table 19 did not fit the heuristic; all dates are `None`.
    Undecodable(String),
}

/// One history playlist (a player session).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdbSession {
    /// The history playlist's row id (table 11).
    pub id: u32,
    /// `HISTORY 001`, ...; `None` for entries whose playlist row is missing.
    pub name: Option<String>,
    /// `YYYY-MM-DD` from table 19, `None` unless [`DatesStatus::Decoded`].
    pub played_on: Option<String>,
    /// In player order (`entry_index`, then row order).
    pub entries: Vec<PdbEntry>,
}

/// One history entry (table 12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdbEntry {
    /// `entry_index` as the player wrote it.
    pub index: u32,
    pub track_id: u32,
    /// `None` when no readable track row has `track_id`.
    pub track: Option<PdbTrack>,
}

/// The fields of a PDB track row the matcher (task 6) uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdbTrack {
    /// As in the PDB: `/`-separated from the stick root (`/Contents/...`).
    pub stick_path: String,
    pub title: String,
    /// `None` when the row has no artist or its artist row is missing.
    pub artist: Option<String>,
    /// From the row's whole seconds; `None` when 0.
    pub duration_ms: Option<i64>,
}

impl PlayHistory {
    /// The input of `reks_core::history::scan` (Phase 2.5 task 6). A
    /// session whose playlist row is missing is named `HISTORY #<id>`.
    pub fn to_source_history(&self) -> SourceHistory {
        SourceHistory {
            volume_serial: self.volume_serial.clone(),
            sessions: self
                .sessions
                .iter()
                .map(|s| SourceSession {
                    library: HistoryLibrary::DeviceLibrary,
                    source_name: s
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("HISTORY #{}", s.id)),
                    played_on: s.played_on.clone(),
                    entries: s
                        .entries
                        .iter()
                        .map(|e| match &e.track {
                            Some(t) => SourceEntry {
                                stick_path: Some(t.stick_path.clone()),
                                title: Some(t.title.clone()),
                                artist: t.artist.clone(),
                                duration_ms: t.duration_ms,
                            },
                            None => SourceEntry::default(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

/// Reads the history in `src`'s `export.pdb`. No `export.pdb` → no
/// sessions. A damaged PDB or history row is an error; a damaged track or
/// artist row only leaves its entries unresolved. For a backup, the
/// manifest must be there and `export.pdb` must hash as it records.
pub fn read_play_history(
    src: &HistorySource,
    opts: &HistoryReadOptions,
) -> Result<PlayHistory, HistoryError> {
    let (root, volume_serial, manifest) = match src {
        HistorySource::Stick(root) => (root, opts.stick_serial.clone(), None),
        HistorySource::Backup(dir) => {
            let report = read_backup_manifest(dir)?;
            (dir, report.volume_serial.clone(), Some(report))
        }
    };
    let mut history = PlayHistory {
        volume_serial,
        dates: if opts.hist1_dates {
            DatesStatus::NoSessions
        } else {
            DatesStatus::NotRead
        },
        ..PlayHistory::default()
    };
    let Some(rb_dir) = rekordbox_dir(root).map_err(io_err(root))? else {
        return Ok(history);
    };
    let Some(pdb) = find_ci(&rb_dir, "export.pdb").map_err(io_err(&rb_dir))? else {
        return Ok(history);
    };
    let bytes = fs::read(&pdb).map_err(io_err(&pdb))?;
    if let Some(report) = manifest {
        check_backup_copy(&report, root, &pdb, &bytes)?;
    }
    let parsed = parse_pdb(&bytes).map_err(|message| HistoryError::Unreadable {
        path: pdb.clone(),
        message,
    })?;
    history.sessions = parsed.sessions;
    if opts.hist1_dates && history.sessions.iter().any(|s| s.name.is_some()) {
        history.dates = apply_dates(&mut history.sessions, parsed.date_rows);
        if let DatesStatus::Undecodable(reason) = &history.dates {
            tracing::info!(path = %pdb.display(), reason, "history dates not decoded (HIST1)");
        }
    }
    Ok(history)
}

/// Every readable track row of the `export.pdb` under `device_root`, by
/// row id, with the fields the matcher uses (adopting a stick, Phase 2.5
/// task 12). No `export.pdb`: none. A damaged track or artist row is left
/// out, as for history entries. Read-only.
pub fn read_stick_tracks(device_root: &Path) -> Result<HashMap<u32, PdbTrack>, HistoryError> {
    let Some(rb_dir) = rekordbox_dir(device_root).map_err(io_err(device_root))? else {
        return Ok(HashMap::new());
    };
    let Some(pdb) = find_ci(&rb_dir, "export.pdb").map_err(io_err(&rb_dir))? else {
        return Ok(HashMap::new());
    };
    let bytes = fs::read(&pdb).map_err(io_err(&pdb))?;
    let unreadable = |message: String| HistoryError::Unreadable {
        path: pdb.clone(),
        message,
    };
    let file = Pdb::parse(&bytes).map_err(|e| unreadable(e.to_string()))?;
    let (tracks, artists) = tracks_and_artists(&file).map_err(unreadable)?;
    Ok(tracks
        .iter()
        .map(|(&id, t)| (id, t.resolve(&artists)))
        .collect())
}

/// The `manifest.json` of the backup folder `dir`.
pub fn read_backup_manifest(dir: &Path) -> Result<BackupReport, HistoryError> {
    let path = dir.join(BACKUP_MANIFEST);
    let bytes = fs::read(&path).map_err(io_err(&path))?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// `path`, a file in the backup folder `root` read as `bytes`, must be
/// listed in the backup's manifest and hash the same.
pub fn check_backup_copy(
    report: &BackupReport,
    root: &Path,
    path: &Path,
    bytes: &[u8],
) -> Result<(), HistoryError> {
    let rel = rel_of(root, path);
    let Some(entry) = report.files.iter().find(|f| f.rel_path == rel) else {
        return Err(HistoryError::Unreadable {
            path: path.to_path_buf(),
            message: "not listed in the backup manifest".into(),
        });
    };
    let actual = blake3::hash(bytes).to_hex().to_string();
    if actual != entry.blake3 {
        return Err(HistoryError::HashMismatch {
            path: path.to_path_buf(),
            expected: entry.blake3.clone(),
            actual,
        });
    }
    Ok(())
}

struct Parsed {
    sessions: Vec<PdbSession>,
    /// Table-19 rows in page-chain and row order; `None` if there is no table 19.
    date_rows: Option<Vec<Vec<u8>>>,
}

fn parse_pdb(bytes: &[u8]) -> Result<Parsed, String> {
    let file = Pdb::parse(bytes).map_err(|e| e.to_string())?;
    let rows = |t| file.rows_of(t).map_err(|e| e.to_string());
    let mut names: HashMap<u32, String> = HashMap::new();
    for r in rows(Table::HistoryPlaylists)? {
        let (id, name) = pdb_rows::history_playlist(r).map_err(|e| e.to_string())?;
        names.insert(id, name);
    }
    // (playlist, entry_index, visit order, track id)
    let mut entries: Vec<(u32, u32, usize, u32)> = Vec::new();
    for r in rows(Table::HistoryEntries)? {
        let (track, playlist, index) = pdb_rows::history_entry(r).map_err(|e| e.to_string())?;
        entries.push((playlist, index, entries.len(), track));
    }
    let (tracks, artists) = tracks_and_artists(&file)?;
    let date_rows = file
        .tables()
        .iter()
        .any(|t| t.table == Table::History)
        .then(|| rows(Table::History))
        .transpose()?
        .map(|rows| rows.into_iter().map(<[u8]>::to_vec).collect());

    let mut by_playlist: HashMap<u32, Vec<(u32, usize, u32)>> = HashMap::new();
    for (playlist, index, order, track) in entries {
        by_playlist
            .entry(playlist)
            .or_default()
            .push((index, order, track));
    }
    for id in names.keys() {
        by_playlist.entry(*id).or_default();
    }
    let mut sessions: Vec<PdbSession> = by_playlist
        .into_iter()
        .map(|(id, mut rows)| {
            rows.sort_unstable();
            PdbSession {
                id,
                name: names.remove(&id),
                played_on: None,
                entries: rows
                    .into_iter()
                    .map(|(index, _, track_id)| PdbEntry {
                        index,
                        track_id,
                        track: tracks.get(&track_id).map(|t| t.resolve(&artists)),
                    })
                    .collect(),
            }
        })
        .collect();
    sessions.sort_by_key(|s| s.id);
    Ok(Parsed {
        sessions,
        date_rows,
    })
}

/// The track and artist rows the history entries resolve against. A row
/// that does not decode is left out (logged): its entries stay unresolved.
/// A later row with the same id replaces an earlier one.
#[allow(clippy::type_complexity)]
fn tracks_and_artists(
    file: &Pdb<'_>,
) -> Result<(HashMap<u32, RawTrack>, HashMap<u32, String>), String> {
    let mut tracks = HashMap::new();
    for r in file.rows_of(Table::Tracks).map_err(|e| e.to_string())? {
        match pdb_rows::track(r) {
            Ok(t) => {
                tracks.insert(
                    t.id,
                    RawTrack {
                        artist_id: t.artist_id,
                        duration_s: t.duration,
                        title: t.title,
                        file_path: t.file_path,
                    },
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "unreadable track row; its history entries stay unresolved")
            }
        }
    }
    let mut artists = HashMap::new();
    for r in file.rows_of(Table::Artists).map_err(|e| e.to_string())? {
        match pdb_rows::artist(r) {
            Ok((id, name)) => {
                artists.insert(id, name);
            }
            Err(e) => tracing::warn!(error = %e, "unreadable artist row"),
        }
    }
    Ok((tracks, artists))
}

struct RawTrack {
    artist_id: u32,
    duration_s: u16,
    title: String,
    file_path: String,
}

impl RawTrack {
    fn resolve(&self, artists: &HashMap<u32, String>) -> PdbTrack {
        PdbTrack {
            stick_path: self.file_path.clone(),
            title: self.title.clone(),
            artist: (self.artist_id != 0)
                .then(|| artists.get(&self.artist_id).cloned())
                .flatten(),
            duration_ms: (self.duration_s != 0).then(|| i64::from(self.duration_s) * 1000),
        }
    }
}

/// The HIST1 heuristic (module docs). Sets `played_on` on every named
/// session, or on none.
fn apply_dates(sessions: &mut [PdbSession], rows: Option<Vec<Vec<u8>>>) -> DatesStatus {
    let Some(rows) = rows else {
        return DatesStatus::NoTable;
    };
    let named: Vec<usize> = (0..sessions.len())
        .filter(|&i| sessions[i].name.is_some())
        .collect();
    if rows.len() != named.len() {
        return DatesStatus::Undecodable(format!(
            "{} table-19 rows for {} history playlists",
            rows.len(),
            named.len()
        ));
    }
    let mut dates = Vec::with_capacity(rows.len());
    for (n, row) in rows.iter().enumerate() {
        let found = dates_in(row);
        if found.len() != 1 {
            return DatesStatus::Undecodable(format!(
                "table-19 row {n} holds {} dates, not one",
                found.len()
            ));
        }
        dates.extend(found);
    }
    // ISO dates order as strings; players create sessions in time order.
    if dates.windows(2).any(|w| w[0] > w[1]) {
        return DatesStatus::Undecodable("dates go backwards by playlist id".into());
    }
    for (i, date) in named.into_iter().zip(dates) {
        sessions[i].played_on = Some(date);
    }
    DatesStatus::Decoded
}

/// Distinct valid ASCII `YYYY-MM-DD` dates anywhere in `row`.
fn dates_in(row: &[u8]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in row.windows(10) {
        let digits = |r: std::ops::Range<usize>| w[r].iter().all(u8::is_ascii_digit);
        if !(digits(0..4) && w[4] == b'-' && digits(5..7) && w[7] == b'-' && digits(8..10)) {
            continue;
        }
        let num =
            |r: std::ops::Range<usize>| w[r].iter().fold(0u32, |n, d| n * 10 + u32::from(d - b'0'));
        let (y, m, d) = (num(0..4), num(5..7), num(8..10));
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let days = match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => 0,
        };
        // Rejects version strings and other digit runs that only look like dates.
        if !(2000..=2999).contains(&y) || d == 0 || d > days {
            continue;
        }
        let date = String::from_utf8_lossy(w).into_owned();
        if !out.contains(&date) {
            out.push(date);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::history::tests::{snapshot, stick};
    use crate::history::{plan_backups, read_history, run_backups_at, HistoryPolicy};
    use crate::testutil::*;

    const PDB: &str = "PIONEER/rekordbox/export.pdb";

    /// Two sessions on different dates; track 8's artist row and track 42's
    /// track row are missing; entries stored out of order across pages.
    fn played(date_rows: Option<Vec<Vec<u8>>>) -> Vec<u8> {
        let mut tables = vec![
            (
                T_TRACKS,
                vec![vec![
                    track_row(5, 1, 301, "Alpha", "/Contents/A/x/01 Alpha.mp3"),
                    track_row(7, 2, 0, "Beta", "/Contents/B/y/Beta.flac"),
                    track_row(8, 99, 200, "Gamma", "/Contents/UnknownArtist/z/Gamma.wav"),
                ]],
            ),
            (
                T_ARTISTS,
                vec![vec![artist_row(1, "Artist A"), artist_row(2, "Artist B")]],
            ),
            (
                T_HISTORY_PLAYLISTS,
                vec![vec![
                    history_playlist(2, "HISTORY 002"),
                    history_playlist(1, "HISTORY 001"),
                ]],
            ),
            (
                T_HISTORY_ENTRIES,
                vec![
                    vec![history_entry(7, 1, 2), history_entry(42, 2, 2)],
                    vec![history_entry(5, 1, 1), history_entry(8, 2, 1)],
                ],
            ),
        ];
        if let Some(rows) = date_rows {
            tables.push((T_HISTORY, vec![rows]));
        }
        synthetic_pdb(&tables)
    }

    fn dated() -> Vec<u8> {
        played(Some(vec![
            history_date_row(1, "2026-10-17"),
            history_date_row(2, "2026-10-18"),
        ]))
    }

    fn opts(hist1_dates: bool) -> HistoryReadOptions {
        HistoryReadOptions {
            stick_serial: Some("4993-1CFB".into()),
            hist1_dates,
        }
    }

    fn track(path: &str, title: &str, artist: Option<&str>, ms: Option<i64>) -> Option<PdbTrack> {
        Some(PdbTrack {
            stick_path: path.into(),
            title: title.into(),
            artist: artist.map(Into::into),
            duration_ms: ms,
        })
    }

    fn expected(dates: [Option<&str>; 2]) -> Vec<PdbSession> {
        vec![
            PdbSession {
                id: 1,
                name: Some("HISTORY 001".into()),
                played_on: dates[0].map(Into::into),
                entries: vec![
                    PdbEntry {
                        index: 1,
                        track_id: 5,
                        track: track(
                            "/Contents/A/x/01 Alpha.mp3",
                            "Alpha",
                            Some("Artist A"),
                            Some(301_000),
                        ),
                    },
                    PdbEntry {
                        index: 2,
                        track_id: 7,
                        track: track("/Contents/B/y/Beta.flac", "Beta", Some("Artist B"), None),
                    },
                ],
            },
            PdbSession {
                id: 2,
                name: Some("HISTORY 002".into()),
                played_on: dates[1].map(Into::into),
                entries: vec![
                    PdbEntry {
                        index: 1,
                        track_id: 8,
                        track: track(
                            "/Contents/UnknownArtist/z/Gamma.wav",
                            "Gamma",
                            None,
                            Some(200_000),
                        ),
                    },
                    PdbEntry {
                        index: 2,
                        track_id: 42,
                        track: None,
                    },
                ],
            },
        ]
    }

    #[test]
    fn sessions_resolve_tracks_and_dates() {
        let s = stick(&[(PDB, &dated())]);
        let before = snapshot(s.path());
        let src = HistorySource::Stick(s.path().to_path_buf());

        let h = read_play_history(&src, &opts(true)).unwrap();
        assert_eq!(h.volume_serial.as_deref(), Some("4993-1CFB"));
        assert_eq!(h.dates, DatesStatus::Decoded);
        assert_eq!(
            h.sessions,
            expected([Some("2026-10-17"), Some("2026-10-18")])
        );

        // Flag off (the default): no dates, everything else the same.
        let h = read_play_history(&src, &HistoryReadOptions::default()).unwrap();
        assert_eq!(h.dates, DatesStatus::NotRead);
        assert_eq!(h.volume_serial, None);
        assert_eq!(h.sessions, expected([None, None]));

        assert_eq!(snapshot(s.path()), before, "reading touched the stick");
    }

    #[test]
    fn undecodable_dates_stay_null() {
        type Case = (Option<Vec<Vec<u8>>>, fn(&DatesStatus) -> bool);
        let cases: Vec<Case> = vec![
            (None, |d| *d == DatesStatus::NoTable),
            // Not a date.
            (
                Some(vec![
                    history_date_row(1, "2026-13-01"),
                    history_date_row(2, "garbage!!!"),
                ]),
                |d| matches!(d, DatesStatus::Undecodable(_)),
            ),
            // One row too few.
            (Some(vec![history_date_row(1, "2026-10-17")]), |d| {
                matches!(d, DatesStatus::Undecodable(_))
            }),
            // Two dates in one row.
            (
                Some(vec![
                    history_date_row(1, "2026-10-17"),
                    [
                        history_date_row(2, "2026-10-18"),
                        short_string("2026-10-19"),
                    ]
                    .concat(),
                ]),
                |d| matches!(d, DatesStatus::Undecodable(_)),
            ),
            // Backwards in time by playlist id.
            (
                Some(vec![
                    history_date_row(1, "2026-10-18"),
                    history_date_row(2, "2026-10-17"),
                ]),
                |d| matches!(d, DatesStatus::Undecodable(_)),
            ),
            // Empty table 19.
            (Some(vec![]), |d| matches!(d, DatesStatus::Undecodable(_))),
        ];
        for (rows, ok) in cases {
            let s = stick(&[(PDB, &played(rows))]);
            let h = read_play_history(&HistorySource::Stick(s.path().into()), &opts(true)).unwrap();
            assert!(ok(&h.dates), "{:?}", h.dates);
            assert_eq!(h.sessions, expected([None, None]));
        }
    }

    #[test]
    fn dates_in_finds_valid_iso_dates_only() {
        assert_eq!(dates_in(b"\x17\x002024-02-29\x00"), ["2024-02-29"]);
        assert!(dates_in(b"2023-02-29").is_empty());
        assert!(dates_in(b"1999-12-31 0000-01-01 2026-00-10 2026-04-31").is_empty());
        assert_eq!(dates_in(b"2026-10-18 2026-10-18"), ["2026-10-18"]);
        assert!(dates_in(b"2026-10-1").is_empty());
    }

    #[test]
    fn raw_rows_follow_the_row_index() {
        let pdb = synthetic_pdb(&[(
            T_HISTORY,
            vec![vec![b"aaaa".to_vec(), b"bbbbbbbb".to_vec(), b"cc".to_vec()]],
        )]);
        let file = Pdb::parse(&pdb).unwrap();
        let rows: Vec<Vec<u8>> = file
            .rows_of(Table::History)
            .unwrap()
            .into_iter()
            .map(<[u8]>::to_vec)
            .collect();
        // Rows are padded to 4 bytes in the heap.
        assert_eq!(
            rows,
            [b"aaaa".to_vec(), b"bbbbbbbb".to_vec(), b"cc\0\0".to_vec()]
        );
    }

    #[test]
    fn no_pdb_or_no_history() {
        let empty = tempfile::tempdir().unwrap();
        let h = read_play_history(&HistorySource::Stick(empty.path().into()), &opts(true)).unwrap();
        assert!(h.sessions.is_empty());
        assert_eq!(h.dates, DatesStatus::NoSessions);

        let s = stick(&[(PDB, &unplayed_pdb())]);
        let h = read_play_history(&HistorySource::Stick(s.path().into()), &opts(false)).unwrap();
        assert!(h.sessions.is_empty());

        let bad = stick(&[(PDB, b"garbage")]);
        assert!(matches!(
            read_play_history(&HistorySource::Stick(bad.path().into()), &opts(false)),
            Err(HistoryError::Unreadable { .. })
        ));
    }

    #[test]
    fn orphan_entries_form_an_unnamed_session() {
        let pdb = synthetic_pdb(&[
            (T_HISTORY_PLAYLISTS, vec![vec![]]),
            (T_HISTORY_ENTRIES, vec![vec![history_entry(5, 9, 1)]]),
            (T_HISTORY, vec![vec![]]),
        ]);
        let s = stick(&[(PDB, &pdb)]);
        let h = read_play_history(&HistorySource::Stick(s.path().into()), &opts(true)).unwrap();
        assert_eq!(
            h.sessions,
            [PdbSession {
                id: 9,
                name: None,
                played_on: None,
                entries: vec![PdbEntry {
                    index: 1,
                    track_id: 5,
                    track: None
                }],
            }]
        );
        // Nothing to date.
        assert_eq!(h.dates, DatesStatus::NoSessions);
    }

    #[test]
    fn backup_folder_reads_like_the_stick() {
        let s = stick(&[(PDB, &dated())]);
        let data = tempfile::tempdir().unwrap();
        let found = read_history(s.path(), Some("4993-1CFB".into()));
        let plan = plan_backups(s.path(), &found, HistoryPolicy::BackUp, false).unwrap();
        let report = run_backups_at(&plan, data.path(), "2026-10-19T15:30:12.123Z")
            .unwrap()
            .unwrap();

        let from_stick =
            read_play_history(&HistorySource::Stick(s.path().into()), &opts(true)).unwrap();
        let backup = HistorySource::Backup(report.dir.clone());
        // The serial comes from the manifest, not the options.
        let from_backup = read_play_history(
            &backup,
            &HistoryReadOptions {
                stick_serial: None,
                hist1_dates: true,
            },
        )
        .unwrap();
        assert_eq!(from_backup, from_stick);

        // A changed copy is refused.
        let copy = report.dir.join(PDB);
        let mut bytes = fs::read(&copy).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&copy, &bytes).unwrap();
        assert!(matches!(
            read_play_history(&backup, &opts(true)),
            Err(HistoryError::HashMismatch { .. })
        ));

        // No manifest: not a (complete) backup.
        fs::remove_file(report.dir.join(BACKUP_MANIFEST)).unwrap();
        assert!(matches!(
            read_play_history(&backup, &opts(true)),
            Err(HistoryError::Io { .. })
        ));
    }

    #[test]
    fn json_shape_is_stable() {
        let s = stick(&[(PDB, &dated())]);
        let h = read_play_history(&HistorySource::Stick(s.path().into()), &opts(true)).unwrap();
        let json = serde_json::to_value(&h).unwrap();
        assert_eq!(json["dates"], serde_json::json!({ "status": "decoded" }));
        assert_eq!(json["sessions"][0]["playedOn"], "2026-10-17");
        assert_eq!(
            json["sessions"][0]["entries"][0]["track"]["stickPath"],
            "/Contents/A/x/01 Alpha.mp3"
        );
        assert_eq!(
            json["sessions"][1]["entries"][1]["track"],
            serde_json::Value::Null
        );
        let back: PlayHistory = serde_json::from_value(json).unwrap();
        assert_eq!(back, h);
    }

    /// Task 6 end to end on a synthetic stick: read, convert, scan, import;
    /// the stick is unchanged and a second import adds nothing.
    #[test]
    fn synthetic_stick_imports_once() {
        use reks_core::history::{self, EntryKind, ImportChoices, MatchKind, SessionAction};
        use reks_core::{Library, ManifestItemKind, ManifestRow};

        let s = stick(&[(PDB, &played(None))]);
        let before = snapshot(s.path());
        let music = tempfile::tempdir().unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/audio");
        for (fixture, rel) in [
            ("sine-click-128.mp3", "a.mp3"),
            ("sine-click-128.flac", "B/y/Beta.flac"),
        ] {
            let dst = music.path().join(rel);
            fs::create_dir_all(dst.parent().unwrap()).unwrap();
            fs::copy(fixtures.join(fixture), dst).unwrap();
        }
        let lib = Library::open_in_memory(&music.path().join("cache")).unwrap();
        reks_core::scan(
            &[music.path().to_path_buf()],
            &lib,
            &reks_core::CancelToken::new(),
            |_| {},
        )
        .unwrap();
        let id_of = |kind: &str| {
            lib.tracks(&reks_core::TrackQuery::default())
                .unwrap()
                .rows
                .iter()
                .find(|r| r.kind == kind)
                .map(|r| r.id)
                .unwrap()
        };
        let ids = [id_of("mp3"), id_of("flac")];
        lib.replace_manifest(
            Some("4993-1CFB"),
            &[
                ManifestRow {
                    stick_path: "/Contents/A/x/01 Alpha.mp3".into(),
                    item_kind: ManifestItemKind::Track,
                    track_id: Some(ids[0]),
                },
                ManifestRow {
                    stick_path: "/Contents/UnknownArtist/z/Gamma.wav".into(),
                    item_kind: ManifestItemKind::Divider,
                    track_id: None,
                },
            ],
        )
        .unwrap();

        let h = read_play_history(&HistorySource::Stick(s.path().into()), &opts(false))
            .unwrap()
            .to_source_history();
        let plan = history::scan(&h, &lib).unwrap();
        let got: Vec<Vec<_>> = plan
            .sessions
            .iter()
            .map(|s| {
                s.entries
                    .iter()
                    .map(|e| (e.kind, e.track_id, e.matched_by))
                    .collect()
            })
            .collect();
        assert_eq!(
            got,
            vec![
                vec![
                    (EntryKind::Track, Some(ids[0]), Some(MatchKind::Manifest)),
                    (EntryKind::Track, Some(ids[1]), Some(MatchKind::PathSuffix)),
                ],
                vec![
                    (EntryKind::Divider, None, Some(MatchKind::Manifest)),
                    (EntryKind::Track, None, None),
                ],
            ]
        );
        let report = history::import(&plan, &ImportChoices::default(), &lib).unwrap();
        assert_eq!((report.created.len(), report.unmatched), (2, 1));

        let again = history::scan(&h, &lib).unwrap();
        assert!(again
            .sessions
            .iter()
            .all(|s| matches!(s.action, SessionAction::Skip(_))));
        assert_eq!(snapshot(s.path()), before, "import touched the stick");
    }

    /// Golden test on a real played stick (hardware checklist step 7),
    /// sanitised, at `fixtures/rekordbox/played-<ver>/` with
    /// `PIONEER/rekordbox/export.pdb` and the expected `history.json` (a
    /// [`PlayHistory`], two or more sessions from different dates). Set
    /// `REKS_PLAYED_FIXTURE` to that folder and run with `--ignored`. Make it
    /// a plain test, and check HIST1 against it, once the fixture is in.
    #[test]
    #[ignore = "needs fixtures/rekordbox/played-<ver>/ from the hardware checklist (step 7)"]
    fn played_stick_fixture_golden_json() {
        let root = PathBuf::from(std::env::var("REKS_PLAYED_FIXTURE").unwrap());
        let expected: PlayHistory =
            serde_json::from_slice(&fs::read(root.join("history.json")).unwrap()).unwrap();
        let h = read_play_history(
            &HistorySource::Stick(root),
            &HistoryReadOptions {
                stick_serial: expected.volume_serial.clone(),
                hist1_dates: true,
            },
        )
        .unwrap();
        assert!(h.sessions.len() >= 2);
        assert_eq!(h, expected);
    }

    /// Task 6 on the same fixture: one session per history playlist, entries
    /// in player order, and a second import adds nothing. With the export
    /// manifest of a reks-made stick, every entry should match (acceptance
    /// criteria); that needs the library the stick came from, so it is not
    /// asserted here.
    #[test]
    #[ignore = "needs fixtures/rekordbox/played-<ver>/ from the hardware checklist (step 7)"]
    fn played_stick_fixture_imports_once() {
        use reks_core::history::{self, ImportChoices};
        let root = PathBuf::from(std::env::var("REKS_PLAYED_FIXTURE").unwrap());
        let h = read_play_history(&HistorySource::Stick(root.clone()), &opts(false))
            .unwrap()
            .to_source_history();
        let before = snapshot(&root);
        let lib = reks_core::Library::open_in_memory(Path::new("cache-unused")).unwrap();
        let plan = history::scan(&h, &lib).unwrap();
        let report = history::import(&plan, &ImportChoices::default(), &lib).unwrap();
        assert_eq!(report.created.len(), h.sessions.len());
        let again = history::import(
            &history::scan(&h, &lib).unwrap(),
            &ImportChoices::default(),
            &lib,
        )
        .unwrap();
        assert_eq!((again.created.len(), again.extended.len()), (0, 0));
        assert_eq!(snapshot(&root), before);
    }
}
