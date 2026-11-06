//! Waveform calibration (phase 3 task 8, D13): how reks's linear waveform
//! columns ([`crate::waveform`]) map to the bytes rekordbox writes in its
//! PWAV/PWV2/PWV3/PWV4/PWV5/PWV6/PWV7/PWVC sections.
//!
//! The model and its constants were fitted against rekordbox 6's own analysis
//! of 50 tracks from the maintainer's library; method, error and pictures in
//! [`docs/analysis-parity/waveforms.md`](../../../docs/analysis-parity/waveforms.md).
//! The fit is reproducible with the ignored test
//! `crates/reks-anlz/tests/calibrate.rs`. The encoders live in
//! `reks-anlz::waveform`; this module only holds numbers and the per-track
//! auto-gain they need.

/// `clamp(gain × x, 0, 1)^gamma` for `x` in 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    pub gain: f32,
    pub gamma: f32,
}

impl Curve {
    /// The curve at `x`; NaN maps to 0.
    pub fn apply(&self, x: f32) -> f32 {
        let v = (self.gain * x).clamp(0.0, 1.0);
        if v > 0.0 {
            v.powf(self.gamma)
        } else {
            0.0
        }
    }
}

/// Level statistic of one band over a track's sounding columns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LevelStat {
    Mean,
    /// The given quantile (0..=1), nearest rank.
    Quantile(f32),
}

/// Per-track gain of one 3-band channel: `clamp(target / level, min, max)`,
/// `level` being [`LevelStat`] of the band (0..=1) over the columns whose
/// mono height is above zero. rekordbox stores this gain ×100 in PWVC
/// (low, mid, high); its values lie in 0.80–2.37 / 1.00–3.00 / 1.01–5.00 on
/// the calibration set, so the clamps are those observed bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoGain {
    pub stat: LevelStat,
    pub target: f32,
    pub min: f32,
    pub max: f32,
}

impl AutoGain {
    /// Gain for `band` (0..=255 per column), counting only columns where
    /// `mono` is above zero. A silent track gets `max`.
    pub fn gain(&self, band: &[u8], mono: &[u8]) -> f32 {
        let mut v: Vec<u8> = band
            .iter()
            .zip(mono)
            .filter(|(_, &m)| m > 0)
            .map(|(&b, _)| b)
            .collect();
        if v.is_empty() {
            return self.max;
        }
        let last = v.len() - 1;
        let level = match self.stat {
            LevelStat::Mean => v.iter().map(|&b| f64::from(b)).sum::<f64>() / v.len() as f64,
            LevelStat::Quantile(q) => {
                let k = (f64::from(q.clamp(0.0, 1.0)) * last as f64).round() as usize;
                let (_, nth, _) = v.select_nth_unstable(k.min(last));
                f64::from(*nth)
            }
        } / 255.0;
        if level <= 0.0 {
            return self.max;
        }
        (f64::from(self.target) / level).clamp(f64::from(self.min), f64::from(self.max)) as f32
    }
}

/// Scaling from reks columns to Pioneer bytes. Bands are indexed low, mid,
/// high throughout; PWV6/PWV7/PWVC store them in that order too (rekordbox's
/// files, contrary to the "mid, high, low" in djl-analysis; see the
/// calibration doc).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveformCalibration {
    /// PWV3/PWV5 height (0..=31) from the column's mono peak (0..=1).
    pub height: Curve,
    /// PWAV height (0..=31) from the mono peak over the preview column.
    pub preview_height: Curve,
    /// PWV2 height (0..=15) from the mono peak over the preview column.
    pub tiny_height: Curve,
    /// Weights of the low, mid and high band peaks in the whiteness ratio
    /// `(w_mid·mid + w_high·high) / (w_low·low + w_mid·mid + w_high·high)`.
    pub whiteness_weights: [f32; 3],
    /// PWAV/PWV3 whiteness (0..=7) from that ratio.
    pub whiteness: Curve,
    /// Whiteness of a column with no band energy at all.
    pub silent_whiteness: u8,
    /// PWV5 colour: each band through its curve, then scaled so the largest
    /// component is 7 (rekordbox's colours are normalised per column). The
    /// UI's RGB waveform mixes with the same curves (`src/lib/waveDraw.ts`
    /// `BAND_CURVES`); keep the two in step.
    pub color: [Curve; 3],
    /// PWV4 byte 0 (0..=127) from the mono peak over the preview column.
    pub pwv4_height: Curve,
    /// PWV4 byte 1 = this minus byte 0, saturating (rekordbox's bytes 0 and
    /// 1 sum to ≈ 257 on loud columns).
    pub pwv4_complement: u8,
    /// PWV4 bytes 2–5 (0..=127) from the peak over the preview column of the
    /// low, low, mid and high band.
    pub pwv4_bands: [Curve; 4],
    /// Per-track gain per band for PWV6/PWV7, written to PWVC ×100.
    pub auto_gain: [AutoGain; 3],
    /// PWV7 release: each band's value is at least this times the previous
    /// column's (a peak-hold envelope; rekordbox's 3-band detail decays
    /// smoothly after transients).
    pub detail_release: [f32; 3],
    /// PWV7 bytes (0..=127) from gain × enveloped band.
    pub detail: [Curve; 3],
    /// PWV6 bytes (0..=127) from track gain × the band's mean over the
    /// preview column.
    pub overview: [Curve; 3],
}

const fn c(gain: f32, gamma: f32) -> Curve {
    Curve { gain, gamma }
}

/// Fitted against rekordbox 6 (see the module docs). Regenerate with
/// `crates/reks-anlz/tests/calibrate.rs`, which prints this block.
pub const CALIBRATION: WaveformCalibration = WaveformCalibration {
    height: c(1.038, 2.124),
    preview_height: c(0.835, 2.115),
    tiny_height: c(1.000, 1.041),
    whiteness_weights: [0.682, 1.000, 0.142],
    whiteness: c(1.298, 1.346),
    silent_whiteness: 7,
    color: [c(1.000, 1.121), c(0.906, 1.022), c(1.660, 0.922)],
    pwv4_height: c(0.993, 1.045),
    pwv4_complement: 255,
    pwv4_bands: [
        c(1.007, 1.038),
        c(0.953, 1.256),
        c(0.439, 0.716),
        c(0.477, 0.856),
    ],
    auto_gain: [
        AutoGain {
            stat: LevelStat::Mean,
            target: 0.2657,
            min: 0.8,
            max: 3.0,
        },
        AutoGain {
            stat: LevelStat::Mean,
            target: 0.1047,
            min: 1.0,
            max: 3.0,
        },
        AutoGain {
            stat: LevelStat::Quantile(0.999),
            target: 0.7041,
            min: 1.0,
            max: 5.0,
        },
    ],
    detail_release: [0.960, 0.897, 0.850],
    detail: [c(1.000, 1.042), c(1.000, 0.789), c(1.000, 1.638)],
    overview: [c(0.667, 1.000), c(1.520, 0.985), c(1.548, 0.986)],
};

impl Default for WaveformCalibration {
    fn default() -> Self {
        CALIBRATION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_clamps_and_shapes() {
        let k = c(2.0, 2.0);
        assert_eq!(k.apply(0.0), 0.0);
        assert_eq!(k.apply(0.25), 0.25);
        assert_eq!(k.apply(0.9), 1.0);
        assert_eq!(k.apply(-1.0), 0.0);
        assert_eq!(k.apply(f32::NAN), 0.0);
    }

    #[test]
    fn auto_gain_uses_sounding_columns_and_clamps() {
        let g = AutoGain {
            stat: LevelStat::Mean,
            target: 0.2,
            min: 0.5,
            max: 4.0,
        };
        // Mean of the sounding columns: (51 + 153) / 2 = 102 → 0.4 → 0.5.
        assert!((g.gain(&[51, 153, 255], &[1, 1, 0]) - 0.5).abs() < 1e-6);
        assert_eq!(g.gain(&[0, 0], &[3, 3]), 4.0);
        assert_eq!(g.gain(&[200], &[0]), 4.0);
        let q = AutoGain {
            stat: LevelStat::Quantile(1.0),
            target: 0.5,
            min: 0.1,
            max: 10.0,
        };
        assert!((q.gain(&[10, 255, 20], &[1, 1, 1]) - 0.5).abs() < 1e-6);
        let tiny = AutoGain { target: 10.0, ..q };
        assert_eq!(tiny.gain(&[255], &[1]), 10.0);
    }
}
