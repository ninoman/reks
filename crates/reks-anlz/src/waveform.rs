//! Pioneer waveform encoders (Phase 3 task 7): turn reks waveform columns
//! (150/s, `reks_analysis::waveform`) into the PWAV/PWV2/PWV3/PWV4/PWV5/
//! PWV6/PWV7/PWVC payloads, and frame them as sections via [`crate::write`].
//!
//! Formats: djl-analysis "Analysis Files" and crate-digger
//! `rekordbox_anlz.ksy` (R§3.3), checked against the rekordbox 6 reference
//! export in rekordcrate's test data. How rekordbox scales band energies is
//! undocumented: every scale below comes from [`WaveformCalibration`], whose
//! [`PLACEHOLDER_CALIBRATION`] values are first guesses from the value ranges
//! of two reference tracks. Task 8 fits them against rekordbox output.

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

/// Scaling from reks columns to Pioneer bytes. All fields are placeholders
/// until task 8 fits them (see [`PLACEHOLDER_CALIBRATION`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveformCalibration {
    /// Per-band gain (low, mid, high) on the 0..=1 band peak, before gamma.
    pub band_gain: [f32; 3],
    /// Per-band gamma (low, mid, high) applied after gain and clamping.
    pub band_gamma: [f32; 3],
    /// High-band share of the band sum that maps to full whiteness (7) in
    /// PWAV/PWV3; whiteness scales linearly below it.
    pub whiteness_full_ratio: f32,
    /// Whiteness of a silent column. The reference files hold 0xE0
    /// (whiteness 7, height 0) for leading silence in PWV3.
    pub silent_whiteness: u8,
    /// Gain on the mono height for PWAV/PWV2 (reference PWAV peaks at
    /// 22–24 where PWV3 peaks at 31, so rekordbox scales previews down).
    pub preview_height_gain: f32,
    /// Full-scale value of PWV4 bytes 2–5 (reference maximum ≈ 120).
    pub pwv4_band_max: u8,
    /// Full-scale value of PWV6 bytes (reference maximum ≈ 43).
    pub pwv6_band_max: u8,
    /// Full-scale value of PWV7 bytes (reference maximum 127).
    pub pwv7_band_max: u8,
    /// PWVC payload, UNVERIFIED semantics. The two reference tracks hold
    /// (0x5b, 0x8a, 0x18c) and (0x91, 0xf9, 0x1f4), so it is per-track, not
    /// the constant the phase doc assumed; we write the first until the
    /// meaning is known.
    pub pwvc: [u16; 3],
}

/// First-guess calibration: linear bands, no gain. Task 8 replaces it.
pub const PLACEHOLDER_CALIBRATION: WaveformCalibration = WaveformCalibration {
    band_gain: [1.0, 1.0, 1.0],
    band_gamma: [1.0, 1.0, 1.0],
    whiteness_full_ratio: 0.5,
    silent_whiteness: WHITENESS_MAX,
    preview_height_gain: 0.8,
    pwv4_band_max: 120,
    pwv6_band_max: 43,
    pwv7_band_max: 127,
    pwvc: [0x005b, 0x008a, 0x018c],
};

impl Default for WaveformCalibration {
    fn default() -> Self {
        PLACEHOLDER_CALIBRATION
    }
}

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
    /// PWV4: whiteness ×2 (placeholder), energy < 10 kHz, low, mid, high.
    pub pwv4: [[u8; 6]; COLOR_PREVIEW_COLUMNS],
    /// PWV5: `R:3 G:3 B:3 H:5 0:2`, 150 columns/s.
    pub pwv5: Vec<u16>,
    /// PWV6: mid, high, low.
    pub pwv6: [[u8; 3]; COLOR_PREVIEW_COLUMNS],
    /// PWV7: mid, high, low, 150 columns/s.
    pub pwv7: Vec<[u8; 3]>,
    /// PWVC payload (UNVERIFIED).
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

struct Encoder<'a> {
    c: &'a WaveformColumns,
    cal: &'a WaveformCalibration,
}

impl Encoder<'_> {
    /// Calibrated band value (0 low, 1 mid, 2 high) of column `i`, 0..=1.
    fn shaped(&self, band: usize, i: usize) -> f32 {
        let raw = [&self.c.low, &self.c.mid, &self.c.high][band][i];
        let x = (self.cal.band_gain[band] * f32::from(raw) / 255.0).clamp(0.0, 1.0);
        x.powf(self.cal.band_gamma[band])
    }

    /// High-band share of the raw band sum over `r`, or `None` when silent.
    fn high_ratio(&self, r: std::ops::Range<usize>) -> Option<f32> {
        let (mut high, mut total) = (0u64, 0u64);
        for i in r {
            let h = u64::from(self.c.high[i]);
            high += h;
            total += u64::from(self.c.low[i]) + u64::from(self.c.mid[i]) + h;
        }
        (total > 0).then(|| high as f32 / total as f32)
    }

    fn whiteness(&self, r: std::ops::Range<usize>) -> u8 {
        match self.high_ratio(r) {
            None => self.cal.silent_whiteness.min(WHITENESS_MAX),
            Some(ratio) => level(ratio / self.cal.whiteness_full_ratio, WHITENESS_MAX),
        }
    }

    fn max_shaped(&self, band: usize, r: std::ops::Range<usize>) -> f32 {
        r.map(|i| self.shaped(band, i)).fold(0.0, f32::max)
    }

    /// PWAV-style preview height (0..=31) over `r`.
    fn preview_height(&self, r: std::ops::Range<usize>) -> u8 {
        let peak = r.map(|i| self.c.mono[i]).max().unwrap_or(0);
        level(
            f32::from(peak) * self.cal.preview_height_gain / f32::from(MONO_MAX),
            MONO_MAX,
        )
    }

    fn pwav(&self) -> [u8; PWAV_COLUMNS] {
        let n = self.c.len();
        std::array::from_fn(|j| {
            let r = source_range(j, PWAV_COLUMNS, n);
            pack_mono(self.preview_height(r.clone()), self.whiteness(r))
        })
    }

    fn pwv2(&self) -> [u8; PWV2_COLUMNS] {
        let n = self.c.len();
        std::array::from_fn(|j| {
            let h = self.preview_height(source_range(j, PWV2_COLUMNS, n));
            pack_tiny(level(f32::from(h) / f32::from(MONO_MAX), TINY_HEIGHT_MAX))
        })
    }

    fn pwv3(&self) -> Vec<u8> {
        (0..self.c.len())
            .map(|i| pack_mono(self.c.mono[i], self.whiteness(i..i + 1)))
            .collect()
    }

    fn pwv4(&self) -> [[u8; 6]; COLOR_PREVIEW_COLUMNS] {
        let n = self.c.len();
        let max = self.cal.pwv4_band_max;
        std::array::from_fn(|j| {
            let r = source_range(j, COLOR_PREVIEW_COLUMNS, n);
            let [low, mid, high] = [0, 1, 2].map(|b| level(self.max_shaped(b, r.clone()), max));
            // Placeholders until task 8: byte 2 ("energy < 10 kHz") as the
            // loudest band; byte 0 tracks byte 2 (their means agree in the
            // reference files); byte 1 falls as the high-band share rises.
            let below_10k = low.max(mid).max(high);
            let ratio = self.high_ratio(r).unwrap_or(0.0);
            [
                below_10k,
                level(1.0 - ratio, u8::MAX),
                below_10k,
                low,
                mid,
                high,
            ]
        })
    }

    fn pwv5(&self) -> Vec<u16> {
        (0..self.c.len())
            .map(|i| {
                pack_color(ColorColumn {
                    red: level(self.shaped(0, i), COLOR_MAX),
                    green: level(self.shaped(1, i), COLOR_MAX),
                    blue: level(self.shaped(2, i), COLOR_MAX),
                    height: self.c.mono[i],
                })
            })
            .collect()
    }

    /// Mid, high, low: the PWV6/PWV7 byte order.
    fn three_band(&self, r: std::ops::Range<usize>, max: u8) -> [u8; 3] {
        [1, 2, 0].map(|b| level(self.max_shaped(b, r.clone()), max))
    }

    fn pwv6(&self) -> [[u8; 3]; COLOR_PREVIEW_COLUMNS] {
        let n = self.c.len();
        std::array::from_fn(|j| {
            self.three_band(
                source_range(j, COLOR_PREVIEW_COLUMNS, n),
                self.cal.pwv6_band_max,
            )
        })
    }

    fn pwv7(&self) -> Vec<[u8; 3]> {
        (0..self.c.len())
            .map(|i| self.three_band(i..i + 1, self.cal.pwv7_band_max))
            .collect()
    }
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
        let e = Encoder { c: columns, cal };
        Ok(Self {
            pwav: e.pwav(),
            pwv2: e.pwv2(),
            pwv3: e.pwv3(),
            pwv4: e.pwv4(),
            pwv5: e.pwv5(),
            pwv6: e.pwv6(),
            pwv7: e.pwv7(),
            pwvc: cal.pwvc,
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

    #[test]
    fn previews_have_fixed_sizes_and_detail_tracks_columns() {
        for n in [0usize, 1, 7, 399, 400, 401, 1199, 1201, 45_001] {
            let w = Waveforms::encode(&sweep_columns(n), &PLACEHOLDER_CALIBRATION).unwrap();
            assert_eq!((w.pwv3.len(), w.pwv5.len(), w.pwv7.len()), (n, n, n));
            if n == 0 {
                assert!(w.pwav.iter().all(|&b| b == pack_mono(0, WHITENESS_MAX)));
                assert!(w.pwv4.iter().all(|c| c[2..] == [0; 4]));
            }
        }
    }

    #[test]
    fn encoders_follow_the_bands() {
        let w = Waveforms::encode(&sweep_columns(3000), &PLACEHOLDER_CALIBRATION).unwrap();
        // Leading silence: 0xE0 as in rekordbox's files.
        assert_eq!(w.pwv3[0], 0xE0);
        // Start: low band only → red PWV5, PWV7 low byte (index 2).
        let start = unpack_color(w.pwv5[20]);
        assert!(start.red > 5 && start.blue == 0);
        assert!(w.pwv7[20][2] > 100 && w.pwv7[20][1] == 0);
        assert_eq!(unpack_mono(w.pwv3[20]).1, 0);
        // Middle: mid band → green / PWV7 byte 0.
        let mid = unpack_color(w.pwv5[1500]);
        assert_eq!(mid.green, COLOR_MAX);
        assert_eq!(w.pwv7[1500][0], 127);
        // End: high band only → blue, full whiteness, PWV7 byte 1.
        let end = unpack_color(w.pwv5[2999]);
        assert!(end.blue > 5 && end.red == 0);
        assert!(w.pwv7[2999][1] > 100);
        assert_eq!(unpack_mono(w.pwv3[2999]).1, WHITENESS_MAX);
        // Previews: same ordering at coarse resolution.
        assert!(w.pwv4[50][3] > w.pwv4[50][5] && w.pwv4[1190][5] > w.pwv4[1190][3]);
        assert!(w.pwv6[50][2] > w.pwv6[50][1] && w.pwv6[1190][1] > w.pwv6[1190][2]);
        assert!(w.pwv6.iter().flatten().all(|&b| b <= 43));
        // Preview height is the scaled column peak (31 × 0.8 → 25).
        assert_eq!(unpack_mono(w.pwav[200]).0, 25);
        assert_eq!(w.pwv2[50], 12);
    }

    #[test]
    fn gain_and_gamma_apply_per_band() {
        let c = WaveformColumns {
            low: vec![64],
            mid: vec![64],
            high: vec![64],
            mono: vec![10],
        };
        let cal = WaveformCalibration {
            band_gain: [4.0, 1.0, 1.0],
            band_gamma: [1.0, 1.0, 0.5],
            ..PLACEHOLDER_CALIBRATION
        };
        let w = Waveforms::encode(&c, &cal).unwrap();
        // low: 64/255×4 → 1.0 → 127; mid: 0.251 → 32; high: √0.251 → 64.
        assert_eq!(w.pwv7[0], [32, 64, 127]);
    }

    #[test]
    fn inconsistent_columns_are_rejected() {
        let mut c = sweep_columns(10);
        c.high.pop();
        assert_eq!(
            Waveforms::encode(&c, &PLACEHOLDER_CALIBRATION).unwrap_err(),
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
        let w = Waveforms::encode(&sweep_columns(4321), &PLACEHOLDER_CALIBRATION).unwrap();
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
