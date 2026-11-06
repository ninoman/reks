//! Pioneer waveform encoders (Phase 3 task 7): turn reks waveform columns
//! (150/s, `reks_analysis::waveform`) into the PWAV/PWV2/PWV3/PWV4/PWV5/
//! PWV6/PWV7/PWVC payloads, and frame them as sections via [`crate::write`].
//!
//! Formats: djl-analysis "Analysis Files" and crate-digger
//! `rekordbox_anlz.ksy` (R§3.3), checked against the rekordbox 6 reference
//! export in rekordcrate's test data. How rekordbox scales band energies is
//! undocumented; the model and its constants ([`WaveformCalibration`],
//! `reks_analysis::calib`) were fitted against rekordbox's own files in task 8
//! (`docs/analysis-parity/waveforms.md`). PWV6/PWV7/PWVC hold low, mid, high
//! in that order (rekordbox's files; djl-analysis says mid, high, low).

pub use reks_analysis::calib::{AutoGain, Curve, LevelStat, WaveformCalibration, CALIBRATION};
use reks_analysis::waveform::{WaveformColumns, MONO_MAX};

use crate::write::{self, WriteError};

/// PWAV columns.
pub const PWAV_COLUMNS: usize = 400;
/// PWV2 columns.
pub const PWV2_COLUMNS: usize = 100;
/// PWV4 and PWV6 columns.
pub const COLOR_PREVIEW_COLUMNS: usize = 1200;
/// Largest whiteness (3 bits).
pub const WHITENESS_MAX: u8 = 7;
/// Largest PWV2 height (4 bits).
pub const TINY_HEIGHT_MAX: u8 = 15;
/// Largest PWV5 colour component (3 bits).
pub const COLOR_MAX: u8 = 7;
/// Full scale of the PWV4/PWV6/PWV7 band bytes (rekordbox's maximum).
pub const BAND_BYTE_MAX: u8 = 127;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    #[error(
        "waveform columns have mismatched lengths (low {low}, mid {mid}, high {high}, mono {mono})"
    )]
    InconsistentColumns {
        low: usize,
        mid: usize,
        high: usize,
        mono: usize,
    },
}

/// Encoded Pioneer waveforms, payloads only (no section headers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waveforms {
    /// PWAV: `whiteness:3 | height:5` per column.
    pub pwav: [u8; PWAV_COLUMNS],
    /// PWV2: 4-bit height in the low nibble.
    pub pwv2: [u8; PWV2_COLUMNS],
    /// PWV3: as PWAV, 150 columns/s.
    pub pwv3: Vec<u8>,
    /// PWV4: height, 255 − height, low, low, mid, high (bytes 0–1 and the
    /// second low are fits, not understood semantics).
    pub pwv4: [[u8; 6]; COLOR_PREVIEW_COLUMNS],
    /// PWV5: `R:3 G:3 B:3 H:5 0:2`, 150 columns/s.
    pub pwv5: Vec<u16>,
    /// PWV6: low, mid, high.
    pub pwv6: [[u8; 3]; COLOR_PREVIEW_COLUMNS],
    /// PWV7: low, mid, high, 150 columns/s.
    pub pwv7: Vec<[u8; 3]>,
    /// PWVC: the per-track band gains (low, mid, high) ×100 applied to
    /// PWV6/PWV7, as rekordbox writes them.
    pub pwvc: [u16; 3],
}

/// PWAV/PWV3 byte: whiteness in bits 7–5, height in bits 4–0. Out-of-range
/// inputs saturate.
pub fn pack_mono(height: u8, whiteness: u8) -> u8 {
    (whiteness.min(WHITENESS_MAX) << 5) | height.min(MONO_MAX)
}

/// Inverse of [`pack_mono`]: `(height, whiteness)`.
pub fn unpack_mono(byte: u8) -> (u8, u8) {
    (byte & 0x1F, byte >> 5)
}

/// PWV2 byte: height in bits 3–0, upper bits zero.
pub fn pack_tiny(height: u8) -> u8 {
    height.min(TINY_HEIGHT_MAX)
}

/// One PWV5 column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorColumn {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub height: u8,
}

/// PWV5 entry: red bits 15–13, green 12–10, blue 9–7, height 6–2, bits 1–0
/// zero. Out-of-range inputs saturate.
pub fn pack_color(c: ColorColumn) -> u16 {
    (u16::from(c.red.min(COLOR_MAX)) << 13)
        | (u16::from(c.green.min(COLOR_MAX)) << 10)
        | (u16::from(c.blue.min(COLOR_MAX)) << 7)
        | (u16::from(c.height.min(MONO_MAX)) << 2)
}

/// Inverse of [`pack_color`] (bits 1–0 ignored).
pub fn unpack_color(v: u16) -> ColorColumn {
    let field = |shift: u16, mask: u16| ((v >> shift) & mask) as u8;
    ColorColumn {
        red: field(13, 0x7),
        green: field(10, 0x7),
        blue: field(7, 0x7),
        height: field(2, 0x1F),
    }
}

/// `x` in 0..=1 to 0..=max, rounded, saturating (NaN → 0).
fn level(x: f32, max: u8) -> u8 {
    (x * f32::from(max)).round().clamp(0.0, f32::from(max)) as u8
}

/// Source column range for output column `j` of `m` over `n` inputs: every
/// output covers at least one input when `n > 0`.
fn source_range(j: usize, m: usize, n: usize) -> std::ops::Range<usize> {
    if n == 0 {
        return 0..0;
    }
    let start = j * n / m;
    let end = ((j + 1) * n / m).max(start + 1).min(n);
    start..end
}

/// Peak-hold envelope: each value is at least `release` times the previous
/// output.
fn release_envelope(x: &[u8], release: f32) -> Vec<f32> {
    let mut held = 0.0f32;
    x.iter()
        .map(|&v| {
            held = (f32::from(v) / 255.0).max(held * release);
            held
        })
        .collect()
}

struct Encoder<'a> {
    c: &'a WaveformColumns,
    cal: &'a WaveformCalibration,
    /// Per-track band gains (PWVC / 100).
    gain: [f32; 3],
}

impl Encoder<'_> {
    fn band(&self, b: usize) -> &[u8] {
        [&self.c.low, &self.c.mid, &self.c.high][b]
    }

    /// Mono peak over `r`, 0..=1.
    fn mono_peak(&self, r: std::ops::Range<usize>) -> f32 {
        f32::from(r.map(|i| self.c.mono[i]).max().unwrap_or(0)) / f32::from(MONO_MAX)
    }

    /// Peak of band `b` over `r`, 0..=1.
    fn band_peak(&self, b: usize, r: std::ops::Range<usize>) -> f32 {
        f32::from(self.band(b)[r].iter().copied().max().unwrap_or(0)) / 255.0
    }

    fn whiteness(&self, r: std::ops::Range<usize>) -> u8 {
        let [wl, wm, wh] = self.cal.whiteness_weights;
        let sum = |b: usize| {
            self.band(b)[r.clone()]
                .iter()
                .map(|&v| f32::from(v))
                .sum::<f32>()
        };
        let (low, mid, high) = (wl * sum(0), wm * sum(1), wh * sum(2));
        let total = low + mid + high;
        if total <= 0.0 {
            return self.cal.silent_whiteness.min(WHITENESS_MAX);
        }
        level(
            self.cal.whiteness.apply((mid + high) / total),
            WHITENESS_MAX,
        )
    }

    fn pwav(&self) -> [u8; PWAV_COLUMNS] {
        let n = self.c.len();
        std::array::from_fn(|j| {
            let r = source_range(j, PWAV_COLUMNS, n);
            let h = level(
                self.cal.preview_height.apply(self.mono_peak(r.clone())),
                MONO_MAX,
            );
            pack_mono(h, self.whiteness(r))
        })
    }

    fn pwv2(&self) -> [u8; PWV2_COLUMNS] {
        let n = self.c.len();
        std::array::from_fn(|j| {
            let peak = self.mono_peak(source_range(j, PWV2_COLUMNS, n));
            pack_tiny(level(self.cal.tiny_height.apply(peak), TINY_HEIGHT_MAX))
        })
    }

    fn height(&self, i: usize) -> u8 {
        level(self.cal.height.apply(self.mono_peak(i..i + 1)), MONO_MAX)
    }

    fn pwv3(&self) -> Vec<u8> {
        (0..self.c.len())
            .map(|i| pack_mono(self.height(i), self.whiteness(i..i + 1)))
            .collect()
    }

    fn pwv4(&self) -> [[u8; 6]; COLOR_PREVIEW_COLUMNS] {
        let n = self.c.len();
        let cal = self.cal;
        std::array::from_fn(|j| {
            let r = source_range(j, COLOR_PREVIEW_COLUMNS, n);
            let height = level(
                cal.pwv4_height.apply(self.mono_peak(r.clone())),
                BAND_BYTE_MAX,
            );
            let band = |k: usize, b: usize| {
                level(
                    cal.pwv4_bands[k].apply(self.band_peak(b, r.clone())),
                    BAND_BYTE_MAX,
                )
            };
            [
                height,
                cal.pwv4_complement.saturating_sub(height),
                band(0, 0),
                band(1, 0),
                band(2, 1),
                band(3, 2),
            ]
        })
    }

    fn pwv5(&self) -> Vec<u16> {
        (0..self.c.len())
            .map(|i| {
                let v: [f32; 3] =
                    std::array::from_fn(|b| self.cal.color[b].apply(self.band_peak(b, i..i + 1)));
                let max = v[0].max(v[1]).max(v[2]);
                let rgb: [u8; 3] = if max > 0.0 {
                    v.map(|x| level(x / max, COLOR_MAX))
                } else {
                    [0; 3]
                };
                pack_color(ColorColumn {
                    red: rgb[0],
                    green: rgb[1],
                    blue: rgb[2],
                    height: self.height(i),
                })
            })
            .collect()
    }

    fn pwv6(&self) -> [[u8; 3]; COLOR_PREVIEW_COLUMNS] {
        let n = self.c.len();
        std::array::from_fn(|j| {
            let r = source_range(j, COLOR_PREVIEW_COLUMNS, n);
            let len = r.len().max(1) as f32;
            std::array::from_fn(|b| {
                let sum: f32 = self.band(b)[r.clone()].iter().map(|&x| f32::from(x)).sum();
                let mean = sum / len / 255.0;
                level(
                    self.cal.overview[b].apply(self.gain[b] * mean),
                    BAND_BYTE_MAX,
                )
            })
        })
    }

    fn pwv7(&self) -> Vec<[u8; 3]> {
        let env: [Vec<f32>; 3] =
            std::array::from_fn(|b| release_envelope(self.band(b), self.cal.detail_release[b]));
        (0..self.c.len())
            .map(|i| {
                std::array::from_fn(|b| {
                    level(
                        self.cal.detail[b].apply(self.gain[b] * env[b][i]),
                        BAND_BYTE_MAX,
                    )
                })
            })
            .collect()
    }
}

/// The PWV4 colour preview alone (what [`Waveforms::encode`] writes as
/// `pwv4`), for the track list's thumbnails; `None` for inconsistent columns.
pub fn color_preview(
    columns: &WaveformColumns,
    cal: &WaveformCalibration,
) -> Option<[[u8; 6]; COLOR_PREVIEW_COLUMNS]> {
    if !columns.is_consistent() {
        return None;
    }
    // PWV4 does not use the per-track gains.
    let e = Encoder {
        c: columns,
        cal,
        gain: [1.0; 3],
    };
    Some(e.pwv4())
}

impl Waveforms {
    /// Encode all forms from 150/s columns.
    pub fn encode(
        columns: &WaveformColumns,
        cal: &WaveformCalibration,
    ) -> Result<Self, EncodeError> {
        if !columns.is_consistent() {
            return Err(EncodeError::InconsistentColumns {
                low: columns.low.len(),
                mid: columns.mid.len(),
                high: columns.high.len(),
                mono: columns.mono.len(),
            });
        }
        let bands = [&columns.low, &columns.mid, &columns.high];
        let gain: [f32; 3] =
            std::array::from_fn(|b| cal.auto_gain[b].gain(bands[b], &columns.mono));
        let e = Encoder {
            c: columns,
            cal,
            gain,
        };
        Ok(Self {
            pwav: e.pwav(),
            pwv2: e.pwv2(),
            pwv3: e.pwv3(),
            pwv4: e.pwv4(),
            pwv5: e.pwv5(),
            pwv6: e.pwv6(),
            pwv7: e.pwv7(),
            pwvc: gain.map(|g| (g * 100.0).round().clamp(0.0, f32::from(u16::MAX)) as u16),
        })
    }

    /// `.DAT` waveform sections, in rekordbox's order: PWAV, PWV2.
    pub fn dat_sections(&self) -> Result<Vec<Vec<u8>>, WriteError> {
        Ok(vec![write::pwav(&self.pwav)?, write::pwv2(&self.pwv2)?])
    }

    /// `.EXT` waveform sections, in rekordbox's order: PWV3, PWV5, PWV4.
    pub fn ext_sections(&self) -> Result<Vec<Vec<u8>>, WriteError> {
        Ok(vec![
            write::pwv3(&self.pwv3)?,
            write::pwv5(&self.pwv5)?,
            write::pwv4(&self.pwv4)?,
        ])
    }

    /// `.2EX` waveform sections, in rekordbox's order: PWV7, PWV6, PWVC.
    pub fn ex2_sections(&self) -> Result<Vec<Vec<u8>>, WriteError> {
        Ok(vec![
            write::pwv7(&self.pwv7)?,
            write::pwv6(&self.pwv6)?,
            write::pwvc(self.pwvc)?,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binrw::{BinRead, BinWrite};
    use rekordcrate::anlz::{Content, ContentKind, ANLZ};
    use std::io::Cursor;

    /// Columns that sweep energy from the low band to the high band, with
    /// leading silence, over `n` columns.
    fn sweep_columns(n: usize) -> WaveformColumns {
        let mut c = WaveformColumns::default();
        for i in 0..n {
            let t = i as f32 / n.max(1) as f32;
            let silent = i < 10;
            let band = |centre: f32| {
                if silent {
                    0
                } else {
                    (255.0 * (1.0 - 3.0 * (t - centre).abs()).max(0.0)) as u8
                }
            };
            c.low.push(band(0.0));
            c.mid.push(band(0.5));
            c.high.push(band(1.0));
            c.mono.push(if silent { 0 } else { (i % 32) as u8 });
        }
        c
    }

    #[test]
    fn mono_packing_is_bit_exact() {
        assert_eq!(pack_mono(0, 7), 0xE0);
        assert_eq!(pack_mono(21, 1), 0x35);
        assert_eq!(pack_mono(31, 0), 0x1F);
        assert_eq!(pack_mono(99, 99), 0xFF);
        for b in 0..=u8::MAX {
            let (h, w) = unpack_mono(b);
            assert_eq!(pack_mono(h, w), b);
        }
        assert_eq!(pack_tiny(14), 0x0E);
        assert_eq!(pack_tiny(200), 0x0F);
    }

    #[test]
    fn pwv5_field_extraction() {
        // 0xE448 from the reference PWV5: R 7, G 1, B 0, height 18.
        let c = unpack_color(0xE448);
        assert_eq!(
            c,
            ColorColumn {
                red: 7,
                green: 1,
                blue: 0,
                height: 18
            }
        );
        assert_eq!(pack_color(c), 0xE448);
        assert_eq!(
            pack_color(ColorColumn {
                red: 1,
                green: 0,
                blue: 0,
                height: 0
            }),
            0x2000
        );
        assert_eq!(
            pack_color(ColorColumn {
                red: 0,
                green: 0,
                blue: 1,
                height: 0
            }),
            0x0080
        );
        assert_eq!(
            pack_color(ColorColumn {
                red: 0,
                green: 0,
                blue: 0,
                height: 1
            }),
            0x0004
        );
        for v in (0..=u16::MAX).filter(|v| v & 3 == 0) {
            assert_eq!(pack_color(unpack_color(v)), v);
        }
    }

    const fn linear(gain: f32) -> Curve {
        Curve { gain, gamma: 1.0 }
    }

    /// Identity curves, unit track gain, no release: encoder structure
    /// without the fitted constants.
    const LINEAR: WaveformCalibration = WaveformCalibration {
        height: linear(1.0),
        preview_height: linear(1.0),
        tiny_height: linear(1.0),
        whiteness_weights: [1.0; 3],
        whiteness: linear(1.0),
        silent_whiteness: WHITENESS_MAX,
        color: [linear(1.0); 3],
        pwv4_height: linear(1.0),
        pwv4_complement: 255,
        pwv4_bands: [linear(1.0); 4],
        auto_gain: [AutoGain {
            stat: LevelStat::Mean,
            target: 1.0,
            min: 1.0,
            max: 1.0,
        }; 3],
        detail_release: [0.0; 3],
        detail: [linear(1.0); 3],
        overview: [linear(1.0); 3],
    };

    #[test]
    fn previews_have_fixed_sizes_and_detail_tracks_columns() {
        for n in [0usize, 1, 7, 399, 400, 401, 1199, 1201, 45_001] {
            for cal in [&LINEAR, &CALIBRATION] {
                let w = Waveforms::encode(&sweep_columns(n), cal).unwrap();
                assert_eq!((w.pwv3.len(), w.pwv5.len(), w.pwv7.len()), (n, n, n));
                if n == 0 {
                    assert!(w.pwav.iter().all(|&b| b == pack_mono(0, WHITENESS_MAX)));
                    assert!(w.pwv4.iter().all(|c| c[2..] == [0; 4]));
                }
            }
        }
    }

    #[test]
    fn encoders_follow_the_bands() {
        let w = Waveforms::encode(&sweep_columns(3000), &LINEAR).unwrap();
        // Leading silence: 0xE0 as in rekordbox's files.
        assert_eq!(w.pwv3[0], 0xE0);
        // Start: low band only → red PWV5, PWV7 low byte (index 0).
        let start = unpack_color(w.pwv5[20]);
        assert!(start.red == COLOR_MAX && start.blue == 0);
        assert!(w.pwv7[20][0] > 100 && w.pwv7[20][1] == 0);
        assert_eq!(unpack_mono(w.pwv3[20]).1, 0);
        // Middle: mid band → green / PWV7 byte 1.
        let mid = unpack_color(w.pwv5[1500]);
        assert_eq!(mid.green, COLOR_MAX);
        assert_eq!(w.pwv7[1500][1], BAND_BYTE_MAX);
        // End: high band only → blue, full whiteness, PWV7 byte 2.
        let end = unpack_color(w.pwv5[2999]);
        assert!(end.blue == COLOR_MAX && end.red == 0);
        assert!(w.pwv7[2999][2] > 100);
        assert_eq!(unpack_mono(w.pwv3[2999]).1, WHITENESS_MAX);
        // Previews: same ordering at coarse resolution.
        assert!(w.pwv4[50][3] > w.pwv4[50][5] && w.pwv4[1190][5] > w.pwv4[1190][3]);
        assert!(w.pwv6[50][0] > w.pwv6[50][2] && w.pwv6[1190][2] > w.pwv6[1190][0]);
        // Preview heights are the column peak over the range (31 here).
        assert_eq!(unpack_mono(w.pwav[200]).0, 31);
        assert_eq!(w.pwv2[50], TINY_HEIGHT_MAX);
        assert!(w
            .pwv4
            .iter()
            .all(|c| u16::from(c[0]) + u16::from(c[1]) == 255));
        assert_eq!(w.pwvc, [100; 3]);
    }

    #[test]
    fn curves_gain_and_release_apply_per_band() {
        let c = WaveformColumns {
            low: vec![64, 0, 0, 0],
            mid: vec![64, 0, 0, 0],
            high: vec![64, 0, 0, 0],
            mono: vec![10, 10, 10, 10],
        };
        let cal = WaveformCalibration {
            detail: [
                linear(4.0),
                linear(1.0),
                Curve {
                    gain: 1.0,
                    gamma: 0.5,
                },
            ],
            ..LINEAR
        };
        let w = Waveforms::encode(&c, &cal).unwrap();
        // low: 64/255×4 → 1.0 → 127; mid: 0.251 → 32; high: √0.251 → 64.
        assert_eq!(w.pwv7[0], [127, 32, 64]);
        assert_eq!(w.pwv7[1], [0, 0, 0]);
        // Release halves the held value per column.
        let cal = WaveformCalibration {
            detail_release: [0.5, 0.0, 0.0],
            ..LINEAR
        };
        let w = Waveforms::encode(&c, &cal).unwrap();
        let low: Vec<u8> = w.pwv7.iter().map(|v| v[0]).collect();
        assert_eq!(low, [32, 16, 8, 4]);
        // Track gain: mean of the low band 16/255 → target 32/255 gives ×2,
        // applied to PWV6/PWV7 and written to PWVC.
        let mut gain = LINEAR.auto_gain;
        gain[0] = AutoGain {
            stat: LevelStat::Mean,
            target: 32.0 / 255.0,
            min: 0.5,
            max: 2.0,
        };
        let cal = WaveformCalibration {
            auto_gain: gain,
            ..LINEAR
        };
        let w = Waveforms::encode(&c, &cal).unwrap();
        assert_eq!(w.pwvc, [200, 100, 100]);
        assert_eq!(w.pwv7[0], [64, 32, 32]);
    }

    #[test]
    fn colour_is_normalised_to_the_loudest_band() {
        let c = WaveformColumns {
            low: vec![200, 0],
            mid: vec![100, 0],
            high: vec![50, 0],
            mono: vec![31, 0],
        };
        let w = Waveforms::encode(&c, &LINEAR).unwrap();
        assert_eq!(
            unpack_color(w.pwv5[0]),
            ColorColumn {
                red: 7,
                green: 4,
                blue: 2,
                height: 31
            }
        );
        assert_eq!(
            unpack_color(w.pwv5[1]),
            ColorColumn {
                red: 0,
                green: 0,
                blue: 0,
                height: 0
            }
        );
    }

    #[test]
    fn inconsistent_columns_are_rejected() {
        let mut c = sweep_columns(10);
        c.high.pop();
        assert_eq!(
            Waveforms::encode(&c, &LINEAR).unwrap_err(),
            EncodeError::InconsistentColumns {
                low: 10,
                mid: 10,
                high: 9,
                mono: 10
            }
        );
    }

    /// Every section parses with rekordcrate (independent parser), fields
    /// agree where rekordcrate's field layout matches djl-analysis, and
    /// writing the parse back is byte-identical.
    #[test]
    fn round_trip_through_rekordcrate() {
        let w = Waveforms::encode(&sweep_columns(4321), &CALIBRATION).unwrap();
        let sections: Vec<Vec<u8>> = [
            w.dat_sections().unwrap(),
            w.ext_sections().unwrap(),
            w.ex2_sections().unwrap(),
        ]
        .concat();
        let bytes = write::file(&sections).unwrap();
        let anlz = ANLZ::read(&mut Cursor::new(&bytes)).unwrap();
        assert_eq!(anlz.sections.len(), 8);

        let mut seen = Vec::new();
        for s in &anlz.sections {
            match (&s.header.kind, &s.content) {
                (ContentKind::WaveformPreview, Content::WaveformPreview(p)) => {
                    let got: Vec<(u8, u8)> =
                        p.data.iter().map(|c| (c.height(), c.whiteness())).collect();
                    let want: Vec<(u8, u8)> = w.pwav.iter().map(|&b| unpack_mono(b)).collect();
                    assert_eq!(got, want);
                    seen.push("PWAV");
                }
                (ContentKind::TinyWaveformPreview, Content::TinyWaveformPreview(p)) => {
                    // rekordcrate 0.3.0 maps `height` to the high nibble;
                    // djl-analysis and the reference files use the low one,
                    // so compare raw bytes.
                    let got: Vec<u8> = p.data.iter().map(|c| c.into_bytes()[0]).collect();
                    assert_eq!(got, w.pwv2);
                    seen.push("PWV2");
                }
                (ContentKind::WaveformDetail, Content::WaveformDetail(p)) => {
                    let got: Vec<(u8, u8)> =
                        p.data.iter().map(|c| (c.height(), c.whiteness())).collect();
                    let want: Vec<(u8, u8)> = w.pwv3.iter().map(|&b| unpack_mono(b)).collect();
                    assert_eq!(got, want);
                    seen.push("PWV3");
                }
                (ContentKind::WaveformColorPreview, Content::WaveformColorPreview(p)) => {
                    let got: Vec<[u8; 4]> = p
                        .data
                        .iter()
                        .map(|c| {
                            [
                                c.energy_bottom_half_freq,
                                c.energy_bottom_third_freq,
                                c.energy_mid_third_freq,
                                c.energy_top_third_freq,
                            ]
                        })
                        .collect();
                    let want: Vec<[u8; 4]> =
                        w.pwv4.iter().map(|c| [c[2], c[3], c[4], c[5]]).collect();
                    assert_eq!(got, want);
                    seen.push("PWV4");
                }
                (ContentKind::WaveformColorDetail, Content::WaveformColorDetail(p)) => {
                    // rekordcrate 0.3.0's bitfield order disagrees with
                    // djl-analysis (it reads red from the low bits), so
                    // compare the raw big-endian words.
                    let got: Vec<u16> = p
                        .data
                        .iter()
                        .map(|c| u16::from_be_bytes(c.into_bytes()))
                        .collect();
                    assert_eq!(got, w.pwv5);
                    seen.push("PWV5");
                }
                (ContentKind::Unknown(tag), Content::Unknown(_)) => {
                    // No PWV6/PWV7/PWVC types in 0.3.0: check framing here,
                    // payload via the byte-identical write-back below.
                    let (len_header, len_tag) = match tag {
                        b"PWV7" => (24, 24 + 3 * 4321),
                        b"PWV6" => (20, 20 + 3 * 1200),
                        b"PWVC" => (14, 20),
                        other => panic!("unexpected section {other:?}"),
                    };
                    assert_eq!((s.header.size, s.header.total_size), (len_header, len_tag));
                    seen.push(match tag {
                        b"PWV7" => "PWV7",
                        b"PWV6" => "PWV6",
                        _ => "PWVC",
                    });
                }
                (kind, _) => panic!("unexpected section {kind:?}"),
            }
        }
        assert_eq!(
            seen,
            ["PWAV", "PWV2", "PWV3", "PWV5", "PWV4", "PWV7", "PWV6", "PWVC"]
        );

        let mut back = Cursor::new(Vec::new());
        anlz.write(&mut back).unwrap();
        assert_eq!(back.into_inner(), bytes);
    }
}
