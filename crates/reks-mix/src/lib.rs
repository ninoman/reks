//! reks real-time audio engine: a cpal output stream, decks and an offline
//! [`render`] of the same graph.
//!
//! Phase 1 ships one deck as the preview player (docs/decisions.md D5); the
//! mixer is Phase 5 (D21, `docs/phases/phase-5.md`).

pub mod engine;
pub mod error;
pub mod graph;
pub mod player;

pub use engine::{output_devices, Engine, EngineConfig, StreamInfo, PREFERRED_RATE};
pub use error::{Error, Result};
pub use graph::{
    render, volume_gain, Control, DeckSource, DeckState, EngineState, Graph, DECKS, RAMP_MS,
};
pub use player::{Player, PlayerState, Reopened};
