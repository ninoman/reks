//! The ANLZ file builder: `.DAT`, `.EXT` and (for CDJ-3000 targets) `.2EX`
//! from a track's beats, cues and encoded waveforms, in rekordbox's section
//! order:
//!
//! | File | Sections |
//! | --- | --- |
//! | `.DAT` | PPTH, PVBR, PQTZ, PWAV, PWV2, PCOB hot (A–C), PCOB memory |
//! | `.EXT` | PPTH, PWV3, PCOB hot (D–H), PCOB memory (empty), PCO2 hot, PCO2 memory, PQT2 (empty), PWV5, PWV4 |
//! | `.2EX` | PPTH, PWV7, PWV6, PWVC |
//!
//! rekordbox also writes `PSSI` (phrases, a Phase 3 non-goal) and, for
//! tracks it analysed itself, a full `PQT2`; we write the empty `PQT2`
//! rekordbox uses without an extended grid. Whether NXS2 features need a
//! full `PQT2` is UNVERIFIED (phase 3 risks). Every set is checked with
//! [`crate::validate`] before it is returned.

use reks_core::track::Cue;

use crate::cue::CueLists;
use crate::validate::{validate_set, Expect, Issue};
use crate::waveform::Waveforms;
use crate::write::{self, PqtzBeat, WriteError, LIST_HOT, LIST_MEMORY};

/// Which files the target players read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnlzTarget {
    /// `.DAT` + `.EXT` (CDJ-2000NXS2 and older, XDJs).
    Standard,
    /// Also the `.2EX` with the 3-band waveforms (CDJ-3000).
    Cdj3000,
}

/// What [`build`] turns into ANLZ files.
#[derive(Debug, Clone, Copy)]
pub struct AnlzInput<'a> {
    /// Stick path of the audio file (`/Contents/...`), as `PPTH` stores it.
    pub contents_path: &'a str,
    /// Track length; beats and cues must lie inside it.
    pub duration_ms: u64,
    /// `PQTZ` beats, e.g. from [`crate::grid::pqtz_beats`]; empty for a
    /// track without a grid.
    pub beats: &'a [PqtzBeat],
    /// Library cues (hot cues past H are dropped).
    pub cues: &'a [Cue],
    /// Encoded waveforms ([`Waveforms::encode`]).
    pub waveforms: &'a Waveforms,
    /// MPEG audio frames of an MP3 source, for `PVBR`; `None` otherwise.
    pub mp3_audio_frames: Option<u32>,
}

/// The built files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnlzSet {
    pub dat: Vec<u8>,
    pub ext: Vec<u8>,
    /// Only for [`AnlzTarget::Cdj3000`].
    pub ex2: Option<Vec<u8>>,
}

impl AnlzSet {
    /// `(extension, bytes)` for each file, `.DAT` first.
    pub fn files(&self) -> Vec<(&'static str, &[u8])> {
        let mut out = vec![("DAT", self.dat.as_slice()), ("EXT", self.ext.as_slice())];
        if let Some(ex2) = &self.ex2 {
            out.push(("2EX", ex2.as_slice()));
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BuildError {
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error("the built ANLZ files fail validation: {}", list(.0))]
    Invalid(Vec<Issue>),
}

fn list(issues: &[Issue]) -> String {
    issues
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

/// Builds the ANLZ set for one track.
pub fn build(input: &AnlzInput<'_>, target: AnlzTarget) -> Result<AnlzSet, BuildError> {
    let path = write::ppth(input.contents_path)?;
    let cues = CueLists::new(input.cues, input.beats);
    let w = input.waveforms;

    let mut dat = vec![
        path.clone(),
        write::pvbr(input.mp3_audio_frames)?,
        write::pqtz(input.beats)?,
    ];
    dat.extend(w.dat_sections()?);
    dat.push(write::pcob(LIST_HOT, &cues.dat_hot())?);
    dat.push(write::pcob(LIST_MEMORY, &cues.memory)?);

    // `Waveforms::ext_sections` order, with the cue lists and PQT2 between
    // PWV3 and PWV5 as rekordbox places them.
    let ext = vec![
        path.clone(),
        write::pwv3(&w.pwv3)?,
        write::pcob(LIST_HOT, &cues.ext_hot())?,
        write::pcob(LIST_MEMORY, &[])?,
        write::pco2(LIST_HOT, &cues.hot)?,
        write::pco2(LIST_MEMORY, &cues.memory)?,
        write::pqt2_empty()?,
        write::pwv5(&w.pwv5)?,
        write::pwv4(&w.pwv4)?,
    ];

    let ex2 = match target {
        AnlzTarget::Standard => None,
        AnlzTarget::Cdj3000 => {
            let mut s = vec![path];
            s.extend(w.ex2_sections()?);
            Some(write::file(&s)?)
        }
    };
    let set = AnlzSet {
        dat: write::file(&dat)?,
        ext: write::file(&ext)?,
        ex2,
    };
    let issues = validate_set(
        &set.dat,
        &set.ext,
        set.ex2.as_deref(),
        Expect {
            path: Some(input.contents_path),
            duration_ms: Some(input.duration_ms),
        },
    );
    if issues.is_empty() {
        Ok(set)
    } else {
        Err(BuildError::Invalid(issues))
    }
}
