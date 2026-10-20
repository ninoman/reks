//! reks real-time audio engine: a cpal output stream, decks and an offline
//! [`render`] of the same graph.
//!
//! Phase 1 ships one deck as the preview player (docs/decisions.md D5); the
//! mixer is Phase 5 (D21, `docs/phases/phase-5.md`).

pub mod beatmap;
pub mod control;
mod cue;
mod deck;
pub mod dsp;
pub mod engine;
pub mod error;
pub mod graph;
mod mixer;
pub mod player;
mod routing;
mod source;
mod strip;
mod sync;
pub mod taper;
mod varispeed;

pub use beatmap::{BeatMap, GridAnchor};
pub use control::{ChannelParam, Control, CueRouting, FilterParam, FilterUnit, MasterParam};
pub use engine::{
    device_options, output_devices, DeviceOptions, Engine, EngineConfig, StreamInfo, COMMON_RATES,
    PREFERRED_RATE,
};
pub use error::{Error, Result};
pub use graph::{
    render, volume_gain, DeckSource, DeckState, EngineState, Graph, MeterState, DECKS, RAMP_MS,
    TEMPO_MAX, TEMPO_MIN,
};
pub use player::{Player, PlayerState, Reopened};
