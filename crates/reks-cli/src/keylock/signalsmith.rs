//! Signalsmith Stretch (C++, MIT) through `signalsmith-stretch`, default
//! preset, primed with `seek` as its README recommends.

use signalsmith_stretch::Stretch;

use super::stretcher::{Feed, Stretcher};

pub struct Signalsmith {
    stretch: Stretch,
    ratio: f64,
    /// Output frames produced so far, for pacing the input.
    produced: u64,
    input: Vec<f32>,
}

impl Signalsmith {
    pub fn new(rate: u32, ratio: f64, block: usize, feed: &mut Feed<'_>) -> Self {
        let mut stretch = Stretch::preset_default(2, rate);
        // Prime with the first `input_latency` frames so that output frame
        // `output_latency` lines up with input frame 0.
        let mut pre = vec![0.0; stretch.input_latency() * 2];
        feed.take(&mut pre);
        stretch.seek(&pre, ratio);
        Self {
            stretch,
            ratio,
            produced: 0,
            input: vec![0.0; ((block as f64 * ratio).ceil() as usize + 2) * 2],
        }
    }
}

impl Stretcher for Signalsmith {
    fn reported_latency(&self) -> usize {
        self.stretch.output_latency()
    }

    fn process(&mut self, feed: &mut Feed<'_>, out: &mut [f32]) {
        let frames = (out.len() / 2) as u64;
        let at = |n: u64| (n as f64 * self.ratio).floor() as usize;
        let n_in = at(self.produced + frames) - at(self.produced);
        let input = &mut self.input[..n_in * 2];
        feed.take(input);
        self.stretch.process(&*input, &mut *out);
        self.produced += frames;
    }
}
