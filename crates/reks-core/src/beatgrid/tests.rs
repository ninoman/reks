use proptest::prelude::*;

use super::rule::tests::{anchor, valid_grid};
use super::*;

const T: TrackId = 1;

fn grid(position_ms: i64, bpm_x100: i64, beat_in_bar: u8) -> Vec<GridAnchor> {
    vec![anchor(position_ms, bpm_x100, beat_in_bar)]
}

/// 140 BPM from 0, 174 BPM from 60 s (on beat 141 of the first section).
fn two_tempo() -> Vec<GridAnchor> {
    vec![anchor(0, 14000, 1), anchor(60_000, 17400, 1)]
}

fn edit(anchors: &[GridAnchor], e: GridEdit) -> Result<Vec<GridAnchor>> {
    apply_edit(T, anchors, &e, None)
}

fn one(anchors: Vec<GridAnchor>) -> GridAnchor {
    assert_eq!(anchors.len(), 1);
    anchors[0]
}

fn downbeats(anchors: &[GridAnchor], end_ms: f64) -> Vec<i64> {
    grid_beats(anchors, end_ms)
        .iter()
        .filter(|b| b.beat_in_bar == 1)
        .map(|b| b.position_ms.round() as i64)
        .collect()
}

fn invalid(r: Result<Vec<GridAnchor>>) -> GridProblem {
    match r {
        Err(Error::InvalidGrid { problem, .. }) => problem,
        other => panic!("expected an invalid grid, got {other:?}"),
    }
}

#[test]
fn nudge_moves_every_marker() {
    let out = edit(&two_tempo(), GridEdit::Nudge { delta_ms: -10 }).unwrap();
    // The first marker would be before 0: it steps a beat forward.
    assert_eq!(out[0].position_ms, 419);
    assert_eq!(out[0].beat_in_bar, 2);
    assert_eq!(out[1].position_ms, 59_990);
    let out = edit(&out, GridEdit::Nudge { delta_ms: 1 }).unwrap();
    assert_eq!(out[1].position_ms, 59_991);
}

#[test]
fn set_downbeat_rephases_one_section() {
    // 120 BPM from 100 ms: beats at 100, 600, 1100...
    let a = one(edit(&grid(100, 12000, 1), GridEdit::SetDownbeat { at_ms: 1080 }).unwrap());
    assert_eq!((a.position_ms, a.beat_in_bar), (100, 3));
    let b = beat_marks(&[a], 2_000);
    assert_eq!((b[2].position_us, b[2].beat_in_bar), (1_100_000, 1));
    // Before the first marker works too.
    let a = one(edit(&grid(2000, 12000, 1), GridEdit::SetDownbeat { at_ms: 1490 }).unwrap());
    assert_eq!(a.beat_in_bar, 2);
    // In the second section only: the first keeps its phase.
    let out = edit(&two_tempo(), GridEdit::SetDownbeat { at_ms: 60_345 }).unwrap();
    assert_eq!(out[0], two_tempo()[0]);
    // 174 BPM = 344.8 ms: beat 1 after the marker becomes the downbeat.
    assert_eq!(out[1].beat_in_bar, 4);
}

#[test]
fn double_and_halve_keep_downbeats_per_section() {
    for b in 1..=4u8 {
        let before = grid(1000, 6400, b);
        let after = edit(&before, GridEdit::Double { section: None }).unwrap();
        assert_eq!(after[0].bpm_x100, 12800);
        let new = downbeats(&after, 30_000.0);
        for p in downbeats(&before, 30_000.0) {
            assert!(new.contains(&p), "beat {b}: downbeat {p} lost");
        }
        let before = grid(1000, 12800, b);
        let after = edit(&before, GridEdit::Halve { section: Some(0) }).unwrap();
        assert_eq!(after[0].bpm_x100, 6400);
        let old = downbeats(&before, 30_000.0);
        let new = downbeats(&after, 30_000.0);
        assert!(!new.is_empty());
        for p in new {
            assert!(old.iter().any(|q| (q - p).abs() <= 1), "beat {b}: {p}");
        }
    }
    // One section of two: the other stays.
    let out = edit(&two_tempo(), GridEdit::Halve { section: Some(1) }).unwrap();
    assert_eq!((out[0].bpm_x100, out[1].bpm_x100), (14000, 8700));
    let out = edit(&two_tempo(), GridEdit::Double { section: Some(0) }).unwrap();
    assert_eq!((out[0].bpm_x100, out[1].bpm_x100), (28000, 17400));
    // Every section: 348 BPM is out of range, so nothing changes.
    assert!(edit(&two_tempo(), GridEdit::Double { section: None }).is_err());
    // Odd tempo rounds half up.
    assert_eq!(
        edit(&grid(0, 12801, 1), GridEdit::Halve { section: None }).unwrap()[0].bpm_x100,
        6401
    );
}

#[test]
fn tempo_limits_are_refused() {
    assert_eq!(
        invalid(edit(&grid(0, 17400, 1), GridEdit::Double { section: None })),
        GridProblem::Tempo {
            index: 0,
            bpm_x100: 34800
        }
    );
    assert!(edit(&grid(0, 5000, 1), GridEdit::Halve { section: None }).is_err());
    assert!(edit(
        &[],
        GridEdit::SetBpm {
            bpm_x100: 500,
            anchor_ms: 0
        }
    )
    .is_err());
}

#[test]
fn set_bpm_creates_or_retempos_the_section_there() {
    let a = one(edit(
        &[],
        GridEdit::SetBpm {
            bpm_x100: 12600,
            anchor_ms: 250,
        },
    )
    .unwrap());
    assert_eq!((a.position_ms, a.bpm_x100, a.beat_in_bar), (250, 12600, 1));
    let out = edit(
        &two_tempo(),
        GridEdit::SetBpm {
            bpm_x100: 17500,
            anchor_ms: 70_000,
        },
    )
    .unwrap();
    assert_eq!((out[0].bpm_x100, out[1].bpm_x100), (14000, 17500));
    let out = edit(
        &two_tempo(),
        GridEdit::SetSectionBpm {
            section: 0,
            bpm_x100: 13950,
        },
    )
    .unwrap();
    assert_eq!(out[0].bpm_x100, 13950);
    assert_eq!(out[1], two_tempo()[1]);
    assert!(matches!(
        edit(
            &two_tempo(),
            GridEdit::SetSectionBpm {
                section: 2,
                bpm_x100: 12000
            }
        ),
        Err(Error::InvalidField {
            field: "section",
            ..
        })
    ));
}

#[test]
fn add_marker_splits_a_section_without_moving_beats() {
    // On a beat: the grid is the same, with one more marker.
    let out = edit(&grid(0, 12000, 1), GridEdit::AddMarker { at_ms: 1_500 }).unwrap();
    assert_eq!(out[1], anchor(1_500, 12000, 4));
    assert_eq!(
        beat_marks(&out, 5_000),
        beat_marks(&grid(0, 12000, 1), 5_000)
    );
    // Off the beat: a marker there with the nearest beat's bar position.
    let out = edit(&grid(0, 12000, 1), GridEdit::AddMarker { at_ms: 1_620 }).unwrap();
    assert_eq!(out[1], anchor(1_620, 12000, 4));
    // Before the first marker.
    // Before the first marker: 500 ms is 9 beats before 5 s, beat 4.
    let out = edit(&grid(5_000, 12000, 1), GridEdit::AddMarker { at_ms: 520 }).unwrap();
    assert_eq!(out[0], anchor(520, 12000, 4));
    // On a marker, or too close to one.
    assert!(edit(&two_tempo(), GridEdit::AddMarker { at_ms: 60_000 }).is_err());
    assert_eq!(
        invalid(edit(&two_tempo(), GridEdit::AddMarker { at_ms: 59_900 })),
        GridProblem::TooClose { index: 2 }
    );
}

#[test]
fn move_and_delete_markers() {
    let out = edit(
        &two_tempo(),
        GridEdit::MoveMarker {
            section: 1,
            to_ms: 59_571,
        },
    )
    .unwrap();
    assert_eq!(out[1].position_ms, 59_571);
    assert_eq!(out[1].bpm_x100, 17400);
    // Moving past a neighbour is refused, not reordered.
    let three = vec![
        anchor(0, 12000, 1),
        anchor(4_000, 12000, 1),
        anchor(8_000, 12000, 1),
    ];
    assert!(edit(
        &three,
        GridEdit::MoveMarker {
            section: 1,
            to_ms: 9_000
        }
    )
    .is_err());
    let out = edit(&two_tempo(), GridEdit::DeleteMarker { section: 1 }).unwrap();
    assert_eq!(out, vec![two_tempo()[0]]);
    // The first marker: the next section runs back to the start.
    let out = edit(&two_tempo(), GridEdit::DeleteMarker { section: 0 }).unwrap();
    assert_eq!(out, vec![two_tempo()[1]]);
    assert!(grid_beats(&out, 1_000.0)[0].position_ms < 345.0);
    assert!(edit(&grid(0, 12000, 1), GridEdit::DeleteMarker { section: 0 }).is_err());
}

#[test]
fn stretch_lands_a_beat_on_the_target() {
    // Beat 8 of a 120 BPM section (4000 ms) onto 4100 ms: 117.07 BPM.
    let out = edit(
        &grid(0, 12000, 1),
        GridEdit::Stretch {
            section: 0,
            beat_ms: 4_010,
            to_ms: 4_100,
        },
    )
    .unwrap();
    assert_eq!(out[0].bpm_x100, 11707);
    let beat8 = grid_beats(&out, 5_000.0)[8].position_ms;
    assert!((beat8 - 4_100.0).abs() <= 1.0, "{beat8}");
    // Section 1 of the two-tempo grid: its marker stays, the first section too.
    let out = edit(
        &two_tempo(),
        GridEdit::Stretch {
            section: 1,
            beat_ms: 71_034, // beat 32 at 174 BPM
            to_ms: 71_100,
        },
    )
    .unwrap();
    assert_eq!(out[0], two_tempo()[0]);
    assert_eq!(out[1].position_ms, 60_000);
    assert_eq!(out[1].bpm_x100, 17297);
    // The marker itself can't be stretched.
    assert!(edit(
        &grid(0, 12000, 1),
        GridEdit::Stretch {
            section: 0,
            beat_ms: 100,
            to_ms: 300
        }
    )
    .is_err());
}

#[test]
fn reset_needs_a_base() {
    assert!(matches!(
        edit(&two_tempo(), GridEdit::Reset { section: None }),
        Err(Error::NoGridBase(T))
    ));
    let base = grid(12, 14000, 1);
    let out = apply_edit(
        T,
        &two_tempo(),
        &GridEdit::Reset { section: None },
        Some(&base),
    );
    assert_eq!(out.unwrap(), base);
}

#[test]
fn reset_one_section() {
    // Base: 140 BPM from 12 ms, then 174 from 60,012 ms. The edit moved the
    // second section 12 ms early and retempoed it.
    let base = vec![anchor(12, 14000, 1), anchor(60_012, 17400, 1)];
    let edited = vec![anchor(12, 14000, 1), anchor(60_000, 17500, 1)];
    let out = apply_edit(
        T,
        &edited,
        &GridEdit::Reset { section: Some(1) },
        Some(&base),
    )
    .unwrap();
    assert_eq!(out, base);
    // A section spanning a base marker gets it back too.
    let edited = vec![anchor(12, 15000, 1)];
    let out = apply_edit(
        T,
        &edited,
        &GridEdit::Reset { section: Some(0) },
        Some(&base),
    )
    .unwrap();
    assert_eq!(out, base);
}

#[test]
fn set_anchors_is_validated() {
    let out = edit(
        &[],
        GridEdit::SetAnchors {
            anchors: two_tempo(),
        },
    )
    .unwrap();
    assert_eq!(out, two_tempo());
    assert_eq!(
        invalid(edit(
            &[],
            GridEdit::SetAnchors {
                anchors: vec![anchor(0, 12000, 1), anchor(100, 12000, 1)]
            }
        )),
        GridProblem::TooClose { index: 1 }
    );
    assert!(matches!(
        edit(&[], GridEdit::Double { section: None }),
        Err(Error::NoGrid(T))
    ));
}

#[test]
fn snap_targets() {
    let mut e = GridEdit::MoveMarker {
        section: 1,
        to_ms: 5,
    };
    *e.snap_target().unwrap() = 7;
    assert_eq!(
        e,
        GridEdit::MoveMarker {
            section: 1,
            to_ms: 7
        }
    );
    assert!(GridEdit::Double { section: None }.snap_target().is_none());
}

#[test]
fn serde_shapes_match_ipc_ts() {
    for (json, edit) in [
        (
            r#"{"op":"nudge","deltaMs":-10}"#,
            GridEdit::Nudge { delta_ms: -10 },
        ),
        (
            r#"{"op":"setBpm","bpmX100":12800,"anchorMs":5}"#,
            GridEdit::SetBpm {
                bpm_x100: 12800,
                anchor_ms: 5,
            },
        ),
        (
            r#"{"op":"setDownbeat","atMs":1}"#,
            GridEdit::SetDownbeat { at_ms: 1 },
        ),
        (
            r#"{"op":"double","section":null}"#,
            GridEdit::Double { section: None },
        ),
        (
            r#"{"op":"halve","section":2}"#,
            GridEdit::Halve { section: Some(2) },
        ),
        (
            r#"{"op":"setSectionBpm","section":1,"bpmX100":17400}"#,
            GridEdit::SetSectionBpm {
                section: 1,
                bpm_x100: 17400,
            },
        ),
        (
            r#"{"op":"addMarker","atMs":3}"#,
            GridEdit::AddMarker { at_ms: 3 },
        ),
        (
            r#"{"op":"moveMarker","section":1,"toMs":4}"#,
            GridEdit::MoveMarker {
                section: 1,
                to_ms: 4,
            },
        ),
        (
            r#"{"op":"deleteMarker","section":1}"#,
            GridEdit::DeleteMarker { section: 1 },
        ),
        (
            r#"{"op":"stretch","section":0,"beatMs":9,"toMs":10}"#,
            GridEdit::Stretch {
                section: 0,
                beat_ms: 9,
                to_ms: 10,
            },
        ),
        (
            r#"{"op":"reset","section":null}"#,
            GridEdit::Reset { section: None },
        ),
        (
            r#"{"op":"setAnchors","anchors":[{"positionMs":1,"bpmX100":12000,"beatInBar":2,"beatsPerBar":4}]}"#,
            GridEdit::SetAnchors {
                anchors: grid(1, 12000, 2),
            },
        ),
        (
            r#"{"op":"setLocked","locked":true}"#,
            GridEdit::SetLocked { locked: true },
        ),
    ] {
        assert_eq!(
            serde_json::from_str::<GridEdit>(json).unwrap(),
            edit,
            "{json}"
        );
    }
    let g = Beatgrid {
        anchors: grid(1, 12000, 2),
        source: Some("manual".into()),
        locked: false,
        beats: vec![BeatMark {
            position_us: 1000,
            beat_in_bar: 2,
        }],
    };
    assert_eq!(
        serde_json::to_string(&g).unwrap(),
        r#"{"anchors":[{"positionMs":1,"bpmX100":12000,"beatInBar":2,"beatsPerBar":4}],"source":"manual","locked":false,"beats":[{"positionUs":1000,"beatInBar":2}]}"#
    );
}

#[test]
fn beat_marks_follow_the_rule() {
    let g = [anchor(0, 12000, 1), anchor(1_200, 6000, 1)];
    let ms: Vec<i64> = beat_marks(&g, 3_200)
        .iter()
        .map(|m| m.position_us / 1000)
        .collect();
    // The end is exclusive.
    assert_eq!(ms, [0, 500, 1200, 2200]);
    assert!(beat_marks(&[], 1000).is_empty());
}

/// Any edit on a valid grid gives a valid grid or an error.
fn any_edit() -> impl Strategy<Value = GridEdit> {
    let pos = 0i64..120_000;
    prop_oneof![
        (-500i64..500).prop_map(|d| GridEdit::Nudge { delta_ms: d }),
        pos.clone().prop_map(|t| GridEdit::SetDownbeat { at_ms: t }),
        (0usize..6).prop_map(|s| GridEdit::Double { section: Some(s) }),
        (0usize..6).prop_map(|s| GridEdit::Halve { section: Some(s) }),
        (0usize..6, 2_000i64..40_000).prop_map(|(s, b)| GridEdit::SetSectionBpm {
            section: s,
            bpm_x100: b
        }),
        pos.clone().prop_map(|t| GridEdit::AddMarker { at_ms: t }),
        (0usize..6, pos.clone()).prop_map(|(s, t)| GridEdit::MoveMarker {
            section: s,
            to_ms: t
        }),
        (0usize..6).prop_map(|s| GridEdit::DeleteMarker { section: s }),
        (0usize..6, pos.clone(), pos).prop_map(|(s, b, t)| GridEdit::Stretch {
            section: s,
            beat_ms: b,
            to_ms: t
        }),
    ]
}

proptest! {
    #[test]
    fn edits_keep_grids_valid(g in valid_grid(), edits in prop::collection::vec(any_edit(), 1..8)) {
        let mut g = g;
        for e in edits {
            if let Ok(next) = edit(&g, e) {
                prop_assert_eq!(validate_anchors(&next), Ok(()));
                let beats = grid_beats(&next, 200_000.0);
                for pair in beats.windows(2) {
                    prop_assert!(pair[1].position_ms.round() > pair[0].position_ms.round());
                }
                g = next;
            }
        }
    }
}
