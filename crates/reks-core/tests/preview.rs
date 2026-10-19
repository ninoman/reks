//! Preview audio and the overview waveform cache on a scanned fixture.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use reks_core::{scan, CancelToken, Error, Library, TrackId};

/// 30 s at 44.1 kHz in 1,000-frame columns, 8 bytes each.
const OVERVIEW_BYTES: usize = 1_323 * 8;

fn setup() -> (tempfile::TempDir, Library, TrackId) {
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

fn cached_files(cache: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(cache.join("waveform/overview")) {
        Ok(rd) => rd.map(|e| e.unwrap().path()).collect(),
        Err(_) => Vec::new(),
    }
}

#[test]
fn preview_audio_resamples_and_caches_the_overview() {
    let (dir, lib, id) = setup();
    let audio = lib.preview_audio(id, 48_000).unwrap();
    assert_eq!(audio.pcm.sample_rate, 48_000);
    assert_eq!(audio.pcm.frames(), 1_440_000);
    assert_eq!(audio.overview.len() * 4, OVERVIEW_BYTES);
    let files = cached_files(&dir.path().join("cache"));
    assert_eq!(files.len(), 1);
    assert_eq!(std::fs::read(&files[0]).unwrap().len(), OVERVIEW_BYTES);
}

#[test]
fn overview_waveform_reads_the_cache_after_the_first_decode() {
    let (dir, lib, id) = setup();
    let bytes = lib.overview_waveform(id).unwrap();
    assert_eq!(bytes.len(), OVERVIEW_BYTES);
    let files = cached_files(&dir.path().join("cache"));
    assert_eq!(files.len(), 1);

    // A planted cache entry is returned as is: no second decode.
    let planted: Vec<u8> = [0.5f32, -0.5]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    std::fs::write(&files[0], &planted).unwrap();
    assert_eq!(lib.overview_waveform(id).unwrap(), planted);

    // A corrupt entry (not whole pairs) is recomputed.
    std::fs::write(&files[0], [1u8, 2, 3]).unwrap();
    assert_eq!(lib.overview_waveform(id).unwrap(), bytes);
}

#[test]
fn unknown_track_and_missing_file_are_errors() {
    let (dir, lib, id) = setup();
    assert!(matches!(
        lib.overview_waveform(9_999),
        Err(Error::TrackNotFound(9_999))
    ));
    std::fs::remove_file(dir.path().join("music/a.wav")).unwrap();
    assert!(matches!(
        lib.preview_audio(id, 48_000),
        Err(Error::Audio(_))
    ));
}
