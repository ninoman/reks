//! Rubber Band Library 4 (C++, GPL-2.0-or-later) through `rubberband`, in
//! real-time mode with the R3 ("finer") or R2 ("faster") engine. Rubber Band
//! works in its own hop sizes, so a FIFO turns its output into fixed blocks.

use rubberband::{Options, Stretcher as Rb};

use super::stretcher::{Feed, Stretcher};

/// Largest input the stretcher is given per call, in frames.
const MAX_IN: usize = 16_384;

pub struct RubberBand {
    rb: Rb,
    interleaved: Vec<f32>,
    left: Vec<f32>,
    right: Vec<f32>,
    /// Output not yet handed out, interleaved.
    fifo: Vec<f32>,
}

impl RubberBand {
    pub fn new(finer: bool, rate: u32, ratio: f64) -> Self {
        let engine = if finer {
            Options::ENGINE_FINER
        } else {
            Options::ENGINE_FASTER
        };
        let opts = Options::PROCESS_REALTIME
            | engine
            | Options::PITCH_HIGH_CONSISTENCY
            | Options::THREADING_NEVER;
        // Time ratio is output length over input length.
        let mut rb = Rb::new(rate, 2, opts, 1.0 / ratio, 1.0);
        rb.set_max_process_size(MAX_IN as u32);
        let mut me = Self {
            rb,
            interleaved: vec![0.0; MAX_IN * 2],
            left: vec![0.0; MAX_IN],
            right: vec![0.0; MAX_IN],
            fifo: Vec::with_capacity(MAX_IN * 8),
        };
        // Pad with silence as the API asks; `start_delay` output frames
        // are then lead-in (the render harness drops them).
        let pad = (me.rb.preferred_start_pad() as usize).min(MAX_IN);
        me.left[..pad].fill(0.0);
        me.right[..pad].fill(0.0);
        me.rb.process(&[&me.left[..pad], &me.right[..pad]], false);
        me.drain();
        me
    }

    /// Moves everything the stretcher has ready into the FIFO.
    fn drain(&mut self) {
        loop {
            let avail = (self.rb.available().unwrap_or(0) as usize).min(MAX_IN);
            if avail == 0 {
                return;
            }
            let got = self
                .rb
                .retrieve(&mut [&mut self.left[..avail], &mut self.right[..avail]])
                as usize;
            for i in 0..got {
                self.fifo.push(self.left[i]);
                self.fifo.push(self.right[i]);
            }
            if got == 0 {
                return;
            }
        }
    }
}

impl Stretcher for RubberBand {
    fn reported_latency(&self) -> usize {
        self.rb.start_delay() as usize
    }

    fn process(&mut self, feed: &mut Feed<'_>, out: &mut [f32]) {
        while self.fifo.len() < out.len() {
            let need = (self.rb.samples_required() as usize).clamp(1, MAX_IN);
            let chunk = &mut self.interleaved[..need * 2];
            feed.take(chunk);
            for (i, [l, r]) in chunk.as_chunks::<2>().0.iter().enumerate() {
                self.left[i] = *l;
                self.right[i] = *r;
            }
            self.rb
                .process(&[&self.left[..need], &self.right[..need]], false);
            self.drain();
        }
        out.copy_from_slice(&self.fifo[..out.len()]);
        self.fifo.drain(..out.len());
    }
}
