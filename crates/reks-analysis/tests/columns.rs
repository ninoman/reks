//! On-demand waveform columns for a library track, and their cache.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use reks_analysis::columns::{columns_path, track_columns};
use reks_analysis::waveform::COLUMNS_PER_SECOND;
use reks_core::{scan, CancelToken, Library, TrackId};

#[test]
fn columns_are_computed_once_then_read_from_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("cache");
    let lib = Library::open(&dir.path().join("reks.sqlite"), &cache).unwrap();
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
