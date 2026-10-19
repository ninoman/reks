//! Bit-depth conversion for the converter: exact widening, rounding to 24
//! bits and TPDF dither to 16 bits (docs/phases/phase-2.6.md).
//!
//! TPDF (triangular) dither adds the difference of two uniform values in
//! `[0, 1)` LSB before rounding, which turns quantisation distortion into a
//! constant noise floor. The added noise is under 1 LSB, so a dithered sample
//! is never more than 1 LSB from the plainly rounded one. The generator is
//! seeded, so the same input gives the same bytes on every run.

/// Converts samples to `to_bits` (16 or 24 in practice).
#[derive(Debug, Clone)]
pub struct Requantiser {
    to_bits: u8,
    rng: SplitMix64,
}

impl Requantiser {
    /// A requantiser to `to_bits` (1..=32). Dithers only when the target is
    /// 16 bits or fewer and precision is actually lost.
    pub fn new(to_bits: u8) -> Self {
        Self {
            to_bits: to_bits.clamp(1, 32),
            rng: SplitMix64(0x5245_4B53_2D44_4954), // "REKS-DIT"
        }
    }

    /// Target bit depth.
    pub fn bits(&self) -> u8 {
        self.to_bits
    }

    /// `sample` at `from_bits` (right-aligned, as [`crate::NativeStream`]
    /// yields it) to the target depth. Widening and same-depth are exact.
    pub fn int(&mut self, sample: i32, from_bits: u8) -> i32 {
        let from_bits = from_bits.clamp(1, 32);
        if from_bits <= self.to_bits {
            // Exact: at most a 31-bit shift of a value that fits the result.
            return sample << (self.to_bits - from_bits);
        }
        let shift = i32::from(from_bits - self.to_bits);
        self.quantise(f64::from(sample) / 2f64.powi(shift))
    }

    /// `x` in full scale (`-1.0..1.0`, from the resampler) to the target depth.
    pub fn float(&mut self, x: f64) -> i32 {
        self.quantise(x * 2f64.powi(i32::from(self.to_bits) - 1))
    }

    /// Rounds `v` (in target LSBs), dithered at 16 bits or fewer, and clamps.
    fn quantise(&mut self, v: f64) -> i32 {
        let v = if self.to_bits <= 16 {
            v + self.rng.unit() - self.rng.unit()
        } else {
            v
        };
        let max = f64::from(((1i64 << (self.to_bits - 1)) - 1) as i32);
        v.round().clamp(-max - 1.0, max) as i32
    }
}

/// `sample` at `bits` (right-aligned) to full scale `-1.0..1.0`, the
/// resampler's input.
pub fn to_float(sample: i32, bits: u8) -> f64 {
    f64::from(sample) / 2f64.powi(i32::from(bits.clamp(1, 32)) - 1)
}

/// SplitMix64 (Steele, Lea, Flood 2014): small, fast, good enough for dither.
#[derive(Debug, Clone)]
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)` with 53 random bits.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic 24-bit test values covering the whole range.
    fn samples_24() -> impl Iterator<Item = i32> {
        let mut rng = SplitMix64(7);
        (0..200_000).map(move |_| (rng.next() >> 40) as i32 - (1 << 23))
    }

    #[test]
    fn same_depth_and_widening_are_exact() {
        let mut q16 = Requantiser::new(16);
        let mut q24 = Requantiser::new(24);
        for s in [-32_768, -1, 0, 1, 12_345, 32_767] {
            assert_eq!(q16.int(s, 16), s);
            assert_eq!(q24.int(s, 16), s << 8);
        }
        for s in [-0x80_0000, -1, 0, 0x7F_FFFF] {
            assert_eq!(q24.int(s, 24), s);
        }
        // 20-bit ALAC to 24 bits.
        assert_eq!(q24.int(-0x8_0000, 20), -0x80_0000);
    }

    #[test]
    fn reduction_to_24_bits_rounds_without_dither() {
        let mut q = Requantiser::new(24);
        assert_eq!(q.int(0x7FFF_FF80, 32), 0x7F_FFFF); // clamps the round-up
        assert_eq!(q.int(i32::MIN, 32), -0x80_0000);
        assert_eq!(q.int(0x180, 32), 2); // 1.5 rounds away from zero
        assert_eq!(q.int(0x17F, 32), 1);
        assert_eq!(q.float(0.5), 0x40_0000);
        assert_eq!(q.float(1.0), 0x7F_FFFF);
        assert_eq!(q.float(-1.0), -0x80_0000);
    }

    #[test]
    fn dither_to_16_bits_is_within_one_lsb() {
        let mut q = Requantiser::new(16);
        let (mut sum_err, mut differs, mut n) = (0.0f64, 0usize, 0usize);
        for s in samples_24() {
            let exact = f64::from(s) / 256.0;
            let rounded = exact.round().clamp(-32_768.0, 32_767.0) as i32;
            let out = q.int(s, 24);
            assert!((-32_768..=32_767).contains(&out), "{s} -> {out}");
            assert!(
                (out - rounded).abs() <= 1,
                "{s}: {out} vs rounded {rounded}"
            );
            assert!(
                (f64::from(out) - exact).abs() < 1.5,
                "{s}: {out} vs {exact}"
            );
            sum_err += f64::from(out) - exact;
            differs += usize::from(out != rounded);
            n += 1;
        }
        // Unbiased, and actually dithering: TPDF moves 1 in 3 samples off the
        // plainly rounded value.
        let mean = sum_err / n as f64;
        assert!(mean.abs() < 0.01, "mean error {mean}");
        let share = differs as f64 / n as f64;
        assert!((0.30..0.37).contains(&share), "share changed {share}");
    }

    #[test]
    fn dithered_silence_stays_within_one_lsb() {
        let mut q = Requantiser::new(16);
        let out: Vec<i32> = (0..10_000).map(|_| q.int(0, 24)).collect();
        assert!(out.iter().all(|s| (-1..=1).contains(s)));
        assert!(out.iter().any(|&s| s != 0));
        let mut f = Requantiser::new(16);
        assert!((0..10_000).all(|_| (-1..=1).contains(&f.float(0.0))));
    }

    #[test]
    fn dither_clamps_at_full_scale() {
        let mut q = Requantiser::new(16);
        for _ in 0..1_000 {
            assert!((32_766..=32_767).contains(&q.int(0x7F_FFFF, 24)));
            assert!((-32_768..=-32_767).contains(&q.int(-0x80_0000, 24)));
            assert!((32_766..=32_767).contains(&q.float(1.0)));
        }
    }

    #[test]
    fn is_deterministic() {
        let run = || {
            let mut q = Requantiser::new(16);
            samples_24()
                .take(1_000)
                .map(|s| q.int(s, 24))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn float_round_trip() {
        assert_eq!(to_float(-0x80_0000, 24), -1.0);
        assert_eq!(to_float(0x40_0000, 24), 0.5);
        let mut q = Requantiser::new(24);
        for s in [-0x80_0000, -3, 0, 5, 0x7F_FFFF] {
            assert_eq!(q.float(to_float(s, 24)), s);
        }
    }
}
