//! The audio graph: decks and master gain, rendered sample by sample.
//!
//! Everything here runs on the audio thread: no allocation, locks or I/O.
//! [`render`] runs the same graph without a device, for tests (D21).

use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

/// Decks in the graph. Phase 5 raises this to 4.
pub const DECKS: usize = 1;

/// Length of every gain ramp: play, pause, load, volume (phase-5.md: ≥ 5 ms).
pub const RAMP_MS: u32 = 5;

/// Equal-power crossfade from the old to the new position on a seek while
/// playing. Long enough to hide the jump in the bass, short enough to feel
/// instant; a listening choice, not a measured rekordbox value.
pub const SEEK_XFADE_MS: u32 = 20;

/// A track ready to play: interleaved stereo `i16` at the engine's rate.
/// Built off the audio thread; the graph only reads it.
pub struct DeckSource {
    pcm: Arc<[i16]>,
}

impl DeckSource {
    /// `samples` is `L R L R …`; a trailing odd sample is dropped.
    pub fn new(mut samples: Vec<i16>) -> Self {
        samples.truncate(samples.len() & !1);
        Self {
            pcm: samples.into(),
        }
    }

    /// Number of stereo frames.
    pub fn frames(&self) -> i64 {
        (self.pcm.len() / 2) as i64
    }
}

impl std::fmt::Debug for DeckSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeckSource")
            .field("frames", &self.frames())
            .finish()
    }
}

/// A command for the graph. Positions are frames at the engine's rate.
#[derive(Debug, Clone)]
pub enum Control {
    /// Replaces the deck's track and stops it at the start.
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
    /// Clamped to the track. The new position plays from the next frame;
    /// while playing, the old one fades out under it over [`SEEK_XFADE_MS`],
    /// so the jump is instant without a gap or a click.
    Seek {
        deck: u8,
        frame: i64,
    },
    /// Master volume, 0..=1 like a knob; see [`volume_gain`].
    Volume {
        value: f32,
    },
}

/// One deck as the UI sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeckState {
    pub loaded: bool,
    pub playing: bool,
    /// Frames from the start of the track.
    pub position: i64,
    pub frames: i64,
    /// Seek controls taken so far (wrapping), so the control side can tell
    /// whether a snapshot already includes its latest seek.
    pub seeks: u32,
    /// Play and pause controls taken so far (wrapping), for the same reason.
    pub transports: u32,
}

/// Graph snapshot, sent to the control side about 60 times a second.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EngineState {
    pub decks: [DeckState; DECKS],
    pub volume: f32,
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
struct Ramp {
    value: f32,
    target: f32,
    step: f32,
}

impl Ramp {
    fn new(value: f32, sample_rate: u32) -> Self {
        let frames = (sample_rate * RAMP_MS / 1000).max(1);
        Self {
            value,
            target: value,
            step: 1.0 / frames as f32,
        }
    }

    fn next(&mut self) -> f32 {
        if self.value < self.target {
            self.value = (self.value + self.step).min(self.target);
        } else if self.value > self.target {
            self.value = (self.value - self.step).max(self.target);
        }
        self.value
    }

    fn settled(&self) -> bool {
        self.value == self.target
    }
}

/// What a deck does once its gate has faded out.
#[derive(Debug, Default)]
enum Pending {
    #[default]
    None,
    Pause,
    Load(Option<Arc<DeckSource>>),
}

/// The read head a seek left behind, fading out under the new one.
#[derive(Debug, Clone, Copy)]
struct Tail {
    position: i64,
    /// Frames of the crossfade still to play, `xfade..=1`.
    left: u32,
}

/// One stereo frame of `src` as `f32`, silence past either end.
fn frame_at(src: &DeckSource, position: i64) -> (f32, f32) {
    if !(0..src.frames()).contains(&position) {
        return (0.0, 0.0);
    }
    let i = (position * 2) as usize;
    (
        f32::from(src.pcm[i]) / 32_768.0,
        f32::from(src.pcm[i + 1]) / 32_768.0,
    )
}

#[derive(Debug)]
struct Deck {
    src: Option<Arc<DeckSource>>,
    position: i64,
    playing: bool,
    /// Fades the output in and out around play, pause and load, so none of
    /// them clicks.
    gate: Ramp,
    pending: Pending,
    seeks: u32,
    transports: u32,
    tail: Option<Tail>,
    /// [`SEEK_XFADE_MS`] in frames.
    xfade: u32,
}

impl Deck {
    fn new(sample_rate: u32) -> Self {
        Self {
            src: None,
            position: 0,
            playing: false,
            gate: Ramp::new(0.0, sample_rate),
            pending: Pending::None,
            seeks: 0,
            transports: 0,
            tail: None,
            xfade: (sample_rate * SEEK_XFADE_MS / 1000).max(1),
        }
    }

    /// True while the deck makes sound, so a change must fade first.
    fn audible(&self) -> bool {
        self.playing && self.gate.value > 0.0
    }

    /// Applies `c`; returns a source the caller must drop off the audio thread.
    fn apply(&mut self, c: Control) -> Option<Arc<DeckSource>> {
        match c {
            Control::Load { src, .. } => self.change(Pending::Load(Some(src))),
            Control::Unload { .. } => self.change(Pending::Load(None)),
            Control::Play { .. } => {
                self.transports = self.transports.wrapping_add(1);
                if self.src.is_some() && self.position < self.frames() {
                    // Cancels a pending pause; a pending load still runs.
                    if matches!(self.pending, Pending::Pause) {
                        self.pending = Pending::None;
                    }
                    self.playing = true;
                    if matches!(self.pending, Pending::None) {
                        self.gate.target = 1.0;
                    }
                }
                None
            }
            Control::Pause { .. } => {
                self.transports = self.transports.wrapping_add(1);
                if self.audible() && matches!(self.pending, Pending::None) {
                    self.pending = Pending::Pause;
                    self.gate.target = 0.0;
                } else if !self.audible() {
                    self.playing = false;
                    self.gate = Ramp {
                        value: 0.0,
                        target: 0.0,
                        ..self.gate
                    };
                }
                None
            }
            Control::Seek { frame, .. } => {
                // Ignored while a load is pending: it is for the old track.
                // Counted either way, so the control side stays in step.
                self.seeks = self.seeks.wrapping_add(1);
                if !matches!(self.pending, Pending::Load(_)) {
                    let frame = frame.clamp(0, self.frames());
                    if self.audible() && frame != self.position {
                        // A seek during a crossfade restarts it from the
                        // head heard most recently.
                        self.tail = Some(Tail {
                            position: self.position,
                            left: self.xfade,
                        });
                    }
                    self.position = frame;
                }
                None
            }
            Control::Volume { .. } => None,
        }
    }

    /// Runs `action` now when silent, else after a fade-out.
    fn change(&mut self, action: Pending) -> Option<Arc<DeckSource>> {
        if self.audible() {
            // A load replaces whatever was pending; drop that safely.
            let replaced = std::mem::replace(&mut self.pending, action);
            self.gate.target = 0.0;
            match replaced {
                Pending::Load(src) => src,
                _ => None,
            }
        } else {
            self.run(action)
        }
    }

    fn run(&mut self, action: Pending) -> Option<Arc<DeckSource>> {
        match action {
            Pending::None => None,
            Pending::Pause => {
                self.playing = false;
                self.tail = None;
                None
            }
            Pending::Load(src) => {
                self.playing = false;
                self.position = 0;
                self.tail = None;
                self.gate = Ramp {
                    value: 0.0,
                    target: 0.0,
                    ..self.gate
                };
                std::mem::replace(&mut self.src, src)
            }
        }
    }

    fn frames(&self) -> i64 {
        self.src.as_ref().map_or(0, |s| s.frames())
    }

    /// Next stereo frame; may hand back a source to retire.
    fn next(&mut self, retired: &mut Option<Arc<DeckSource>>) -> (f32, f32) {
        if !self.playing {
            return (0.0, 0.0);
        }
        let gate = self.gate.next();
        let out = match &self.src {
            Some(src) if self.position < src.frames() => {
                let (mut l, mut r) = frame_at(src, self.position);
                self.position += 1;
                if let Some(t) = &mut self.tail {
                    // x runs 1 -> 0: old head cos-out, new head sin-in.
                    let x = t.left as f32 / self.xfade as f32;
                    let (old, new) = ((x * FRAC_PI_2).sin(), (x * FRAC_PI_2).cos());
                    let (ol, or) = frame_at(src, t.position);
                    l = l * new + ol * old;
                    r = r * new + or * old;
                    t.position += 1;
                    t.left -= 1;
                    if t.left == 0 {
                        self.tail = None;
                    }
                }
                (l * gate, r * gate)
            }
            _ => {
                // End of track. A pending load still runs below.
                if matches!(self.pending, Pending::None | Pending::Pause) {
                    self.playing = false;
                    self.pending = Pending::None;
                }
                self.tail = None;
                self.gate.value = 0.0;
                (0.0, 0.0)
            }
        };
        if self.gate.settled() && self.gate.value == 0.0 {
            let action = std::mem::take(&mut self.pending);
            if let Some(old) = self.run(action) {
                // At most one per frame; the caller drains it every frame.
                *retired = Some(old);
            }
        }
        out
    }

    fn state(&self) -> DeckState {
        DeckState {
            loaded: self.src.is_some(),
            playing: self.playing,
            position: self.position,
            frames: self.frames(),
            seeks: self.seeks,
            transports: self.transports,
        }
    }
}

/// The whole graph. Created off the audio thread, then moved into it.
#[derive(Debug)]
pub struct Graph {
    decks: [Deck; DECKS],
    master: Ramp,
    volume: f32,
}

impl Graph {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            decks: std::array::from_fn(|_| Deck::new(sample_rate)),
            master: Ramp::new(volume_gain(1.0), sample_rate),
            volume: 1.0,
        }
    }

    /// Applies one control. A replaced source comes back so the caller can
    /// drop it off the audio thread. Out-of-range decks are ignored.
    pub fn apply(&mut self, c: Control) -> Option<Arc<DeckSource>> {
        let deck = match &c {
            Control::Volume { value } => {
                self.volume = value.clamp(0.0, 1.0);
                self.master.target = volume_gain(*value);
                return None;
            }
            Control::Load { deck, .. }
            | Control::Unload { deck }
            | Control::Play { deck }
            | Control::Pause { deck }
            | Control::Seek { deck, .. } => usize::from(*deck),
        };
        match self.decks.get_mut(deck) {
            Some(d) => d.apply(c),
            None => match c {
                Control::Load { src, .. } => Some(src),
                _ => None,
            },
        }
    }

    pub fn state(&self) -> EngineState {
        EngineState {
            decks: std::array::from_fn(|i| self.decks[i].state()),
            volume: self.volume,
        }
    }

    /// Renders interleaved stereo into `out`, calling `retire` with every
    /// source a deck lets go of.
    pub fn render_with(&mut self, out: &mut [f32], mut retire: impl FnMut(Arc<DeckSource>)) {
        for frame in out.as_chunks_mut::<2>().0 {
            let mut l = 0.0;
            let mut r = 0.0;
            for deck in &mut self.decks {
                let mut retired = None;
                let (dl, dr) = deck.next(&mut retired);
                if let Some(old) = retired {
                    retire(old);
                }
                l += dl;
                r += dr;
            }
            let gain = self.master.next();
            frame[0] = l * gain;
            frame[1] = r * gain;
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
        Arc::new(DeckSource::new(samples))
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
        let mut g = Graph::new(RATE);
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(10_000),
        });
        g.apply(Control::Play { deck: 0 });
        let out = render_frames(&mut g, 1_000);
        // Fade-in: rising, never above the source.
        assert!(sample(out[2 * 10]) <= 10);
        for f in RAMP..1_000 {
            assert_eq!(sample(out[2 * f]), f as i32, "frame {f}");
            assert_eq!(sample(out[2 * f + 1]), -(f as i32), "frame {f}");
        }
        assert_eq!(g.state().decks[0].position, 1_000);
    }

    #[test]
    fn silent_until_play_and_stops_at_the_end() {
        let mut g = Graph::new(RATE);
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
        let mut g = Graph::new(RATE);
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(48_000),
        });
        g.apply(Control::Play { deck: 0 });
        render_frames(&mut g, 1_000);
        g.apply(Control::Pause { deck: 0 });
        assert!(g.state().decks[0].playing, "still fading");
        assert_eq!(g.state().decks[0].transports, 2, "play and pause counted");
        let out = render_frames(&mut g, RAMP + 10);
        assert!(!g.state().decks[0].playing);
        let pos = g.state().decks[0].position;
        // One extra frame is float rounding in the ramp.
        assert!(
            (1_000 + RAMP as i64..=1_001 + RAMP as i64).contains(&pos),
            "{pos}"
        );
        // Gain falls through the fade and ends silent.
        assert!(out[2].abs() > out[2 * (RAMP - 10)].abs());
        assert!(out[2 * (RAMP + 5)..].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn play_during_a_fade_out_cancels_the_pause() {
        let mut g = Graph::new(RATE);
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
        let mut g = Graph::new(RATE);
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
        let out = render_frames(&mut g, XFADE + 3);
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
        let mut g = Graph::new(RATE);
        g.apply(Control::Load {
            deck: 0,
            src: ramp_source(20_000),
        });
        g.apply(Control::Seek {
            deck: 0,
            frame: 5_000,
        });
        g.apply(Control::Play { deck: 0 });
        let out = render_frames(&mut g, RAMP + 2);
        // Only the fade-in of the new position, nothing from frame 0.
        assert_eq!(sample(out[2 * (RAMP + 1)]), 5_000 + RAMP as i32 + 1);
    }

    #[test]
    fn seek_during_a_pause_fade_jumps_and_still_pauses() {
        let mut g = Graph::new(RATE);
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
        let mut g = Graph::new(RATE);
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
        let mut g = Graph::new(RATE);
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
        let mut g = Graph::new(RATE);
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
        let mut g = Graph::new(RATE);
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
        let mut g = Graph::new(RATE);
        g.apply(Control::Load {
            deck: 0,
            src: Arc::new(DeckSource::new(vec![16_384; 96_000])),
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
    fn unknown_deck_is_ignored_and_its_source_returned() {
        let mut g = Graph::new(RATE);
        assert!(g.apply(Control::Play { deck: 9 }).is_none());
        assert!(g
            .apply(Control::Load {
                deck: 9,
                src: ramp_source(1)
            })
            .is_some());
    }
}
