//! Peak meter with a falling display, for channel and master meters.
//!
//! The level jumps up to each new peak instantly and falls at
//! [`FALL_DB_PER_S`] (a common PPM-style fall; the hardware's LED ballistics
//! are not documented). The UI polls [`PeakMeter::level`] at about 60 Hz.

/// Display fall rate.
pub const FALL_DB_PER_S: f32 = 20.0;

#[derive(Debug, Clone)]
pub struct PeakMeter {
    level: f32,
    fall: f32,
    clipped: bool,
}

impl PeakMeter {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            level: 0.0,
            fall: 10f32.powf(-FALL_DB_PER_S / 20.0 / sample_rate.max(1) as f32),
            clipped: false,
        }
    }

    /// Reads interleaved stereo; the meter shows the louder side.
    pub fn process(&mut self, buf: &[f32]) {
        for frame in buf.as_chunks::<2>().0 {
            let peak = frame[0].abs().max(frame[1].abs());
            // NaN compares false and is skipped.
            self.level = if peak > self.level {
                peak
            } else {
                self.level * self.fall
            };
            if peak >= 1.0 {
                self.clipped = true;
            }
        }
        if self.level < 1e-10 {
            self.level = 0.0;
        }
    }

    /// Current level, linear (1.0 = 0 dBFS).
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Whether any sample reached 0 dBFS since the last call; clears it.
    pub fn take_clip(&mut self) -> bool {
        std::mem::take(&mut self.clipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jumps_to_the_peak_and_falls_at_the_set_rate() {
        let rate = 48_000;
        let mut m = PeakMeter::new(rate);
        m.process(&[0.1, -0.5, 0.0, 0.0]);
        assert!((m.level() - 0.5).abs() < 1e-3);
        let silence = vec![0.0; rate as usize * 2];
        m.process(&silence);
        let fell = 20.0 * (0.5f32 / m.level()).log10();
        assert!((fell - FALL_DB_PER_S).abs() < 0.1, "{fell}");
        assert!(!m.take_clip());
    }

    #[test]
    fn latches_clips_until_read_and_ignores_nan() {
        let mut m = PeakMeter::new(44_100);
        m.process(&[f32::NAN, 1.2, 0.0, 0.0]);
        assert!(m.level().is_finite());
        assert!(m.take_clip());
        assert!(!m.take_clip());
    }
}
