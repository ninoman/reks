//! One deck: transport, CDJ cue, hot cues, quantise, beat jump, beat loops
//! and varispeed.
//!
//! At exactly unit step (speed 1.0 and the source at the engine's rate) the
//! deck copies source frames, so playback is bit-exact; at any other step it
//! reads through [`Varispeed`], so pitch follows tempo. With master tempo on
//! it reads through [`KeyLock`] instead, at any step it covers, so the key
//! stays put. The graph sets the speed before every block
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
use crate::keylock::{self, KeyLock};
use crate::source::{DeckSource, AUTO_LOOP_SLOTS, HOT_CUE_SLOTS};
use crate::varispeed::{Varispeed, MIN_STEP};

/// The read head a deck plays through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Head {
    /// Whole source frames at unit step.
    Copy,
    /// [`Varispeed`]: pitch follows tempo.
    Varispeed,
    /// [`KeyLock`]: master tempo, the key stays put.
    KeyLock,
}

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
    /// The fade-out was rendered into [`Deck::tail_buf`] (from key lock,
    /// whose output is not the source read at `position`).
    baked: bool,
    /// Frames of the crossfade still to play, `len..=1`.
    left: u32,
    /// Length of this crossfade in frames.
    len: u32,
}

/// A loop, in source frames: `end` is `beats` beats after `start` on the
/// grid (`None` without one).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Loop {
    start: f64,
    end: f64,
    beats: Option<f64>,
}

impl Loop {
    fn region(self) -> LoopRegion {
        LoopRegion {
            start: self.start.round() as i64,
            end: self.end.round() as i64,
            beats: self.beats,
        }
    }
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
    kl: KeyLock,
    /// The head playing; `vs` or `kl` holds the position while it plays.
    head: Head,
    /// Master tempo (key lock): tempo changes keep the key.
    master_tempo: bool,
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
    /// PLAY arrived while a load faded out the old track: the new one starts
    /// once it is in, with this play-order clock.
    play_after_load: Option<u64>,
    seeks: u32,
    transports: u32,
    tail: Option<Tail>,
    /// A baked [`Tail`], interleaved; the longest crossfade.
    tail_buf: Box<[f32]>,
    /// [`SEEK_XFADE_MS`] in frames.
    xfade: u32,
    /// [`LOOP_XFADE_MS`] in frames.
    loop_xfade: u32,
    looping: Option<Loop>,
    /// A pending loop IN, waiting for OUT ([`Control::LoopIn`]).
    loop_in: Option<f64>,
    /// The loop RELOOP enters: the last one left, or a memory loop called.
    last_loop: Option<Loop>,
    cue: f64,
    /// The loaded track's hot cues by slot: the source's at load, then as
    /// edited ([`Control::SetHotCue`]).
    hot_cues: [Option<i64>; HOT_CUE_SLOTS as usize],
    /// Active memory loops `(start, end)`: the source's at load, then as
    /// edited ([`Control::AutoLoop`]).
    auto_loops: [Option<(f64, f64)>; AUTO_LOOP_SLOTS],
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
    /// Allocates the varispeed and key lock heads; `None` if rubato or
    /// timestretch refuses its setup.
    pub(crate) fn new(sample_rate: u32) -> Option<Self> {
        let xfade = (sample_rate * SEEK_XFADE_MS / 1000).max(1);
        let loop_xfade = (sample_rate * LOOP_XFADE_MS / 1000).max(1);
        Some(Self {
            src: None,
            engine_rate: sample_rate.max(1),
            position: 0.0,
            vs: Varispeed::new()?,
            kl: KeyLock::new(sample_rate)?,
            head: Head::Copy,
            master_tempo: false,
            step: 1.0,
            held: false,
            motion: Ramp::new(1.0, sample_rate),
            playing: false,
            gate: Ramp::new(0.0, sample_rate),
            pending: Pending::None,
            play_after_load: None,
            seeks: 0,
            transports: 0,
            tail: None,
            tail_buf: vec![0.0; xfade.max(loop_xfade) as usize * 2].into_boxed_slice(),
            xfade,
            loop_xfade,
            looping: None,
            loop_in: None,
            last_loop: None,
            cue: 0.0,
            hot_cues: [None; HOT_CUE_SLOTS as usize],
            auto_loops: [None; AUTO_LOOP_SLOTS],
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
        match self.head {
            Head::Copy => self.position,
            Head::Varispeed => self.vs.position(),
            Head::KeyLock => self.kl.position(),
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

    /// Output frames per beat at the playhead and the current speed, for the
    /// channel echo; `None` without a grid or while held.
    pub(crate) fn beat_frames(&self) -> Option<f64> {
        let step = self.step();
        if step <= 0.0 {
            return None;
        }
        let period = self.src.as_ref()?.beats().period_at(self.position())?;
        Some(period / step)
    }

    /// Length of the active loop in beats; `None` without a loop or a grid.
    pub(crate) fn loop_beats(&self) -> Option<f64> {
        self.looping.and_then(|l| l.beats)
    }

    pub(crate) fn looping(&self) -> bool {
        self.looping.is_some()
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
            Control::Load { src, .. } => {
                self.play_after_load = None;
                self.change(Pending::Load(Some(src)))
            }
            Control::Unload { .. } => {
                self.play_after_load = None;
                self.change(Pending::Load(None))
            }
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
                self.play_after_load = None;
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
            Control::Seek { frame, .. } | Control::SeekExact { frame, .. } => {
                let exact = matches!(c, Control::SeekExact { .. });
                // Counted even when ignored, so the control side stays in step.
                self.seeks = self.seeks.wrapping_add(1);
                self.seek(frame as f64, exact);
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
            Control::SetGrid { src, .. } => {
                if matches!(self.pending, Pending::Load(_)) || self.src.is_none() {
                    return Some(src);
                }
                self.src.replace(src)
            }
            Control::AutoLoop { index, region, .. } => {
                let loading = matches!(self.pending, Pending::Load(_));
                let frames = self.frames() as f64;
                if let Some(slot) = self
                    .auto_loops
                    .get_mut(usize::from(index))
                    .filter(|_| !loading)
                {
                    *slot = region
                        .map(|(s, e)| (s as f64, (e as f64).min(frames)))
                        .filter(|(s, e)| *s >= 0.0 && e > s);
                }
                None
            }
            Control::CallCue { frame, end, .. } => {
                self.call_cue(frame, end);
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
            Control::MasterTempo { on, .. } => {
                // The head changes at the next frame, with a crossfade.
                self.master_tempo = on;
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
                self.exit_loop();
                None
            }
            Control::LoopIn { .. } => {
                self.loop_in_here();
                None
            }
            Control::LoopOut { .. } => {
                self.loop_out_here();
                None
            }
            Control::Reloop { .. } => {
                self.reloop();
                None
            }
            Control::LoopResize { double, .. } => {
                self.resize_loop(double);
                None
            }
            Control::TakeMaster { .. }
            | Control::Channel { .. }
            | Control::Filter { .. }
            | Control::ChannelFx { .. }
            | Control::FxParameter { .. }
            | Control::Crossfader { .. }
            | Control::Master { .. }
            | Control::Volume { .. } => None,
        }
    }

    fn play(&mut self, clock: u64) {
        if matches!(self.pending, Pending::Load(Some(_))) {
            self.play_after_load = Some(clock);
            return;
        }
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
    /// A memory loop (`end`) becomes the loop RELOOP enters.
    fn call_cue(&mut self, frame: i64, end: Option<i64>) {
        if matches!(self.pending, Pending::Load(_)) || self.src.is_none() {
            return;
        }
        self.cue_preview = false;
        let frames = self.frames() as f64;
        self.cue = (frame as f64).clamp(0.0, frames);
        self.leave_loop_unless(self.cue);
        self.jump(self.cue);
        if let Some(end) = end.map(|e| (e as f64).min(frames)) {
            if end - self.cue >= self.min_loop() {
                self.last_loop = Some(self.make_loop(self.cue, end));
            }
        }
    }

    /// Moves the head to `to`, clamped to the track. With quantise (and not
    /// `exact`), a jump while playing keeps the beat phase and one while
    /// paused lands on the nearest beat. Ignored while a load is pending: it
    /// is for the old track.
    fn seek(&mut self, to: f64, exact: bool) {
        if matches!(self.pending, Pending::Load(_)) {
            return;
        }
        let to = match self.src.as_ref() {
            Some(s) if self.quantise && !exact && self.playing => {
                s.beats().keep_phase(self.position(), to)
            }
            Some(s) if self.quantise && !exact => s.beats().snap(to),
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
            self.exit_loop();
        }
    }

    /// Leaves the active loop, keeping it for RELOOP.
    fn exit_loop(&mut self) {
        if let Some(l) = self.looping.take() {
            self.last_loop = Some(l);
        }
    }

    /// Shortest manual loop, in source frames: two loop crossfades.
    fn min_loop(&self) -> f64 {
        self.src.as_ref().map_or(f64::INFINITY, |s| {
            f64::from(s.sample_rate()) * f64::from(2 * LOOP_XFADE_MS) / 1000.0
        })
    }

    /// A loop from `start` to `end`, measured in beats when there is a grid.
    fn make_loop(&self, start: f64, end: f64) -> Loop {
        let beats = self
            .grid()
            .and_then(|m| Some(m.beat_at(end)? - m.beat_at(start)?));
        Loop { start, end, beats }
    }

    /// The head for loop IN / OUT: the nearest beat with quantise (the head
    /// itself without a grid), clamped to the track.
    fn loop_point(&self) -> f64 {
        let here = self.position();
        match self.src.as_ref() {
            Some(s) if self.quantise => s.beats().snap(here),
            _ => here,
        }
        .clamp(0.0, self.frames() as f64)
    }

    /// Loop IN: looping, moves the loop's start here (end kept); otherwise
    /// sets a pending IN for OUT.
    fn loop_in_here(&mut self) {
        if matches!(self.pending, Pending::Load(_)) || self.src.is_none() {
            return;
        }
        let at = self.loop_point();
        match self.looping {
            Some(l) => {
                if l.end - at >= self.min_loop() {
                    self.looping = Some(self.make_loop(at, l.end));
                }
            }
            None => self.loop_in = Some(at),
        }
    }

    /// Loop OUT: looping, moves the loop's end here (start kept); with a
    /// pending IN behind the head, loops from it to here at once. While
    /// paused the head goes to IN, so PLAY starts the loop.
    fn loop_out_here(&mut self) {
        if matches!(self.pending, Pending::Load(_)) || self.src.is_none() {
            return;
        }
        let frames = self.frames() as f64;
        let at = self.loop_point();
        if let Some(l) = self.looping {
            if at - l.start >= self.min_loop() {
                self.looping = Some(self.make_loop(l.start, at));
                // Snapping may have put the end behind the head.
                self.wrap_loop();
            }
            return;
        }
        let Some(start) = self.loop_in.filter(|&i| i < self.position()) else {
            return;
        };
        let end = if at > start {
            at
        } else {
            // Snapped back onto IN or before it: one beat after IN.
            let Some(e) = self
                .grid()
                .and_then(|m| m.frame_at(m.beat_at(start)? + 1.0))
            else {
                return;
            };
            e.min(frames)
        };
        if end - start < self.min_loop() {
            return;
        }
        self.loop_in = None;
        self.looping = Some(self.make_loop(start, end));
        if self.playing {
            self.wrap_loop();
        } else {
            self.jump(start);
        }
    }

    /// RELOOP: looping, leaves the loop as LOOP EXIT does; otherwise jumps
    /// back into the last loop and loops. With quantise while playing the
    /// jump keeps the beat phase, landing inside the loop when it is a beat
    /// or longer, else on its start.
    fn reloop(&mut self) {
        if self.looping.is_some() {
            self.exit_loop();
            return;
        }
        if matches!(self.pending, Pending::Load(_)) {
            return;
        }
        let (Some(l), Some(src)) = (self.last_loop, self.src.as_ref()) else {
            return;
        };
        self.cue_preview = false;
        let map = src.beats();
        let mut to = if self.quantise && self.playing {
            map.keep_phase(self.position(), l.start)
        } else {
            l.start
        };
        if to < l.start {
            to = map
                .beat_at(to)
                .and_then(|b| map.frame_at(b + 1.0))
                .unwrap_or(l.start);
        }
        if !(l.start..l.end).contains(&to) {
            to = l.start;
        }
        self.looping = Some(l);
        self.jump(to);
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
            let end = start.and_then(|s| match l.beats {
                Some(b) => self.loop_end(s, b),
                None => Some(s + l.end - l.start).filter(|&e| e <= frames),
            });
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
            self.looping = Some(Loop {
                start,
                end,
                beats: Some(beats),
            });
        }
    }

    /// Halves or doubles the active loop, keeping its start: along the grid,
    /// or by time without one.
    fn resize_loop(&mut self, double: bool) {
        let Some(l) = self.looping else {
            return;
        };
        let factor = if double { 2.0 } else { 0.5 };
        match l.beats {
            Some(b) => {
                let beats = b * factor;
                if !(LOOP_MIN_BEATS..=LOOP_MAX_BEATS).contains(&beats) {
                    return;
                }
                if let Some(end) = self.loop_end(l.start, beats) {
                    self.looping = Some(Loop {
                        end,
                        beats: Some(beats),
                        ..l
                    });
                }
            }
            None => {
                let end = l.start + (l.end - l.start) * factor;
                if end - l.start >= self.min_loop() && end <= self.frames() as f64 {
                    self.looping = Some(Loop { end, ..l });
                }
            }
        }
    }

    /// Enters an active memory loop whose start the head passed this frame,
    /// moving from `before` (`start` in `before..now`, so a head parked on
    /// the start enters as it plays). From then on it is an ordinary loop:
    /// [`Self::wrap_loop`] crossfades at its end, LOOP EXIT leaves it.
    fn enter_auto_loop(&mut self, before: f64) {
        if self.looping.is_some() {
            return;
        }
        let now = self.position();
        let hit = self
            .auto_loops
            .iter()
            .flatten()
            .find(|(s, _)| (before..now).contains(s))
            .copied();
        if let Some((start, end)) = hit {
            self.looping = Some(self.make_loop(start, end));
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
        if to != from {
            // A refcount, not a copy: the head is borrowed mutably below.
            let src = self.src.clone();
            self.fade_from(src.as_deref(), from, xfade);
        }
        self.head = Head::Copy;
        self.position = to;
    }

    /// While audible, fades the current head (at `from`) out under the one
    /// that plays next. A fade during a crossfade restarts it from the head
    /// heard most recently. Key lock's output is not the source at `from`,
    /// so its fade-out is rendered ahead, before the head is reused.
    fn fade_from(&mut self, src: Option<&DeckSource>, from: f64, xfade: u32) {
        if !self.audible() {
            return;
        }
        let baked = match src {
            Some(src) if self.head == Head::KeyLock => {
                let frames = (xfade as usize * 2).min(self.tail_buf.len());
                for f in self.tail_buf[..frames].as_chunks_mut::<2>().0 {
                    let (l, r) = self.kl.next(src, self.step);
                    *f = [l, r];
                }
                true
            }
            _ => false,
        };
        self.tail = Some(Tail {
            position: from,
            step: self.step,
            baked,
            left: xfade,
            len: xfade,
        });
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
                self.head = Head::Copy;
                self.position = self.cue;
                None
            }
            Pending::Load(src) => {
                self.stop();
                self.head = Head::Copy;
                self.position = 0.0;
                self.cue = 0.0;
                self.looping = None;
                self.loop_in = None;
                self.last_loop = None;
                for (slot, c) in (0..HOT_CUE_SLOTS).zip(&mut self.hot_cues) {
                    *c = src.as_ref().and_then(|s| s.hot_cue(slot));
                }
                let loops = src.as_ref().map_or(&[][..], |s| s.auto_loops());
                for (i, slot) in self.auto_loops.iter_mut().enumerate() {
                    *slot = loops.get(i).map(|&(s, e)| (s as f64, e as f64));
                }
                std::mem::replace(&mut self.src, src)
            }
        }
    }

    pub(crate) fn frames(&self) -> i64 {
        self.src.as_ref().map_or(0, |s| s.frames())
    }

    /// The head for the current step: key lock with master tempo where it
    /// covers the step (and the source is at the engine's rate, as the app
    /// loads it), else copying at unit step and resampling off it.
    fn wanted_head(&self) -> Head {
        if self.master_tempo && self.rate_ratio() == 1.0 && keylock::in_range(self.step) {
            Head::KeyLock
        } else if self.step == 1.0 {
            Head::Copy
        } else {
            Head::Varispeed
        }
    }

    /// The current head's frame at `step`, changing heads as the step or
    /// master tempo asks.
    fn read(&mut self, src: &DeckSource) -> (f32, f32) {
        let want = self.wanted_head();
        if want != self.head {
            self.switch_head(src, want);
        }
        match self.head {
            Head::Copy => {
                let out = src.frame(self.position as i64);
                self.position += 1.0;
                out
            }
            Head::Varispeed => self.vs.next(src, self.step),
            Head::KeyLock => self.kl.next(src, self.step),
        }
    }

    /// Moves playback onto `want` where the current head is. Copying to
    /// resampling is seamless (the same samples); back to copying rounds
    /// onto a whole frame and crossfades if that moves the head. Into or
    /// out of key lock the sound changes, so that crossfades too, unless a
    /// jump's crossfade is already running.
    fn switch_head(&mut self, src: &DeckSource, want: Head) {
        let at = self.position();
        let to = if want == Head::Copy { at.round() } else { at };
        let key_change = self.head == Head::KeyLock || want == Head::KeyLock;
        if to != at || (key_change && self.tail.is_none()) {
            self.fade_from(Some(src), at, self.xfade);
        }
        self.head = want;
        self.position = to;
        match want {
            Head::Copy => {}
            Head::Varispeed => self.vs.jump(src, to, self.step),
            Head::KeyLock => self.kl.jump(src, to, self.step),
        }
    }

    /// Next stereo frame; may hand back a source to retire.
    pub(crate) fn next(&mut self, retired: &mut Option<Arc<DeckSource>>) -> (f32, f32) {
        if !self.playing {
            return (0.0, 0.0);
        }
        let gate = self.gate.next() * self.motion.next();
        let before = self.position();
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
                    let (ol, or) = if t.baked {
                        let i = (t.len - t.left) as usize * 2;
                        (self.tail_buf[i], self.tail_buf[i + 1])
                    } else {
                        src.frame_lerp(t.position)
                    };
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
        if self.playing {
            self.enter_auto_loop(before);
        }
        // After the read, so the head never rests on the loop end.
        self.wrap_loop();
        if self.gate.settled() && self.gate.value == 0.0 {
            let action = std::mem::take(&mut self.pending);
            let loaded = matches!(action, Pending::Load(_));
            if let Some(old) = self.run(action) {
                // At most one per frame; the caller drains it every frame.
                *retired = Some(old);
            }
            if let Some(clock) = self.play_after_load.take().filter(|_| loaded) {
                self.play(clock);
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
            master_tempo: self.master_tempo,
            master: false,
            loop_region: self.looping.map(Loop::region),
            loop_in: self.loop_in.map(|f| f.round() as i64),
            last_loop: self.last_loop.map(Loop::region),
        }
    }
}
