use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::history::tests::stick;
use crate::testutil::*;

fn reference_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/onelibrary/ref-7.2.19")
}

fn fixture_audio(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/audio")
        .join(name)
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The rekordbox reference stick with the WAV it lists put back (the
/// fixture keeps no audio).
fn reference_with_wav() -> (tempfile::TempDir, u32) {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(
        &reference_root().join("PIONEER"),
        &dir.path().join("PIONEER"),
    );
    let lib = read_library(dir.path()).unwrap().unwrap();
    let wav = lib
        .tracks
        .iter()
        .find(|t| t.stick_path.ends_with("sine-click-128.wav"))
        .unwrap();
    let target = dir.path().join(wav.stick_path.trim_start_matches('/'));
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::copy(fixture_audio("sine-click-128.wav"), target).unwrap();
    (dir, wav.id)
}

#[test]
fn reads_the_rekordbox_reference() {
    let lib = read_library(&reference_root()).unwrap().unwrap();
    assert_eq!(lib.tracks.len(), 7);
    let wav = lib
        .tracks
        .iter()
        .find(|t| t.stick_path.ends_with("sine-click-128.wav"))
        .unwrap();
    assert_eq!(wav.key.as_deref(), Some("Am"));
    assert_eq!(wav.bpm_x100, Some(12800));
    assert!(wav.duration_ms.is_some_and(|ms| ms > 0));
    assert_eq!(lib.playlists.len(), 1);
    let list = &lib.playlists[0];
    assert_eq!((list.name.as_str(), list.parent_id), ("reks-test", None));
    assert_eq!(list.track_ids.len(), 7);
    assert!(list
        .track_ids
        .iter()
        .all(|id| lib.tracks.iter().any(|t| t.id == *id)));
    assert!(lib.histories.is_empty());
}

#[test]
fn a_drive_without_a_library_has_none() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read_library(dir.path()).unwrap(), None);
}

#[test]
fn lists_the_sessions_players_logged() {
    let dir = stick(&[("PIONEER/rekordbox/export.pdb", &played_pdb())]);
    let lib = read_library(dir.path()).unwrap().unwrap();
    let sessions: Vec<_> = lib
        .histories
        .iter()
        .map(|s| (s.name.as_str(), s.track_ids.clone()))
        .collect();
    assert_eq!(
        sessions,
        [("HISTORY 001", vec![5, 7]), ("HISTORY 002", vec![5])]
    );
}

#[test]
fn siblings_follow_sort_order_and_parents_come_first() {
    let pdb = synthetic_pdb(&[
        (T_TRACKS, vec![vec![]]),
        (
            T_PLAYLIST_TREE,
            vec![vec![
                playlist_node(3, 1, 0, false, "Inner"),
                playlist_node(2, 0, 1, false, "Second"),
                playlist_node(1, 0, 0, true, "First"),
            ]],
        ),
    ]);
    let dir = stick(&[("PIONEER/rekordbox/export.pdb", &pdb)]);
    let lib = read_library(dir.path()).unwrap().unwrap();
    let names: Vec<_> = lib
        .playlists
        .iter()
        .map(|p| (p.name.as_str(), p.parent_id))
        .collect();
    assert_eq!(
        names,
        [("First", None), ("Second", None), ("Inner", Some(1))]
    );
}

#[test]
fn grid_comes_from_the_anlz() {
    let lib = read_library(&reference_root()).unwrap().unwrap();
    let wav = lib
        .tracks
        .iter()
        .find(|t| t.stick_path.ends_with("sine-click-128.wav"))
        .unwrap();
    let grid = beatgrid(&reference_root(), wav.id).unwrap();
    assert_eq!(grid.anchors.len(), 1);
    assert_eq!(grid.anchors[0].bpm_x100, 12800);
    assert!(grid.locked);
    assert_eq!(grid.source, Some(DataSource::Stick));
    // 128 BPM over the track: about two beats a second.
    let seconds = wav.duration_ms.unwrap() / 1000;
    assert!(grid.beats.len() as i64 >= 2 * seconds);
}

#[test]
fn unknown_track_is_an_error() {
    assert!(matches!(
        beatgrid(&reference_root(), 999_999),
        Err(BrowseError::NoTrack(999_999))
    ));
}

#[test]
fn deck_audio_decodes_the_stick_file_with_its_grid() {
    let (dir, id) = reference_with_wav();
    let audio = deck_audio(dir.path(), id, 44_100).unwrap();
    assert_eq!(audio.pcm.sample_rate, 44_100);
    assert!(!audio.pcm.samples.is_empty());
    // 30 s at 128 BPM: a beat every 468.75 ms, one tempo.
    assert!(audio.beats.len() >= 63, "{}", audio.beats.len());
    assert!(audio.beats.iter().all(|b| (b.bpm - 128.0).abs() < 1e-9));
    let columns = waveform_columns(dir.path(), id).unwrap();
    assert!(!columns.is_empty() && columns.len().is_multiple_of(4));
}

/// rekordbox's waveform for the click track, decoded from the stick alone
/// (the fixture has no audio), against the columns reks computes from the
/// same WAV: same length, and close per column.
#[test]
fn waveform_comes_from_the_stick_and_matches_ours() {
    let lib = read_library(&reference_root()).unwrap().unwrap();
    let wav = lib
        .tracks
        .iter()
        .find(|t| t.stick_path.ends_with("sine-click-128.wav"))
        .unwrap();
    let stick = waveform_columns(&reference_root(), wav.id).unwrap();
    let stick = reks_analysis::waveform::WaveformColumns::from_interleaved(&stick).unwrap();
    let ours = reks_analysis::columns::file_columns(&fixture_audio("sine-click-128.wav")).unwrap();
    assert!(
        stick.len().abs_diff(ours.len()) <= 2,
        "{} vs {}",
        stick.len(),
        ours.len()
    );
    let n = stick.len().min(ours.len());
    let mean_err = |a: &[u8], b: &[u8]| {
        a[..n]
            .iter()
            .zip(&b[..n])
            .map(|(&x, &y)| f64::from(x.abs_diff(y)))
            .sum::<f64>()
            / n as f64
    };
    let errs = [
        mean_err(&stick.low, &ours.low) / 255.0,
        mean_err(&stick.mid, &ours.mid) / 255.0,
        mean_err(&stick.high, &ours.high) / 255.0,
        mean_err(&stick.mono, &ours.mono) / 31.0,
    ];
    eprintln!("mean error (fraction of full scale) low/mid/high/mono: {errs:?}");
    assert!(errs.iter().all(|&e| e < 0.12), "{errs:?}");
}

#[test]
fn missing_audio_is_reported() {
    let lib = read_library(&reference_root()).unwrap().unwrap();
    let err = deck_audio(&reference_root(), lib.tracks[0].id, 44_100).unwrap_err();
    assert!(matches!(err, BrowseError::FileMissing { .. }), "{err}");
}

#[test]
fn paths_leaving_the_stick_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        stick_file(dir.path(), "/Contents/../../etc/passwd"),
        Err(BrowseError::OutsideStick(_))
    ));
}

#[test]
fn sibling_swaps_the_extension() {
    assert_eq!(
        sibling("/PIONEER/USBANLZ/P027/0000B55F/ANLZ0000.DAT", "EXT"),
        "/PIONEER/USBANLZ/P027/0000B55F/ANLZ0000.EXT"
    );
}

mod anlz_tests {
    use reks_anlz::{CueEntry, PqtzBeat};

    use super::super::anlz::*;

    fn beat(beat_number: u16, tempo: u16, time_ms: u32) -> PqtzBeat {
        PqtzBeat {
            beat_number,
            tempo,
            time_ms,
        }
    }

    #[test]
    fn one_anchor_per_tempo_run() {
        let beats = [
            beat(2, 12000, 100),
            beat(3, 12000, 600),
            beat(4, 12400, 1100),
            beat(1, 12400, 1584),
        ];
        let a = reks_anlz::grid::anchors_from_pqtz(&beats);
        assert_eq!(a.len(), 2);
        assert_eq!(
            (a[0].position_ms, a[0].bpm_x100, a[0].beat_in_bar),
            (100, 12000, 2)
        );
        assert_eq!(
            (a[1].position_ms, a[1].bpm_x100, a[1].beat_in_bar),
            (1100, 12400, 4)
        );
    }

    #[test]
    fn reads_a_synthetic_dat() {
        let dat = crate::testutil::anlz_dat("/Contents/a.mp3", 8, 12800, 250);
        let a = analysis(&dat, None).unwrap();
        assert_eq!(a.anchors.len(), 1);
        assert_eq!(a.anchors[0].position_ms, 250);
        assert!(a.cues.is_empty());
        assert!(a.last_beat_ms > 250);
    }

    #[test]
    fn hot_loop_maps_to_a_library_loop_cue() {
        let dat = reks_anlz::write::pqtz(&[beat(1, 12000, 0)]).unwrap();
        let dat = reks_anlz::write::file(&[dat]).unwrap();
        let entry = CueEntry {
            hot_cue: 2,
            active_loop: false,
            time_ms: 1000,
            loop_end_ms: Some(3000),
            color_id: 0,
            loop_fraction: (4, 1),
            comment: "drop".into(),
            hot_color: [0x2A, 0xFF, 0x00, 0x00],
        };
        let ext =
            reks_anlz::write::file(&[
                reks_anlz::write::pco2(reks_anlz::write::LIST_HOT, &[entry]).unwrap()
            ])
            .unwrap();
        let a = analysis(&dat, Some(&ext)).unwrap();
        assert_eq!(a.cues.len(), 1);
        let c = &a.cues[0];
        assert_eq!(c.slot, Some(1));
        assert_eq!(c.kind, 4);
        assert_eq!((c.start_ms, c.end_ms), (1000.0, Some(3000.0)));
        assert_eq!(c.name.as_deref(), Some("drop"));
        assert_eq!(c.color_rgb, Some(0xE62828));
    }
}
