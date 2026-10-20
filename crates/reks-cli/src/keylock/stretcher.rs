//! The interface every candidate engine implements, and the harness that
//! drives one like an audio callback: fixed output blocks, each timed and
//! watched for heap allocations.

use std::time::{Duration, Instant};

use super::alloc;

/// Interleaved stereo input, read front to back; silence past the end.
#[derive(Debug)]
pub struct Feed<'a> {
    input: &'a [f32],
    /// Next frame to hand out.
    pos: usize,
}

impl<'a> Feed<'a> {
    pub fn new(input: &'a [f32]) -> Self {
        Self { input, pos: 0 }
    }

    /// Fills `dst` (interleaved stereo) with the next `dst.len() / 2` frames.
    pub fn take(&mut self, dst: &mut [f32]) {
        let start = (self.pos * 2).min(self.input.len());
        let n = (dst.len()).min(self.input.len() - start);
        dst[..n].copy_from_slice(&self.input[start..start + n]);
        dst[n..].fill(0.0);
        self.pos += dst.len() / 2;
    }
}

/// A tempo changer running at a fixed tempo ratio (1.06 = 6% faster).
pub trait Stretcher {
    /// Output frames before the output lines up with input frame 0, as the
    /// engine reports it.
    fn reported_latency(&self) -> usize;
    /// Fills `out` (interleaved stereo, one block), pulling input from `feed`.
    fn process(&mut self, feed: &mut Feed<'_>, out: &mut [f32]);
    /// Input frames the engine had to play as silence because the feed
    /// was late; anything but 0 means the render is not trustworthy.
    fn underruns(&self) -> u64 {
        0
    }
}

/// One render, untrimmed: the first `reported_latency` frames are lead-in.
#[derive(Debug)]
pub struct Render {
    pub samples: Vec<f32>,
    pub reported_latency: usize,
    /// Sum of all process calls.
    pub cpu: Duration,
    /// Slowest single block.
    pub worst_block: Duration,
    /// Rust heap allocation and free events inside process calls.
    pub alloc_events: u64,
}

impl Render {
    /// The output with the lead-in dropped, `frames` long.
    pub fn aligned(&self, frames: usize) -> &[f32] {
        let start = (self.reported_latency * 2).min(self.samples.len());
        let end = (start + frames * 2).min(self.samples.len());
        &self.samples[start..end]
    }
}

/// Renders `frames` aligned output frames (plus the lead-in) from `feed`
/// in blocks of `block` frames.
pub fn render(
    engine: &mut dyn Stretcher,
    feed: &mut Feed<'_>,
    frames: usize,
    block: usize,
) -> Render {
    let reported_latency = engine.reported_latency();
    let total = frames + reported_latency;
    let mut samples = vec![0.0f32; total.div_ceil(block) * block * 2];
    let mut cpu = Duration::ZERO;
    let mut worst_block = Duration::ZERO;
    let mut alloc_events = 0;
    for out in samples.chunks_exact_mut(block * 2) {
        let started = Instant::now();
        let ((), events) = alloc::count(|| engine.process(feed, out));
        let took = started.elapsed();
        cpu += took;
        worst_block = worst_block.max(took);
        alloc_events += events;
    }
    samples.truncate(total * 2);
    Render {
        samples,
        reported_latency,
        cpu,
        worst_block,
        alloc_events,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_pads_with_silence() {
        let input = [1.0, 2.0, 3.0, 4.0];
        let mut feed = Feed::new(&input);
        let mut a = [9.0; 2];
        feed.take(&mut a);
        assert_eq!(a, [1.0, 2.0]);
        let mut b = [9.0; 4];
        feed.take(&mut b);
        assert_eq!(b, [3.0, 4.0, 0.0, 0.0]);
        feed.take(&mut b);
        assert_eq!(b, [0.0; 4]);
    }

    /// Copies input through with a fixed delay.
    struct Delay(usize, Vec<f32>);
    impl Stretcher for Delay {
        fn reported_latency(&self) -> usize {
            self.0
        }
        fn process(&mut self, feed: &mut Feed<'_>, out: &mut [f32]) {
            let mut fresh = vec![0.0; out.len()];
            feed.take(&mut fresh);
            self.1.extend_from_slice(&fresh);
            out.copy_from_slice(&self.1[..out.len()]);
            self.1.drain(..out.len());
        }
    }

    #[test]
    fn render_drops_the_lead_in() {
        let input: Vec<f32> = (0..200).map(|i| i as f32).collect();
        let mut delay = Delay(3, vec![0.0; 6]);
        let r = render(&mut delay, &mut Feed::new(&input), 100, 16);
        assert_eq!(r.aligned(100), &input[..]);
        assert!(r.alloc_events > 0);
    }
}
