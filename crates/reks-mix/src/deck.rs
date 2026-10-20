//! One deck: transport, CDJ cue, hot cues, quantise and varispeed.
//!
//! At exactly unit step (speed 1.0 and the source at the engine's rate) the
//! deck copies source frames, so playback is bit-exact; at any other step it
//! reads through [`Varispeed`]. The graph sets the speed before every block
//! (tempo × nudge, or what sync asks for).
//!
//! Runs on the audio thread: no allocation, locks or I/O after [`Deck::new`].

use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

use crate::control::Control;
use crate::graph::{DeckState, Ramp, SEEK_XFADE_MS, TEMPO_MAX, TEMPO_MIN};
use crate::source::DeckSource;
use crate::varispeed::Varispeed;

/// What a deck does once its gate has faded out.
#[derive(Debug, Default)]
enum Pending {
    #[default]
    None,
    Pause,
    /// Pause and return to the cue point.
    Cue,
    Load(Option<Arc<DeckSource>>),
}

/// The read head a jump left behind, fading out under the new one.
#[derive(Debug, Clone, Copy)]
struct Tail {
    position: f64,
    step: f64,
    /// Frames of the crossfade still to play, `xfade..=1`.
    left: u32,
}

/// Clamps a ratio control to the tempo range; non-finite values give 1.0.
fn ratio(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(TEMPO_MIN, TEMPO_MAX)
    } else {
        1.0
    }
}

#[derive(Debug)]
pub(crate) struct Deck {
    src: Option<Arc<DeckSource>>,
    engine_rate: u32,
    /// Source time of the next frame while copying (always whole then).
    position: f64,
    vs: Varispeed,
    /// `vs` holds the position (the deck is reading through it).
    resampling: bool,
    /// Source frames per output frame for the current block.
    step: f64,
    playing: bool,
    /// Fades the output in and out around play, pause, cue and load, so none
    /// of them clicks.
    gate: Ramp,
    pending: Pending,
    seeks: u32,
    transports: u32,
    tail: Option<Tail>,
    /// [`SEEK_XFADE_MS`] in frames.
    xfade: u32,
    cue: f64,
    pub(crate) tempo: f64,
    pub(crate) nudge: f64,
    pub(crate) sync: bool,
    pub(crate) sync_bar: bool,
    quantise: bool,
    /// Play order, for the automatic sync leader; 0 when stopped.
    pub(crate) started: u64,
}

impl Deck {
    /// Allocates the varispeed head; `None` if rubato refuses its setup.
    pub(crate) fn new(sample_rate: u32) -> Option<Self> {
        Some(Self {
            src: None,
            engine_rate: sample_rate.max(1),
            position: 0.0,
            vs: Varispeed::new()?,
            resampling: false,
            step: 1.0,
            playing: false,
            gate: Ramp::new(0.0, sample_rate),
            pending: Pending::None,
            seeks: 0,
            transports: 0,
            tail: None,
            xfade: (sample_rate * SEEK_XFADE_MS / 1000).max(1),
            cue: 0.0,
            tempo: 1.0,
            nudge: 1.0,
            sync: false,
            sync_bar: false,
            quantise: false,
            started: 0,
        })
    }

    /// Source time of the next frame.
    pub(crate) fn position(&self) -> f64 {
        if self.resampling {
            self.vs.position()
        } else {
            self.position
        }
    }

    /// Source frames per output frame at speed 1.
    pub(crate) fn rate_ratio(&self) -> f64 {
        self.src.as_ref().map_or(1.0, |s| {
            f64::from(s.sample_rate()) / f64::from(self.engine_rate)
        })
    }

    /// Speed for the next block (1.0 = as recorded).
    pub(crate) fn set_speed(&mut self, speed: f64) {
        self.step = crate::varispeed::clamp_step(speed * self.rate_ratio());
    }

    /// Speed the deck's own controls ask for.
    pub(crate) fn own_speed(&self) -> f64 {
        self.tempo * self.nudge
    }

    /// True while the deck makes sound, so a change must fade first.
    fn audible(&self) -> bool {
        self.playing && self.gate.value > 0.0
    }

    /// Applies `c`; returns a source the caller must drop off the audio
    /// thread. `clock` orders play starts.
    pub(crate) fn apply(&mut self, c: Control, clock: u64) -> Option<Arc<DeckSource>> {
        match c {
            Control::Load { src, .. } => self.change(Pending::Load(Some(src))),
            Control::Unload { .. } => self.change(Pending::Load(None)),
            Control::Play { .. } => {
                self.transports = self.transports.wrapping_add(1);
                self.play(clock);
                None
            }
            Control::Pause { .. } => {
                self.transports = self.transports.wrapping_add(1);
                if self.audible() && matches!(self.pending, Pending::None) {
                    self.pending = Pending::Pause;
                    self.gate.target = 0.0;
                } else if !self.audible() {
                    self.stop();
                }
                None
            }
            Control::Cue { .. } => {
                self.cue_button();
                None
            }
            Control::Seek { frame, .. } => {
                // Counted even when ignored, so the control side stays in step.
                self.seeks = self.seeks.wrapping_add(1);
                self.seek(frame as f64);
                None
            }
            Control::HotCue { slot, .. } => {
                self.hot_cue(slot, clock);
                None
            }
            Control::Tempo { ratio: r, .. } => {
                self.tempo = ratio(r);
                None
            }
            Control::Nudge { ratio: r, .. } => {
                self.nudge = ratio(r);
                None
            }
            Control::Sync { on, bar, .. } => {
                self.sync = on;
                self.sync_bar = bar;
                None
            }
            Control::Quantise { on, .. } => {
                self.quantise = on;
                None
            }
            Control::Leader { .. }
            | Control::Channel { .. }
            | Control::Filter { .. }
            | Control::Crossfader { .. }
            | Control::Master { .. }
            | Control::Volume { .. } => None,
        }
    }

    fn play(&mut self, clock: u64) {
        if self.src.is_none() || self.position() >= self.frames() as f64 {
            return;
        }
        // Cancels a pending pause or cue; a pending load still runs.
        if matches!(self.pending, Pending::Pause | Pending::Cue) {
            self.pending = Pending::None;
        }
        if !self.playing {
            self.started = clock;
        }
        self.playing = true;
        if matches!(self.pending, Pending::None) {
            self.gate.target = 1.0;
        }
    }

    fn stop(&mut self) {
        self.playing = false;
        self.started = 0;
        self.tail = None;
        self.gate.value = 0.0;
        self.gate.target = 0.0;
    }

    /// CDJ cue: playing, back to the cue point and pause; paused, the cue
    /// point moves here (to the nearest beat with quantise).
    fn cue_button(&mut self) {
        if matches!(self.pending, Pending::Load(_)) || self.src.is_none() {
            return;
        }
        if self.playing {
            if self.audible() {
                self.pending = Pending::Cue;
                self.gate.target = 0.0;
            } else {
                self.run(Pending::Cue);
            }
        } else {
            let here = self.position();
            self.cue = match self.src.as_ref() {
                Some(s) if self.quantise => s.beats().snap(here),
                _ => here,
            }
            .clamp(0.0, self.frames() as f64);
        }
    }

    /// Jumps to hot cue `slot` and plays. With quantise while playing, the
    /// jump keeps the beat phase; otherwise it lands on the cue exactly.
    fn hot_cue(&mut self, slot: u8, clock: u64) {
        let Some(src) = self.src.as_ref() else {
            return;
        };
        let Some(at) = src.hot_cue(slot) else {
            return;
        };
        if matches!(self.pending, Pending::Load(_)) {
            return;
        }
        let at = at as f64;
        let to = if self.quantise && self.playing {
            src.beats().keep_phase(self.position(), at)
        } else {
            at
        };
        self.jump(to.clamp(0.0, self.frames() as f64));
        self.play(clock);
    }

    /// Moves the head to `to`, clamped to the track. With quantise, a jump
    /// while playing keeps the beat phase and one while paused lands on the
    /// nearest beat. Ignored while a load is pending: it is for the old track.
    fn seek(&mut self, to: f64) {
        if matches!(self.pending, Pending::Load(_)) {
            return;
        }
        let to = match self.src.as_ref() {
            Some(s) if self.quantise && self.playing => s.beats().keep_phase(self.position(), to),
            Some(s) if self.quantise => s.beats().snap(to),
            _ => to,
        };
        self.jump(to.clamp(0.0, self.frames() as f64));
    }

    /// Moves the head now; while audible, the old head fades out under it.
    pub(crate) fn jump(&mut self, to: f64) {
        let from = self.position();
        if self.audible() && to != from {
            // A jump during a crossfade restarts it from the head heard most
            // recently.
            self.tail = Some(Tail {
                position: from,
                step: self.step,
                left: self.xfade,
            });
        }
        self.resampling = false;
        self.position = to;
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
                self.stop();
                None
            }
            Pending::Cue => {
                self.stop();
                self.resampling = false;
                self.position = self.cue;
                None
            }
            Pending::Load(src) => {
                self.stop();
                self.resampling = false;
                self.position = 0.0;
                self.cue = 0.0;
                std::mem::replace(&mut self.src, src)
            }
        }
    }

    pub(crate) fn frames(&self) -> i64 {
        self.src.as_ref().map_or(0, |s| s.frames())
    }

    /// The current head's frame at `step`, switching between copying and
    /// resampling as the step crosses 1.0.
    fn read(&mut self, src: &DeckSource) -> (f32, f32) {
        if self.step == 1.0 {
            if self.resampling {
                // Back to copying: round onto a whole frame, crossfading
                // from the resampled head if that moves it.
                let at = self.vs.position();
                self.resampling = false;
                self.position = at;
                if at.fract() != 0.0 {
                    self.jump(at.round());
                }
            }
            let out = src.frame(self.position as i64);
            self.position += 1.0;
            out
        } else {
            if !self.resampling {
                self.vs.jump(src, self.position, self.step);
                self.resampling = true;
            }
            self.vs.next(src, self.step)
        }
    }

    /// Next stereo frame; may hand back a source to retire.
    pub(crate) fn next(&mut self, retired: &mut Option<Arc<DeckSource>>) -> (f32, f32) {
        if !self.playing {
            return (0.0, 0.0);
        }
        let gate = self.gate.next();
        // Out for the frame, so the head can borrow `self` mutably.
        let src = self.src.take();
        let out = match &src {
            Some(src) if self.position() < src.frames() as f64 => {
                let (mut l, mut r) = self.read(src);
                if let Some(t) = &mut self.tail {
                    // x runs 1 -> 0: old head cos-out, new head sin-in.
                    let x = t.left as f32 / self.xfade as f32;
                    let (old, new) = ((x * FRAC_PI_2).sin(), (x * FRAC_PI_2).cos());
                    let (ol, or) = src.frame_lerp(t.position);
                    l = l * new + ol * old;
                    r = r * new + or * old;
                    t.position += t.step;
                    t.left -= 1;
                    if t.left == 0 {
                        self.tail = None;
                    }
                }
                (l * gate, r * gate)
            }
            _ => {
                // End of track. A pending load still runs below.
                if matches!(self.pending, Pending::None | Pending::Pause | Pending::Cue) {
                    self.stop();
                    self.pending = Pending::None;
                }
                self.tail = None;
                self.gate.value = 0.0;
                (0.0, 0.0)
            }
        };
        self.src = src;
        if self.gate.settled() && self.gate.value == 0.0 {
            let action = std::mem::take(&mut self.pending);
            if let Some(old) = self.run(action) {
                // At most one per frame; the caller drains it every frame.
                *retired = Some(old);
            }
        }
        out
    }

    pub(crate) fn state(&self) -> DeckState {
        let position = self.position();
        let beats = self.src.as_ref().map(|s| s.beats());
        let speed = self.step / self.rate_ratio();
        let bpm = beats
            .and_then(|b| b.period_at(position))
            .zip(self.src.as_ref())
            .map(|(p, s)| 60.0 * f64::from(s.sample_rate()) / p * speed);
        DeckState {
            loaded: self.src.is_some(),
            playing: self.playing,
            position: position.floor() as i64,
            frames: self.frames(),
            seeks: self.seeks,
            transports: self.transports,
            cue: self.cue.round() as i64,
            tempo: self.tempo,
            nudge: self.nudge,
            speed,
            bpm,
            beat: beats.and_then(|b| b.beat_at(position)),
            sync: self.sync,
            sync_bar: self.sync_bar,
            quantise: self.quantise,
        }
    }
}
