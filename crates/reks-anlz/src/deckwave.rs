//! Deck waveform columns from a stick's ANLZ files (D32), so a track played
//! off a stick draws its waveform at once instead of decoding its audio.
//!
//! The inverse of two encoders in [`crate::waveform`], both 150 columns/s:
//! `PWV7` (`.2EX`) gives the low, mid and high bands, undoing the detail
//! curves and the track's `PWVC` gains; `PWV3` (`.EXT`) gives the mono
//! height, undoing the height curve. Approximate by nature: the bands come
//! back as the encoder's peak-hold envelope (decays are slower than the
//! audio's), 7-bit and 5-bit fields lose resolution, and rekordbox's own
//! scaling is only modelled by [`WaveformCalibration`]. Files without `PWV7`
//! (no `.2EX`, rekordbox before 6) give an error; callers fall back to the
//! audio.

use reks_analysis::cache::RekordboxEntry;
use reks_analysis::calib::Curve;
use reks_analysis::waveform::{WaveformColumns, MONO_MAX};

use crate::rbwave::{chunks, entries, find, pwvc, RbWaveError};
use crate::read;
use crate::waveform::{WaveformCalibration, BAND_BYTE_MAX};
use crate::write::{PWV3, PWV7, PWVC};

/// Columns from the `.EXT` and `.2EX` bytes. Without `PWVC` the gains are 1;
/// without `PWV3` the mono height is the loudest band's.
pub fn columns(
    ext: &[u8],
    ex2: &[u8],
    cal: &WaveformCalibration,
) -> Result<WaveformColumns, RbWaveError> {
    let ext = read::sections(ext)?;
    let ex2 = read::sections(ex2)?;
    let pwv7 = chunks::<3>(entries(find(&ex2, PWV7, "PWV7")?, "PWV7", 3)?);
    let gain = match find(&ex2, PWVC, "PWVC") {
        Ok(s) => pwvc(s)?.map(|g| f32::from(g) / 100.0),
        Err(_) => [1.0; 3],
    };
    let pwv3 = match find(&ext, PWV3, "PWV3") {
        Ok(s) => Some(entries(s, "PWV3", 1)?),
        Err(_) => None,
    };
    let mut c = WaveformColumns::default();
    for (i, col) in pwv7.iter().enumerate() {
        let bands: [u8; 3] = std::array::from_fn(|b| band(col[b], cal.detail[b], gain[b]));
        let [low, mid, high] = bands;
        c.low.push(low);
        c.mid.push(mid);
        c.high.push(high);
        c.mono.push(match pwv3.and_then(|p| p.get(i)) {
            Some(&byte) => mono(byte & 0x1F, cal.height),
            None => scale(f32::from(low.max(mid).max(high)) / 255.0, MONO_MAX),
        });
    }
    Ok(c)
}

/// Columns of a library track's mapped rekordbox analysis (D10), for the
/// track list's thumbnails; `None` without `PWV7` or when it does not decode.
pub fn entry_columns(entry: &RekordboxEntry) -> Option<WaveformColumns> {
    columns(&entry.ext, &entry.ex2, &WaveformCalibration::default())
        .ok()
        .filter(|c| !c.is_empty())
}

/// A `PWV7` band byte back to a column band peak (0..=255).
pub fn band(byte: u8, curve: Curve, gain: f32) -> u8 {
    let y = f32::from(byte.min(BAND_BYTE_MAX)) / f32::from(BAND_BYTE_MAX);
    scale(invert(y, curve) / gain.max(f32::MIN_POSITIVE), 255)
}

/// A `PWV3` height (0..=31) back to a column mono peak (0..=31).
pub fn mono(height: u8, curve: Curve) -> u8 {
    let y = f32::from(height.min(MONO_MAX)) / f32::from(MONO_MAX);
    scale(invert(y, curve), MONO_MAX)
}

/// `x` with `curve.apply(x) == y`, for `y` in 0..=1.
fn invert(y: f32, curve: Curve) -> f32 {
    if y <= 0.0 || curve.gain <= 0.0 || curve.gamma <= 0.0 {
        return 0.0;
    }
    y.powf(1.0 / curve.gamma) / curve.gain
}

/// `x` in 0..=1 to 0..=max, rounded, saturating.
fn scale(x: f32, max: u8) -> u8 {
    (x * f32::from(max)).round().clamp(0.0, f32::from(max)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::waveform::{Waveforms, CALIBRATION};

    fn ex2_and_ext(w: &Waveforms) -> (Vec<u8>, Vec<u8>) {
        let ex2 = crate::write::file(&w.ex2_sections().unwrap()).unwrap();
        let ext = crate::write::file(&w.ext_sections().unwrap()).unwrap();
        (ext, ex2)
    }

    /// Steady bands: the envelope is the band, so encode then decode gives
    /// it back within the fields' resolution.
    #[test]
    fn steady_columns_round_trip() {
        let n = 600;
        let mut cols = WaveformColumns::default();
        for i in 0..n {
            let step = (i / 150) as u8; // a new level every second
            cols.low.push(40 + 40 * step);
            cols.mid.push(30 + 20 * step);
            cols.high.push(20 + 10 * step);
            cols.mono.push(10 + 4 * step);
        }
        let w = Waveforms::encode(&cols, &CALIBRATION).unwrap();
        let (ext, ex2) = ex2_and_ext(&w);
        let back = columns(&ext, &ex2, &CALIBRATION).unwrap();
        assert_eq!(back.len(), n);
        // Mid-second, past any envelope release.
        for i in [75, 225, 375, 525] {
            let close = |a: u8, b: u8, tol: u8| a.abs_diff(b) <= tol;
            assert!(
                close(back.low[i], cols.low[i], 6),
                "low {i}: {}",
                back.low[i]
            );
            assert!(
                close(back.mid[i], cols.mid[i], 6),
                "mid {i}: {}",
                back.mid[i]
            );
            assert!(
                close(back.high[i], cols.high[i], 6),
                "high {i}: {}",
                back.high[i]
            );
            assert!(
                close(back.mono[i], cols.mono[i], 1),
                "mono {i}: {}",
                back.mono[i]
            );
        }
    }

    #[test]
    fn without_pwv3_mono_follows_the_bands() {
        let mut cols = WaveformColumns::default();
        for _ in 0..10 {
            cols.low.push(255);
            cols.mid.push(0);
            cols.high.push(0);
            cols.mono.push(31);
        }
        let w = Waveforms::encode(&cols, &CALIBRATION).unwrap();
        let ex2 = crate::write::file(&w.ex2_sections().unwrap()).unwrap();
        let ext = crate::write::file(&[]).unwrap();
        let back = columns(&ext, &ex2, &CALIBRATION).unwrap();
        assert!(back.mono.iter().all(|&m| m > 0));
    }

    #[test]
    fn no_pwv7_is_an_error() {
        let empty = crate::write::file(&[]).unwrap();
        assert!(matches!(
            columns(&empty, &empty, &CALIBRATION),
            Err(RbWaveError::Missing("PWV7"))
        ));
    }

    #[test]
    fn silence_stays_silent() {
        assert_eq!(band(0, CALIBRATION.detail[2], 5.0), 0);
        assert_eq!(mono(0, CALIBRATION.height), 0);
    }
}
