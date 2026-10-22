//! A stick track's grid and cues from its ANLZ files, in library shapes
//! ([`GridAnchor`], [`Cue`]) so the deck code takes them as it takes a
//! library track's.
//!
//! Grid: one anchor per run of `PQTZ` beats at the same tempo, at the run's
//! first beat. Cues: the `.EXT`'s `PCO2` lists when it has any (they carry
//! colours and comments), else every `PCOB` list in `.DAT` and `.EXT`
//! (rekordbox puts hot cues A–C in the `.DAT`, D–H in the `.EXT`).

use reks_anlz::cue::library_colour;
use reks_anlz::read::{cue_list, pqtz_beats, sections, ReadError, PCO2, PCOB, PQTZ};
use reks_anlz::{CueEntry, PqtzBeat};
use reks_core::{Cue, GridAnchor};

/// rekordbox cue `Type` ([`Cue::kind`]) of a point cue and of a loop.
const KIND_CUE: i64 = 0;
const KIND_LOOP: i64 = 4;

/// Bars on players are 4/4 (`PQTZ` beat numbers run 1–4).
const BEATS_PER_BAR: u8 = 4;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StickAnalysis {
    /// Sorted by position; empty without a `PQTZ`.
    pub anchors: Vec<GridAnchor>,
    /// Position of the last beat, ms (0 without a grid).
    pub last_beat_ms: i64,
    /// Hot cues and memory cues; ids are list positions (they are no library rows).
    pub cues: Vec<Cue>,
}

/// Decodes the `.DAT` and, when there is one, the `.EXT`.
pub fn analysis(dat: &[u8], ext: Option<&[u8]>) -> Result<StickAnalysis, ReadError> {
    let dat = sections(dat)?;
    let ext = ext.map(sections).transpose()?.unwrap_or_default();
    let mut beats = Vec::new();
    for s in dat.iter().filter(|s| s.tag == PQTZ) {
        beats.extend(pqtz_beats(s)?);
    }
    let lists = |tag| dat.iter().chain(&ext).filter(move |s| s.tag == tag);
    let mut entries = Vec::new();
    let extended = ext.iter().any(|s| s.tag == PCO2);
    for s in lists(if extended { PCO2 } else { PCOB }) {
        entries.extend(cue_list(s)?.cues);
    }
    Ok(StickAnalysis {
        anchors: anchors(&beats),
        last_beat_ms: beats.last().map_or(0, |b| i64::from(b.time_ms)),
        cues: entries.iter().enumerate().map(|(i, e)| cue(i, e)).collect(),
    })
}

/// One anchor where the tempo changes (and at the first beat).
pub fn anchors(beats: &[PqtzBeat]) -> Vec<GridAnchor> {
    let mut out: Vec<GridAnchor> = Vec::new();
    for b in beats.iter().filter(|b| b.tempo > 0) {
        if out.last().is_some_and(|a| a.bpm_x100 == i64::from(b.tempo)) {
            continue;
        }
        out.push(GridAnchor {
            position_ms: i64::from(b.time_ms),
            bpm_x100: i64::from(b.tempo),
            beat_in_bar: u8::try_from(b.beat_number)
                .ok()
                .filter(|n| (1..=BEATS_PER_BAR).contains(n))
                .unwrap_or(1),
            beats_per_bar: BEATS_PER_BAR,
        });
    }
    out
}

fn cue(i: usize, e: &CueEntry) -> Cue {
    // `hot_cue` is 1 = A … 8 = H; library slots are 0-based.
    let slot = e.hot_cue.checked_sub(1).and_then(|s| u8::try_from(s).ok());
    Cue {
        id: i as i64,
        slot,
        kind: if e.loop_end_ms.is_some() {
            KIND_LOOP
        } else {
            KIND_CUE
        },
        start_ms: f64::from(e.time_ms),
        end_ms: e.loop_end_ms.map(f64::from),
        name: Some(e.comment.clone()).filter(|c| !c.is_empty()),
        color_rgb: slot.and_then(|_| library_colour(e.hot_color)),
        active_loop: e.active_loop,
    }
}
