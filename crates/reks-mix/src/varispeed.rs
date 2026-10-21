//! Varispeed read head: plays a source at any speed through `rubato`'s
//! asynchronous sinc resampler (D21: pitch follows tempo).
//!
//! The head keeps its own source-time clock: each output frame advances it by
//! `step` source frames, the same arithmetic rubato uses, so the reported
//! position matches what is heard to well under a sample, whatever the speed
//! history. A jump resets the resampler and runs it over the
//! [`SINC_LEN`] frames before the target, so the first frame heard has full
//! filter history and sits exactly on the target.
//!
//! Built off the audio thread ([`Varispeed::new`] allocates); everything else
//! is real-time safe.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Adjustable, Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};

use crate::source::DeckSource;

/// Output frames per resampler call: speed changes take effect within it.
pub(crate) const CHUNK: usize = 32;
/// Sinc filter length. 128 taps with Blackman-Harris keep aliasing below
/// −100 dB in the passband at a fraction of the cost of 256.
const SINC_LEN: usize = 128;
/// Steps (source frames per output frame) the resampler accepts:
/// `1 / MAX_STEP..=MAX_STEP`. Covers +100% tempo (WIDE) with a 4:1 rate
/// ratio, and down to −87.5% at 1:1; slower than that the deck stands
/// still ([`MIN_STEP`]). The cost per output frame does not depend on it.
pub(crate) const MAX_STEP: f64 = 8.0;
/// Below this step the deck holds its position instead of resampling.
pub(crate) const MIN_STEP: f64 = 1.0 / MAX_STEP;
/// Source time of rubato's first output after a reset, less one step, in
/// frames from the first frame fed. Rubato starts its index at
/// `-(SINC_LEN - 1)` and a kernel at index `i` is centred on
/// `i + SINC_LEN / 2 - 1`. Measured, and pinned by `impulse_lands_on_its_frame`.
const LEAD: f64 = -((SINC_LEN / 2) as f64);

/// A resampling read head over a [`DeckSource`].
pub(crate) struct Varispeed {
    rs: Async<f32>,
    /// Interleaved input scratch, sized for the largest call.
    input: Box<[f32]>,
    /// The last chunk produced, interleaved.
    out: Box<[f32]>,
    /// Next frame of `out` to play; `CHUNK` when it is used up.
    next: usize,
    /// Next source frame to feed rubato.
    feed: i64,
    /// Source time of the last frame produced.
    last: f64,
    /// Step of the chunk in `out`.
    step: f64,
}

impl std::fmt::Debug for Varispeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Varispeed")
            .field("position", &self.position())
            .field("step", &self.step)
            .finish()
    }
}

/// Clamps a step to what the resampler takes.
pub(crate) fn clamp_step(step: f64) -> f64 {
    if step.is_finite() {
        step.clamp(1.0 / MAX_STEP, MAX_STEP)
    } else {
        1.0
    }
}

impl Varispeed {
    /// Allocates the resampler and its buffers. `None` only if rubato rejects
    /// the fixed parameters, which a test rules out.
    pub(crate) fn new() -> Option<Self> {
        let params = SincInterpolationParameters::new(SINC_LEN, WindowFunction::BlackmanHarris2)
            .oversampling_factor(256)
            .interpolation(SincInterpolationType::Linear);
        let rs =
            Async::<f32>::new_sinc(1.0, MAX_STEP, &params, CHUNK, 2, FixedAsync::Output).ok()?;
        let input = vec![0.0; rs.input_frames_max() * 2].into_boxed_slice();
        Some(Self {
            rs,
            input,
            out: vec![0.0; CHUNK * 2].into_boxed_slice(),
            next: CHUNK,
            feed: 0,
            last: 0.0,
            step: 1.0,
        })
    }

    /// Source time of the next frame [`Self::next`] returns.
    pub(crate) fn position(&self) -> f64 {
        self.last + (self.next as f64 - (CHUNK - 1) as f64) * self.step
    }

    /// Restarts at source time `at`, playing on at `step`.
    pub(crate) fn jump(&mut self, src: &DeckSource, at: f64, step: f64) {
        let step = clamp_step(step);
        self.rs.reset();
        // Discarded frames: enough to fill the filter with source before `at`.
        let warm = (SINC_LEN as f64 / step).ceil() as usize;
        let chunks = warm.div_ceil(CHUNK) + 1;
        let frames = (chunks * CHUNK) as f64;
        // Output `n` after a reset sits at `feed + LEAD` plus the steps of
        // outputs `0..=n`. The first kept one is output `frames`, at
        // `feed + LEAD + frames * warm_step + step`. Pick the whole feed
        // frame for `step`, then bend the warm-up step so that is exactly `at`.
        let feed = (at - LEAD - (frames + 1.0) * step).floor();
        let warm_step = clamp_step((at - feed - LEAD - step) / frames);
        self.feed = feed as i64;
        self.set_step(warm_step);
        for _ in 0..chunks {
            self.produce(src);
        }
        self.set_step(step);
        // `produce` dated the warm-up from a stale clock; date it from here.
        self.last = at - step;
        self.next = CHUNK;
    }

    /// Next stereo frame; `step` applies from the next chunk on.
    pub(crate) fn next(&mut self, src: &DeckSource, step: f64) -> (f32, f32) {
        if self.next >= CHUNK {
            self.set_step(clamp_step(step));
            self.produce(src);
            self.next = 0;
        }
        let i = self.next * 2;
        self.next += 1;
        (self.out[i], self.out[i + 1])
    }

    fn set_step(&mut self, step: f64) {
        if step != self.step {
            // In range by `clamp_step`, so this cannot fail.
            let _ = self.rs.set_resample_ratio(1.0 / step, false);
            self.step = step;
        }
    }

    /// Runs one chunk into `out`.
    fn produce(&mut self, src: &DeckSource) {
        let n = self.rs.input_frames_next();
        let input = &mut self.input[..n * 2];
        src.read_f32(self.feed, input);
        self.feed += n as i64;
        let result = match (
            InterleavedSlice::new(&*input, 2, n),
            InterleavedSlice::new_mut(&mut self.out[..], 2, CHUNK),
        ) {
            (Ok(i), Ok(mut o)) => self.rs.process_into_buffer(&i, &mut o, None).map(|_| ()),
            _ => Ok(()),
        };
        if result.is_err() {
            // Buffers are sized from the resampler, so this does not happen;
            // silence beats garbage if it ever does.
            self.out.fill(0.0);
        }
        self.last += CHUNK as f64 * self.step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source that is silent except for one full-scale frame at `at`.
    fn impulse(frames: usize, at: usize) -> DeckSource {
        let mut pcm = vec![0i16; frames * 2];
        pcm[at * 2] = i16::MAX;
        pcm[at * 2 + 1] = i16::MAX;
        DeckSource::new(pcm, 48_000)
    }

    /// Output frames from the head until `count`, with the source time of each.
    fn play(v: &mut Varispeed, src: &DeckSource, step: f64, count: usize) -> Vec<(f64, f32)> {
        (0..count)
            .map(|_| {
                let t = v.position();
                (t, v.next(src, step).0)
            })
            .collect()
    }

    #[test]
    fn impulse_lands_on_its_frame() {
        let src = impulse(4_000, 2_000);
        for step in [1.0, 1.05, 0.93, 1.5, 0.5] {
            let mut v = Varispeed::new().unwrap();
            v.jump(&src, 1_000.5, step);
            assert!((v.position() - 1_000.5).abs() < 1e-9, "{step}");
            let out = play(&mut v, &src, step, (2_000.0 / step) as usize);
            let (t, peak) = out
                .iter()
                .copied()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            assert!(peak > 0.5, "step {step}: peak {peak}");
            assert!(
                (t - 2_000.0).abs() <= step / 2.0 + 1e-6,
                "step {step}: at {t}"
            );
        }
    }

    #[test]
    fn position_advances_by_the_step() {
        let src = impulse(10_000, 0);
        let mut v = Varispeed::new().unwrap();
        v.jump(&src, 100.0, 1.0);
        play(&mut v, &src, 1.0, 100);
        assert!((v.position() - 200.0).abs() < 1e-9);
        // A new step takes effect at the next chunk, from the gap before
        // its first frame (rubato steps before it reads).
        play(&mut v, &src, 1.25, 3 * CHUNK);
        let t = v.position();
        let left = (CHUNK - 100 % CHUNK) as f64;
        let want = 200.0 + (left - 1.0) + ((3 * CHUNK) as f64 - left + 1.0) * 1.25;
        assert!((t - want).abs() < 1e-9, "{t} vs {want}");
    }

    #[test]
    fn sine_passes_unchanged_at_unit_step() {
        // 1 kHz at 48 kHz: well inside the passband.
        let pcm: Vec<i16> = (0..20_000)
            .flat_map(|i| {
                let v = (f64::from(i) * 2.0 * std::f64::consts::PI / 48.0).sin() * 16_000.0;
                [v as i16, v as i16]
            })
            .collect();
        let src = DeckSource::new(pcm, 48_000);
        let mut v = Varispeed::new().unwrap();
        v.jump(&src, 5_000.0, 1.0);
        for (t, s) in play(&mut v, &src, 1.0, 2_000) {
            let want = (t * 2.0 * std::f64::consts::PI / 48.0).sin() * 16_000.0 / 32_768.0;
            assert!((f64::from(s) - want).abs() < 2e-3, "at {t}: {s} vs {want}");
        }
    }
}
