//! The Phase 1 preview player: one deck on the [`Engine`], in milliseconds.
//!
//! The engine opens on first use, so a machine without an output device
//! still runs the rest of the app. Decoding is the caller's job: hand
//! [`Player::load`] stereo `i16` at [`Player::sample_rate`].

use std::sync::Arc;

use crate::engine::{Engine, EngineConfig};
use crate::error::{Error, Result};
use crate::graph::{Control, DeckSource, DeckState};

const DECK: u8 = 0;

/// What the transport bar shows.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub loaded: bool,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    /// 0..=1, the knob position.
    pub volume: f32,
    /// The open device, if any.
    pub device: Option<String>,
    /// The device went away; the next command reopens the output.
    pub device_lost: bool,
}

/// Whether the caller must decode the track again after a device change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reopened {
    /// Same rate: the track, position and play state carried over.
    Kept,
    /// The rate changed: load the track again at the new rate, then seek to
    /// `position_ms` (and play if `playing`).
    Reload { position_ms: u64, playing: bool },
    /// No track was loaded.
    Empty,
}

#[derive(Debug)]
struct Loaded {
    src: Arc<DeckSource>,
    sample_rate: u32,
}

/// The latest seek sent, reported as the position until the audio thread's
/// snapshot shows it applied, so the playhead never snaps back.
#[derive(Debug, Clone, Copy)]
struct SentSeek {
    /// The deck's `seeks` count once this seek is applied.
    seq: u32,
    frame: i64,
}

/// One-deck player.
#[derive(Debug)]
pub struct Player {
    device: Option<String>,
    engine: Option<Engine>,
    loaded: Option<Loaded>,
    volume: f32,
    /// Seeks sent on the current engine (wrapping); matches the deck's count.
    seeks: u32,
    last_seek: Option<SentSeek>,
    /// Play/pause controls sent on the current engine (wrapping); matches
    /// the deck's count once the last one is applied.
    transports: u32,
    /// The last play/pause sent, reported until the deck takes it.
    want_playing: bool,
}

impl Player {
    /// A player for `device` (`None`: system default). Opens nothing yet.
    pub fn new(device: Option<String>) -> Self {
        Self {
            device,
            engine: None,
            loaded: None,
            volume: 1.0,
            seeks: 0,
            last_seek: None,
            transports: 0,
            want_playing: false,
        }
    }

    /// Starts at `volume` (0..=1) instead of full; sent when the output opens.
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self
    }

    /// The output's sample rate; opens the output if needed.
    pub fn sample_rate(&mut self) -> Result<u32> {
        Ok(self.engine()?.info().sample_rate)
    }

    /// Loads interleaved stereo at `sample_rate`, stopped at the start.
    pub fn load(&mut self, samples: Vec<i16>, sample_rate: u32) -> Result<()> {
        let rate = self.sample_rate()?;
        if rate != sample_rate {
            return Err(Error::RateMismatch {
                track: sample_rate,
                device: rate,
            });
        }
        let src = Arc::new(DeckSource::new(samples));
        self.engine()?.send(Control::Load {
            deck: DECK,
            src: Arc::clone(&src),
        })?;
        self.loaded = Some(Loaded { src, sample_rate });
        self.last_seek = None;
        self.want_playing = false;
        Ok(())
    }

    pub fn unload(&mut self) -> Result<()> {
        self.loaded = None;
        self.want_playing = false;
        self.send(Control::Unload { deck: DECK })
    }

    pub fn play(&mut self) -> Result<()> {
        self.send(Control::Play { deck: DECK })?;
        self.transports = self.transports.wrapping_add(1);
        self.want_playing = true;
        Ok(())
    }

    pub fn pause(&mut self) -> Result<()> {
        self.send(Control::Pause { deck: DECK })?;
        self.transports = self.transports.wrapping_add(1);
        self.want_playing = false;
        Ok(())
    }

    pub fn seek_ms(&mut self, ms: u64) -> Result<()> {
        let Some(l) = &self.loaded else {
            return Ok(());
        };
        let frame = i64::try_from(ms.saturating_mul(u64::from(l.sample_rate)) / 1000)
            .unwrap_or(i64::MAX)
            .clamp(0, l.src.frames());
        self.send(Control::Seek { deck: DECK, frame })?;
        self.seeks = self.seeks.wrapping_add(1);
        self.last_seek = Some(SentSeek {
            seq: self.seeks,
            frame,
        });
        Ok(())
    }

    /// 0..=1 like a knob (squared taper, [`crate::volume_gain`]).
    pub fn set_volume(&mut self, v: f32) -> Result<()> {
        self.volume = if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.send(Control::Volume { value: self.volume })
    }

    /// Current state. Does not open the output.
    pub fn state(&mut self) -> PlayerState {
        let duration_ms = self
            .loaded
            .as_ref()
            .map_or(0, |l| frames_to_ms(l.src.frames(), l.sample_rate));
        let mut st = PlayerState {
            loaded: self.loaded.is_some(),
            duration_ms,
            volume: self.volume,
            ..PlayerState::default()
        };
        if let Some(engine) = self.engine.as_mut() {
            let deck = engine.poll_state().decks[usize::from(DECK)];
            let rate = engine.info().sample_rate;
            st.device = Some(engine.info().device.clone());
            st.device_lost = engine.device_lost();
            let loaded_frames = self.loaded.as_ref().map(|l| l.src.frames());
            let want = Wanted {
                playing: self.want_playing,
                taken: deck.transports == self.transports,
            };
            (st.playing, st.position_ms) = shown(deck, loaded_frames, self.last_seek, want, rate);
        }
        st
    }

    /// Switches to `device` (`None`: system default) and carries the track
    /// over when the new output runs at the same rate.
    pub fn set_device(&mut self, device: Option<String>) -> Result<Reopened> {
        let before = self.state();
        self.engine = None;
        self.device = device;
        let rate = self.sample_rate()?;
        let Some(l) = &self.loaded else {
            return Ok(Reopened::Empty);
        };
        if l.sample_rate != rate {
            self.loaded = None;
            return Ok(Reopened::Reload {
                position_ms: before.position_ms,
                playing: before.playing,
            });
        }
        let src = Arc::clone(&l.src);
        self.send(Control::Load { deck: DECK, src })?;
        self.seek_ms(before.position_ms)?;
        if before.playing {
            self.play()?;
        }
        Ok(Reopened::Kept)
    }

    /// The open engine; reopens it after a lost device.
    fn engine(&mut self) -> Result<&mut Engine> {
        if self.engine.as_ref().is_some_and(Engine::device_lost) {
            tracing::info!("reopening audio output after device loss");
            self.engine = None;
            self.loaded = None;
        }
        match &mut self.engine {
            Some(e) => Ok(e),
            slot @ None => {
                let start = |device| Engine::start(EngineConfig { device });
                let mut e = match start(self.device.clone()) {
                    // A chosen device that is not connected: play on the
                    // system default, keep the choice for when it is back.
                    Err(Error::NoDevice(Some(name))) => {
                        tracing::warn!(device = %name, "output device not found, using the default");
                        start(None)?
                    }
                    r => r?,
                };
                e.send(Control::Volume { value: self.volume })?;
                // A new graph counts seeks and play/pause from zero.
                self.seeks = 0;
                self.last_seek = None;
                self.transports = 0;
                Ok(slot.insert(e))
            }
        }
    }

    fn send(&mut self, c: Control) -> Result<()> {
        self.engine()?.send(c)
    }
}

/// The last play/pause sent, and whether the deck's snapshot includes it.
#[derive(Debug, Clone, Copy)]
struct Wanted {
    playing: bool,
    taken: bool,
}

/// Play state and position to report. Until the audio thread has taken the
/// load (a new track, or the same one on a new device), the deck state is
/// stale: report what was asked for, so the transport keeps polling and the
/// playhead does not drop to zero. The same goes for a play or pause the
/// snapshot does not include yet: `player_play` answers at once, and a stale
/// "stopped" would leave the transport showing Play over a running track.
fn shown(
    deck: DeckState,
    loaded_frames: Option<i64>,
    last_seek: Option<SentSeek>,
    want: Wanted,
    rate: u32,
) -> (bool, u64) {
    match loaded_frames {
        None => (false, 0),
        Some(frames) if deck.frames != frames => (
            want.playing,
            last_seek.map_or(0, |s| frames_to_ms(s.frame, rate)),
        ),
        Some(_) => {
            let position = match last_seek {
                Some(s) if s.seq != deck.seeks => s.frame,
                _ => deck.position,
            };
            let playing = if want.taken {
                deck.playing
            } else {
                want.playing
            };
            (playing, frames_to_ms(position, rate))
        }
    }
}

fn frames_to_ms(frames: i64, rate: u32) -> u64 {
    u64::try_from(frames).unwrap_or(0) * 1000 / u64::from(rate.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_without_an_output_is_empty_and_opens_nothing() {
        let mut p = Player::new(None);
        let st = p.state();
        assert_eq!(
            st,
            PlayerState {
                volume: 1.0,
                ..PlayerState::default()
            }
        );
        assert!(p.engine.is_none());
        // Seeking with nothing loaded is a no-op, not an error.
        assert!(p.seek_ms(1_000).is_ok());
        assert!(p.engine.is_none());
    }

    /// A play/pause the snapshot does not include yet.
    fn want(playing: bool) -> Wanted {
        Wanted {
            playing,
            taken: false,
        }
    }

    /// The last play/pause is in the snapshot; `playing` no longer matters.
    const TAKEN: Wanted = Wanted {
        playing: false,
        taken: true,
    };

    #[test]
    fn pending_load_reports_what_was_asked_for() {
        // A device switch sent Load, Seek to 1 s and Play; the new engine's
        // deck is still empty.
        let seek = Some(SentSeek {
            seq: 1,
            frame: 48_000,
        });
        assert_eq!(
            shown(DeckState::default(), Some(96_000), seek, want(true), 48_000),
            (true, 1_000)
        );
        // A fresh load with no seek shows zero, stopped unless asked to play.
        assert_eq!(
            shown(
                DeckState::default(),
                Some(96_000),
                None,
                want(false),
                48_000
            ),
            (false, 0)
        );
        assert_eq!(
            shown(DeckState::default(), None, None, want(true), 48_000),
            (false, 0)
        );
    }

    #[test]
    fn taken_load_reports_the_deck() {
        let deck = DeckState {
            playing: true,
            position: 72_000,
            frames: 96_000,
            seeks: 1,
            ..DeckState::default()
        };
        let applied = Some(SentSeek {
            seq: 1,
            frame: 48_000,
        });
        assert_eq!(
            shown(deck, Some(96_000), applied, TAKEN, 48_000),
            (true, 1_500)
        );
        // A newer seek not applied yet still shows its target.
        let newer = Some(SentSeek {
            seq: 2,
            frame: 24_000,
        });
        assert_eq!(shown(deck, Some(96_000), newer, TAKEN, 48_000), (true, 500));
    }

    #[test]
    fn untaken_play_or_pause_reports_what_was_asked_for() {
        let stopped = DeckState {
            position: 48_000,
            frames: 96_000,
            ..DeckState::default()
        };
        // Play sent, the snapshot still shows the deck stopped.
        assert_eq!(
            shown(stopped, Some(96_000), None, want(true), 48_000),
            (true, 1_000)
        );
        // Once taken, the deck decides (it may have run off the end).
        assert_eq!(
            shown(stopped, Some(96_000), None, TAKEN, 48_000),
            (false, 1_000)
        );
        // Pause sent while the snapshot still shows it playing.
        let running = DeckState {
            playing: true,
            ..stopped
        };
        assert_eq!(
            shown(running, Some(96_000), None, want(false), 48_000),
            (false, 1_000)
        );
    }

    #[test]
    fn frames_to_ms_rounds_down() {
        assert_eq!(frames_to_ms(48_000, 48_000), 1_000);
        assert_eq!(frames_to_ms(47_999, 48_000), 999);
        assert_eq!(frames_to_ms(-5, 48_000), 0);
    }
}
