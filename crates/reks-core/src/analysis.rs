//! Analysis results in the library (phase 3 task 11): which tracks need
//! analysing, and storing what the analyser found without overwriting data
//! a person or rekordbox provided.
//!
//! The analyser itself and the queue live in `reks-analysis` (it depends on
//! this crate, not the other way round). Columns: migration 0007.
//!
//! Precedence rules (docs/phases/phase-3.md, task 11):
//! - `beatgrid` rows are replaced only when the track has no rows or all of
//!   them have `source = 'analysis'`, and the grid is not locked
//!   (`grid_locked`, migration 0006). Manual, locked and rekordbox grids
//!   stay; reks's own grid is still in the on-disk cache.
//! - `bpm_x100` follows the grid: set (`bpm_source = 'analysis'`) when the
//!   grid was written, unless `bpm_source = 'manual'`.
//! - `key` is set only when it is empty or came from analysis; manual, tag
//!   and rekordbox keys stay.
//! - `analysis_version`, `analysis_hash`, confidences and `analysed_at` are
//!   always written, so the track leaves the pending list either way. The
//!   hash is the file's current one, so it also refreshes `content_hash`
//!   (a file edited since the last scan would otherwise stay pending).
//! - Pending tracks with a rekordbox grid are queued after the rest.
//! - A track whose rekordbox analysis was mapped into the cache (D10,
//!   [`Library::store_rekordbox_mapping`]) is not pending while its file is
//!   unchanged; a run of reks's analyser (Re-analyse) takes over from it
//!   (`analysis_source` = `reks`).

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::db::Library;
use crate::error::{Error, Result};
use crate::fileinfo;
use crate::keys::Key;
use crate::scan::TrackId;
use crate::timefmt::now_utc;

/// `beatgrid.source` and `key_source`/`bpm_source` for analyser output.
pub const SOURCE_ANALYSIS: &str = "analysis";
/// `beatgrid.source` and `key_source`/`bpm_source` for user edits.
pub const SOURCE_MANUAL: &str = "manual";
/// `analysis_source` of a track whose rekordbox analysis was mapped.
pub const SOURCE_REKORDBOX: &str = "rekordbox";
/// `analysis_source` of a track reks's analyser ran on.
pub const SOURCE_REKS: &str = "reks";

/// One track to analyse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisJob {
    pub track_id: TrackId,
    /// Absolute path of the audio file.
    pub path: String,
}

/// One tempo anchor of an analysed grid (a `beatgrid` row).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalysisAnchor {
    /// First beat at or after the anchor, in whole ms from the first decoded
    /// sample (beatgrid positions are always whole ms, see D10).
    pub position_ms: i64,
    /// Tempo from this anchor on, BPM × 100.
    pub bpm_x100: i64,
    /// 1–4: position of the beat at `position_ms` in its bar.
    pub beat_in_bar: u8,
}

/// What one analyser run produced, in library terms.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisRecord {
    /// `reks_analysis::ANALYSER_VERSION`.
    pub version: u32,
    /// [`file_content_hash`] of the file that was analysed.
    pub content_hash: String,
    /// Empty when no grid could be built (silence, too few beats).
    pub grid: Vec<AnalysisAnchor>,
    /// Display BPM × 100 (the first anchor's for a constant grid).
    pub bpm_x100: Option<i64>,
    /// `None` = silence.
    pub key: Option<Key>,
    pub beat_confidence: f32,
    pub key_confidence: f32,
}

/// Which library fields [`Library::store_analysis`] changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreOutcome {
    pub grid: bool,
    pub bpm: bool,
    pub key: bool,
}

/// Content hash used for cache invalidation (blake3 of the file's edges and
/// size, the same value the scan stores in `track.content_hash`).
pub fn file_content_hash(path: &Path) -> Result<String> {
    fileinfo::content_hash(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// What the library recorded about a track's file when it last hashed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFile {
    pub path: String,
    pub size: i64,
    pub mtime: String,
    pub content_hash: String,
}

/// [`file_content_hash`] of `path`, taken from `stored` without reading the
/// file when it is the same path with the same size and mtime. That is the
/// folder scan's own test for "unchanged", with the same blind spot: an edit
/// that keeps both size and mtime goes unnoticed.
pub fn current_content_hash(path: &Path, stored: Option<&StoredFile>) -> Result<String> {
    if let Some(s) = stored {
        let unchanged = Path::new(&s.path) == path
            && fileinfo::stat(&s.path)
                .is_ok_and(|(size, mtime)| size == s.size && mtime == s.mtime);
        if unchanged {
            return Ok(s.content_hash.clone());
        }
    }
    file_content_hash(path)
}

/// Tracks that need an analyser run: never analysed, analysed by another
/// analyser version, or whose file changed since (by `content_hash`). A
/// failed run counts as a run, so failures are not retried until either
/// changes; missing files are skipped. A track whose rekordbox analysis was
/// mapped into the cache (`analysis_source = 'rekordbox'`) is done while its
/// file is unchanged, whatever the analyser version (D10).
const PENDING_WHERE: &str = "missing = 0
       AND NOT (analysis_source IS 'rekordbox' AND content_hash IS NOT NULL
                AND analysis_hash IS content_hash)
       AND (analysis_version IS NULL OR analysis_version <> ?1
            OR (content_hash IS NOT NULL AND analysis_hash IS NOT content_hash))";

const STORED_FILE: &str = "SELECT id, path, file_size, file_mtime, content_hash FROM track";

fn stored_file_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(TrackId, StoredFile)> {
    Ok((
        r.get(0)?,
        StoredFile {
            path: r.get(1)?,
            size: r.get(2)?,
            mtime: r.get(3)?,
            content_hash: r.get(4)?,
        },
    ))
}

/// Whether the track has a rekordbox grid (SQL on `track`).
const HAS_RB_GRID: &str = "EXISTS (SELECT 1 FROM beatgrid
                      WHERE beatgrid.track_id = track.id AND beatgrid.source = 'rekordbox')";

/// Pending tracks; those with a rekordbox grid come last: the library
/// already has their grid and BPM from the XML import, which analysis keeps
/// (D10), so tracks with nothing yet get their grid first.
fn pending_sql() -> String {
    format!("SELECT id, path FROM track WHERE {PENDING_WHERE} ORDER BY {HAS_RB_GRID}, id")
}

/// Pending tracks that rekordbox analysed: a rekordbox grid and a key.
/// Columns: id, path, duration_ms, first rekordbox anchor's position_ms and
/// bpm_x100.
fn mapping_sql() -> String {
    format!(
        "SELECT id, path, duration_ms,
                (SELECT position_ms FROM beatgrid b WHERE b.track_id = track.id
                   AND b.source = 'rekordbox' ORDER BY position_ms LIMIT 1),
                (SELECT bpm_x100 FROM beatgrid b WHERE b.track_id = track.id
                   AND b.source = 'rekordbox' ORDER BY position_ms LIMIT 1)
         FROM track WHERE {PENDING_WHERE} AND key IS NOT NULL AND {HAS_RB_GRID}
         ORDER BY id"
    )
}

/// A pending track whose rekordbox analysis may be mapped into the cache
/// instead of running the analyser (D10).
#[derive(Debug, Clone, PartialEq)]
pub struct MappingCandidate {
    pub track_id: TrackId,
    /// Absolute path of the audio file.
    pub path: String,
    pub duration_ms: Option<i64>,
    /// First rekordbox grid anchor: whole ms and BPM × 100 (the XML's first
    /// `TEMPO`, which rekordbox's `PQTZ` must agree with).
    pub first_beat_ms: i64,
    pub bpm_x100: i64,
}

impl Library {
    /// Every track's [`StoredFile`] (tracks never hashed have none).
    pub fn stored_files(&self) -> Result<HashMap<TrackId, StoredFile>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!("{STORED_FILE} WHERE content_hash IS NOT NULL"))?;
        let rows = stmt.query_map([], stored_file_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Track `id`'s [`StoredFile`].
    pub fn stored_file(&self, id: TrackId) -> Result<Option<StoredFile>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare_cached(&format!(
            "{STORED_FILE} WHERE id = ?1 AND content_hash IS NOT NULL"
        ))?;
        let row = stmt.query_row([id], stored_file_row).optional()?;
        Ok(row.map(|(_, f)| f))
    }

    /// Tracks needing analysis by analyser `version`, oldest first.
    pub fn analysis_pending(&self, version: u32) -> Result<Vec<AnalysisJob>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&pending_sql())?;
        let rows = stmt.query_map([version], |r| {
            Ok(AnalysisJob {
                track_id: r.get(0)?,
                path: r.get(1)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Jobs for `ids` (a re-analyse selection), in the given order. Unknown
    /// ids and missing files are skipped.
    pub fn analysis_jobs(&self, ids: &[TrackId]) -> Result<Vec<AnalysisJob>> {
        let conn = self.conn()?;
        let mut stmt =
            conn.prepare_cached("SELECT path FROM track WHERE id = ?1 AND missing = 0")?;
        let mut out = Vec::with_capacity(ids.len());
        for &id in ids {
            if let Some(path) = stmt.query_row([id], |r| r.get(0)).optional()? {
                out.push(AnalysisJob { track_id: id, path });
            }
        }
        Ok(out)
    }

    /// Pending tracks (by analyser `version`) that have a rekordbox grid and
    /// a key, oldest first.
    pub fn mapping_candidates(&self, version: u32) -> Result<Vec<MappingCandidate>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&mapping_sql())?;
        let rows = stmt.query_map([version], |r| {
            Ok(MappingCandidate {
                track_id: r.get(0)?,
                path: r.get(1)?,
                duration_ms: r.get(2)?,
                first_beat_ms: r.get::<_, f64>(3)?.round() as i64,
                bpm_x100: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Records that track `id`'s rekordbox analysis was mapped into the
    /// cache for the file with `content_hash`: the track leaves the pending
    /// list until the file changes. Grid, key and BPM stay the library's
    /// (rekordbox's, from the import); there are no confidences.
    pub fn store_rekordbox_mapping(&self, id: TrackId, content_hash: &str) -> Result<()> {
        let conn = self.conn()?;
        let n = conn.execute(
            "UPDATE track SET analysis_source = ?2, analysis_version = NULL,
                    analysis_hash = ?3, content_hash = ?3, analysis_error = NULL,
                    beat_confidence = NULL, key_confidence = NULL, analysed_at = ?4
             WHERE id = ?1",
            params![id, SOURCE_REKORDBOX, content_hash, now_utc()],
        )?;
        if n == 0 {
            return Err(Error::TrackNotFound(id));
        }
        Ok(())
    }

    /// Stores `rec` for track `id` following the precedence rules in the
    /// module docs, in one transaction.
    pub fn store_analysis(&self, id: TrackId, rec: &AnalysisRecord) -> Result<StoreOutcome> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let out = store(&tx, id, rec)?;
        tx.commit()?;
        Ok(out)
    }

    /// Records a failed run so the track leaves the pending list until the
    /// analyser version or the file changes. Grid and key stay as they are.
    pub fn store_analysis_error(
        &self,
        id: TrackId,
        version: u32,
        content_hash: Option<&str>,
        message: &str,
    ) -> Result<()> {
        let conn = self.conn()?;
        // Without a hash of our own (the file could not be read), take the
        // scan's, so the pending query doesn't pick the track up again at once.
        let n = conn.execute(
            "UPDATE track SET analysis_version = ?2, analysis_source = 'reks',
                    analysis_hash = coalesce(?3, content_hash),
                    content_hash = coalesce(?3, content_hash),
                    analysis_error = ?4, analysed_at = ?5
             WHERE id = ?1",
            params![id, version, content_hash, message, now_utc()],
        )?;
        if n == 0 {
            return Err(Error::TrackNotFound(id));
        }
        Ok(())
    }
}

fn store(conn: &Connection, id: TrackId, rec: &AnalysisRecord) -> Result<StoreOutcome> {
    let (key_source, bpm_source, grid_locked): (Option<String>, Option<String>, bool) = conn
        .query_row(
            "SELECT key_source, bpm_source, grid_locked FROM track WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or(Error::TrackNotFound(id))?;
    let mut out = StoreOutcome::default();

    let foreign_rows: i64 = conn.query_row(
        "SELECT count(*) FROM beatgrid WHERE track_id = ?1 AND source <> ?2",
        params![id, SOURCE_ANALYSIS],
        |r| r.get(0),
    )?;
    if foreign_rows == 0 && !grid_locked {
        conn.execute("DELETE FROM beatgrid WHERE track_id = ?1", [id])?;
        let mut insert = conn.prepare_cached(
            "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, beat_in_bar, source)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for a in &rec.grid {
            insert.execute(params![
                id,
                a.position_ms,
                a.bpm_x100,
                a.beat_in_bar,
                SOURCE_ANALYSIS
            ])?;
        }
        out.grid = true;
        if bpm_source.as_deref() != Some(SOURCE_MANUAL) {
            conn.execute(
                "UPDATE track SET bpm_x100 = ?2, bpm_source = ?3 WHERE id = ?1",
                params![id, rec.bpm_x100, rec.bpm_x100.map(|_| SOURCE_ANALYSIS)],
            )?;
            out.bpm = true;
        }
    }

    // `key IS NULL` with a stale source cannot happen through our writers,
    // but an empty key is always fair game.
    let key_empty: bool =
        conn.query_row("SELECT key IS NULL FROM track WHERE id = ?1", [id], |r| {
            r.get(0)
        })?;
    if key_empty || key_source.as_deref() == Some(SOURCE_ANALYSIS) {
        conn.execute(
            "UPDATE track SET key = ?2, key_source = ?3 WHERE id = ?1",
            params![
                id,
                rec.key.map(Key::canonical),
                rec.key.map(|_| SOURCE_ANALYSIS)
            ],
        )?;
        out.key = true;
    }

    conn.execute(
        "UPDATE track SET analysis_version = ?2, analysis_hash = ?3, content_hash = ?3,
                analysis_error = NULL, analysis_source = ?7,
                beat_confidence = ?4, key_confidence = ?5, analysed_at = ?6
         WHERE id = ?1",
        params![
            id,
            rec.version,
            rec.content_hash,
            f64::from(rec.beat_confidence),
            f64::from(rec.key_confidence),
            now_utc(),
            SOURCE_REKS
        ],
    )?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open_in_memory(dir.path()).unwrap();
        (dir, lib)
    }

    fn insert(lib: &Library, path: &str) -> TrackId {
        let c = lib.conn().unwrap();
        c.execute(
            "INSERT INTO track (path, file_size, file_mtime, kind, content_hash,
                                date_added, created_at, updated_at)
             VALUES (?1, 1, '2026-01-01T00:00:00Z', 'wav', 'h1',
                     '2026-01-01', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [path],
        )
        .unwrap();
        c.last_insert_rowid()
    }

    #[test]
    fn stored_hash_is_used_only_while_the_file_matches_its_row() {
        let (dir, lib) = lib();
        let file = dir.path().join("a.wav");
        std::fs::write(&file, b"audio").unwrap();
        let path = file.to_str().unwrap();
        let id = insert(&lib, path);
        let (size, mtime) = fileinfo::stat(path).unwrap();
        lib.conn()
            .unwrap()
            .execute(
                "UPDATE track SET file_size = ?2, file_mtime = ?3 WHERE id = ?1",
                params![id, size, mtime],
            )
            .unwrap();
        let stored = lib.stored_file(id).unwrap().unwrap();
        assert_eq!(lib.stored_files().unwrap().get(&id), Some(&stored));

        // Same path, size and mtime: the row's hash, the file is not read.
        assert_eq!(current_content_hash(&file, Some(&stored)).unwrap(), "h1");
        // Any difference, or no row: the file's own hash.
        let real = file_content_hash(&file).unwrap();
        let other_size = StoredFile {
            size: size + 1,
            ..stored.clone()
        };
        let other_path = StoredFile {
            path: dir.path().join("b.wav").to_string_lossy().into(),
            ..stored.clone()
        };
        for s in [Some(&other_size), Some(&other_path), None] {
            assert_eq!(current_content_hash(&file, s).unwrap(), real);
        }
        let other_mtime = StoredFile {
            mtime: "2000-01-01T00:00:00Z".into(),
            ..stored
        };
        assert_eq!(
            current_content_hash(&file, Some(&other_mtime)).unwrap(),
            real
        );

        // A track never hashed has no stored file.
        lib.conn()
            .unwrap()
            .execute("UPDATE track SET content_hash = NULL WHERE id = ?1", [id])
            .unwrap();
        assert_eq!(lib.stored_file(id).unwrap(), None);
        assert!(lib.stored_files().unwrap().is_empty());
    }

    fn record(hash: &str) -> AnalysisRecord {
        AnalysisRecord {
            version: 1,
            content_hash: hash.into(),
            grid: vec![AnalysisAnchor {
                position_ms: 13,
                bpm_x100: 12800,
                beat_in_bar: 1,
            }],
            bpm_x100: Some(12800),
            key: Some(Key::parse("Am").unwrap()),
            beat_confidence: 0.9,
            key_confidence: 0.4,
        }
    }

    fn pending_ids(lib: &Library, v: u32) -> Vec<TrackId> {
        lib.analysis_pending(v)
            .unwrap()
            .into_iter()
            .map(|j| j.track_id)
            .collect()
    }

    fn grid(lib: &Library, id: TrackId) -> Vec<(f64, i64, String)> {
        let c = lib.conn().unwrap();
        let mut s = c
            .prepare("SELECT position_ms, bpm_x100, source FROM beatgrid WHERE track_id = ?1 ORDER BY position_ms")
            .unwrap();
        s.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    fn key_bpm(
        lib: &Library,
        id: TrackId,
    ) -> (Option<String>, Option<String>, Option<i64>, Option<String>) {
        lib.conn()
            .unwrap()
            .query_row(
                "SELECT key, key_source, bpm_x100, bpm_source FROM track WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap()
    }

    #[test]
    fn pending_follows_version_and_hash() {
        let (_d, lib) = lib();
        let a = insert(&lib, "/m/a.wav");
        let b = insert(&lib, "/m/b.wav");
        assert_eq!(pending_ids(&lib, 1), [a, b]);
        lib.store_analysis(a, &record("h1")).unwrap();
        assert_eq!(pending_ids(&lib, 1), [b]);
        // New analyser version: everything again.
        assert_eq!(pending_ids(&lib, 2), [a, b]);
        // File changed (scan updated content_hash).
        lib.conn()
            .unwrap()
            .execute("UPDATE track SET content_hash = 'h2' WHERE id = ?1", [a])
            .unwrap();
        assert_eq!(pending_ids(&lib, 1), [a, b]);
        // Failures leave the list too; missing files are never listed.
        lib.store_analysis_error(b, 1, None, "boom").unwrap();
        lib.conn()
            .unwrap()
            .execute("UPDATE track SET missing = 1 WHERE id = ?1", [a])
            .unwrap();
        assert!(pending_ids(&lib, 1).is_empty());
        assert!(lib.analysis_jobs(&[a, b, 999]).unwrap().len() == 1);
    }

    #[test]
    fn rekordbox_gridded_tracks_are_analysed_last() {
        let (_d, lib) = lib();
        let a = insert(&lib, "/m/a.mp3");
        let b = insert(&lib, "/m/b.mp3");
        let c = insert(&lib, "/m/c.mp3");
        lib.conn()
            .unwrap()
            .execute(
                "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, source)
                 VALUES (?1, 50.0, 12000, 'rekordbox')",
                [a],
            )
            .unwrap();
        assert_eq!(pending_ids(&lib, 1), [b, c, a]);
    }

    #[test]
    fn mapped_tracks_leave_the_pending_list_until_the_file_changes() {
        let (_d, lib) = lib();
        let a = insert(&lib, "/m/a.mp3");
        let b = insert(&lib, "/m/b.mp3");
        let c = insert(&lib, "/m/c.mp3");
        {
            let conn = lib.conn().unwrap();
            for (id, at) in [(a, 50.4), (a, 10_000.0), (b, 80.0)] {
                conn.execute(
                    "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, source)
                     VALUES (?1, ?2, 12800, 'rekordbox')",
                    params![id, at],
                )
                .unwrap();
            }
            conn.execute(
                "UPDATE track SET key = 'Am', key_source = 'rekordbox', duration_ms = 300000
                 WHERE id IN (?1, ?2)",
                [a, c],
            )
            .unwrap();
        }
        // b has no key, c no rekordbox grid: only a can be mapped.
        let cands = lib.mapping_candidates(1).unwrap();
        assert_eq!(
            cands,
            [MappingCandidate {
                track_id: a,
                path: "/m/a.mp3".into(),
                duration_ms: Some(300_000),
                first_beat_ms: 50,
                bpm_x100: 12800,
            }]
        );
        lib.store_rekordbox_mapping(a, "h1").unwrap();
        assert_eq!(pending_ids(&lib, 1), [c, b]);
        // A new analyser version doesn't bring it back; a changed file does.
        assert_eq!(pending_ids(&lib, 2), [c, b]);
        assert!(lib.mapping_candidates(2).unwrap().is_empty());
        lib.conn()
            .unwrap()
            .execute("UPDATE track SET content_hash = 'h2' WHERE id = ?1", [a])
            .unwrap();
        assert_eq!(pending_ids(&lib, 1), [c, a, b]);
        // Re-analyse: reks's run takes over, grid and key stay rekordbox's.
        lib.store_analysis(a, &record("h2")).unwrap();
        let source: String = lib
            .conn()
            .unwrap()
            .query_row(
                "SELECT analysis_source FROM track WHERE id = ?1",
                [a],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(source, "reks");
        assert_eq!(grid(&lib, a)[0].2, "rekordbox");
        assert!(lib.store_rekordbox_mapping(999, "h").is_err());
    }

    #[test]
    fn analysis_fills_empty_track_and_replaces_its_own_output() {
        let (_d, lib) = lib();
        let a = insert(&lib, "/m/a.wav");
        let out = lib.store_analysis(a, &record("h1")).unwrap();
        assert_eq!(
            out,
            StoreOutcome {
                grid: true,
                bpm: true,
                key: true
            }
        );
        assert_eq!(grid(&lib, a), [(13.0, 12800, "analysis".into())]);
        let mut again = record("h1");
        again.grid[0].bpm_x100 = 12900;
        again.bpm_x100 = Some(12900);
        again.key = None;
        lib.store_analysis(a, &again).unwrap();
        assert_eq!(grid(&lib, a), [(13.0, 12900, "analysis".into())]);
        assert_eq!(
            key_bpm(&lib, a),
            (None, None, Some(12900), Some("analysis".into()))
        );
    }

    #[test]
    fn manual_and_rekordbox_data_survive() {
        let (_d, lib) = lib();
        let a = insert(&lib, "/m/a.wav");
        {
            let c = lib.conn().unwrap();
            c.execute(
                "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, source)
                 VALUES (?1, 100.0, 12400, 'manual')",
                [a],
            )
            .unwrap();
            c.execute(
                "UPDATE track SET key = 'Dm', key_source = 'manual',
                                  bpm_x100 = 12400, bpm_source = 'manual' WHERE id = ?1",
                [a],
            )
            .unwrap();
        }
        let out = lib.store_analysis(a, &record("h1")).unwrap();
        assert_eq!(out, StoreOutcome::default());
        assert_eq!(grid(&lib, a), [(100.0, 12400, "manual".into())]);
        assert_eq!(
            key_bpm(&lib, a),
            (
                Some("Dm".into()),
                Some("manual".into()),
                Some(12400),
                Some("manual".into())
            )
        );
        assert!(pending_ids(&lib, 1).is_empty());

        // A rekordbox grid stays, and a tag key too.
        let b = insert(&lib, "/m/b.wav");
        {
            let c = lib.conn().unwrap();
            c.execute(
                "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, source)
                 VALUES (?1, 50.0, 12000, 'rekordbox')",
                [b],
            )
            .unwrap();
            c.execute(
                "UPDATE track SET key = 'C', key_source = 'tag' WHERE id = ?1",
                [b],
            )
            .unwrap();
        }
        assert_eq!(
            lib.store_analysis(b, &record("h1")).unwrap(),
            StoreOutcome::default()
        );
        assert_eq!(grid(&lib, b), [(50.0, 12000, "rekordbox".into())]);
        assert_eq!(key_bpm(&lib, b).0.as_deref(), Some("C"));
    }

    #[test]
    fn manual_bpm_survives_an_analysis_grid() {
        let (_d, lib) = lib();
        let a = insert(&lib, "/m/a.wav");
        lib.conn()
            .unwrap()
            .execute(
                "UPDATE track SET bpm_x100 = 6400, bpm_source = 'manual' WHERE id = ?1",
                [a],
            )
            .unwrap();
        let out = lib.store_analysis(a, &record("h1")).unwrap();
        assert!(out.grid && !out.bpm);
        assert_eq!(key_bpm(&lib, a).2, Some(6400));
    }

    #[test]
    fn locked_analysis_grid_survives() {
        let (_d, lib) = lib();
        let a = insert(&lib, "/m/a.wav");
        {
            let c = lib.conn().unwrap();
            c.execute(
                "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, source)
                 VALUES (?1, 75.0, 12600, 'analysis')",
                [a],
            )
            .unwrap();
            c.execute("UPDATE track SET grid_locked = 1 WHERE id = ?1", [a])
                .unwrap();
        }
        let out = lib.store_analysis(a, &record("h1")).unwrap();
        assert!(!out.grid && !out.bpm);
        assert_eq!(grid(&lib, a), [(75.0, 12600, "analysis".into())]);
    }

    #[test]
    fn unknown_track_is_an_error() {
        let (_d, lib) = lib();
        assert!(matches!(
            lib.store_analysis(7, &record("h")),
            Err(Error::TrackNotFound(7))
        ));
        assert!(lib.store_analysis_error(7, 1, None, "x").is_err());
    }
}
