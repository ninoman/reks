//! Comparing our ANLZ files with rekordbox's: section layout and a waveform
//! error metric. Used by the golden tests; task 8 (calibration) and task 12
//! (parity) can reuse it.
//!
//! Waveforms are compared by error, not equality (our analysis is not
//! rekordbox's): per channel, the mean absolute difference over the columns
//! both files have, divided by the channel's full scale. Bit fields use
//! their field maximum (height 31, whiteness 7, PWV5 colour 7, PWV2 height
//! 15); byte channels (PWV4/PWV6/PWV7) use the larger of the two files'
//! maxima for that channel, because rekordbox's bytes do not span 0–255.

use crate::read::{self, ReadError, Section};
use crate::write::{PWAV, PWV2, PWV3, PWV4, PWV5, PWV6, PWV7};

/// One section's tag and lengths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionLayout {
    pub tag: String,
    pub len_header: usize,
    pub len_tag: usize,
}

/// The sections of a file, in order.
pub fn layout(bytes: &[u8]) -> Result<Vec<SectionLayout>, ReadError> {
    Ok(read::sections(bytes)?
        .iter()
        .map(|s| SectionLayout {
            tag: s.tag_str(),
            len_header: s.len_header,
            len_tag: s.bytes.len(),
        })
        .collect())
}

/// Waveform error of one section pair.
#[derive(Debug, Clone, PartialEq)]
pub struct WaveformError {
    pub tag: String,
    /// Columns in ours and theirs.
    pub columns: (usize, usize),
    /// Channel names, in the order of [`Self::error`].
    pub channels: &'static [&'static str],
    /// Normalised mean absolute error per channel, 0 (equal) to 1.
    pub error: Vec<f64>,
}

impl WaveformError {
    /// Mean of the channel errors.
    pub fn mean(&self) -> f64 {
        if self.error.is_empty() {
            0.0
        } else {
            self.error.iter().sum::<f64>() / self.error.len() as f64
        }
    }
}

/// How a waveform section's columns split into channels.
struct Shape {
    entry: usize,
    channels: &'static [&'static str],
    /// Fixed full scale per channel; `None` = from the data.
    scale: &'static [Option<f64>],
    decode: fn(&[u8]) -> Vec<f64>,
}

fn mono(e: &[u8]) -> Vec<f64> {
    vec![f64::from(e[0] & 0x1F), f64::from(e[0] >> 5)]
}

fn bytes(e: &[u8]) -> Vec<f64> {
    e.iter().map(|&b| f64::from(b)).collect()
}

fn shape(tag: [u8; 4]) -> Option<Shape> {
    const MONO: &[&str] = &["height", "whiteness"];
    const MONO_SCALE: &[Option<f64>] = &[Some(31.0), Some(7.0)];
    const THREE: &[&str] = &["mid", "high", "low"];
    const THREE_SCALE: &[Option<f64>] = &[None, None, None];
    Some(match tag {
        PWAV | PWV3 => Shape {
            entry: 1,
            channels: MONO,
            scale: MONO_SCALE,
            decode: mono,
        },
        PWV2 => Shape {
            entry: 1,
            channels: &["height"],
            scale: &[Some(15.0)],
            decode: |e| vec![f64::from(e[0] & 0x0F)],
        },
        PWV4 => Shape {
            entry: 6,
            channels: &["byte0", "byte1", "below10k", "low", "mid", "high"],
            scale: &[None; 6],
            decode: bytes,
        },
        PWV5 => Shape {
            entry: 2,
            channels: &["red", "green", "blue", "height"],
            scale: &[Some(7.0), Some(7.0), Some(7.0), Some(31.0)],
            decode: |e| {
                let v = u16::from_be_bytes([e[0], e[1]]);
                [13, 10, 7]
                    .iter()
                    .map(|s| f64::from((v >> s) & 7))
                    .chain(std::iter::once(f64::from((v >> 2) & 0x1F)))
                    .collect()
            },
        },
        PWV6 | PWV7 => Shape {
            entry: 3,
            channels: THREE,
            scale: THREE_SCALE,
            decode: bytes,
        },
        _ => return None,
    })
}

fn columns(s: &Section<'_>, sh: &Shape) -> Vec<Vec<f64>> {
    s.payload()
        .chunks_exact(sh.entry)
        .map(|e| (sh.decode)(e))
        .collect()
}

/// Error between two waveform sections of the same tag; `None` when the
/// tags differ or are not waveform sections.
pub fn waveform_error(ours: &Section<'_>, theirs: &Section<'_>) -> Option<WaveformError> {
    if ours.tag != theirs.tag {
        return None;
    }
    let sh = shape(ours.tag)?;
    let (a, b) = (columns(ours, &sh), columns(theirs, &sh));
    let n = a.len().min(b.len());
    let error = (0..sh.channels.len())
        .map(|ch| {
            let scale = sh.scale[ch]
                .unwrap_or_else(|| a.iter().chain(&b).map(|c| c[ch]).fold(1.0, f64::max));
            if n == 0 {
                return 0.0;
            }
            let sum: f64 = (0..n).map(|i| (a[i][ch] - b[i][ch]).abs()).sum();
            sum / n as f64 / scale
        })
        .collect();
    Some(WaveformError {
        tag: ours.tag_str(),
        columns: (a.len(), b.len()),
        channels: sh.channels,
        error,
    })
}

/// Waveform errors for every waveform tag both files have (first section of
/// each tag).
pub fn waveform_errors(ours: &[u8], theirs: &[u8]) -> Result<Vec<WaveformError>, ReadError> {
    let (a, b) = (read::sections(ours)?, read::sections(theirs)?);
    Ok(a.iter()
        .filter_map(|s| {
            let t = b.iter().find(|t| t.tag == s.tag)?;
            waveform_error(s, t)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write;

    #[test]
    fn identical_sections_have_no_error() {
        let a = write::file(&[write::pwv3(&[0x1F, 0xE0, 0x35]).unwrap()]).unwrap();
        let e = waveform_errors(&a, &a).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].error, [0.0, 0.0]);
        assert_eq!(e[0].columns, (3, 3));
    }

    #[test]
    fn errors_are_normalised_per_channel() {
        // Heights 31 vs 0 on one of two columns: 0.5; whiteness equal.
        let a = write::file(&[write::pwv3(&[31, 0]).unwrap()]).unwrap();
        let b = write::file(&[write::pwv3(&[0, 0, 5]).unwrap()]).unwrap();
        let e = &waveform_errors(&a, &b).unwrap()[0];
        assert_eq!(e.columns, (2, 3));
        assert_eq!(e.error, [0.5, 0.0]);
        assert_eq!(e.mean(), 0.25);
        // Byte channels scale by the larger maximum: 40 vs 20 → 0.5.
        let a = write::file(&[write::pwv7(&[[40, 0, 0]]).unwrap()]).unwrap();
        let b = write::file(&[write::pwv7(&[[20, 0, 0]]).unwrap()]).unwrap();
        let e = &waveform_errors(&a, &b).unwrap()[0];
        assert_eq!(e.error, [0.5, 0.0, 0.0]);
        // PWV5 channels.
        let a = write::file(&[write::pwv5(&[0xE000]).unwrap()]).unwrap();
        let b = write::file(&[write::pwv5(&[0x0000]).unwrap()]).unwrap();
        let e = &waveform_errors(&a, &b).unwrap()[0];
        assert_eq!(e.error, [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn layout_lists_sections() {
        let f = write::file(&[
            write::ppth("/a.mp3").unwrap(),
            write::pwv2(&[0; 100]).unwrap(),
        ])
        .unwrap();
        let l = layout(&f).unwrap();
        assert_eq!(
            l,
            [
                SectionLayout {
                    tag: "PPTH".into(),
                    len_header: 16,
                    len_tag: 16 + 14
                },
                SectionLayout {
                    tag: "PWV2".into(),
                    len_header: 20,
                    len_tag: 120
                }
            ]
        );
    }
}
