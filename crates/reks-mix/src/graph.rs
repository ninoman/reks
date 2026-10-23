//! The audio graph: four decks into the mixer ([`crate::mixer`]), rendered
//! in short blocks.
//!
//! Everything here runs on the audio thread: no allocation, locks or I/O.
//! [`render`] runs the same graph without a device, for tests (D21).

use std::sync::Arc;

use crate::control::Control;
use crate::deck::Deck;
use crate::error::{Error, Result};
pub use crate::mixer::MeterState;
use crate::mixer::Mixer;
// Deck speeds are set once per block, and the mixer runs a block at a time.
pub use crate::source::DeckSource;
use crate::strip::{Block, BLOCK};
use crate::sync;

/// Deck slots in the graph. The UI shows 2 or 4; idle decks cost nothing.
pub const DECKS: usize = 4;

/// Slowest tempo fader setting: −100%, the bottom of the CDJ-3000's WIDE
/// range, where the deck stands still.
pub const TEMPO_MIN: f64 = 0.0;
/// Fastest tempo fader setting: +100% (WIDE).
pub const TEMPO_MAX: f64 = 2.0;

/// Length of every gain ramp: play, pause, load, volume (phase-5.md: ≥ 5 ms).
pub const RAMP_MS: u32 = 5;

/// Equal-power crossfade from the old to the new position on a seek while
/// playing. Long enough to hide the jump in the bass, short enough to feel
/// instant; a listening choice, not a measured rekordbox value.
pub const SEEK_XFADE_MS: u32 = 20;

/// Crossfade at a loop's end, from the head running on past it to the
/// loop start: short, so even a 1/8-beat loop (about 60 ms at 128 BPM) is
/// mostly loop, long enough not to click. A listening choice.
pub const LOOP_XFADE_MS: u32 = 4;

/// FINE beat jump. A fixed time, not a share of the beat; rekordbox's own
/// FINE amount is not documented (UNVERIFIED).
pub const FINE_JUMP_MS: u32 = 10;

/// Shortest and longest beat loop (halve/double stop here).
pub const LOOP_MIN_BEATS: f64 = 1.0 / 32.0;
pub const LOOP_MAX_BEATS: f64 = 512.0;

/// A loop, in the deck's source frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopRegion {
    pub start: i64,
    /// Where it jumps back to `start`.
    pub end: i64,
    /// Length in beats along the grid (fractional for a manual loop);
    /// `None` without a grid.
    pub beats: Option<f64>,
}

/// One deck as the UI sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeckState {
    pub loaded: bool,
    pub playing: bool,
    /// Source frames from the start of the track.
    pub position: i64,
    pub frames: i64,
    /// Seek controls taken so far (wrapping), so the control side can tell
    /// whether a snapshot already includes its latest seek.
    pub seeks: u32,
    /// Play and pause controls taken so far (wrapping), for the same reason.
    pub transports: u32,
    /// The cue point, in source frames.
    pub cue: i64,
    /// Tempo fader, as a speed ratio.
    pub tempo: f64,
    /// Pitch bend held on top of the tempo; 1.0 when released.
    pub nudge: f64,
    /// The speed the deck plays at (tempo × nudge, or what sync sets).
    pub speed: f64,
    /// Tempo heard at the playhead, from the grid.
    pub bpm: Option<f64>,
    /// Beat number at the playhead (0 at the grid's first anchor).
    pub beat: Option<f64>,
    /// CUE is held from the cue point: the deck plays until it is let go.
    pub cue_preview: bool,
    /// Sync is on.
    pub sync: bool,
    /// Sync matches the downbeat too.
    pub sync_bar: bool,
    /// Sync is following the leader (on, not the leader, and both decks
    /// have a grid).
    pub locked: bool,
    pub quantise: bool,
    /// Master tempo (key lock) is on.
    pub master_tempo: bool,
    /// This deck is the sync master ([`EngineState::master_deck`]).
    pub master: bool,
    /// The active loop, if any.
    pub loop_region: Option<LoopRegion>,
    /// A pending loop IN, waiting for OUT.
    pub loop_in: Option<i64>,
    /// The loop RELOOP enters: the last one left, or a memory loop called.
    pub last_loop: Option<LoopRegion>,
}

impl Default for DeckState {
    fn default() -> Self {
        Self {
            loaded: false,
            playing: false,
            position: 0,
            frames: 0,
            seeks: 0,
            transports: 0,
            cue: 0,
            tempo: 1.0,
            nudge: 1.0,
            speed: 1.0,
            bpm: None,
            beat: None,
            cue_preview: false,
            sync: false,
            sync_bar: false,
            locked: false,
            quantise: false,
            master_tempo: false,
            master: false,
            loop_region: None,
            loop_in: None,
            last_loop: None,
        }
    }
}

/// Graph snapshot, sent to the control side about 60 times a second.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EngineState {
    pub decks: [DeckState; DECKS],
    /// The sync master (leader), as on a CDJ: the deck that took it with
    /// [`Control::TakeMaster`], else the first deck with a grid to play.
    /// When it stops, ends or is unloaded, the role passes to a playing
    /// synced deck, which keeps its tempo; with none, nobody is master.
    pub master_deck: Option<u8>,
    pub volume: f32,
    /// Channel meters, after EQ and filter, before the fader.
    pub channels: [MeterState; DECKS],
    /// Master meter, after the master level, before the limiter.
    pub master: MeterState,
}

/// Knob position to gain: squared, so the top of the travel is finer.
pub fn volume_gain(value: f32) -> f32 {
    let v = if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    };
    v * v
}

/// A linear gain ramp to a target over [`RAMP_MS`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ramp {
    pub(crate) value: f32,
    pub(crate) target: f32,
    step: f32,
}

impl Ramp {
    pub(crate) fn new(value: f32, sample_rate: u32) -> Self {
        let frames = (sample_rate * RAMP_MS / 1000).max(1);
        Self {
            value,
            target: value,
            step: 1.0 / frames as f32,
        }
    }

    pub(crate) fn next(&mut self) -> f32 {
        if self.value < self.target {
            self.value = (self.value + self.step).min(self.target);
        } else if self.value > self.target {
            self.value = (self.value - self.step).max(self.target);
        }
        self.value
    }

    pub(crate) fn settled(&self) -> bool {
        self.value == self.target
    }
}

/// The whole graph. Created off the audio thread, then moved into it.
#[derive(Debug)]
pub struct Graph {
    decks: [Deck; DECKS],
    /// The sync master.
    master: Option<usize>,
    /// The master was playing at the last block, so stopping hands it on.
    master_playing: bool,
    /// The master's track was replaced or unloaded since the last block.
    master_unloaded: bool,
    engine_rate: f64,
    /// Counts play starts, for electing a master.
    clock: u64,
    mixer: Mixer,
    /// One block per deck, the mixer's input.
    inputs: [Block; DECKS],
}

impl Graph {
    /// Builds the graph for `sample_rate`; allocates every deck's buffers.
    pub fn new(sample_rate: u32) -> Result<Self> {
        let decks: Vec<Deck> = (0..DECKS)
            .map(|_| Deck::new(sample_rate))
            .collect::<Option<_>>()
            .ok_or(Error::Resampler)?;
        Ok(Self {
            decks: decks.try_into().map_err(|_| Error::Resampler)?,
            master: None,
            master_playing: false,
            master_unloaded: false,
            engine_rate: f64::from(sample_rate.max(1)),
            clock: 0,
            mixer: Mixer::new(sample_rate),
            inputs: [[0.0; BLOCK * 2]; DECKS],
        })
    }

    /// Applies one control. A replaced source comes back so the caller can
    /// drop it off the audio thread. Out-of-range decks are ignored.
    pub fn apply(&mut self, c: Control) -> Option<Arc<DeckSource>> {
        if self.mixer.apply(&c) {
            return None;
        }
        if let Control::TakeMaster { deck } = c {
            let i = usize::from(deck);
            if let Some(d) = self.decks.get_mut(i).filter(|d| has_grid(d)) {
                // A follower taking over keeps the tempo it matched.
                if let Some(tempo) = d.matched.take() {
                    d.set_tempo(tempo);
                }
                self.master = Some(i);
                self.master_playing = self.decks[i].playing();
                self.master_unloaded = false;
            }
            return None;
        }
        let deck = c.deck()?;
        if matches!(c, Control::Load { .. } | Control::Unload { .. })
            && self.master == Some(usize::from(deck))
        {
            self.master_unloaded = true;
        }
        self.clock += 1;
        match self.decks.get_mut(usize::from(deck)) {
            Some(d) => d.apply(c, self.clock),
            None => match c {
                Control::Load { src, .. } => Some(src),
                _ => None,
            },
        }
    }

    /// A snapshot; clears the meters' clip latches.
    pub fn state(&mut self) -> EngineState {
        let (channels, master) = self.mixer.meters();
        EngineState {
            decks: std::array::from_fn(|i| DeckState {
                master: self.master == Some(i),
                ..self.decks[i].state()
            }),
            master_deck: self.master.map(|d| d as u8),
            volume: self.mixer.volume(),
            channels,
            master,
        }
    }

    /// Frames between a deck sample and its output: the safety limiter's
    /// look-ahead.
    pub fn latency(&self) -> usize {
        self.mixer.latency()
    }

    /// A playing deck with a grid to take the master role: a synced one
    /// first, then the one playing longest.
    fn elect(&self, except: Option<usize>) -> Option<usize> {
        self.decks
            .iter()
            .enumerate()
            .filter(|&(i, d)| Some(i) != except && d.playing() && has_grid(d))
            .min_by_key(|(_, d)| (!d.locked, d.started))
            .map(|(i, _)| i)
    }

    /// Hands the master role on when the master stopped, ended or lost its
    /// track, and elects one when there is none.
    fn update_master(&mut self) {
        if let Some(m) = self.master {
            let d = &self.decks[m];
            let lost = std::mem::take(&mut self.master_unloaded)
                || !has_grid(d)
                || (self.master_playing && !d.playing());
            if lost {
                self.hand_off(m);
            }
        }
        if self.master.is_none() {
            self.master = self.elect(None);
        }
        self.master_playing = self.master.is_some_and(|m| self.decks[m].playing());
    }

    /// The old master's followers keep the tempo they matched: the new
    /// master on its tempo fader, and with no new master every follower
    /// left, so nothing changes speed.
    fn hand_off(&mut self, from: usize) {
        let next = self.elect(Some(from));
        for (i, d) in self.decks.iter_mut().enumerate() {
            if i == from || next.is_some_and(|n| n != i) {
                continue;
            }
            if let Some(tempo) = d.matched.take() {
                d.set_tempo(tempo);
            }
        }
        self.master = next;
    }

    /// Sets every deck's speed for the next block: the master and unsynced
    /// decks at their own speed, followers from [`sync::follow`].
    fn plan(&mut self) {
        self.update_master();
        let master = self.master;
        let lead = master.and_then(|i| {
            let d = &mut self.decks[i];
            let speed = d.own_speed();
            d.set_speed(speed);
            let src = d.src()?;
            sync::Lead::new(
                src.beats(),
                d.position(),
                d.step(),
                d.playing(),
                d.loop_beats(),
            )
        });
        for (i, d) in self.decks.iter_mut().enumerate() {
            if Some(i) == master {
                d.locked = false;
                d.matched = None;
                continue;
            }
            let me = d.src().map(|src| sync::Follower {
                beats: src.beats(),
                position: d.position(),
                rate_ratio: d.rate_ratio(),
                playing: d.playing(),
                bar: d.sync_bar,
                nudge: d.nudge,
                looping: d.looping(),
            });
            let (plan, matched) = match (&lead, d.sync, me) {
                (Some(lead), true, Some(me)) => (
                    sync::follow(lead, &me, self.engine_rate),
                    sync::matched_speed(lead, &me),
                ),
                _ => (None, None),
            };
            d.locked = plan.is_some();
            d.matched = matched;
            match plan {
                None => {
                    let speed = d.own_speed();
                    d.set_speed(speed);
                }
                Some(sync::Plan::Speed(speed)) => d.set_speed(speed),
                Some(sync::Plan::Jump { to, speed }) => {
                    let to = to.clamp(0.0, d.frames() as f64);
                    d.jump(to);
                    d.set_speed(speed);
                }
            }
        }
    }

    /// Renders the master as interleaved stereo into `out`, calling
    /// `retire` with every source a deck lets go of.
    pub fn render_with(&mut self, out: &mut [f32], retire: impl FnMut(Arc<DeckSource>)) {
        self.render_outputs(out, None, retire);
    }

    /// Tells the cue bus how the device carries it: split mono forces split
    /// cue (cue left, master right) on the headphone output.
    pub fn set_cue_routing(&mut self, routing: crate::CueRouting) {
        self.mixer.set_cue_routing(routing);
    }

    /// Renders the master into `master` and, when given, the headphone cue
    /// into `phones` (the same length), both interleaved stereo.
    pub fn render_outputs(
        &mut self,
        master: &mut [f32],
        mut phones: Option<&mut [f32]>,
        mut retire: impl FnMut(Arc<DeckSource>),
    ) {
        for (i, block) in master.chunks_mut(BLOCK * 2).enumerate() {
            self.plan();
            let n = block.len();
            for (deck, input) in self.decks.iter_mut().zip(&mut self.inputs) {
                for frame in input[..n].as_chunks_mut::<2>().0 {
                    let mut retired = None;
                    let (l, r) = deck.next(&mut retired);
                    if let Some(old) = retired {
                        retire(old);
                    }
                    *frame = [l, r];
                }
            }
            for (ch, deck) in self.decks.iter().enumerate() {
                self.mixer.set_beat_frames(ch, deck.beat_frames());
            }
            let at = i * BLOCK * 2;
            let phones = phones.as_deref_mut().map(|p| &mut p[at..at + n]);
            self.mixer.process(&mut self.inputs, block, phones);
        }
    }
}

fn has_grid(d: &Deck) -> bool {
    d.src().is_some_and(|s| !s.beats().is_empty())
}

/// Renders interleaved stereo into `out` with no device. Retired sources
/// are dropped here, so use [`Graph::render_with`] on the audio thread.
pub fn render(graph: &mut Graph, out: &mut [f32]) {
    graph.render_with(out, drop);
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const RAMP: usize = (RATE * RAMP_MS / 1000) as usize;

    /// A source whose left sample is its frame index (mod 16,384) and right
    /// its negation.
    fn ramp_source(frames: i64) -> Arc<DeckSource> {
        let samples = (0..frames)
            .flat_map(|i| {
                let v = (i % 16_384) as i16;
                [v, -v]
            })
            .collect();
        Arc::new(DeckSource::new(samples, RATE))
    }

    fn render_frames(g: &mut Graph, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        render(g, &mut out);
        out
    }

    fn sample(v: f32) -> i32 {
        (v * 32_768.0).round() as i32
    }

    #[test]
    fn plays_the_source_sample_for_sample_after_the_fade_in() {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(10_000),
        });
        g.apply(Control::Play { deck: 0 });
        let lat = g.latency();
        let out = render_frames(&mut g, 1_000 + lat);
        // The limiter's look-ahead delays everything by `lat` frames.
        assert!(out[..2 * lat].iter().all(|&s| s == 0.0));
        let out = &out[2 * lat..];
        // Fade-in: rising, never above the source.
        assert!(sample(out[2 * 10]) <= 10);
        for f in RAMP..1_000 {
            assert_eq!(sample(out[2 * f]), f as i32, "frame {f}");
            assert_eq!(sample(out[2 * f + 1]), -(f as i32), "frame {f}");
        }
        assert_eq!(g.state().decks[0].position, (1_000 + lat) as i64);
    }

    #[test]
    fn silent_until_play_and_stops_at_the_end() {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(500),
        });
        assert!(render_frames(&mut g, 100).iter().all(|&s| s == 0.0));
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 1_000);
        let st = g.state().decks[0];
        assert!(!st.playing);
        assert_eq!(st.position, 500);
        assert_eq!(st.frames, 500);
        // Play at the end does nothing until a seek.
        g.apply(Control::Play { deck: 0 });
        assert!(!g.state().decks[0].playing);
    }

    #[test]
    fn pause_fades_out_then_stops() {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(48_000),
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 1_000);
        g.apply(Control::Pause { deck: 0 });
        assert!(g.state().decks[0].playing, "still fading");
        assert_eq!(g.state().decks[0].transports, 2, "play and pause counted");
        let lat = g.latency();
        let out = render_frames(&mut g, RAMP + 10 + lat);
        assert!(!g.state().decks[0].playing);
        let pos = g.state().decks[0].position;
        // One extra frame is float rounding in the ramp.
        assert!(
            (1_000 + RAMP as i64..=1_001 + RAMP as i64).contains(&pos),
            "{pos}"
        );
        // Gain falls through the fade and ends silent.
        let out = &out[2 * lat..];
        assert!(out[2].abs() > out[2 * (RAMP - 10)].abs());
        assert!(out[2 * (RAMP + 5)..].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn play_during_a_fade_out_cancels_the_pause() {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(48_000),
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 1_000);
        g.apply(Control::Pause { deck: 0 });
        render_frames(&mut g, 10);
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, RAMP * 3);
        assert!(g.state().decks[0].playing);
    }

    #[test]
    fn seek_while_playing_crossfades_from_the_old_position() {
        const XFADE: usize = (RATE * SEEK_XFADE_MS / 1000) as usize;
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(20_000),
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 1_000);
        g.apply(Control::Seek {
            deck: 0,
            frame: 12_000,
        });
        // The position moves at once.
        assert_eq!(g.state().decks[0].position, 12_000);
        assert_eq!(g.state().decks[0].seeks, 1);
        let lat = g.latency();
        let out = render_frames(&mut g, XFADE + 3 + lat);
        let out = &out[2 * lat..];
        // First frame is still the old head at full level: no gap, no step.
        assert_eq!(sample(out[0]), 1_000);
        // Halfway, both heads at equal power (cos 45° each).
        let mid = XFADE / 2;
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let want = (1_000 + mid) as f32 * half + (12_000 + mid) as f32 * half;
        assert!((sample(out[2 * mid]) as f32 - want).abs() < 2.0);
        // After the crossfade, only the new head, sample for sample.
        assert_eq!(sample(out[2 * XFADE]), (12_000 + XFADE) as i32);
        assert_eq!(sample(out[2 * XFADE + 1]), -((12_000 + XFADE) as i32));
        assert!(g.state().decks[0].playing);
    }

    #[test]
    fn seek_while_paused_has_no_crossfade() {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(20_000),
        });
        g.apply(Control::Seek {
            deck: 0,
            frame: 5_000,
        });
        g.apply(Control::Play { deck: 0 });
        let lat = g.latency();
        let out = render_frames(&mut g, RAMP + 2 + lat);
        let out = &out[2 * lat..];
        // Only the fade-in of the new position, nothing from frame 0.
        assert_eq!(sample(out[2 * (RAMP + 1)]), 5_000 + RAMP as i32 + 1);
    }

    #[test]
    fn seek_during_a_pause_fade_jumps_and_still_pauses() {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(48_000),
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 1_000);
        g.apply(Control::Pause { deck: 0 });
        g.apply(Control::Seek {
            deck: 0,
            frame: 30_000,
        });
        render_frames(&mut g, RAMP + 10);
        let st = g.state().decks[0];
        assert!(!st.playing);
        assert!((30_000..=30_000 + RAMP as i64 + 1).contains(&st.position));
    }

    #[test]
    fn seek_is_ignored_while_a_load_is_pending() {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(48_000),
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 1_000);
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(100),
        });
        g.apply(Control::Seek {
            deck: 0,
            frame: 30_000,
        });
        render_frames(&mut g, RAMP + 10);
        let st = g.state().decks[0];
        assert_eq!((st.position, st.frames, st.seeks), (0, 100, 1));
    }

    #[test]
    fn seek_while_paused_is_immediate_and_clamped() {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(1_000),
        });
        g.apply(Control::Seek {
            deck: 0,
            frame: 400,
        });
        assert_eq!(g.state().decks[0].position, 400);
        g.apply(Control::Seek {
            deck: 0,
            frame: 5_000,
        });
        assert_eq!(g.state().decks[0].position, 1_000);
        g.apply(Control::Seek { deck: 0, frame: -3 });
        assert_eq!(g.state().decks[0].position, 0);
    }

    #[test]
    fn load_while_playing_fades_then_hands_back_the_old_source() {
        let mut g = Graph::new(RATE).unwrap();
        let first = ramp_source(48_000);
        g.apply(Control::Load {
            deck: 0,
            src: Arc::clone(&first),
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 1_000);
        assert!(g
            .apply(Control::Load {
                deck: 0,
                src: ramp_source(100)
            })
            .is_none());
        let mut retired = Vec::new();
        let mut out = vec![0.0; (RAMP + 10) * 2];
        g.render_with(&mut out, |s| retired.push(s));
        assert_eq!(retired.len(), 1);
        assert!(Arc::ptr_eq(&retired[0], &first));
        let st = g.state().decks[0];
        assert_eq!(
            (st.loaded, st.playing, st.position, st.frames),
            (true, false, 0, 100)
        );
    }

    #[test]
    fn play_after_a_load_while_playing_starts_the_new_track() {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(48_000),
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 1_000);
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(48_000),
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, RAMP * 3);
        let st = g.state().decks[0];
        assert!(st.playing);
        assert!(
            st.position > 0 && st.position < (RAMP * 3) as i64,
            "{}",
            st.position
        );
    }

    #[test]
    fn pause_after_a_queued_play_keeps_the_new_track_stopped() {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(48_000),
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 1_000);
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(48_000),
        });
        g.apply(Control::Play { deck: 0 });
        g.apply(Control::Pause { deck: 0 });
        render_frames(&mut g, RAMP * 3);
        let st = g.state().decks[0];
        assert_eq!((st.playing, st.position), (false, 0));
    }

    #[test]
    fn load_while_stopped_is_immediate() {
        let mut g = Graph::new(RATE).unwrap();
        let first = ramp_source(10);
        g.apply(Control::Load {
            deck: 0,
            src: Arc::clone(&first),
        });
        let old = g.apply(Control::Unload { deck: 0 }).unwrap();
        assert!(Arc::ptr_eq(&old, &first));
        assert!(!g.state().decks[0].loaded);
    }

    #[test]
    fn volume_ramps_to_the_taper() {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: Arc::new(DeckSource::new(vec![16_384; 96_000], RATE)),
        });
        g.apply(Control::Play { deck: 0 });
        g.apply(Control::Volume { value: 0.5 });
        let out = render_frames(&mut g, RAMP * 4);
        let last = out[out.len() - 1];
        assert!((last - 0.5 * 0.25).abs() < 1e-6, "{last}");
        assert_eq!(g.state().volume, 0.5);
    }

    #[test]
    fn taper_and_bad_input() {
        assert_eq!(volume_gain(0.0), 0.0);
        assert_eq!(volume_gain(1.0), 1.0);
        assert_eq!(volume_gain(0.5), 0.25);
        assert_eq!(volume_gain(2.0), 1.0);
        assert_eq!(volume_gain(f32::NAN), 0.0);
    }

    #[test]
    fn decks_play_independently_and_are_summed() {
        let mut g = Graph::new(RATE).unwrap();
        for (deck, value) in [(0, 1_000), (3, 200)] {
            g.apply(Control::Load {
                deck,
                src: Arc::new(DeckSource::new(vec![value; 20_000], RATE)),
            });
            g.apply(Control::Play { deck });
        }
        let out = render_frames(&mut g, 1_000);
        assert_eq!(sample(out[2 * 999]), 1_200);
        g.apply(Control::Pause { deck: 0 });
        let out = render_frames(&mut g, 1_000);
        assert_eq!(sample(out[2 * 999]), 200);
        let st = g.state();
        assert!(!st.decks[0].playing && st.decks[3].playing);
        assert!(!st.decks[1].loaded && !st.decks[2].loaded);
        assert_eq!(st.decks[3].position, 2_000);
    }

    #[test]
    fn deck_settings_are_kept_and_reported() {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Tempo {
            deck: 1,
            ratio: 1.08,
        });
        g.apply(Control::Tempo {
            deck: 2,
            ratio: 9.0,
        });
        g.apply(Control::Nudge {
            deck: 1,
            ratio: f64::NAN,
        });
        g.apply(Control::Sync {
            deck: 1,
            on: true,
            bar: true,
        });
        g.apply(Control::Quantise { deck: 1, on: true });
        let st = g.state();
        let d = st.decks[1];
        assert_eq!((d.tempo, d.nudge), (1.08, 1.0));
        assert!(d.sync && d.sync_bar && d.quantise);
        assert_eq!(st.decks[2].tempo, TEMPO_MAX);
        // Only a deck with a grid can take master.
        g.apply(Control::TakeMaster { deck: 2 });
        g.apply(Control::TakeMaster { deck: 7 });
        assert_eq!(g.state().master_deck, None);
    }

    /// 120 BPM at 48 kHz (24,000 frames a beat) from frame 0, with hot cue
    /// 1 at beat 10.
    fn gridded(frames: i64) -> Arc<DeckSource> {
        let samples = (0..frames)
            .flat_map(|i| {
                let v = (i % 16_384) as i16;
                [v, -v]
            })
            .collect();
        let grid = [crate::GridAnchor {
            frame: 0,
            bpm: 120.0,
            beat_in_bar: 1,
            beats_per_bar: 4,
        }];
        Arc::new(
            DeckSource::new(samples, RATE)
                .with_grid(&grid)
                .with_hot_cues(vec![(1, 240_000)]),
        )
    }

    fn load(g: &mut Graph, src: Arc<DeckSource>) {
        g.apply(Control::Load { deck: 0, src });
    }

    #[test]
    fn cue_returns_to_the_cue_point_and_pauses() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, ramp_source(48_000));
        g.apply(Control::Seek {
            deck: 0,
            frame: 5_000,
        });
        // Paused: CUE sets the cue point here.
        g.apply(Control::Cue { deck: 0 });
        assert_eq!(g.state().decks[0].cue, 5_000);
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 2_000);
        // Playing: CUE fades out, then jumps back and stops.
        g.apply(Control::Cue { deck: 0 });
        assert!(g.state().decks[0].playing, "still fading");
        let lat = g.latency();
        let out = render_frames(&mut g, RAMP + 10 + lat);
        let st = g.state().decks[0];
        assert!(!st.playing);
        assert_eq!((st.position, st.cue), (5_000, 5_000));
        assert!(out[2 * (lat + RAMP + 5)..].iter().all(|&s| s == 0.0));
        // A play during the fade cancels the cue.
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 100);
        g.apply(Control::Cue { deck: 0 });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, RAMP * 3);
        assert!(g.state().decks[0].playing);
        assert!(g.state().decks[0].position > 5_100);
    }

    #[test]
    fn holding_cue_on_the_cue_point_previews() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, ramp_source(48_000));
        g.apply(Control::Seek {
            deck: 0,
            frame: 5_000,
        });
        g.apply(Control::Cue { deck: 0 });
        assert!(!g.state().decks[0].playing, "the first press sets the cue");
        // Pressed again on the cue point: plays while held.
        g.apply(Control::Cue { deck: 0 });
        let st = g.state().decks[0];
        assert!(st.playing && st.cue_preview);
        render_frames(&mut g, 2_000);
        g.apply(Control::CueRelease { deck: 0 });
        render_frames(&mut g, RAMP + 10);
        let st = g.state().decks[0];
        assert!(!st.playing && !st.cue_preview);
        assert_eq!((st.position, st.cue), (5_000, 5_000));
        // PLAY during the preview: letting go keeps it playing.
        g.apply(Control::Cue { deck: 0 });
        render_frames(&mut g, 500);
        g.apply(Control::Play { deck: 0 });
        g.apply(Control::CueRelease { deck: 0 });
        render_frames(&mut g, RAMP * 3);
        let st = g.state().decks[0];
        assert!(st.playing && !st.cue_preview);
        assert!(st.position > 5_500);
        // A release with no preview does nothing.
        g.apply(Control::CueRelease { deck: 0 });
        render_frames(&mut g, RAMP * 3);
        assert!(g.state().decks[0].playing);
    }

    #[test]
    fn wide_tempo_doubles_and_stands_still_at_the_ends() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, ramp_source(48_000));
        g.apply(Control::Tempo {
            deck: 0,
            ratio: 2.0,
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 1_000);
        let st = g.state().decks[0];
        assert_eq!(st.speed, 2.0);
        assert!((st.position - 2_000).abs() <= 1, "{}", st.position);
        // −100%: fades out, then holds its place in silence.
        g.apply(Control::Tempo {
            deck: 0,
            ratio: 0.0,
        });
        let lat = g.latency();
        render_frames(&mut g, 2 * RAMP + lat);
        let held = g.state().decks[0];
        assert!(held.playing);
        assert_eq!(held.speed, 0.0);
        let out = render_frames(&mut g, 1_000);
        assert!(out.iter().all(|&s| s == 0.0));
        assert_eq!(g.state().decks[0].position, held.position);
        // Back up to speed: moving again.
        g.apply(Control::Tempo {
            deck: 0,
            ratio: 0.5,
        });
        render_frames(&mut g, 1_000);
        assert!(g.state().decks[0].position > held.position + 400);
    }

    #[test]
    fn quantised_cue_and_seek_snap_to_the_beat() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        g.apply(Control::Seek {
            deck: 0,
            frame: 13_000,
        });
        g.apply(Control::Quantise { deck: 0, on: true });
        g.apply(Control::Cue { deck: 0 });
        assert_eq!(g.state().decks[0].cue, 24_000);
        assert_eq!(
            g.state().decks[0].position,
            24_000,
            "the head moves to the cue point, as on a CDJ"
        );
        g.apply(Control::Seek {
            deck: 0,
            frame: 59_000,
        });
        assert_eq!(g.state().decks[0].position, 48_000);
    }

    #[test]
    fn hot_cue_jumps_and_plays() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        g.apply(Control::HotCue { deck: 0, slot: 2 });
        assert!(!g.state().decks[0].playing, "empty slot");
        g.apply(Control::Quantise { deck: 0, on: true });
        g.apply(Control::HotCue { deck: 0, slot: 1 });
        let st = g.state().decks[0];
        assert!(st.playing);
        assert_eq!(
            st.position, 240_000,
            "exact when paused, even with quantise"
        );
        assert_eq!(st.beat, Some(10.0));
    }

    #[test]
    fn hot_cues_set_and_cleared_on_the_deck_jump_or_stay_put() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        g.apply(Control::SetHotCue {
            deck: 0,
            slot: 2,
            frame: Some(96_000),
        });
        g.apply(Control::HotCue { deck: 0, slot: 2 });
        let st = g.state().decks[0];
        assert!(st.playing);
        assert_eq!(st.position, 96_000);
        g.apply(Control::Pause { deck: 0 });
        g.apply(Control::SetHotCue {
            deck: 0,
            slot: 1,
            frame: None,
        });
        g.apply(Control::HotCue { deck: 0, slot: 1 });
        assert_eq!(
            g.state().decks[0].position,
            96_000,
            "deleted: nothing happens"
        );
        g.apply(Control::SetHotCue {
            deck: 0,
            slot: 9,
            frame: Some(1),
        });
        // A new load brings the source's hot cues back.
        load(&mut g, gridded(480_000));
        g.apply(Control::HotCue { deck: 0, slot: 1 });
        assert_eq!(g.state().decks[0].position, 240_000);
        g.apply(Control::Pause { deck: 0 });
        g.apply(Control::HotCue { deck: 0, slot: 2 });
        assert_eq!(
            g.state().decks[0].position,
            240_000,
            "edit gone with the old load"
        );
    }

    #[test]
    fn calling_a_memory_cue_moves_the_cue_point_and_plays_on() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        g.apply(Control::Quantise { deck: 0, on: true });
        g.apply(Control::CallCue {
            deck: 0,
            frame: 100_001,
            end: None,
        });
        let st = g.state().decks[0];
        assert_eq!(
            (st.position, st.cue, st.playing),
            (100_001, 100_001, false),
            "exact"
        );
        g.apply(Control::Play { deck: 0 });
        g.apply(Control::CallCue {
            deck: 0,
            frame: 50_000,
            end: None,
        });
        let st = g.state().decks[0];
        assert_eq!((st.position, st.cue, st.playing), (50_000, 50_000, true));
    }

    #[test]
    fn quantised_jumps_while_playing_keep_the_beat_phase() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 6_000);
        g.apply(Control::Quantise { deck: 0, on: true });
        g.apply(Control::HotCue { deck: 0, slot: 1 });
        assert_eq!(g.state().decks[0].position, 246_000);
        g.apply(Control::Seek {
            deck: 0,
            frame: 100_000,
        });
        // A quarter beat in, near beat 4: beat 4.25.
        assert_eq!(g.state().decks[0].position, 102_000);
    }

    #[test]
    fn tempo_plays_faster_and_returns_to_exact_copying() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, ramp_source(48_000));
        g.apply(Control::Tempo {
            deck: 0,
            ratio: 1.25,
        });
        g.apply(Control::Play { deck: 0 });
        let out = render_frames(&mut g, 4_000);
        let st = g.state().decks[0];
        assert_eq!(st.position, 5_000);
        assert_eq!(st.speed, 1.25);
        // The ramp source rises by 1.25 a frame at this speed.
        let rise = sample(out[2 * 3_001]) - sample(out[2 * 3_000]);
        assert!((1..=2).contains(&rise), "{rise}");
        g.apply(Control::Tempo {
            deck: 0,
            ratio: 1.0,
        });
        render_frames(&mut g, 2_000);
        let at = g.state().decks[0].position;
        let lat = g.latency();
        let out = render_frames(&mut g, 100 + lat);
        for (f, pair) in out[2 * lat..].as_chunks::<2>().0.iter().enumerate() {
            assert_eq!(sample(pair[0]), (at + f as i64) as i32);
        }
    }

    #[test]
    fn nudge_bends_on_top_of_the_tempo() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        g.apply(Control::Tempo {
            deck: 0,
            ratio: 1.05,
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 64);
        let st = g.state().decks[0];
        assert!((st.bpm.unwrap() - 126.0).abs() < 1e-9);
        g.apply(Control::Nudge {
            deck: 0,
            ratio: 1.04,
        });
        let before = g.state().decks[0].position;
        render_frames(&mut g, 3_200);
        let st = g.state().decks[0];
        assert!((st.speed - 1.05 * 1.04).abs() < 1e-12);
        let moved = (st.position - before) as f64;
        assert!((moved - 3_200.0 * 1.092).abs() <= 1.0, "{moved}");
        g.apply(Control::Nudge {
            deck: 0,
            ratio: 1.0,
        });
        render_frames(&mut g, 64);
        assert_eq!(g.state().decks[0].speed, 1.05);
    }

    #[test]
    fn source_at_another_rate_plays_at_its_own_speed() {
        let mut g = Graph::new(RATE).unwrap();
        let src = Arc::new(DeckSource::new(vec![1_000; 2 * 44_100], 44_100));
        load(&mut g, src);
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 4_800);
        // 100 ms of output is 4,410 source frames.
        let st = g.state().decks[0];
        assert!((st.position - 4_410).abs() <= 1, "{}", st.position);
        assert!((st.speed - 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_master_is_the_first_deck_to_play_until_another_takes_it() {
        let mut g = Graph::new(RATE).unwrap();
        for deck in 0..3 {
            g.apply(Control::Load {
                deck,
                src: gridded(480_000),
            });
        }
        g.apply(Control::Load {
            deck: 3,
            src: ramp_source(480_000),
        });
        assert_eq!(g.state().master_deck, None);
        // Deck 3 has no grid, so it is never master.
        for deck in [3, 2, 1] {
            g.apply(Control::Play { deck });
        }
        render_frames(&mut g, 64);
        let st = g.state();
        assert_eq!(st.master_deck, Some(2));
        assert!(st.decks[2].master && !st.decks[1].master);
        // A paused deck can take master and keeps it while paused.
        g.apply(Control::TakeMaster { deck: 0 });
        render_frames(&mut g, 64);
        assert_eq!(g.state().master_deck, Some(0));
        // Loading a new track on the master hands it on.
        g.apply(Control::Load {
            deck: 0,
            src: gridded(480_000),
        });
        render_frames(&mut g, 64);
        assert_eq!(g.state().master_deck, Some(2));
    }

    /// Deck 0 master at +6% tempo, playing; deck 1 synced to it, playing
    /// (or paused) at its own fader 1.0.
    fn master_and_follower(follower_plays: bool) -> Graph {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: gridded(480_000),
        });
        g.apply(Control::Load {
            deck: 1,
            src: gridded(960_000),
        });
        g.apply(Control::Tempo {
            deck: 0,
            ratio: 1.06,
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 64);
        g.apply(Control::Sync {
            deck: 1,
            on: true,
            bar: false,
        });
        if follower_plays {
            g.apply(Control::Play { deck: 1 });
        }
        render_frames(&mut g, 48_000);
        let st = g.state();
        assert_eq!(st.master_deck, Some(0));
        assert!(st.decks[1].locked);
        g
    }

    #[test]
    fn the_master_ending_hands_over_to_the_synced_deck_at_the_same_tempo() {
        let mut g = master_and_follower(true);
        // Deck 0's 10 s run out; deck 1 plays on.
        render_frames(&mut g, 480_000);
        let st = g.state();
        assert!(!st.decks[0].playing && st.decks[1].playing);
        assert_eq!(st.master_deck, Some(1));
        assert!(st.decks[1].master && !st.decks[1].locked);
        assert!(
            (st.decks[1].speed - 1.06).abs() < 1e-9,
            "{}",
            st.decks[1].speed
        );
        assert!((st.decks[1].tempo - 1.06).abs() < 1e-9);
        // And it stays there.
        render_frames(&mut g, 48_000);
        assert!((g.state().decks[1].speed - 1.06).abs() < 1e-9);
    }

    #[test]
    fn the_master_pausing_hands_over_to_the_synced_deck_at_the_same_tempo() {
        let mut g = master_and_follower(true);
        g.apply(Control::Pause { deck: 0 });
        render_frames(&mut g, RAMP * 2);
        let st = g.state();
        assert_eq!(st.master_deck, Some(1));
        assert!((st.decks[1].speed - 1.06).abs() < 1e-9);
        // Deck 0 plays again: deck 1 stays master; deck 0 (not synced)
        // plays at its own tempo.
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 64);
        assert_eq!(g.state().master_deck, Some(1));
    }

    #[test]
    fn with_no_synced_deck_playing_nobody_is_master() {
        let mut g = master_and_follower(false);
        g.apply(Control::Pause { deck: 0 });
        render_frames(&mut g, RAMP * 2);
        let st = g.state();
        assert_eq!(st.master_deck, None);
        assert!(st.decks.iter().all(|d| !d.master));
        // The paused follower keeps the tempo it had matched.
        assert!((st.decks[1].speed - 1.06).abs() < 1e-9);
        assert!((st.decks[1].tempo - 1.06).abs() < 1e-9);
        // Unloading a master with no follower playing: nobody either.
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 64);
        assert_eq!(g.state().master_deck, Some(0));
        g.apply(Control::Unload { deck: 0 });
        render_frames(&mut g, RAMP * 2);
        assert_eq!(g.state().master_deck, None);
    }

    #[test]
    fn taking_master_from_a_follower_keeps_its_tempo() {
        let mut g = master_and_follower(true);
        g.apply(Control::TakeMaster { deck: 1 });
        render_frames(&mut g, 64);
        let st = g.state();
        assert_eq!(st.master_deck, Some(1));
        assert!((st.decks[1].speed - 1.06).abs() < 1e-9);
        assert!((st.decks[0].speed - 1.06).abs() < 1e-9);
    }

    #[test]
    fn sync_follows_the_leaders_tempo_and_needs_grids() {
        let mut g = Graph::new(RATE).unwrap();
        g.apply(Control::Load {
            deck: 0,
            src: gridded(480_000),
        });
        g.apply(Control::Tempo {
            deck: 0,
            ratio: 1.06,
        });
        g.apply(Control::Play { deck: 0 });
        g.apply(Control::Load {
            deck: 1,
            src: gridded(480_000),
        });
        g.apply(Control::Sync {
            deck: 1,
            on: true,
            bar: false,
        });
        render_frames(&mut g, 64);
        // Paused follower: tempo matched, phase left alone.
        let d = g.state().decks[1];
        assert!(d.locked);
        assert!((d.speed - 1.06).abs() < 1e-12);
        assert_eq!(d.position, 0);
        // The leader cannot follow; a deck without a grid cannot either.
        g.apply(Control::Sync {
            deck: 0,
            on: true,
            bar: false,
        });
        g.apply(Control::Load {
            deck: 2,
            src: ramp_source(480_000),
        });
        g.apply(Control::Sync {
            deck: 2,
            on: true,
            bar: false,
        });
        render_frames(&mut g, 64);
        let st = g.state();
        assert!(!st.decks[0].locked && !st.decks[2].locked);
        assert_eq!(st.decks[2].speed, 1.0);
        // Sync off: back to the deck's own tempo.
        g.apply(Control::Sync {
            deck: 1,
            on: false,
            bar: false,
        });
        render_frames(&mut g, 64);
        assert_eq!(g.state().decks[1].speed, 1.0);
    }

    fn beat_jump(g: &mut Graph, beats: f64) -> i64 {
        g.apply(Control::BeatJump { deck: 0, beats });
        g.state().decks[0].position
    }

    #[test]
    fn beat_jump_moves_by_beats_and_keeps_the_place_in_the_beat() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        g.apply(Control::Seek {
            deck: 0,
            frame: 30_000,
        });
        assert_eq!(beat_jump(&mut g, 4.0), 126_000);
        assert_eq!(beat_jump(&mut g, -0.5), 114_000);
        g.apply(Control::Quantise { deck: 0, on: true });
        assert_eq!(beat_jump(&mut g, 0.125), 117_000);
        // Out of the track: ignored.
        assert_eq!(beat_jump(&mut g, -32.0), 117_000);
        assert_eq!(beat_jump(&mut g, 32.0), 117_000);
        // FINE: 10 ms.
        g.apply(Control::FineJump {
            deck: 0,
            forward: true,
        });
        assert_eq!(g.state().decks[0].position, 117_480);
        g.apply(Control::FineJump {
            deck: 0,
            forward: false,
        });
        assert_eq!(g.state().decks[0].position, 117_000);
        // No grid: beat jumps are ignored, FINE still works.
        load(&mut g, ramp_source(48_000));
        assert_eq!(beat_jump(&mut g, 1.0), 0);
        g.apply(Control::FineJump {
            deck: 0,
            forward: true,
        });
        assert_eq!(g.state().decks[0].position, 480);
    }

    #[test]
    fn quantised_beat_jumps_follow_a_dynamic_grid() {
        // 4 beats at 120 BPM, then 100 BPM (28,800 frames a beat).
        let grid = [
            crate::GridAnchor {
                frame: 0,
                bpm: 120.0,
                beat_in_bar: 1,
                beats_per_bar: 4,
            },
            crate::GridAnchor {
                frame: 96_000,
                bpm: 100.0,
                beat_in_bar: 1,
                beats_per_bar: 4,
            },
        ];
        let src = Arc::new(DeckSource::new(vec![0; 960_000], RATE).with_grid(&grid));
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, src);
        g.apply(Control::Seek {
            deck: 0,
            frame: 84_000, // beat 3.5
        });
        // Unquantised: one beat of the local length.
        assert_eq!(beat_jump(&mut g, 1.0), 108_000);
        assert_eq!(beat_jump(&mut g, -1.0), 79_200);
        g.apply(Control::Seek {
            deck: 0,
            frame: 84_000,
        });
        g.apply(Control::Quantise { deck: 0, on: true });
        // Quantised: beat 4.5, half a beat into the 100 BPM part.
        assert_eq!(beat_jump(&mut g, 1.0), 110_400);
    }

    fn region(g: &mut Graph) -> Option<(i64, i64, Option<f64>)> {
        g.state().decks[0]
            .loop_region
            .map(|l| (l.start, l.end, l.beats))
    }

    /// `gridded`, with an active memory loop over beats 2–4.
    fn with_auto_loop(frames: i64) -> Arc<DeckSource> {
        let grid = [crate::GridAnchor {
            frame: 0,
            bpm: 120.0,
            beat_in_bar: 1,
            beats_per_bar: 4,
        }];
        let samples = (0..frames).flat_map(|i| [(i % 16_384) as i16; 2]).collect();
        Arc::new(
            DeckSource::new(samples, RATE)
                .with_grid(&grid)
                .with_auto_loops(vec![(96_000, 48_000), (48_000, 96_000)]),
        )
    }

    #[test]
    fn playback_enters_an_active_memory_loop_at_its_start() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, with_auto_loop(480_000));
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 40_000);
        assert_eq!(region(&mut g), None, "not there yet");
        render_frames(&mut g, 90_000);
        assert_eq!(
            region(&mut g),
            Some((48_000, 96_000, Some(2.0))),
            "the reversed one is dropped"
        );
        // Exact, wrap after wrap, as a beat loop.
        assert_eq!(
            g.state().decks[0].position,
            48_000 + (130_000 - 48_000) % 48_000
        );

        // Left, it plays on and is not entered again from inside.
        g.apply(Control::LoopExit { deck: 0 });
        render_frames(&mut g, 60_000);
        assert_eq!(region(&mut g), None);
        assert!(g.state().decks[0].position > 96_000);
        // Back before the start: entered again on the way through.
        g.apply(Control::Seek {
            deck: 0,
            frame: 40_000,
        });
        render_frames(&mut g, 10_000);
        assert_eq!(region(&mut g), Some((48_000, 96_000, Some(2.0))));
    }

    #[test]
    fn a_head_parked_on_the_start_enters_and_edits_take_effect() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, with_auto_loop(480_000));
        g.apply(Control::CallCue {
            deck: 0,
            frame: 48_000,
            end: None,
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 64);
        assert_eq!(region(&mut g), Some((48_000, 96_000, Some(2.0))));

        // Cleared: passing its start does nothing; set elsewhere: entered there.
        g.apply(Control::LoopExit { deck: 0 });
        g.apply(Control::AutoLoop {
            deck: 0,
            index: 0,
            region: None,
        });
        g.apply(Control::AutoLoop {
            deck: 0,
            index: 3,
            region: Some((144_000, 156_000)),
        });
        g.apply(Control::Seek {
            deck: 0,
            frame: 40_000,
        });
        render_frames(&mut g, 20_000);
        assert_eq!(region(&mut g), None);
        render_frames(&mut g, 90_000);
        assert_eq!(region(&mut g), Some((144_000, 156_000, Some(0.5))));
    }

    #[test]
    fn beat_loop_repeats_from_the_quantised_beat() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 30_000); // beat 1.25
        g.apply(Control::Quantise { deck: 0, on: true });
        g.apply(Control::BeatLoop {
            deck: 0,
            beats: 1.0,
        });
        assert_eq!(region(&mut g), Some((24_000, 48_000, Some(1.0))));
        // Exact, wrap after wrap, at unit speed.
        render_frames(&mut g, 100_000);
        let pos = g.state().decks[0].position;
        assert_eq!(pos, 24_000 + (130_000 - 24_000) % 24_000);
        // Halve and double keep the start; past the new end it wraps.
        g.apply(Control::LoopResize {
            deck: 0,
            double: false,
        });
        assert_eq!(region(&mut g), Some((24_000, 36_000, Some(0.5))));
        render_frames(&mut g, 64);
        let pos = g.state().decks[0].position;
        assert!((24_000..36_000).contains(&pos), "{pos}");
        g.apply(Control::LoopResize {
            deck: 0,
            double: true,
        });
        assert_eq!(region(&mut g), Some((24_000, 48_000, Some(1.0))));
        // A 1/8 loop starts on the last 1/8 step.
        g.apply(Control::BeatLoop {
            deck: 0,
            beats: 0.125,
        });
        let (start, end, _) = region(&mut g).unwrap();
        assert_eq!(end - start, 3_000);
        assert_eq!(start % 3_000, 0);
        // A jump moves the loop with the head.
        g.apply(Control::BeatJump {
            deck: 0,
            beats: 4.0,
        });
        assert_eq!(
            region(&mut g),
            Some((start + 96_000, end + 96_000, Some(0.125)))
        );
        // Exit: plays on past the end.
        g.apply(Control::LoopExit { deck: 0 });
        render_frames(&mut g, 10_000);
        assert!(g.state().decks[0].position > end + 96_000);
        assert_eq!(region(&mut g), None);
    }

    #[test]
    fn unquantised_loops_start_at_the_head_and_seeks_out_of_them_end_them() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        g.apply(Control::Seek {
            deck: 0,
            frame: 30_000,
        });
        g.apply(Control::BeatLoop {
            deck: 0,
            beats: 2.0,
        });
        assert_eq!(region(&mut g), Some((30_000, 78_000, Some(2.0))));
        // A seek inside keeps it; one outside ends it.
        g.apply(Control::Seek {
            deck: 0,
            frame: 40_000,
        });
        assert!(region(&mut g).is_some());
        g.apply(Control::Seek {
            deck: 0,
            frame: 200_000,
        });
        assert_eq!(region(&mut g), None);
        // Past the end of the track: no loop. Without a grid: none either.
        g.apply(Control::Seek {
            deck: 0,
            frame: 470_000,
        });
        g.apply(Control::BeatLoop {
            deck: 0,
            beats: 1.0,
        });
        assert_eq!(region(&mut g), None);
        load(&mut g, ramp_source(48_000));
        g.apply(Control::BeatLoop {
            deck: 0,
            beats: 1.0,
        });
        assert_eq!(region(&mut g), None);
    }

    #[test]
    fn the_loop_point_crossfades_without_a_click() {
        // 100 Hz at half scale: a 1/8-beat loop (3,000 frames) ends 6.25
        // cycles in, a step of the full swing without the crossfade.
        let samples = (0..480_000)
            .flat_map(|i| {
                let v = (f64::from(i) * 2.0 * std::f64::consts::PI / 480.0).sin() * 16_000.0;
                [v as i16, v as i16]
            })
            .collect();
        let grid = [crate::GridAnchor {
            frame: 0,
            bpm: 120.0,
            beat_in_bar: 1,
            beats_per_bar: 4,
        }];
        let src = Arc::new(DeckSource::new(samples, RATE).with_grid(&grid));
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, src);
        g.apply(Control::Quantise { deck: 0, on: true });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 24_000);
        g.apply(Control::BeatLoop {
            deck: 0,
            beats: 0.125,
        });
        let out = render_frames(&mut g, 24_000); // 8 wraps
        let worst = out
            .as_chunks::<2>()
            .0
            .windows(2)
            .map(|w| (sample(w[1][0]) - sample(w[0][0])).abs())
            .max()
            .unwrap();
        // The sine itself moves up to 210 a frame; a bare jump would be ~30,000.
        assert!(worst < 600, "worst step {worst}");
        let pos = g.state().decks[0].position;
        assert!((24_000..27_000).contains(&pos), "{pos}");
    }

    #[test]
    fn a_synced_loop_stays_in_phase_with_the_master() {
        let mut g = master_and_follower(true);
        g.apply(Control::Quantise { deck: 1, on: true });
        g.apply(Control::BeatLoop {
            deck: 1,
            beats: 1.0,
        });
        // 6 s: about 13 wraps while resampling at 1.06.
        render_frames(&mut g, 288_000);
        let st = g.state();
        assert!(st.decks[1].loop_region.is_some());
        let (a, b) = (st.decks[0].beat.unwrap(), st.decks[1].beat.unwrap());
        let off = crate::beatmap::wrap(a - b, 1.0);
        assert!(off.abs() < 1e-3, "{off} beats");
    }

    fn deck0(g: &mut Graph) -> DeckState {
        g.state().decks[0]
    }

    fn send(g: &mut Graph, controls: &[fn(u8) -> Control]) {
        for c in controls {
            g.apply(c(0));
        }
    }

    fn seek(g: &mut Graph, frame: i64) {
        g.apply(Control::Seek { deck: 0, frame });
    }

    const IN: fn(u8) -> Control = |deck| Control::LoopIn { deck };
    const OUT: fn(u8) -> Control = |deck| Control::LoopOut { deck };
    const RELOOP: fn(u8) -> Control = |deck| Control::Reloop { deck };

    #[test]
    fn loop_in_and_out_loop_from_the_head_or_the_nearest_beat() {
        // No quantise: exactly where the head was.
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 30_000);
        send(&mut g, &[IN]);
        assert_eq!(deck0(&mut g).loop_in, Some(30_000));
        assert_eq!(region(&mut g), None, "pending");
        render_frames(&mut g, 40_000);
        send(&mut g, &[OUT]);
        let (start, end, beats) = region(&mut g).unwrap();
        assert_eq!((start, end), (30_000, 70_000));
        assert!((beats.unwrap() - 40.0 / 24.0).abs() < 1e-9, "{beats:?}");
        let st = deck0(&mut g);
        assert_eq!((st.loop_in, st.position), (None, 30_000), "wraps at once");

        // Quantised: both ends on the nearest beat.
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        g.apply(Control::Quantise { deck: 0, on: true });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 30_000); // beat 1.25
        send(&mut g, &[IN]);
        assert_eq!(deck0(&mut g).loop_in, Some(24_000));
        render_frames(&mut g, 40_000); // beat 2.92
        send(&mut g, &[OUT]);
        assert_eq!(region(&mut g), Some((24_000, 72_000, Some(2.0))));
        assert_eq!(deck0(&mut g).position, 70_000, "plays on to the end");

        // OUT snapping onto IN: a beat after IN.
        g.apply(Control::LoopExit { deck: 0 });
        seek(&mut g, 100_000); // keeps the phase: 102,000, beat 4.25
        send(&mut g, &[IN]);
        assert_eq!(deck0(&mut g).loop_in, Some(96_000));
        render_frames(&mut g, 3_000);
        send(&mut g, &[OUT]);
        assert_eq!(region(&mut g), Some((96_000, 120_000, Some(1.0))));

        // No grid: quantise has no beat to snap to, so the head itself.
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, ramp_source(480_000));
        g.apply(Control::Quantise { deck: 0, on: true });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 10_001);
        send(&mut g, &[IN]);
        render_frames(&mut g, 20_000);
        send(&mut g, &[OUT]);
        assert_eq!(region(&mut g), Some((10_001, 30_001, None)));
        render_frames(&mut g, 50_000);
        assert_eq!(deck0(&mut g).position, 10_001 + 50_000 % 20_000);
    }

    #[test]
    fn in_and_out_move_an_active_loops_ends() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        seek(&mut g, 30_000);
        g.apply(Control::BeatLoop {
            deck: 0,
            beats: 2.0,
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 10_000);
        send(&mut g, &[IN]);
        let (start, end, beats) = region(&mut g).unwrap();
        assert_eq!((start, end), (40_000, 78_000), "start moved, end kept");
        assert!((beats.unwrap() - 38.0 / 24.0).abs() < 1e-9);
        assert_eq!(deck0(&mut g).loop_in, None, "not a pending IN");
        render_frames(&mut g, 20_000);
        send(&mut g, &[OUT]);
        assert_eq!(region(&mut g).map(|r| (r.0, r.1)), Some((40_000, 60_000)));
        assert_eq!(deck0(&mut g).position, 40_000, "past the new end: wraps");

        // Quantised, the end snaps; behind the head it wraps at once.
        g.apply(Control::Quantise { deck: 0, on: true });
        render_frames(&mut g, 15_000); // 55,000: beat 2.29
        send(&mut g, &[OUT]);
        let (start, end, beats) = region(&mut g).unwrap();
        assert_eq!((start, end), (40_000, 48_000));
        assert!((beats.unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(deck0(&mut g).position, 40_000 + 15_000 % 8_000);
        // IN that would leave under 8 ms is ignored.
        render_frames(&mut g, 7_900 - 7_000); // 47,900 (unquantised below)
        g.apply(Control::Quantise { deck: 0, on: false });
        send(&mut g, &[IN]);
        assert_eq!(region(&mut g).map(|r| r.0), Some(40_000));
    }

    #[test]
    fn out_under_8_ms_or_without_an_in_is_ignored() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, ramp_source(480_000));
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 1_000);
        send(&mut g, &[OUT]);
        assert_eq!(region(&mut g), None, "no IN");
        send(&mut g, &[IN]);
        render_frames(&mut g, 380); // 7.9 ms
        send(&mut g, &[OUT]);
        assert_eq!(region(&mut g), None);
        assert_eq!(deck0(&mut g).loop_in, Some(1_000), "IN kept");
        render_frames(&mut g, 20);
        send(&mut g, &[OUT]);
        assert_eq!(region(&mut g), Some((1_000, 1_400, None)));
        // A head back before IN (a jump) has nothing to close.
        g.apply(Control::LoopExit { deck: 0 });
        seek(&mut g, 5_000);
        send(&mut g, &[IN]);
        seek(&mut g, 4_000);
        send(&mut g, &[OUT]);
        assert_eq!(region(&mut g), None);
    }

    #[test]
    fn reloop_keeps_the_phase_while_playing_and_exits_while_looping() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        send(&mut g, &[RELOOP]);
        assert_eq!(region(&mut g), None, "no loop yet");
        g.apply(Control::Quantise { deck: 0, on: true });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 30_000);
        g.apply(Control::BeatLoop {
            deck: 0,
            beats: 1.0,
        });
        render_frames(&mut g, 10_000); // 40,000
        g.apply(Control::LoopExit { deck: 0 });
        let kept = Some(LoopRegion {
            start: 24_000,
            end: 48_000,
            beats: Some(1.0),
        });
        assert_eq!(deck0(&mut g).last_loop, kept);
        render_frames(&mut g, 66_000); // 106,000: beat 4.42
        send(&mut g, &[RELOOP]);
        assert_eq!(region(&mut g), Some((24_000, 48_000, Some(1.0))));
        assert_eq!(deck0(&mut g).position, 34_000, "beat 1.42");
        // RELOOP while looping leaves it, keeping it.
        send(&mut g, &[RELOOP]);
        assert_eq!(region(&mut g), None);
        assert_eq!(deck0(&mut g).last_loop, kept);
        // A phase behind the start lands a beat later, inside the loop.
        render_frames(&mut g, 80_000); // 114,000: beat 4.75
        send(&mut g, &[RELOOP]);
        assert_eq!(deck0(&mut g).position, 42_000, "beat 1.75");
        // A seek out of the loop also keeps it.
        seek(&mut g, 300_000);
        assert_eq!(region(&mut g), None);
        assert_eq!(deck0(&mut g).last_loop, kept);

        // Under a beat, the phase can't land inside: the start.
        g.apply(Control::BeatLoop {
            deck: 0,
            beats: 0.25,
        });
        let (start, _, _) = region(&mut g).unwrap();
        send(&mut g, &[RELOOP]);
        render_frames(&mut g, 10_000);
        send(&mut g, &[RELOOP]);
        assert_eq!(deck0(&mut g).position, start);
    }

    #[test]
    fn out_and_reloop_while_paused_move_the_head_to_the_loop() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        seek(&mut g, 30_000);
        send(&mut g, &[IN]);
        seek(&mut g, 70_000);
        send(&mut g, &[OUT]);
        let st = deck0(&mut g);
        assert_eq!(region(&mut g).map(|r| (r.0, r.1)), Some((30_000, 70_000)));
        assert_eq!((st.position, st.playing), (30_000, false), "PLAY starts it");
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 50_000);
        assert_eq!(deck0(&mut g).position, 40_000);
        g.apply(Control::Pause { deck: 0 });
        render_frames(&mut g, 2 * RAMP);
        assert!(!deck0(&mut g).playing);
        g.apply(Control::LoopExit { deck: 0 });
        seek(&mut g, 200_000);
        send(&mut g, &[RELOOP]);
        let st = deck0(&mut g);
        assert_eq!((st.position, st.playing), (30_000, false));
        assert!(st.loop_region.is_some());
    }

    #[test]
    fn calling_a_memory_loop_arms_reloop_and_loading_clears_it() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, gridded(480_000));
        g.apply(Control::CallCue {
            deck: 0,
            frame: 48_000,
            end: Some(96_000),
        });
        let st = deck0(&mut g);
        assert_eq!((st.position, st.loop_region), (48_000, None), "no loop yet");
        let called = Some(LoopRegion {
            start: 48_000,
            end: 96_000,
            beats: Some(2.0),
        });
        assert_eq!(st.last_loop, called);
        seek(&mut g, 200_000);
        send(&mut g, &[RELOOP]);
        assert_eq!(region(&mut g), Some((48_000, 96_000, Some(2.0))));
        assert_eq!(deck0(&mut g).position, 48_000);

        send(&mut g, &[RELOOP, IN]);
        load(&mut g, gridded(480_000));
        let st = deck0(&mut g);
        assert_eq!(
            (st.loop_region, st.loop_in, st.last_loop),
            (None, None, None)
        );
    }

    #[test]
    fn resize_without_a_grid_halves_and_doubles_by_time() {
        let mut g = Graph::new(RATE).unwrap();
        load(&mut g, ramp_source(100_000));
        seek(&mut g, 10_000);
        send(&mut g, &[IN]);
        seek(&mut g, 58_000);
        send(&mut g, &[OUT]);
        assert_eq!(region(&mut g), Some((10_000, 58_000, None)));
        let resize = |g: &mut Graph, double| {
            g.apply(Control::LoopResize { deck: 0, double });
            region(g)
        };
        assert_eq!(resize(&mut g, false), Some((10_000, 34_000, None)));
        assert_eq!(resize(&mut g, true), Some((10_000, 58_000, None)));
        assert_eq!(
            resize(&mut g, true),
            Some((10_000, 58_000, None)),
            "past the track"
        );
        for _ in 0..12 {
            resize(&mut g, false);
        }
        // 48,000 / 2^n stops above 8 ms (384 frames): 750.
        assert_eq!(region(&mut g), Some((10_000, 10_750, None)));
    }

    #[test]
    fn a_master_looping_no_whole_number_of_beats_gives_no_phase() {
        let mut g = master_and_follower(true);
        // Master (deck 0) loops 1.5 beats by IN / OUT.
        g.apply(Control::LoopIn { deck: 0 });
        render_frames(&mut g, 33_962); // 36,000 source frames at 1.06
        g.apply(Control::LoopOut { deck: 0 });
        let l = g.state().decks[0].loop_region.unwrap();
        assert!((l.beats.unwrap() - 1.5).abs() < 0.01, "{l:?}");
        render_frames(&mut g, 96_000);
        // The follower runs on at the matched tempo, not dragged into the
        // master's wraps: 48,000 frames at 1.06 are 2.12 beats.
        let before = g.state().decks[1].beat.unwrap();
        render_frames(&mut g, 48_000);
        let after = g.state().decks[1].beat.unwrap();
        assert!((after - before - 2.12).abs() < 1e-3, "{}", after - before);
    }

    #[test]
    fn unknown_deck_is_ignored_and_its_source_returned() {
        let mut g = Graph::new(RATE).unwrap();
        assert!(g.apply(Control::Play { deck: 9 }).is_none());
        assert!(g
            .apply(Control::Load {
                deck: 9,
                src: ramp_source(1)
            })
            .is_some());
    }
}
