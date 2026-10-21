//! Reference: plain varispeed (pitch follows tempo) through `rubato`'s
//! asynchronous sinc resampler, with the parameters `reks-mix` decks use.

use anyhow::Context;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};

use super::stretcher::{Feed, Stretcher};

pub struct Varispeed {
    rs: Async<f32>,
    input: Vec<f32>,
}

impl Varispeed {
    pub fn new(ratio: f64, block: usize) -> anyhow::Result<Self> {
        let params = SincInterpolationParameters::new(128, WindowFunction::BlackmanHarris2)
            .oversampling_factor(256)
            .interpolation(SincInterpolationType::Linear);
        let rs = Async::<f32>::new_sinc(1.0 / ratio, 1.0, &params, block, 2, FixedAsync::Output)
            .context("building the varispeed resampler")?;
        let input = vec![0.0; rs.input_frames_max() * 2];
        Ok(Self { rs, input })
    }
}

impl Stretcher for Varispeed {
    fn reported_latency(&self) -> usize {
        self.rs.output_delay()
    }

    fn process(&mut self, feed: &mut Feed<'_>, out: &mut [f32]) {
        let n = self.rs.input_frames_next();
        let input = &mut self.input[..n * 2];
        feed.take(input);
        let frames = out.len() / 2;
        let done = match (
            InterleavedSlice::new(&*input, 2, n),
            InterleavedSlice::new_mut(out, 2, frames),
        ) {
            (Ok(i), Ok(mut o)) => self.rs.process_into_buffer(&i, &mut o, None).is_ok(),
            _ => false,
        };
        if !done {
            out.fill(0.0);
        }
    }
}
