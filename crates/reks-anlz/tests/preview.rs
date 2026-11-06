//! Track-list previews come from reks's own columns, never from a mapped
//! rekordbox analysis, and decode only when asked
//! (`reks_anlz::preview::track_preview`).

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use reks_analysis::beats::BeatList;
use reks_analysis::cache::{self, RekordboxEntry};
use reks_analysis::columns::{columns_path, track_columns};
use reks_analysis::key::KeyAnalysis;
use reks_analysis::pipeline::TrackAnalysis;
use reks_analysis::waveform::WaveformColumns;
use reks_anlz::preview::{from_columns, track_preview};
use reks_anlz::{Waveforms, CALIBRATION};
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

fn steady(level: u8, n: usize) -> WaveformColumns {
    WaveformColumns {
        low: vec![level; n],
        mid: vec![level / 2; n],
        high: vec![level / 4; n],
        mono: vec![20; n],
    }
}

#[test]
fn own_analysis_then_decoded_columns_never_rekordbox() {
    let (_dir, lib, id) = one_track();
    let cache_dir = lib.cache_dir();
    let (path, _) = lib.track_file_key(id).unwrap();
    let hash = file_content_hash(&path).unwrap();

    // Nothing stored: no preview without decoding, and nothing decoded.
    assert!(track_preview(&lib, id, false).unwrap().is_empty());
    assert!(!columns_path(&cache_dir, id).exists());

    // Decoding fills the columns cache, which later reads without decoding.
    let decoded = track_preview(&lib, id, true).unwrap();
    assert_eq!(decoded.len(), 1200 * 4);
    assert!(decoded.as_chunks::<4>().0.iter().any(|c| c[0] > 0));
    assert_eq!(track_preview(&lib, id, false).unwrap(), decoded);

    // A mapped rekordbox analysis is not used.
    let rb = Waveforms::encode(&steady(200, 4500), &CALIBRATION).unwrap();
    let entry = RekordboxEntry {
        duration_ms: 30_000,
        dat: vec![],
        ext: reks_anlz::write::file(&rb.ext_sections().unwrap()).unwrap(),
        ex2: vec![],
    };
    cache::write_rekordbox(&cache_dir, id, &entry, &hash, &path).unwrap();
    assert_eq!(track_preview(&lib, id, false).unwrap(), decoded);

    // reks's analysis wins over the decoded columns.
    let own = steady(60, 4500);
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
    assert_eq!(track_preview(&lib, id, false).unwrap(), from_columns(&own));
    // The decoded columns are still there for the decks.
    assert!(!track_columns(&lib, id).unwrap().is_empty());
}
