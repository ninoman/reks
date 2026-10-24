//! On-demand waveform columns for a library track, and their cache.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use reks_analysis::beats::BeatList;
use reks_analysis::cache::{self, RekordboxEntry};
use reks_analysis::columns::{columns_path, stored_columns, track_columns, track_overview};
use reks_analysis::key::KeyAnalysis;
use reks_analysis::pipeline::TrackAnalysis;
use reks_analysis::waveform::{WaveformColumns, COLUMNS_PER_SECOND};
use reks_core::analysis::file_content_hash;
use reks_core::{scan, CancelToken, Library, TrackId};

/// A library with the 30 s fixture scanned in: (temp dir, library, track id).
fn one_track() -> (tempfile::TempDir, Library, TrackId) {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap();
    let music = dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    let src =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/audio/sine-click-128.wav");
    std::fs::copy(src, music.join("a.wav")).unwrap();
    scan(&[music], &lib, &CancelToken::new(), |_| {}).unwrap();
    let id: TrackId = rusqlite::Connection::open(dir.path().join("reks.sqlite"))
        .unwrap()
        .query_row("SELECT id FROM track", [], |r| r.get(0))
        .unwrap();
    (dir, lib, id)
}

fn flat(c: &WaveformColumns) -> Vec<u8> {
    c.to_interleaved().unwrap()
}

#[test]
fn columns_are_computed_once_then_read_from_the_cache() {
    let (_dir, lib, id) = one_track();
    let cache = lib.cache_dir();

    let first = track_columns(&lib, id).unwrap();
    // 30 s at 150 columns/s, 4 bytes each.
    assert_eq!(first.len(), 30 * COLUMNS_PER_SECOND as usize * 4);
    assert!(
        first.as_chunks::<4>().0.iter().any(|c| c[3] > 0),
        "silent columns"
    );
    let path = columns_path(&cache, id);
    assert!(path.exists());

    // A cache hit returns the same bytes; a corrupt file is recomputed.
    assert_eq!(track_columns(&lib, id).unwrap(), first);
    std::fs::write(&path, b"junk").unwrap();
    assert_eq!(track_columns(&lib, id).unwrap(), first);
    assert!(std::fs::read(&path).unwrap().len() > first.len());
}

#[test]
fn thumbnails_use_stored_analyses_before_decoding() {
    let (_dir, lib, id) = one_track();
    let cache_dir = lib.cache_dir();
    let (path, _) = lib.track_file_key(id).unwrap();
    let hash = file_content_hash(&path).unwrap();
    let never = |_: &RekordboxEntry| -> Option<WaveformColumns> { panic!("no rekordbox entry") };

    // Nothing stored: no thumbnail without decoding, and nothing decoded.
    assert_eq!(stored_columns(&lib, id, never).unwrap(), None);
    assert!(track_overview(&lib, id, 8, false, never)
        .unwrap()
        .is_empty());
    assert!(!columns_path(&cache_dir, id).exists());

    // A mapped rekordbox analysis is converted by the caller's function.
    let rb = WaveformColumns {
        low: vec![9; 4],
        mid: vec![8; 4],
        high: vec![7; 4],
        mono: vec![6; 4],
    };
    let entry = RekordboxEntry {
        duration_ms: 30_000,
        dat: vec![],
        ext: b"ext".to_vec(),
        ex2: b"2ex".to_vec(),
    };
    cache::write_rekordbox(&cache_dir, id, &entry, &hash, &path).unwrap();
    let got = stored_columns(&lib, id, |e| (e.ex2 == b"2ex").then(|| rb.clone())).unwrap();
    assert_eq!(got, Some(flat(&rb)));

    // reks's own analysis wins over rekordbox's.
    let own = WaveformColumns {
        low: vec![1, 2],
        mid: vec![3, 4],
        high: vec![5, 6],
        mono: vec![7, 8],
    };
    let analysis = TrackAnalysis {
        duration_ms: 30_000,
        beats: BeatList {
            sample_rate: 44_100,
            beats: vec![],
            confidence: 0.0,
        },
        grid: None,
        key: KeyAnalysis {
            key: None,
            confidence: 0.0,
            similarity: 0.0,
            chroma: [0.0; 12],
        },
        columns: own.clone(),
    };
    cache::write(&cache_dir, id, &analysis, &hash).unwrap();
    assert_eq!(stored_columns(&lib, id, never).unwrap(), Some(flat(&own)));
    assert_eq!(
        track_overview(&lib, id, 1, false, never).unwrap(),
        [2, 4, 6, 8]
    );

    // The columns cache wins over both; decoding fills it.
    let decoded = track_columns(&lib, id).unwrap();
    assert_eq!(stored_columns(&lib, id, never).unwrap(), Some(decoded));
}
