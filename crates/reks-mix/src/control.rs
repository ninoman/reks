//! Commands from the control side to the graph (`docs/phases/phase-5.md`).
//!
//! Knob-like values are 0..=1; tempo and nudge are speed ratios (1.0 = as
//! recorded). Positions are frames of the deck's source.

use std::sync::Arc;

use crate::graph::DeckSource;

/// A command for the graph.
#[derive(Debug, Clone)]
pub enum Control {
    /// Replaces the deck's track and stops it at the start. Tempo, sync and
    /// quantise stay as they were.
    Load {
        deck: u8,
        src: Arc<DeckSource>,
    },
    Unload {
        deck: u8,
    },
    Play {
        deck: u8,
    },
    Pause {
        deck: u8,
    },
    /// CUE pressed (CDJ): while playing, return to the cue point and pause;
    /// paused at the cue point, play until [`Control::CueRelease`] (cue
    /// preview); paused elsewhere, set the cue point here and move to it.
    Cue {
        deck: u8,
    },
    /// CUE let go: ends a cue preview (back to the cue point, paused).
    /// PLAY during the preview keeps the deck playing instead.
    CueRelease {
        deck: u8,
    },
    /// Clamped to the track. The new position plays from the next frame;
    /// while playing, the old one fades out under it over
    /// [`crate::graph::SEEK_XFADE_MS`], so the jump is instant without a gap
    /// or a click.
    Seek {
        deck: u8,
        frame: i64,
    },
    /// Jumps to hot cue `slot` and plays. Ignored when the slot is empty.
    HotCue {
        deck: u8,
        slot: u8,
    },
    /// Tempo fader as a speed ratio, clamped to [`crate::graph::TEMPO_MIN`]..=
    /// [`crate::graph::TEMPO_MAX`]. Pitch follows tempo.
    Tempo {
        deck: u8,
        ratio: f64,
    },
    /// Pitch bend on top of the tempo while held; 1.0 releases it.
    Nudge {
        deck: u8,
        ratio: f64,
    },
    /// Beat sync to the leader; `bar` also matches the downbeat.
    Sync {
        deck: u8,
        on: bool,
        bar: bool,
    },
    /// MASTER pressed: `deck` becomes the sync master (leader) now. It keeps
    /// the role until it stops, ends or is unloaded; see
    /// [`crate::graph::EngineState::master_deck`].
    TakeMaster {
        deck: u8,
    },
    /// Beat jump by `beats` (negative: back), along the grid: the place in
    /// the beat is kept. During a loop the loop moves with it. Ignored
    /// without a grid.
    BeatJump {
        deck: u8,
        beats: f64,
    },
    /// FINE jump: [`crate::graph::FINE_JUMP_MS`] forward or back, no grid
    /// needed.
    FineJump {
        deck: u8,
        forward: bool,
    },
    /// Beat loop of `beats` from the playhead (from the last 1/`beats`
    /// grid step with quantise); replaces an active loop. Ignored without a
    /// grid or when the loop would run past the track.
    BeatLoop {
        deck: u8,
        beats: f64,
    },
    /// Leaves the active loop; playback carries on past its end.
    LoopExit {
        deck: u8,
    },
    /// Halves (`double: false`) or doubles the active loop from its start.
    LoopResize {
        deck: u8,
        double: bool,
    },
    /// Quantise: cue points snap to the beat, jumps keep the beat phase.
    Quantise {
        deck: u8,
        on: bool,
    },
    /// Mixer channel strip.
    Channel {
        ch: u8,
        param: ChannelParam,
        value: f32,
    },
    /// Filter unit X or Y.
    Filter {
        unit: FilterUnit,
        param: FilterParam,
        value: f32,
    },
    /// Crossfader position and curve (0 progressive, 1 near-instant cut).
    Crossfader {
        pos: f32,
        curve: f32,
    },
    /// Master and cue section.
    Master {
        param: MasterParam,
        value: f32,
    },
    /// Preview player volume, 0..=1 like a knob; see [`crate::volume_gain`].
    Volume {
        value: f32,
    },
}

impl Control {
    /// The deck this control addresses, for deck controls.
    pub fn deck(&self) -> Option<u8> {
        match self {
            Self::Load { deck, .. }
            | Self::Unload { deck }
            | Self::Play { deck }
            | Self::Pause { deck }
            | Self::Cue { deck }
            | Self::CueRelease { deck }
            | Self::Seek { deck, .. }
            | Self::HotCue { deck, .. }
            | Self::Tempo { deck, .. }
            | Self::Nudge { deck, .. }
            | Self::Sync { deck, .. }
            | Self::Quantise { deck, .. }
            | Self::BeatJump { deck, .. }
            | Self::FineJump { deck, .. }
            | Self::BeatLoop { deck, .. }
            | Self::LoopExit { deck }
            | Self::LoopResize { deck, .. } => Some(*deck),
            Self::TakeMaster { .. }
            | Self::Channel { .. }
            | Self::Filter { .. }
            | Self::Crossfader { .. }
            | Self::Master { .. }
            | Self::Volume { .. } => None,
        }
    }
}

/// A channel strip control (R§9.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelParam {
    Trim,
    EqHi,
    EqMid1,
    EqMid2,
    EqLo,
    Fader,
    /// 0 = X, 0.5 = off, 1 = Y.
    FilterAssign,
    /// 0 = X, 0.5 = off (direct), 1 = Y.
    XfadeAssign,
    /// 0 = off, 1 = on.
    Cue,
}

/// One of the two filter units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterUnit {
    X,
    Y,
}

/// A filter unit control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterParam {
    Freq,
    Resonance,
    /// Switches: 0 = off, 1 = on.
    HighPass,
    BandPass,
    LowPass,
    On,
    VcfToXfader,
    /// Y follows X's FREQ (unit Y only).
    Link,
}

/// A master section control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterParam {
    Level,
    CueLevel,
    /// 0 = cue only, 1 = master only.
    CueMix,
    /// 0 = off, 1 = on.
    SplitCue,
    /// 0 = pre-EQ, 1 = post-EQ.
    CuePostEq,
}

/// Where the headphone cue goes on the output device. The engine refuses a
/// routing the device has no outputs for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CueRouting {
    /// Master on outputs `master`, cue on outputs `cue` (0-based).
    Outputs { master: [u16; 2], cue: [u16; 2] },
    /// One stereo output for both: cue (mono) on the left, master (mono)
    /// on the right, at the cue level, as with split cue.
    SplitMono,
    /// Master only.
    #[default]
    Off,
}
