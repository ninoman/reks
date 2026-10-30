//! Cue sections from library cues ([`reks_core::track::Cue`]).
//!
//! Placement and order follow what baken-export 4.6.1 (MIT) found on real
//! rekordbox 7 exports: the `.DAT` holds a hot `PCOB` with hot cues A–C and
//! the memory `PCOB`; the `.EXT` holds a hot `PCOB` with D–H, an empty memory
//! `PCOB`, then `PCO2` hot and memory lists with every cue. rekordbox writes
//! each list in reverse database order; in every export seen that put hot
//! cues in descending slot order and the memory list in descending time, so
//! we write those orders. Hot cue colours use rekordbox's 16-colour palette
//! (code + the player's RGB bytes, from the same exports).

use reks_core::track::Cue;
use reks_core::CueKind;
use serde::{Deserialize, Serialize};

use crate::grid::bpm_at;
use crate::write::{CueEntry, PqtzBeat};

/// Hot cue slots (A–H).
const HOT_SLOTS: u8 = 8;
/// Hot cues A–C go into the `.DAT` `PCOB`, D–H into the `.EXT` one.
const DAT_HOT_SLOTS: u8 = 3;

/// The palette code rekordbox writes for green and for "no colour".
const GREEN: u8 = 0x16;
/// Hot cue without a colour (and green): code 0, green player bytes.
const NO_COLOUR: [u8; 4] = [0, 0x1A, 0xFF, 0x00];

/// rekordbox's hot cue palette: `.EXT` code, the RGB rekordbox shows (and
/// the library stores from its XML), the player's RGB bytes. From rekordbox
/// 7.2.18 exports (baken-export issue #236).
const HOT_CUE_COLOURS: [(u8, u32, [u8; 3]); 16] = [
    (0x01, 0x305AFF, [0x00, 0x00, 0xFF]),
    (0x05, 0x50B4FF, [0x00, 0x70, 0xFF]),
    (0x09, 0x00E0FF, [0x00, 0xE0, 0xFF]),
    (0x0E, 0x1FA392, [0x00, 0xFF, 0xA3]),
    (0x12, 0x10B176, [0x00, 0xFF, 0x47]),
    (GREEN, 0x28E214, [0x1A, 0xFF, 0x00]),
    (0x1A, 0xA5E116, [0x80, 0xFF, 0x00]),
    (0x1E, 0xB4BE04, [0xE6, 0xFF, 0x00]),
    (0x20, 0xC3AF04, [0xFF, 0xE8, 0x00]),
    (0x26, 0xE0641B, [0xFF, 0x5E, 0x00]),
    (0x2A, 0xE62828, [0xFF, 0x00, 0x00]),
    (0x2D, 0xFF127B, [0xFF, 0x00, 0x45]),
    (0x31, 0xDE44CF, [0xFF, 0x00, 0xA1]),
    (0x38, 0xB432FF, [0xB3, 0x00, 0xFF]),
    (0x3C, 0xAA72FF, [0x4D, 0x00, 0xFF]),
    (0x3E, 0x6473FF, [0x1A, 0x00, 0xFF]),
];

/// `PCP2` hot cue colour bytes for a library colour: the nearest palette
/// entry; green and no colour both go out as [`NO_COLOUR`], as rekordbox
/// writes them.
pub fn hot_colour(rgb: Option<u32>) -> [u8; 4] {
    let Some(rgb) = rgb else {
        return NO_COLOUR;
    };
    let channels = |c: u32| [(c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF].map(|v| v as i32);
    let want = channels(rgb);
    let distance = |c: u32| -> i32 {
        channels(c)
            .iter()
            .zip(want)
            .map(|(a, b)| (a - b).pow(2))
            .sum()
    };
    match HOT_CUE_COLOURS
        .iter()
        .min_by_key(|(_, xml, _)| distance(*xml))
    {
        Some(&(code, _, [r, g, b])) if code != GREEN => [code, r, g, b],
        _ => NO_COLOUR,
    }
}

/// Inverse of [`hot_colour`]: the library colour (the RGB rekordbox shows)
/// for `PCP2` hot cue colour bytes; `None` for code 0 or an unknown code.
pub fn library_colour(hot_color: [u8; 4]) -> Option<u32> {
    HOT_CUE_COLOURS
        .iter()
        .find(|(code, _, _)| *code == hot_color[0] && hot_color[0] != 0)
        .map(|&(_, rgb, _)| rgb)
}

fn ms(v: f64) -> u32 {
    v.round().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// Loop length in beats when it is a whole number of beats (within 2%).
fn loop_fraction(start_ms: f64, end_ms: f64, bpm: Option<f64>) -> (u16, u16) {
    let Some(bpm) = bpm.filter(|b| *b > 0.0) else {
        return (0, 0);
    };
    let beats = (end_ms - start_ms) / 1000.0 * bpm / 60.0;
    let rounded = beats.round();
    if (1.0..=f64::from(u16::MAX)).contains(&rounded) && (beats - rounded).abs() < 0.02 {
        (rounded as u16, 1)
    } else {
        (0, 0)
    }
}

/// One library cue as a cue entry; `beats` gives the tempo for quantised
/// loop lengths.
pub fn entry(cue: &Cue, beats: &[PqtzBeat]) -> CueEntry {
    let time_ms = ms(cue.start_ms);
    // A loop must still end after it starts once rounded to whole ms.
    let loop_end = cue
        .end_ms
        .filter(|&end| cue.cue_kind() == Some(CueKind::Loop) && ms(end) > time_ms);
    let hot_cue = cue.slot.map_or(0, |s| u32::from(s) + 1);
    CueEntry {
        hot_cue,
        // The library has no "active loop" flag yet.
        active_loop: false,
        time_ms,
        loop_end_ms: loop_end.map(ms),
        // rekordbox's XML carries no memory cue colour.
        color_id: 0,
        // From the stored whole-ms times, so reading the file back and
        // rebuilding gives the same fraction.
        loop_fraction: loop_end.map_or((0, 0), |end| {
            loop_fraction(
                f64::from(time_ms),
                f64::from(ms(end)),
                bpm_at(beats, time_ms),
            )
        }),
        comment: cue.name.clone().unwrap_or_default(),
        hot_color: if hot_cue > 0 {
            hot_colour(cue.color_rgb)
        } else {
            [0; 4]
        },
    }
}

/// The cue lists for one track, in file placement.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CueLists {
    /// Hot cues, descending slot.
    pub hot: Vec<CueEntry>,
    /// Memory cues and loops, descending time.
    pub memory: Vec<CueEntry>,
}

impl CueLists {
    /// Splits library cues; hot cues past H are dropped.
    pub fn new(cues: &[Cue], beats: &[PqtzBeat]) -> Self {
        let mut hot: Vec<(u8, CueEntry)> = Vec::new();
        let mut memory: Vec<CueEntry> = Vec::new();
        for c in cues {
            match c.slot {
                Some(s) if s < HOT_SLOTS => hot.push((s, entry(c, beats))),
                Some(_) => {}
                None => memory.push(entry(c, beats)),
            }
        }
        hot.sort_by_key(|h| std::cmp::Reverse(h.0));
        memory.sort_by_key(|c| std::cmp::Reverse(c.time_ms));
        Self {
            hot: hot.into_iter().map(|(_, e)| e).collect(),
            memory,
        }
    }

    /// Hot cues A–C (the `.DAT` hot `PCOB`).
    pub fn dat_hot(&self) -> Vec<CueEntry> {
        self.hot_where(|slot| slot < DAT_HOT_SLOTS)
    }

    /// Hot cues D–H (the `.EXT` hot `PCOB`).
    pub fn ext_hot(&self) -> Vec<CueEntry> {
        self.hot_where(|slot| slot >= DAT_HOT_SLOTS)
    }

    fn hot_where(&self, keep: impl Fn(u8) -> bool) -> Vec<CueEntry> {
        self.hot
            .iter()
            // hot_cue is 1..=8 here.
            .filter(|c| keep((c.hot_cue - 1) as u8))
            .cloned()
            .collect()
    }
}

/// How many entries each file's cue lists hold for a track, placed as
/// [`CueLists`] (and baken) place them; export validation compares these
/// with what it reads back from the stick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CueCounts {
    /// Memory cues and loops.
    pub memory: u8,
    /// Hot cues A–C.
    pub hot_dat: u8,
    /// Hot cues D–H.
    pub hot_ext: u8,
}

impl CueCounts {
    /// Counts library cues (hot cues past H are dropped, as [`CueLists`]
    /// drops them); each count saturates at 255.
    pub fn of(cues: &[Cue]) -> Self {
        let mut n = Self::default();
        for c in cues {
            let count = match c.slot {
                Some(s) if s < DAT_HOT_SLOTS => &mut n.hot_dat,
                Some(s) if s < HOT_SLOTS => &mut n.hot_ext,
                Some(_) => continue,
                None => &mut n.memory,
            };
            *count = count.saturating_add(1);
        }
        n
    }

    /// Entries across the `.DAT` `PCOB` lists: memory cues and hot cues A–C.
    pub fn dat_pcob(self) -> u32 {
        u32::from(self.memory) + u32::from(self.hot_dat)
    }

    /// Entries across the `.EXT` `PCOB` lists: hot cues D–H (its memory
    /// list stays empty).
    pub fn ext_pcob(self) -> u32 {
        u32::from(self.hot_ext)
    }

    /// Entries across the `.EXT` `PCO2` lists: every hot and memory cue.
    pub fn ext_pco2(self) -> u32 {
        self.dat_pcob() + self.ext_pcob()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(slot: Option<u8>, kind: i64, start_ms: f64, end_ms: Option<f64>) -> Cue {
        Cue {
            id: 0,
            slot,
            kind,
            start_ms,
            end_ms,
            name: None,
            color_rgb: None,
            active_loop: false,
        }
    }

    #[test]
    fn colours_follow_rekordbox() {
        for (rgb, bytes) in [
            (0xDE44CF, [0x31, 0xFF, 0x00, 0xA1]),
            (0x305AFF, [0x01, 0x00, 0x00, 0xFF]),
            (0xFF127B, [0x2D, 0xFF, 0x00, 0x45]),
            (0xC3AF04, [0x20, 0xFF, 0xE8, 0x00]),
            (0x28E214, NO_COLOUR),
            // Another tool's colour takes the nearest.
            (0xFA0000, [0x2A, 0xFF, 0x00, 0x00]),
            (0x00FF00, NO_COLOUR),
        ] {
            assert_eq!(hot_colour(Some(rgb)), bytes, "{rgb:06x}");
        }
        assert_eq!(hot_colour(None), NO_COLOUR);
        for (code, rgb, _) in HOT_CUE_COLOURS {
            let back = library_colour(hot_colour(Some(rgb)));
            assert_eq!(back, (code != GREEN).then_some(rgb));
        }
        assert_eq!(library_colour(NO_COLOUR), None);
        assert_eq!(library_colour([0x02, 0, 0, 0]), None);
    }

    /// Hot cues set on a deck (Phase 5 task 17) take `HOT_CUE_PALETTE`:
    /// each is one of rekordbox's colours, so it exports as itself, and no
    /// two share a player colour.
    #[test]
    fn deck_palette_is_rekordbox_colours() {
        let mut codes: Vec<u8> = reks_core::HOT_CUE_PALETTE
            .iter()
            .map(|&rgb| {
                assert!(HOT_CUE_COLOURS.iter().any(|c| c.1 == rgb), "{rgb:06x}");
                hot_colour(Some(rgb))[0]
            })
            .collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), reks_core::HOT_CUE_PALETTE.len());
    }

    #[test]
    fn splits_orders_and_places_cues() {
        let beats = [PqtzBeat {
            beat_number: 1,
            tempo: 12_000,
            time_ms: 0,
        }];
        let cues = [
            cue(Some(0), 0, 48.4, None),
            cue(Some(6), 0, 185_805.0, None),
            cue(Some(2), 4, 1_000.0, Some(3_000.0)),
            cue(Some(9), 0, 5.0, None),
            cue(None, 0, 1_547.0, None),
            cue(None, 4, 121_155.0, Some(126_904.0)),
            cue(None, 0, 253.0, None),
        ];
        let l = CueLists::new(&cues, &beats);
        let hot: Vec<_> = l.hot.iter().map(|c| c.hot_cue).collect();
        assert_eq!(hot, [7, 3, 1]);
        let mem: Vec<_> = l.memory.iter().map(|c| c.time_ms).collect();
        assert_eq!(mem, [121_155, 1_547, 253]);
        assert_eq!(
            l.dat_hot().iter().map(|c| c.hot_cue).collect::<Vec<_>>(),
            [3, 1]
        );
        assert_eq!(
            l.ext_hot().iter().map(|c| c.hot_cue).collect::<Vec<_>>(),
            [7]
        );
        // Hot loop C: 2 s at 120 BPM = 4 beats.
        let c = &l.hot[1];
        assert_eq!(
            (c.time_ms, c.loop_end_ms, c.loop_fraction),
            (1_000, Some(3_000), (4, 1))
        );
        assert_eq!(c.cue_type(), 2);
        assert_eq!(c.hot_color, NO_COLOUR);
        // A memory loop that is not a whole number of beats.
        assert_eq!(l.memory[0].loop_fraction, (0, 0));
        assert_eq!(l.memory[0].hot_color, [0; 4]);
        assert_eq!(l.memory[1].cue_type(), 1);
        assert_eq!(l.hot[2].time_ms, 48);
    }

    #[test]
    fn end_without_loop_kind_is_a_point_cue() {
        let e = entry(&cue(None, 0, 10.0, Some(20.0)), &[]);
        assert_eq!((e.loop_end_ms, e.cue_type()), (None, 1));
        // A loop ending before it starts is a point cue too.
        let e = entry(&cue(None, 4, 10.0, Some(5.0)), &[]);
        assert_eq!(e.loop_end_ms, None);
        let e = entry(&cue(None, 4, 10.2, Some(10.4)), &[]);
        assert_eq!(e.loop_end_ms, None);
        // No grid: no quantised length.
        let e = entry(&cue(None, 4, 0.0, Some(2_000.0)), &[]);
        assert_eq!(e.loop_fraction, (0, 0));
        let mut named = cue(Some(1), 0, 0.0, None);
        named.name = Some("Drop".into());
        named.color_rgb = Some(0xE62828);
        let e = entry(&named, &[]);
        assert_eq!((e.hot_cue, e.comment.as_str()), (2, "Drop"));
        assert_eq!(e.hot_color, [0x2A, 0xFF, 0x00, 0x00]);
    }

    /// The counts match the lists the builder writes: hot cues B, D–H and
    /// two memory cues give a `.DAT` `PCOB` of 3 (B + memory), an `.EXT`
    /// `PCOB` of 5 (D–H) and `PCO2` of all 8 (perf audit 2026-10-30).
    #[test]
    fn counts_follow_list_placement() {
        let mut cues: Vec<Cue> = [1, 3, 4, 5, 6, 7]
            .into_iter()
            .map(|s| cue(Some(s), 0, f64::from(s) * 100.0, None))
            .collect();
        cues.push(cue(None, 0, 50.0, None));
        cues.push(cue(None, 4, 60.0, Some(900.0)));
        cues.push(cue(Some(8), 0, 70.0, None));
        let n = CueCounts::of(&cues);
        let l = CueLists::new(&cues, &[]);
        assert_eq!(n.dat_pcob() as usize, l.dat_hot().len() + l.memory.len());
        assert_eq!(n.ext_pcob() as usize, l.ext_hot().len());
        assert_eq!(n.ext_pco2() as usize, l.hot.len() + l.memory.len());
        assert_eq!((n.dat_pcob(), n.ext_pcob(), n.ext_pco2()), (3, 5, 8));
    }
}
