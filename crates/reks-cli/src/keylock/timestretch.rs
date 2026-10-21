//! `timestretch` (pure Rust): its real-time deck engine, in the `Keylock`
//! (two-band SOLA) or `WideKeylock` (phase vocoder) profile.

use anyhow::Context;
use timestretch::engine::{
    Engine, EngineConfig, EngineController, EngineProcessor, EngineProfile, SourceProducer,
};

use super::stretcher::{Feed, Stretcher};

/// Frames pushed into the engine's source ring per feed step.
const FEED_STEP: usize = 1024;

pub struct Timestretch {
    controller: EngineController,
    processor: EngineProcessor,
    source: SourceProducer,
    scratch: Vec<f32>,
}

impl Timestretch {
    pub fn new(
        profile: EngineProfile,
        rate: u32,
        ratio: f64,
        block: usize,
    ) -> anyhow::Result<Self> {
        let handles = Engine::build(EngineConfig {
            sample_rate: rate,
            channels: 2,
            profile,
            initial_tempo_rate: ratio,
            max_block_frames: block,
            ..EngineConfig::default()
        })
        .context("building the timestretch engine")?;
        let mut source = handles.source;
        source.set_track_position(0);
        Ok(Self {
            controller: handles.controller,
            processor: handles.processor,
            source,
            scratch: vec![0.0; FEED_STEP * 2],
        })
    }
}

impl Stretcher for Timestretch {
    fn reported_latency(&self) -> usize {
        self.processor.pipeline_latency_frames()
    }

    fn process(&mut self, feed: &mut Feed<'_>, out: &mut [f32]) {
        // The feed thread's job, done inline: keep the ring ahead of demand.
        // `demand_hint` leaves out the head's lookahead, which the
        // WideKeylock phase vocoder needs (it underruns without it).
        let demand = self.source.demand_hint(out.len() / 2, 2.0)
            + self.processor.varispeed_lookahead_frames();
        while self.source.occupied_frames() < demand {
            let n = self.source.free_frames().min(FEED_STEP);
            if n == 0 {
                break;
            }
            let chunk = &mut self.scratch[..n * 2];
            feed.take(chunk);
            self.source.push(chunk);
        }
        self.processor.process(out);
    }

    fn underruns(&self) -> u64 {
        self.controller.underrun_frames()
    }
}
