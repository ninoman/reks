//! Key lock (master tempo) read head: plays a source at any speed with its
//! pitch kept, through Signalsmith Stretch (MIT, C++, default preset; D21,
//! chosen by listening in phase 5 task 10).
//!
//! The engine is a phase vocoder: its processing time trails the input fed
//! by an input latency, and its output trails the processing time by an
//! output latency (≈ 60 ms). A jump primes it synchronously: it resets,
//! seeks with the history before the target and runs one output latency
//! with the output thrown away, so the next frame out is the target. So what
//! is heard lines up with [`KeyLock::position`] and the deck needs no latency
//! compensation. The position comes from a short history of processing
//! times, so it stays exact through tempo changes.
//!
//! Built off the audio thread ([`KeyLock::new`] allocates, in Rust and C++);
//! everything else is real-time safe: the library sizes every buffer when it
//! is configured (`docs/research.md` §9.3; its C++ heap is not visible to the
//! no-allocation test).

use signalsmith_stretch::Stretch;

use crate::source::DeckSource;

/// Output frames per engine call: speed changes take effect within it.
const CHUNK: usize = 64;
/// Step range the key lock head plays; outside it the deck uses varispeed.
const MIN_STEP: f64 = 0.25;
const MAX_STEP: f64 = 2.0;
/// Seek history: the default preset's block (120 ms) plus one interval
/// (30 ms), which the library asks for.
const SEEK_MS: usize = 150;

/// Steps the key lock head plays; outside them the deck uses varispeed.
pub(crate) fn in_range(step: f64) -> bool {
    (MIN_STEP..=MAX_STEP).contains(&step)
}

/// A pitch-keeping read head over a [`DeckSource`].
pub(crate) struct KeyLock {
    stretch: Stretch,
    /// Source frames fed beyond the engine's processing time.
    in_latency: usize,
    /// Output frames between the processing time and what leaves the engine.
    out_latency: usize,
    seek_frames: usize,
    /// Interleaved input scratch: the seek history or one chunk's input.
    in_buf: Box<[f32]>,
    /// The last chunk produced, interleaved.
    out: Box<[f32]>,
    /// Next frame of `out` to play; `CHUNK` when it is used up.
    next: usize,
    /// Next source frame to feed.
    feed: i64,
    /// Source time of the engine's processing time.
    time: f64,
    /// Processing time at the start of each recent chunk, by chunk number
    /// modulo its length: enough to look back one output latency.
    times: Box<[f64]>,
    /// Chunks produced since the last jump.
    chunks: usize,
    step: f64,
}

impl std::fmt::Debug for KeyLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyLock")
            .field("position", &self.position())
            .field("step", &self.step)
            .finish()
    }
}

impl KeyLock {
    /// Builds the engine and its buffers; `None` outside 8–192 kHz.
    pub(crate) fn new(sample_rate: u32) -> Option<Self> {
        if !(8_000..=192_000).contains(&sample_rate) {
            return None;
        }
        let stretch = Stretch::preset_default(2, sample_rate);
        let in_latency = stretch.input_latency();
        let out_latency = stretch.output_latency();
        let seek_frames = sample_rate as usize * SEEK_MS / 1000;
        let chunk_in = (CHUNK as f64 * MAX_STEP).ceil() as usize + 1;
        Some(Self {
            stretch,
            in_latency,
            out_latency,
            seek_frames,
            in_buf: vec![0.0; seek_frames.max(chunk_in) * 2].into_boxed_slice(),
            out: vec![0.0; CHUNK * 2].into_boxed_slice(),
            next: CHUNK,
            feed: 0,
            time: 0.0,
            times: vec![0.0; out_latency / CHUNK + 4].into_boxed_slice(),
            chunks: 0,
            step: 1.0,
        })
    }

    /// Source time of the next frame [`Self::next`] returns: the processing
    /// time one output latency of frames ago.
    pub(crate) fn position(&self) -> f64 {
        let played = (self.chunks * CHUNK + self.next).saturating_sub(CHUNK);
        let q = played.saturating_sub(self.out_latency);
        let (c, j) = (q / CHUNK, q % CHUNK);
        let at = |c: usize| self.times[c % self.times.len()];
        if j == 0 {
            at(c)
        } else {
            at(c) + (at(c + 1) - at(c)) * j as f64 / CHUNK as f64
        }
    }

    /// Restarts at source time `at`, playing on at `step`.
    pub(crate) fn jump(&mut self, src: &DeckSource, at: f64, step: f64) {
        self.set_step(step);
        self.stretch.reset();
        // The processing time is the input's end less the input latency.
        self.feed = at.floor() as i64 + self.in_latency as i64;
        let history = &mut self.in_buf[..self.seek_frames * 2];
        src.read_f32(self.feed - self.seek_frames as i64, history);
        self.stretch.seek(&*history, self.step);
        self.time = at;
        self.chunks = 0;
        self.times[0] = at;
        // Throw away one output latency; the frame after it is `at`.
        for _ in 0..=self.out_latency / CHUNK {
            self.process(src);
        }
        self.next = self.out_latency % CHUNK;
    }

    /// Next stereo frame; `step` applies from the next chunk on.
    pub(crate) fn next(&mut self, src: &DeckSource, step: f64) -> (f32, f32) {
        if self.next >= CHUNK {
            if step != self.step {
                self.set_step(step);
            }
            self.process(src);
            self.next = 0;
        }
        let i = self.next * 2;
        self.next += 1;
        (self.out[i], self.out[i + 1])
    }

    fn set_step(&mut self, step: f64) {
        self.step = step.clamp(MIN_STEP, MAX_STEP);
    }

    /// Feeds one chunk's worth of input at the step and runs one chunk out.
    fn process(&mut self, src: &DeckSource) {
        let time = self.time + self.step * CHUNK as f64;
        let end = time.floor() as i64 + self.in_latency as i64;
        let n = (end - self.feed).max(0) as usize;
        let input = &mut self.in_buf[..n * 2];
        src.read_f32(self.feed, input);
        self.stretch.process(&*input, &mut self.out[..]);
        self.feed += n as i64;
        self.time = time;
        self.chunks += 1;
        let len = self.times.len();
        self.times[self.chunks % len] = time;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn sine(hz: f64, frames: usize) -> DeckSource {
        let pcm = (0..frames)
            .flat_map(|i| {
                let v = (i as f64 * 2.0 * std::f64::consts::PI * hz / f64::from(RATE)).sin();
                let v = (v * 16_000.0) as i16;
                [v, v]
            })
            .collect();
        DeckSource::new(pcm, RATE)
    }

    /// Silence with a 10 ms Hann-windowed 2 kHz burst centred on `centre`.
    fn burst(centre: usize, frames: usize) -> DeckSource {
        let len = RATE as usize / 100;
        let start = centre - len / 2;
        let mut pcm = vec![0i16; frames * 2];
        for i in 0..len {
            let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / len as f64).cos();
            let x = w * (2.0 * std::f64::consts::PI * 2_000.0 * i as f64 / f64::from(RATE)).sin();
            let v = (x * 16_000.0) as i16;
            pcm[(start + i) * 2] = v;
            pcm[(start + i) * 2 + 1] = v;
        }
        DeckSource::new(pcm, RATE)
    }

    /// Output frame at the centre of the loudest 5 ms window.
    fn envelope_peak(left: &[f32]) -> usize {
        let win = RATE as usize / 200;
        let sq: Vec<f32> = left.iter().map(|x| x * x).collect();
        let mut sum: f32 = sq[..win].iter().sum();
        let (mut best, mut at) = (sum, 0);
        for i in win..sq.len() {
            sum += sq[i] - sq[i - win];
            if sum > best {
                best = sum;
                at = i + 1 - win;
            }
        }
        at + win / 2
    }

    /// What is heard after a jump is the source at the jump's target: a
    /// burst 200 ms past it comes out 200 ms / step later, so the deck
    /// needs no latency compensation.
    #[test]
    fn the_sound_lines_up_with_the_position() {
        let centre = RATE as usize * 2;
        let src = burst(centre, RATE as usize * 4);
        let mut k = KeyLock::new(RATE).unwrap();
        let lead = f64::from(RATE) * 0.2;
        for step in [1.0, 0.94, 1.06] {
            k.jump(&src, centre as f64 - lead, step);
            let left: Vec<f32> = (0..RATE as usize / 2)
                .map(|_| k.next(&src, step).0)
                .collect();
            let off = envelope_peak(&left) as f64 - lead / step;
            // 1 ms: the analysis hop smears where the burst lands.
            assert!(off.abs() < f64::from(RATE) * 0.001, "{step}: {off} frames");
        }
    }

    #[test]
    fn jump_lands_on_the_target_and_the_clock_runs_at_the_step() {
        let src = sine(440.0, RATE as usize * 4);
        let mut k = KeyLock::new(RATE).unwrap();
        for step in [1.0, 1.06, 0.94] {
            k.jump(&src, 48_000.0, step);
            assert!(
                (k.position() - 48_000.0).abs() <= step,
                "{step}: {}",
                k.position()
            );
            for _ in 0..24_000 {
                k.next(&src, step);
            }
            let want = 48_000.0 + 24_000.0 * step;
            assert!(
                (k.position() - want).abs() < 2.0,
                "{step}: {}",
                k.position()
            );
        }
    }
}
