//! Key lock (master tempo) read head: plays a source at any speed with its
//! pitch kept, through `timestretch`'s real-time Keylock engine (D21).
//!
//! The engine runs its own varispeed head plus a pitch corrector and delays
//! the output by a fixed pipeline latency (≈ 12.7 ms at 44.1 kHz). A jump
//! primes it synchronously: it resets, is fed from a preroll before the
//! target and runs with the output thrown away until the frame about to
//! leave it is the target. So what is heard lines up with [`KeyLock::position`]
//! and the deck needs no latency compensation. The position comes from the
//! engine's own source clock, so it stays exact through tempo changes.
//!
//! Built off the audio thread ([`KeyLock::new`] allocates); everything else
//! is real-time safe (the engine measured no allocation in this profile,
//! `docs/research.md` §9.3).

use timestretch::engine::{
    Engine, EngineConfig, EngineController, EngineProcessor, EngineProfile, SourceProducer,
    MAX_TEMPO_RATE, MIN_TEMPO_RATE,
};

use crate::source::DeckSource;

/// Output frames per engine call: speed changes take effect within it.
/// The engine's smallest block.
const CHUNK: usize = 64;
/// Largest engine call (priming runs in these).
const MAX_BLOCK: usize = 1024;
/// Source frames pushed into the engine's ring per feed step.
const FEED: usize = 1024;
/// Fastest step the ring is kept fed for.
const FEED_RATE: f64 = 2.0;

/// Steps the key lock head plays; outside them the deck uses varispeed.
pub(crate) fn in_range(step: f64) -> bool {
    (MIN_TEMPO_RATE..=FEED_RATE.min(MAX_TEMPO_RATE)).contains(&step)
}

/// A pitch-keeping read head over a [`DeckSource`].
pub(crate) struct KeyLock {
    controller: EngineController,
    processor: EngineProcessor,
    source: SourceProducer,
    /// Interleaved feed scratch.
    feed_buf: Box<[f32]>,
    /// The last chunk produced, interleaved.
    out: Box<[f32]>,
    /// Next frame of `out` to play; `CHUNK` when it is used up.
    next: usize,
    /// Next source frame to push into the engine.
    feed: i64,
    /// Source frame of the engine's position 0 (the first pushed after a reset).
    origin: f64,
    /// Source time of `out[0]`.
    start: f64,
    /// Source time of the frame after `out`.
    end: f64,
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
    /// Builds the engine and its buffers; `None` if `timestretch` rejects the
    /// rate (it takes 8–192 kHz).
    pub(crate) fn new(sample_rate: u32) -> Option<Self> {
        let handles = Engine::build(EngineConfig {
            sample_rate,
            channels: 2,
            profile: EngineProfile::Keylock,
            initial_tempo_rate: 1.0,
            max_block_frames: MAX_BLOCK,
            ..EngineConfig::default()
        })
        .ok()?;
        Some(Self {
            controller: handles.controller,
            processor: handles.processor,
            source: handles.source,
            feed_buf: vec![0.0; FEED * 2].into_boxed_slice(),
            out: vec![0.0; MAX_BLOCK * 2].into_boxed_slice(),
            next: CHUNK,
            feed: 0,
            origin: 0.0,
            start: 0.0,
            end: 0.0,
            step: 1.0,
        })
    }

    /// Source time of the next frame [`Self::next`] returns.
    pub(crate) fn position(&self) -> f64 {
        self.start + (self.end - self.start) * self.next as f64 / CHUNK as f64
    }

    /// Source time of the frame about to leave the engine.
    fn heard(&self) -> f64 {
        self.origin + self.controller.source_position()
    }

    /// Restarts at source time `at`, playing on at `step`.
    pub(crate) fn jump(&mut self, src: &DeckSource, at: f64, step: f64) {
        self.processor.reset();
        self.set_step(step);
        self.controller.set_tempo_rate(step);
        let preroll = self.processor.warm_start_preroll_frames()
            + self.processor.varispeed_lookahead_frames();
        self.feed = at.floor() as i64 - preroll as i64;
        self.origin = self.feed as f64;
        // Bounded: the preroll and the pipeline delay, in output frames,
        // with room to spare.
        let most = (preroll as f64 / step) as usize + self.processor.pipeline_latency_frames();
        let mut run = 0;
        while run <= 2 * most + MAX_BLOCK {
            let left = (at - self.heard()) / step;
            if left < 0.5 {
                break;
            }
            let n = (left.round() as usize).clamp(1, MAX_BLOCK);
            self.process(src, n);
            run += n;
        }
        self.start = self.heard();
        self.end = self.start;
        self.next = CHUNK;
    }

    /// Next stereo frame; `step` applies from the next chunk on.
    pub(crate) fn next(&mut self, src: &DeckSource, step: f64) -> (f32, f32) {
        if self.next >= CHUNK {
            if step != self.step {
                self.set_step(step);
                self.controller.set_tempo_rate(self.step);
            }
            self.start = self.heard();
            self.process(src, CHUNK);
            self.end = self.heard();
            self.next = 0;
        }
        let i = self.next * 2;
        self.next += 1;
        (self.out[i], self.out[i + 1])
    }

    fn set_step(&mut self, step: f64) {
        self.step = step.clamp(MIN_TEMPO_RATE, FEED_RATE);
    }

    /// Tops the engine's ring up, then runs `frames` frames into `out`.
    fn process(&mut self, src: &DeckSource, frames: usize) {
        let demand = self.source.demand_hint(frames, FEED_RATE)
            + self.processor.varispeed_lookahead_frames();
        while self.source.occupied_frames() < demand {
            let n = self.source.free_frames().min(FEED);
            if n == 0 {
                break;
            }
            let chunk = &mut self.feed_buf[..n * 2];
            src.read_f32(self.feed, chunk);
            let pushed = self.source.push(chunk);
            self.feed += pushed as i64;
        }
        self.processor.process(&mut self.out[..frames * 2]);
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
