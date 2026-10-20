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

/// Slowest tempo fader setting (−50%, the widest CDJ range).
pub const TEMPO_MIN: f64 = 0.5;
/// Fastest tempo fader setting (+50%).
pub const TEMPO_MAX: f64 = 1.5;

/// Length of every gain ramp: play, pause, load, volume (phase-5.md: ≥ 5 ms).
pub const RAMP_MS: u32 = 5;

/// Equal-power crossfade from the old to the new position on a seek while
/// playing. Long enough to hide the jump in the bass, short enough to feel
/// instant; a listening choice, not a measured rekordbox value.
pub const SEEK_XFADE_MS: u32 = 20;

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
    /// Sync is on.
    pub sync: bool,
    /// Sync matches the downbeat too.
    pub sync_bar: bool,
    /// Sync is following the leader (on, not the leader, and both decks
    /// have a grid).
    pub locked: bool,
    pub quantise: bool,
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
            sync: false,
            sync_bar: false,
            locked: false,
            quantise: false,
        }
    }
}

/// Graph snapshot, sent to the control side about 60 times a second.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EngineState {
    pub decks: [DeckState; DECKS],
    /// The sync leader: the chosen deck, else the loaded deck with a grid
    /// that has been playing longest (kept while everything is stopped).
    pub leader: Option<u8>,
    /// The leader was chosen with [`Control::Leader`].
    pub leader_chosen: bool,
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
    /// Chosen leader; `None` for automatic.
    leader: Option<usize>,
    /// The automatic leader, kept while no deck plays.
    auto_leader: Option<usize>,
    engine_rate: f64,
    /// Counts play starts, for the automatic leader.
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
            leader: None,
            auto_leader: None,
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
        if let Control::Leader { deck } = &c {
            self.leader = deck.map(usize::from).filter(|&d| d < DECKS);
            return None;
        }
        let deck = c.deck()?;
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
            decks: std::array::from_fn(|i| self.decks[i].state()),
            leader: self.current_leader().map(|d| d as u8),
            leader_chosen: self.leader.is_some(),
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

    fn current_leader(&self) -> Option<usize> {
        self.leader.or(self.auto_leader)
    }

    /// The automatic leader: the loaded deck with a grid that has been
    /// playing longest; the previous one while nothing plays.
    fn update_auto_leader(&mut self) {
        let has_grid = |d: &Deck| d.src().is_some_and(|s| !s.beats().is_empty());
        let playing = self
            .decks
            .iter()
            .enumerate()
            .filter(|(_, d)| d.playing() && has_grid(d))
            .min_by_key(|(_, d)| d.started)
            .map(|(i, _)| i);
        self.auto_leader = playing.or(self.auto_leader.filter(|&i| has_grid(&self.decks[i])));
    }

    /// Sets every deck's speed for the next block: the leader and unsynced
    /// decks at their own speed, followers from [`sync::follow`].
    fn plan(&mut self) {
        self.update_auto_leader();
        let leader = self.current_leader();
        let lead = leader.and_then(|i| {
            let d = &mut self.decks[i];
            let speed = d.own_speed();
            d.set_speed(speed);
            let src = d.src()?;
            sync::Lead::new(src.beats(), d.position(), d.step(), d.playing())
        });
        for (i, d) in self.decks.iter_mut().enumerate() {
            if Some(i) == leader {
                d.locked = false;
                continue;
            }
            let plan = match (&lead, d.sync, d.src()) {
                (Some(lead), true, Some(src)) => sync::follow(
                    lead,
                    &sync::Follower {
                        beats: src.beats(),
                        position: d.position(),
                        rate_ratio: d.rate_ratio(),
                        playing: d.playing(),
                        bar: d.sync_bar,
                        nudge: d.nudge,
                    },
                    self.engine_rate,
                ),
                _ => None,
            };
            d.locked = plan.is_some();
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

    /// Renders interleaved stereo into `out`, calling `retire` with every
    /// source a deck lets go of.
    pub fn render_with(&mut self, out: &mut [f32], mut retire: impl FnMut(Arc<DeckSource>)) {
        for block in out.chunks_mut(BLOCK * 2) {
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
            self.mixer.process(&mut self.inputs, block);
        }
    }
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
        g.apply(Control::Leader { deck: Some(2) });
        let st = g.state();
        let d = st.decks[1];
        assert_eq!((d.tempo, d.nudge), (1.08, 1.0));
        assert!(d.sync && d.sync_bar && d.quantise);
        assert_eq!(st.decks[2].tempo, TEMPO_MAX);
        assert_eq!((st.leader, st.leader_chosen), (Some(2), true));
        g.apply(Control::Leader { deck: Some(7) });
        assert_eq!((g.state().leader, g.state().leader_chosen), (None, false));
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
            13_000,
            "cue does not move the head"
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
    fn the_leader_is_the_deck_playing_longest_unless_chosen() {
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
        assert_eq!(g.state().leader, None);
        // Deck 3 has no grid, so it never leads.
        for deck in [3, 2, 1] {
            g.apply(Control::Play { deck });
        }
        render_frames(&mut g, 64);
        assert_eq!(g.state().leader, Some(2));
        g.apply(Control::Pause { deck: 2 });
        render_frames(&mut g, RAMP * 2);
        assert_eq!(g.state().leader, Some(1));
        // With nothing playing, the last leader keeps the tempo.
        g.apply(Control::Pause { deck: 1 });
        render_frames(&mut g, RAMP * 2);
        assert_eq!(g.state().leader, Some(1));
        g.apply(Control::Leader { deck: Some(0) });
        render_frames(&mut g, 64);
        let st = g.state();
        assert_eq!((st.leader, st.leader_chosen), (Some(0), true));
        g.apply(Control::Leader { deck: None });
        render_frames(&mut g, 64);
        assert_eq!(g.state().leader, Some(1));
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
