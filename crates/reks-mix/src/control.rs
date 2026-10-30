//! Commands from the control side to the graph (`docs/phases/phase-5.md`).
//!
//! Knob-like values are 0..=1; tempo and nudge are speed ratios (1.0 = as
//! recorded). Positions are frames of the deck's source.

use std::sync::Arc;

use crate::graph::DeckSource;

/// A command for the graph.
#[derive(Debug, Clone)]
pub enum Control {
    /// Replaces the deck's track and stops it at the start. Tempo, sync,
    /// quantise and master tempo stay as they were.
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
    /// [`Control::Seek`] without quantise: the head goes exactly to `frame`
    /// (grid mode places it between beats to set the grid there).
    SeekExact {
        deck: u8,
        frame: i64,
    },
    /// Jumps to hot cue `slot` and plays. Ignored when the slot is empty.
    HotCue {
        deck: u8,
        slot: u8,
    },
    /// Sets hot cue `slot` (0..[`crate::HOT_CUE_SLOTS`]) of the loaded track
    /// at `frame`, or clears it with `None`: a cue edited on the deck, which
    /// the app also stores in the library. Ignored while a load is pending
    /// (the new track brings its own).
    SetHotCue {
        deck: u8,
        slot: u8,
        frame: Option<i64>,
    },
    /// Replaces the loaded track's source with `src`, the same audio with
    /// an edited grid ([`DeckSource::with_new_grid`]): position, playback
    /// and loops stay. Ignored while a load is pending (the new track brings
    /// its own grid).
    SetGrid {
        deck: u8,
        src: Arc<DeckSource>,
    },
    /// Sets active memory loop slot `index` (0..[`crate::AUTO_LOOP_SLOTS`])
    /// of the loaded track to `(start, end)` frames, or clears it: the app
    /// sends every slot after an edit. Playback enters an active loop by
    /// itself when the head reaches its start. Ignored while a load is
    /// pending (the new track brings its own).
    AutoLoop {
        deck: u8,
        index: u8,
        region: Option<(i64, i64)>,
    },
    /// Memory cue call: the cue point and the head move to `frame` exactly
    /// (no quantise); a playing deck plays on from there. A memory loop
    /// (`end`) becomes the loop [`Control::Reloop`] enters; the call itself
    /// does not loop. UNVERIFIED against a CDJ's CALL buttons; this is
    /// rekordbox's memory cue list.
    CallCue {
        deck: u8,
        frame: i64,
        end: Option<i64>,
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
    /// Beat sync to the leader (beats only: bars are the DJ's call).
    Sync {
        deck: u8,
        on: bool,
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
    /// Leaves the active loop, keeping it for [`Control::Reloop`]; playback
    /// carries on past its end.
    LoopExit {
        deck: u8,
    },
    /// Loop IN at the head (the nearest beat with quantise). Looping: moves
    /// the loop's start there, keeping its end. Otherwise: a pending IN for
    /// [`Control::LoopOut`]. Works without a grid.
    LoopIn {
        deck: u8,
    },
    /// Loop OUT at the head (the nearest beat with quantise; one beat after
    /// IN if that is on or before it). Looping: moves the loop's end there.
    /// Otherwise, with a pending IN behind the head: loops IN → OUT at once;
    /// paused, the head moves to IN. Ignored for a loop shorter than
    /// 2 × [`crate::graph::LOOP_XFADE_MS`] or without an IN.
    LoopOut {
        deck: u8,
    },
    /// RELOOP: looping, as [`Control::LoopExit`]; otherwise back into the
    /// last loop left (or memory loop called) and loop it. With quantise
    /// while playing the jump keeps the beat phase.
    Reloop {
        deck: u8,
    },
    /// Halves (`double: false`) or doubles the active loop from its start:
    /// along the grid, or by time without one.
    LoopResize {
        deck: u8,
        double: bool,
    },
    /// Quantise: cue points snap to the beat, jumps keep the beat phase.
    Quantise {
        deck: u8,
        on: bool,
    },
    /// Master tempo (key lock): tempo changes keep the key. Stays across
    /// loads, like tempo.
    MasterTempo {
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
    /// The effect every channel's FX knob drives (3-band layout).
    ChannelFx {
        kind: FxKind,
    },
    /// The mixer-wide PARAMETER knob of the channel FX, 0..=1: FILTER
    /// resonance, ECHO feedback or REVERB decay.
    FxParameter {
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
            | Self::SeekExact { deck, .. }
            | Self::HotCue { deck, .. }
            | Self::SetHotCue { deck, .. }
            | Self::SetGrid { deck, .. }
            | Self::CallCue { deck, .. }
            | Self::AutoLoop { deck, .. }
            | Self::Tempo { deck, .. }
            | Self::Nudge { deck, .. }
            | Self::Sync { deck, .. }
            | Self::Quantise { deck, .. }
            | Self::MasterTempo { deck, .. }
            | Self::BeatJump { deck, .. }
            | Self::FineJump { deck, .. }
            | Self::BeatLoop { deck, .. }
            | Self::LoopExit { deck }
            | Self::LoopIn { deck }
            | Self::LoopOut { deck }
            | Self::Reloop { deck }
            | Self::LoopResize { deck, .. } => Some(*deck),
            Self::TakeMaster { .. }
            | Self::Channel { .. }
            | Self::Filter { .. }
            | Self::ChannelFx { .. }
            | Self::FxParameter { .. }
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
    /// 3-band EQ (`docs/research/djm-eq-fx.md` §2), 0.5 flat. Runs in series
    /// with the 4-band EQ; the hidden layout's knobs sit at noon.
    Eq3Hi,
    Eq3Mid,
    Eq3Lo,
    /// Channel FX knob: 0 full left, 0.5 off, 1 full right.
    Fx,
    Fader,
    /// 0 = X, 0.5 = off, 1 = Y.
    FilterAssign,
    /// 0 = X, 0.5 = off (direct), 1 = Y.
    XfadeAssign,
    /// 0 = off, 1 = on.
    Cue,
}

/// The channel FX effects (`docs/research/djm-eq-fx.md` §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FxKind {
    /// Left low-pass, right high-pass.
    #[default]
    Filter,
    /// Left echo on the mids, right on the highs.
    Echo,
    /// Left reverb on the mids and lows, right on the mids and highs.
    Reverb,
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
