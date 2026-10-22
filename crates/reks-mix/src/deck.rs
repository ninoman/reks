//! One deck: transport, CDJ cue, hot cues, quantise, beat jump, beat loops
//! and varispeed.
//!
//! At exactly unit step (speed 1.0 and the source at the engine's rate) the
//! deck copies source frames, so playback is bit-exact; at any other step it
//! reads through [`Varispeed`]. The graph sets the speed before every block
//! (tempo × nudge, or what sync asks for). Slower than the resampler goes
//! (WIDE tempo near −100%) the deck fades out and holds its position.
//!
//! Runs on the audio thread: no allocation, locks or I/O after [`Deck::new`].

use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

use crate::control::Control;
use crate::graph::{
    DeckState, LoopRegion, Ramp, FINE_JUMP_MS, LOOP_MAX_BEATS, LOOP_MIN_BEATS, LOOP_XFADE_MS,
    SEEK_XFADE_MS, TEMPO_MAX, TEMPO_MIN,
};
use crate::source::{DeckSource, HOT_CUE_SLOTS};
use crate::varispeed::{Varispeed, MIN_STEP};

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
    /// Frames of the crossfade still to play, `len..=1`.
    left: u32,
    /// Length of this crossfade in frames.
    len: u32,
}

/// An active loop, in source frames: `end` is `beats` beats after `start`
/// on the grid.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Loop {
    start: f64,
    end: f64,
    beats: f64,
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
    /// Too slow to resample: the head stands still once `motion` is silent.
    held: bool,
    /// Fades the output out into a hold and back in after it.
    motion: Ramp,
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
    /// [`LOOP_XFADE_MS`] in frames.
    loop_xfade: u32,
    looping: Option<Loop>,
    cue: f64,
    /// The loaded track's hot cues by slot: the source's at load, then as
    /// edited ([`Control::SetHotCue`]).
    hot_cues: [Option<i64>; HOT_CUE_SLOTS as usize],
    /// CUE is held from the cue point; letting go returns there.
    cue_preview: bool,
    pub(crate) tempo: f64,
    pub(crate) nudge: f64,
    pub(crate) sync: bool,
    pub(crate) sync_bar: bool,
    quantise: bool,
    /// Play order, for the automatic sync leader; 0 when stopped.
    pub(crate) started: u64,
    /// Sync is following a leader this block.
    pub(crate) locked: bool,
    /// While locked: the tempo ratio that matches the master, without phase
    /// bend or nudge. A deck that takes over as master keeps it.
    pub(crate) matched: Option<f64>,
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
            held: false,
            motion: Ramp::new(1.0, sample_rate),
            playing: false,
            gate: Ramp::new(0.0, sample_rate),
            pending: Pending::None,
            seeks: 0,
            transports: 0,
            tail: None,
            xfade: (sample_rate * SEEK_XFADE_MS / 1000).max(1),
            loop_xfade: (sample_rate * LOOP_XFADE_MS / 1000).max(1),
            looping: None,
            cue: 0.0,
            hot_cues: [None; HOT_CUE_SLOTS as usize],
            cue_preview: false,
            tempo: 1.0,
            nudge: 1.0,
            sync: false,
            sync_bar: false,
            quantise: false,
            started: 0,
            locked: false,
            matched: None,
        })
    }

    pub(crate) fn src(&self) -> Option<&DeckSource> {
        self.src.as_deref()
    }

    pub(crate) fn playing(&self) -> bool {
        self.playing
    }

    /// Source frames per output frame for the current block (0 while held).
    pub(crate) fn step(&self) -> f64 {
        if self.held {
            0.0
        } else {
            self.step
        }
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
        let step = speed * self.rate_ratio();
        self.held = step < MIN_STEP;
        self.motion.target = if self.held { 0.0 } else { 1.0 };
        self.step = crate::varispeed::clamp_step(step);
    }

    /// Length of the active loop in beats.
    pub(crate) fn loop_beats(&self) -> Option<f64> {
        self.looping.map(|l| l.beats)
    }

    /// Sets the tempo fader, clamped to the tempo range.
    pub(crate) fn set_tempo(&mut self, r: f64) {
        self.tempo = ratio(r);
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
                // PLAY during a cue preview keeps it playing on release.
                self.cue_preview = false;
                self.play(clock);
                None
            }
            Control::Pause { .. } => {
                self.transports = self.transports.wrapping_add(1);
                self.cue_preview = false;
                if self.audible() && matches!(self.pending, Pending::None) {
                    self.pending = Pending::Pause;
                    self.gate.target = 0.0;
                } else if !self.audible() {
                    self.stop();
                }
                None
            }
            Control::Cue { .. } => {
                self.cue_button(clock);
                None
            }
            Control::CueRelease { .. } => {
                if std::mem::take(&mut self.cue_preview) && self.playing {
                    self.return_to_cue();
                }
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
            Control::SetHotCue { slot, frame, .. } => {
                let loading = matches!(self.pending, Pending::Load(_));
                let frames = self.frames();
                if let Some(c) = self
                    .hot_cues
                    .get_mut(usize::from(slot))
                    .filter(|_| !loading)
                {
                    *c = frame.map(|f| f.clamp(0, frames));
                }
                None
            }
            Control::CallCue { frame, .. } => {
                self.call_cue(frame);
                None
            }
            Control::Tempo { ratio: r, .. } => {
                self.set_tempo(r);
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
            Control::BeatJump { beats, .. } => {
                self.beat_jump(beats);
                None
            }
            Control::FineJump { forward, .. } => {
                self.fine_jump(forward);
                None
            }
            Control::BeatLoop { beats, .. } => {
                self.beat_loop(beats);
                None
            }
            Control::LoopExit { .. } => {
                self.looping = None;
                None
            }
            Control::LoopResize { double, .. } => {
                self.resize_loop(double);
                None
            }
            Control::TakeMaster { .. }
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
        self.cue_preview = false;
        self.started = 0;
        self.tail = None;
        self.gate.value = 0.0;
        self.gate.target = 0.0;
    }

    /// CDJ cue: playing, back to the cue point and pause; paused on the cue
    /// point, play until CUE is let go; paused elsewhere, the cue point
    /// moves here (to the nearest beat with quantise) and so does the head.
    fn cue_button(&mut self, clock: u64) {
        if matches!(self.pending, Pending::Load(_)) || self.src.is_none() {
            return;
        }
        if self.playing {
            self.cue_preview = false;
            self.return_to_cue();
        } else if (self.position() - self.cue).abs() < 1.0 {
            self.play(clock);
            self.cue_preview = self.playing;
        } else {
            let here = self.position();
            self.cue = match self.src.as_ref() {
                Some(s) if self.quantise => s.beats().snap(here),
                _ => here,
            }
            .clamp(0.0, self.frames() as f64);
            self.leave_loop_unless(self.cue);
            self.jump(self.cue);
        }
    }

    /// Back to the cue point and pause, after a fade-out when audible.
    fn return_to_cue(&mut self) {
        if self.audible() {
            self.pending = Pending::Cue;
            self.gate.target = 0.0;
        } else {
            self.run(Pending::Cue);
        }
    }

    /// Jumps to hot cue `slot` and plays. With quantise while playing, the
    /// jump keeps the beat phase; otherwise it lands on the cue exactly.
    fn hot_cue(&mut self, slot: u8, clock: u64) {
        let Some(src) = self.src.as_ref() else {
            return;
        };
        let Some(at) = self.hot_cues.get(usize::from(slot)).copied().flatten() else {
            return;
        };
        if matches!(self.pending, Pending::Load(_)) {
            return;
        }
        self.cue_preview = false;
        let at = at as f64;
        let to = if self.quantise && self.playing {
            src.beats().keep_phase(self.position(), at)
        } else {
            at
        };
        let to = to.clamp(0.0, self.frames() as f64);
        self.leave_loop_unless(to);
        self.jump(to);
        self.play(clock);
    }

    /// Memory cue call: cue point and head to `frame`, exactly; playing on.
    fn call_cue(&mut self, frame: i64) {
        if matches!(self.pending, Pending::Load(_)) || self.src.is_none() {
            return;
        }
        self.cue_preview = false;
        self.cue = (frame as f64).clamp(0.0, self.frames() as f64);
        self.leave_loop_unless(self.cue);
        self.jump(self.cue);
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
        let to = to.clamp(0.0, self.frames() as f64);
        self.leave_loop_unless(to);
        self.jump(to);
    }

    /// A jump out of the active loop ends it (seek, hot cue, cue).
    fn leave_loop_unless(&mut self, to: f64) {
        if self
            .looping
            .is_some_and(|l| !(l.start..l.end).contains(&to))
        {
            self.looping = None;
        }
    }

    /// The grid, when the deck can take a beat jump or loop now.
    fn grid(&self) -> Option<&crate::beatmap::BeatMap> {
        if matches!(self.pending, Pending::Load(_)) {
            return None;
        }
        Some(self.src.as_ref()?.beats()).filter(|b| !b.is_empty())
    }

    /// `frame` moved by `beats`: along the grid with quantise (so the place
    /// in the beat is kept on dynamic grids too), else by `beats` of the
    /// local beat length.
    fn beats_from(&self, frame: f64, beats: f64) -> Option<f64> {
        let map = self.grid()?;
        if self.quantise {
            map.frame_at(map.beat_at(frame)? + beats)
        } else {
            Some(frame + beats * map.period_at(frame)?)
        }
    }

    /// Beat jump; ignored when it would leave the track. The active loop
    /// moves with the head.
    fn beat_jump(&mut self, beats: f64) {
        if !beats.is_finite() || beats == 0.0 {
            return;
        }
        let frames = self.frames() as f64;
        let Some(to) = self.beats_from(self.position(), beats) else {
            return;
        };
        if !(0.0..frames).contains(&to) {
            return;
        }
        if let Some(l) = self.looping {
            let start = self.beats_from(l.start, beats);
            let end = start.and_then(|s| self.loop_end(s, l.beats));
            let (Some(start), Some(end)) = (start, end) else {
                return;
            };
            self.looping = Some(Loop { start, end, ..l });
        }
        self.jump(to);
    }

    /// FINE jump: a fixed time, with or without a grid.
    fn fine_jump(&mut self, forward: bool) {
        if matches!(self.pending, Pending::Load(_)) {
            return;
        }
        let Some(src) = self.src.as_ref() else {
            return;
        };
        let by = f64::from(src.sample_rate() * FINE_JUMP_MS) / 1000.0;
        let to = self.position() + if forward { by } else { -by };
        self.jump(to.clamp(0.0, self.frames() as f64));
    }

    /// End of a loop of `beats` from `start`; `None` past the track.
    fn loop_end(&self, start: f64, beats: f64) -> Option<f64> {
        let map = self.grid()?;
        let end = map.frame_at(map.beat_at(start)? + beats)?;
        (start >= 0.0 && end <= self.frames() as f64).then_some(end)
    }

    /// Starts a loop of `beats` at the head, or with quantise at the last
    /// grid step of `beats` (whole beats for loops of a beat or more).
    fn beat_loop(&mut self, beats: f64) {
        if !beats.is_finite() {
            return;
        }
        let beats = beats.clamp(LOOP_MIN_BEATS, LOOP_MAX_BEATS);
        let here = self.position();
        let start = match self.grid() {
            Some(map) if self.quantise => {
                let q = beats.min(1.0);
                let Some(b) = map.beat_at(here) else {
                    return;
                };
                // Rounding must not put a head sitting on a step a step back.
                let step = (b / q + 1e-9).floor();
                // Before the track starts, begin at the next step instead.
                [step, step + 1.0]
                    .into_iter()
                    .filter_map(|s| map.frame_at(s * q))
                    .find(|&f| f >= 0.0)
            }
            Some(_) => Some(here),
            None => None,
        };
        let Some(start) = start else {
            return;
        };
        if let Some(end) = self.loop_end(start, beats) {
            self.looping = Some(Loop { start, end, beats });
        }
    }

    /// Halves or doubles the active loop, keeping its start.
    fn resize_loop(&mut self, double: bool) {
        let Some(l) = self.looping else {
            return;
        };
        let beats = if double { l.beats * 2.0 } else { l.beats / 2.0 };
        if !(LOOP_MIN_BEATS..=LOOP_MAX_BEATS).contains(&beats) {
            return;
        }
        if let Some(end) = self.loop_end(l.start, beats) {
            self.looping = Some(Loop { end, beats, ..l });
        }
    }

    /// At or past the loop end: back by whole loop lengths, crossfading
    /// over [`LOOP_XFADE_MS`] from the head running on past the end.
    fn wrap_loop(&mut self) {
        let Some(l) = self.looping else {
            return;
        };
        let at = self.position();
        if at >= l.end {
            // Exact in floating point, so loop length and sync do not drift.
            let to = l.start + (at - l.start).rem_euclid(l.end - l.start);
            self.jump_with(to, self.loop_xfade);
        }
    }

    /// Moves the head now; while audible, the old head fades out under it.
    pub(crate) fn jump(&mut self, to: f64) {
        self.jump_with(to, self.xfade);
    }

    fn jump_with(&mut self, to: f64, xfade: u32) {
        let from = self.position();
        if self.audible() && to != from {
            // A jump during a crossfade restarts it from the head heard most
            // recently.
            self.tail = Some(Tail {
                position: from,
                step: self.step,
                left: xfade,
                len: xfade,
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
                self.leave_loop_unless(self.cue);
                self.resampling = false;
                self.position = self.cue;
                None
            }
            Pending::Load(src) => {
                self.stop();
                self.resampling = false;
                let start = src.as_ref().map_or(0, |s| s.start()) as f64;
                self.position = start;
                self.cue = start;
                self.looping = None;
                for (slot, c) in (0..HOT_CUE_SLOTS).zip(&mut self.hot_cues) {
                    *c = src.as_ref().and_then(|s| s.hot_cue(slot));
                }
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
        let gate = self.gate.next() * self.motion.next();
        // Out for the frame, so the head can borrow `self` mutably.
        let src = self.src.take();
        let out = match &src {
            Some(_) if self.held && self.motion.value == 0.0 => {
                // Standing still: nothing to hear, nothing to fade.
                self.tail = None;
                (0.0, 0.0)
            }
            Some(src) if self.position() < src.frames() as f64 => {
                let (mut l, mut r) = self.read(src);
                if let Some(t) = &mut self.tail {
                    // x runs 1 -> 0: old head cos-out, new head sin-in.
                    let x = t.left as f32 / t.len as f32;
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
        // After the read, so the head never rests on the loop end.
        self.wrap_loop();
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
        let speed = self.step() / self.rate_ratio();
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
            cue_preview: self.cue_preview,
            sync: self.sync,
            sync_bar: self.sync_bar,
            locked: self.locked,
            quantise: self.quantise,
            master: false,
            loop_region: self.looping.map(|l| LoopRegion {
                start: l.start.round() as i64,
                end: l.end.round() as i64,
                beats: l.beats,
            }),
        }
    }
}
