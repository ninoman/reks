//! Helpers shared by the integration tests: turn built ANLZ files back into
//! builder input.

#![allow(dead_code, clippy::unwrap_used, clippy::panic)]

use reks_anlz::cue::library_colour;
use reks_anlz::read::{self, Section};
use reks_anlz::waveform::{COLOR_PREVIEW_COLUMNS, PWAV_COLUMNS, PWV2_COLUMNS};
use reks_anlz::write::{self, LIST_HOT};
use reks_anlz::{PqtzBeat, Waveforms};
use reks_core::track::Cue;

pub fn find<'a>(sections: &'a [Section<'a>], tag: [u8; 4]) -> &'a Section<'a> {
    sections
        .iter()
        .find(|s| s.tag == tag)
        .unwrap_or_else(|| panic!("no {}", String::from_utf8_lossy(&tag)))
}

pub fn tags(bytes: &[u8]) -> Vec<String> {
    read::sections(bytes)
        .unwrap()
        .iter()
        .map(|s| s.tag_str())
        .collect()
}

/// Beats of the `.DAT` PQTZ.
pub fn beats(dat: &[u8]) -> Vec<PqtzBeat> {
    read::pqtz_beats(find(&read::sections(dat).unwrap(), write::PQTZ)).unwrap()
}

/// Library cues from the `.EXT` PCO2 lists (they hold every field).
pub fn cues(ext: &[u8]) -> Vec<Cue> {
    let sections = read::sections(ext).unwrap();
    let mut out = Vec::new();
    for s in sections.iter().filter(|s| s.tag == write::PCO2) {
        let list = read::cue_list(s).unwrap();
        for c in list.cues {
            out.push(Cue {
                id: 0,
                slot: (list.list_type == LIST_HOT).then(|| (c.hot_cue - 1) as u8),
                kind: if c.loop_end_ms.is_some() { 4 } else { 0 },
                start_ms: f64::from(c.time_ms),
                end_ms: c.loop_end_ms.map(f64::from),
                name: (!c.comment.is_empty()).then_some(c.comment),
                color_rgb: library_colour(c.hot_color),
            });
        }
    }
    out
}

/// MP3 frames from the `.DAT` PVBR trailer (0 → `None`).
pub fn mp3_frames(dat: &[u8]) -> Option<u32> {
    let sections = read::sections(dat).unwrap();
    let p = find(&sections, write::PVBR).payload();
    let v = u32::from_be_bytes(p[p.len() - 4..].try_into().unwrap());
    (v != 0).then_some(v / 1152)
}

pub fn path(dat: &[u8]) -> String {
    read::path(find(&read::sections(dat).unwrap(), write::PPTH)).unwrap()
}

/// Waveform payloads back into [`Waveforms`]; `ex2` may be absent (its
/// fields stay zero, PWVC = `pwvc_default`).
pub fn waveforms(dat: &[u8], ext: &[u8], ex2: Option<&[u8]>, pwvc_default: [u16; 3]) -> Waveforms {
    let (d, e) = (read::sections(dat).unwrap(), read::sections(ext).unwrap());
    let x = ex2.map(|b| read::sections(b).unwrap()).unwrap_or_default();
    let payload = |s: &[Section<'_>], tag| find(s, tag).payload().to_vec();
    let triples = |b: Vec<u8>| -> Vec<[u8; 3]> { b.as_chunks::<3>().0.to_vec() };
    let mut w = Waveforms {
        pwav: payload(&d, write::PWAV).try_into().unwrap(),
        pwv2: payload(&d, write::PWV2).try_into().unwrap(),
        pwv3: payload(&e, write::PWV3),
        pwv4: payload(&e, write::PWV4)
            .as_chunks::<6>()
            .0
            .to_vec()
            .try_into()
            .unwrap(),
        pwv5: payload(&e, write::PWV5)
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_be_bytes(c))
            .collect(),
        pwv6: [[0; 3]; COLOR_PREVIEW_COLUMNS],
        pwv7: Vec::new(),
        pwvc: pwvc_default,
    };
    assert_eq!((w.pwav.len(), w.pwv2.len()), (PWAV_COLUMNS, PWV2_COLUMNS));
    if !x.is_empty() {
        w.pwv6 = triples(payload(&x, write::PWV6)).try_into().unwrap();
        w.pwv7 = triples(payload(&x, write::PWV7));
        let c = payload(&x, write::PWVC);
        w.pwvc = [0, 2, 4].map(|i| u16::from_be_bytes([c[i], c[i + 1]]));
    } else {
        w.pwv7 = vec![[0; 3]; w.pwv3.len()];
    }
    w
}
