//! Streaming sample-rate conversion on `rubato`'s synchronous FFT resampler.
//!
//! Input goes in as interleaved slices of any length; output comes out in
//! the same layout with the resampler's delay removed, so output frame `n`
//! lines up with input time `n / to` seconds. [`StreamResampler::finish`]
//! flushes the tail and trims the output to `ceil(input_frames * to / from)`
//! frames, so a track keeps its length (and cue positions in ms).
//!
//! The preview player uses it at `f32` (docs/decisions.md D5); the converter
//! at `f64`, which keeps 24-bit sources exact to well below one LSB.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler, Sample, WindowFunction};

/// Input frames per resampler chunk, for most rate pairs (see [`layout`]).
pub const CHUNK: usize = 1024;

/// FFT sub-chunks per [`CHUNK`]: what rubato's `Fft::new` picks.
const SUB_CHUNKS: usize = CHUNK / 256;

/// Input chunk and sub-chunk count for `from` → `to` Hz.
///
/// rubato's `Fft` reports its delay as `fft_size_out / 2` output frames,
/// rounded down. When the FFT output block is odd (96 → 44.1 kHz: 147
/// frames) the true delay is half a frame more, and removing the rounded
/// value would shift the output half a frame early. Such pairs get one
/// sub-chunk of an even number of minimum blocks, which makes the output
/// block even.
fn layout(from: usize, to: usize) -> (usize, usize) {
    let g = gcd(from, to);
    let (min_in, min_out) = (from / g, to / g);
    // As rubato computes it for FixedSync::Input.
    let blocks = (CHUNK / SUB_CHUNKS).div_ceil(min_in).max(1);
    if (blocks * min_out).is_multiple_of(2) {
        return (CHUNK, SUB_CHUNKS);
    }
    let pairs = (CHUNK / (2 * min_in)).max(1);
    (2 * pairs * min_in, 1)
}

fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// The resampler rejected the rates, channel count or buffers.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ResampleError(String);

fn err(e: impl std::fmt::Display) -> ResampleError {
    ResampleError(e.to_string())
}

/// Chunked resampler from `from` Hz to `to` Hz over interleaved samples.
pub struct StreamResampler<T: Sample> {
    fft: Fft<T>,
    channels: usize,
    /// Input frames per chunk.
    chunk: usize,
    /// Interleaved input of the chunk being filled (`< chunk` frames between calls).
    input: Vec<T>,
    /// Scratch output of one chunk.
    output: Vec<T>,
    /// Output frames still to drop for the resampler's delay.
    skip: usize,
    from: u64,
    to: u64,
    frames_in: u64,
    frames_out: u64,
}

impl<T: Sample> std::fmt::Debug for StreamResampler<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamResampler")
            .field("from", &self.from)
            .field("to", &self.to)
            .field("channels", &self.channels)
            .field("frames_in", &self.frames_in)
            .field("frames_out", &self.frames_out)
            .finish_non_exhaustive()
    }
}

impl<T: Sample> StreamResampler<T> {
    /// A resampler for `channels` interleaved channels.
    pub fn new(from: u32, to: u32, channels: usize) -> Result<Self, ResampleError> {
        if from == 0 || to == 0 || channels == 0 {
            return Err(ResampleError(format!(
                "{from} Hz to {to} Hz, {channels} channels"
            )));
        }
        let (chunk, sub_chunks) = layout(from as usize, to as usize);
        let fft = Fft::<T>::new_custom(
            from as usize,
            to as usize,
            chunk,
            sub_chunks,
            channels,
            WindowFunction::BlackmanHarris2,
            FixedSync::Input,
        )
        .map_err(err)?;
        let output = vec![T::zero(); fft.output_frames_max() * channels];
        let skip = fft.output_delay();
        Ok(Self {
            fft,
            channels,
            chunk,
            input: Vec::with_capacity(chunk * channels),
            output,
            skip,
            from: u64::from(from),
            to: u64::from(to),
            frames_in: 0,
            frames_out: 0,
        })
    }

    /// Output frames for `input_frames` input frames: `ceil(input * to / from)`.
    pub fn output_len(&self, input_frames: u64) -> u64 {
        (input_frames * self.to).div_ceil(self.from)
    }

    /// Resamples `input` (whole frames) and appends what is ready to `out`.
    pub fn push(&mut self, input: &[T], out: &mut Vec<T>) -> Result<(), ResampleError> {
        if !input.len().is_multiple_of(self.channels) {
            return Err(ResampleError(format!(
                "{} samples is not a whole number of {}-channel frames",
                input.len(),
                self.channels
            )));
        }
        self.frames_in += (input.len() / self.channels) as u64;
        let chunk = self.chunk * self.channels;
        let mut rest = input;
        while !rest.is_empty() {
            let take = (chunk - self.input.len()).min(rest.len());
            let (head, tail) = rest.split_at(take);
            self.input.extend_from_slice(head);
            rest = tail;
            if self.input.len() == chunk {
                self.run(None, out, u64::MAX)?;
            }
        }
        Ok(())
    }

    /// Flushes the buffered input and the resampler's delay into `out`. After
    /// this, `output_len(input frames)` frames have been appended in total.
    pub fn finish(&mut self, out: &mut Vec<T>) -> Result<(), ResampleError> {
        let expected = self.output_len(self.frames_in);
        let chunk = self.chunk * self.channels;
        // The last partial chunk, then silence until the delay is flushed.
        let partial = self.input.len() / self.channels;
        self.input.resize(chunk, T::zero());
        self.run(Some(partial), out, expected)?;
        while self.frames_out < expected {
            self.input.resize(chunk, T::zero());
            self.run(Some(0), out, expected)?;
        }
        Ok(())
    }

    /// Processes the full `input` chunk; appends at most up to `limit` total
    /// output frames.
    fn run(
        &mut self,
        partial: Option<usize>,
        out: &mut Vec<T>,
        limit: u64,
    ) -> Result<(), ResampleError> {
        let ch = self.channels;
        let input = InterleavedSlice::new(&self.input[..], ch, self.chunk).map_err(err)?;
        let frames_max = self.output.len() / ch;
        let mut output =
            InterleavedSlice::new_mut(&mut self.output[..], ch, frames_max).map_err(err)?;
        let indexing = partial.map(|n| Indexing::new().partial_len(n));
        let (_, written) = self
            .fft
            .process_into_buffer(&input, &mut output, indexing.as_ref())
            .map_err(err)?;
        let drop = self.skip.min(written);
        self.skip -= drop;
        let room = usize::try_from(limit.saturating_sub(self.frames_out)).unwrap_or(usize::MAX);
        let keep = (written - drop).min(room);
        out.extend_from_slice(&self.output[drop * ch..(drop + keep) * ch]);
        self.frames_out += keep as u64;
        self.input.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One sine period every `period` frames, as f64 at full scale 0.5.
    fn sine(frames: usize, period: f64, channels: usize) -> Vec<f64> {
        (0..frames)
            .flat_map(|i| {
                let v = 0.5 * (i as f64 * std::f64::consts::TAU / period).sin();
                std::iter::repeat_n(v, channels)
            })
            .collect()
    }

    #[test]
    fn keeps_length_for_any_push_sizes() {
        for (from, to) in [(96_000, 48_000), (88_200, 44_100), (44_100, 48_000)] {
            let input = sine(100_003, 100.0, 2);
            let mut rs = StreamResampler::<f64>::new(from, to, 2).unwrap();
            let mut out = Vec::new();
            for piece in input.chunks(2 * 333) {
                rs.push(piece, &mut out).unwrap();
            }
            rs.finish(&mut out).unwrap();
            let expected = (100_003u64 * u64::from(to)).div_ceil(u64::from(from));
            assert_eq!(out.len() as u64, expected * 2, "{from} -> {to}");
        }
    }

    #[test]
    fn halving_the_rate_keeps_the_signal_aligned() {
        // 96 kHz mono sine at 1 kHz → 48 kHz: output frame n ≈ input frame 2n.
        let input = sine(96_000, 96.0, 1);
        let mut rs = StreamResampler::<f64>::new(96_000, 48_000, 1).unwrap();
        let mut out = Vec::new();
        rs.push(&input, &mut out).unwrap();
        rs.finish(&mut out).unwrap();
        assert_eq!(out.len(), 48_000);
        // Skip the edges, where the filter sees the zero padding.
        let worst = (1_000..47_000)
            .map(|n| (out[n] - input[2 * n]).abs())
            .fold(0.0f64, f64::max);
        assert!(worst < 1e-3, "worst error {worst}");
    }

    #[test]
    fn no_sub_frame_shift_for_any_converter_rate() {
        // Phase of a 440 Hz sine after resampling, in output frames: 0 when
        // the delay is removed exactly.
        let sources = [
            32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 384_000,
        ];
        for from in sources {
            for to in [44_100u32, 48_000] {
                if from == to {
                    continue;
                }
                let tau = std::f64::consts::TAU;
                let input: Vec<f64> = (0..from as usize)
                    .map(|i| 0.25 * (i as f64 * tau * 440.0 / f64::from(from)).sin())
                    .collect();
                let mut rs = StreamResampler::<f64>::new(from, to, 1).unwrap();
                let mut out = Vec::new();
                rs.push(&input, &mut out).unwrap();
                rs.finish(&mut out).unwrap();
                let w = tau * 440.0 / f64::from(to);
                let (mut s, mut c) = (0.0, 0.0);
                for (k, &y) in out
                    .iter()
                    .enumerate()
                    .skip(to as usize / 4)
                    .take(to as usize / 2)
                {
                    s += y * (k as f64 * w).sin();
                    c += y * (k as f64 * w).cos();
                }
                let lag = c.atan2(s) / w;
                assert!(lag.abs() < 0.01, "{from} -> {to}: {lag} frames");
            }
        }
    }

    #[test]
    fn rejects_bad_input() {
        assert!(StreamResampler::<f64>::new(0, 48_000, 2).is_err());
        let mut rs = StreamResampler::<f64>::new(44_100, 48_000, 2).unwrap();
        assert!(rs.push(&[0.0; 3], &mut Vec::new()).is_err());
    }
}
