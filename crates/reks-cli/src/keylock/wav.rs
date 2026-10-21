//! 24-bit WAV output for the renders.

use std::io::BufWriter;
use std::path::Path;

use anyhow::Context;
use reks_audio::{PcmSpec, PcmWriter, WavWriter};

/// Writes interleaved stereo `samples` (nominally ±1.0, clipped) to `path`.
pub fn write_24(path: &Path, rate: u32, samples: &[f32]) -> anyhow::Result<()> {
    let file =
        std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let spec = PcmSpec {
        sample_rate: rate,
        channels: 2,
        bits_per_sample: 24,
    };
    let mut w = WavWriter::new(BufWriter::new(file), spec)?;
    const FULL: f32 = 8_388_607.0;
    let ints: Vec<i32> = samples
        .iter()
        .map(|x| (x.clamp(-1.0, 1.0) * FULL).round() as i32)
        .collect();
    w.write(&ints)?;
    w.finish()?;
    Ok(())
}
