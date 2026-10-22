//! rekordbox's own waveforms as [`Waveforms`] (D10, "rekordbox mapping"):
//! the PWAV/PWV2 (`.DAT`), PWV3/PWV4/PWV5 (`.EXT`) and PWV6/PWV7/PWVC
//! (`.2EX`) payloads of a rekordbox analysis, read with our own section
//! reader ([`crate::read`]; rekordcrate 0.3.0 can't read PWV6/PWV7/PWVC and
//! misreads PWV2/PWV5, D9).
//!
//! The payloads are kept as they are, not converted into reks's 150/s band
//! columns: our encoders (peak-hold envelope, per-track gain, gamma curves,
//! 3–7-bit fields) are not invertible, so a round trip through columns would
//! only approximate rekordbox's bytes. (That approximation is good enough to
//! draw a deck, [`crate::deckwave`], not to write a stick.) [`crate::build`] writes these
//! payloads back byte for byte with reks's grid and cues.

use crate::read::{self, ReadError, Section};
use crate::waveform::{Waveforms, COLOR_PREVIEW_COLUMNS, PWAV_COLUMNS, PWV2_COLUMNS};
use crate::write::{PWAV, PWV2, PWV3, PWV4, PWV5, PWV6, PWV7, PWVC};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RbWaveError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error("rekordbox's analysis has no {0} section")]
    Missing(&'static str),
    #[error("{tag}: {what}")]
    Shape { tag: &'static str, what: String },
}

/// rekordbox's waveforms from the bytes of its `.DAT`, `.EXT` and `.2EX`.
/// Every section must be present with the column counts rekordbox writes
/// (400 PWAV, 100 PWV2, 1200 PWV4/PWV6) and PWV3, PWV5 and PWV7 must have
/// the same number of 150/s columns.
pub fn read_waveforms(dat: &[u8], ext: &[u8], ex2: &[u8]) -> Result<Waveforms, RbWaveError> {
    let dat = read::sections(dat)?;
    let ext = read::sections(ext)?;
    let ex2 = read::sections(ex2)?;
    let pwav = fixed::<PWAV_COLUMNS, _>(preview(find(&dat, PWAV, "PWAV")?, "PWAV")?, "PWAV")?;
    let pwv2 = fixed::<PWV2_COLUMNS, _>(preview(find(&dat, PWV2, "PWV2")?, "PWV2")?, "PWV2")?;
    let pwv3 = entries(find(&ext, PWV3, "PWV3")?, "PWV3", 1)?.to_vec();
    let pwv4 = fixed::<COLOR_PREVIEW_COLUMNS, _>(
        chunks::<6>(entries(find(&ext, PWV4, "PWV4")?, "PWV4", 6)?),
        "PWV4",
    )?;
    let pwv5: Vec<u16> = entries(find(&ext, PWV5, "PWV5")?, "PWV5", 2)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_be_bytes(c))
        .collect();
    let pwv6 = fixed::<COLOR_PREVIEW_COLUMNS, _>(
        chunks::<3>(entries(find(&ex2, PWV6, "PWV6")?, "PWV6", 3)?),
        "PWV6",
    )?;
    let pwv7 = chunks::<3>(entries(find(&ex2, PWV7, "PWV7")?, "PWV7", 3)?);
    let pwvc = pwvc(find(&ex2, PWVC, "PWVC")?)?;
    if pwv5.len() != pwv3.len() || pwv7.len() != pwv3.len() {
        return Err(RbWaveError::Shape {
            tag: "PWV3/PWV5/PWV7",
            what: format!(
                "column counts differ: {}, {}, {}",
                pwv3.len(),
                pwv5.len(),
                pwv7.len()
            ),
        });
    }
    Ok(Waveforms {
        pwav,
        pwv2,
        pwv3,
        pwv4,
        pwv5,
        pwv6,
        pwv7,
        pwvc,
    })
}

pub(crate) fn find<'a>(
    sections: &[Section<'a>],
    tag: [u8; 4],
    name: &'static str,
) -> Result<Section<'a>, RbWaveError> {
    sections
        .iter()
        .find(|s| s.tag == tag)
        .copied()
        .ok_or(RbWaveError::Missing(name))
}

/// PWAV/PWV2 payload: header `len_preview`, unknown.
fn preview<'a>(s: Section<'a>, tag: &'static str) -> Result<&'a [u8], RbWaveError> {
    let len = s.u32(12)? as usize;
    let payload = payload(&s);
    if payload.len() != len {
        return Err(RbWaveError::Shape {
            tag,
            what: format!("len_preview {len}, payload {} bytes", payload.len()),
        });
    }
    Ok(payload)
}

/// Payload of a section with header `len_entry_bytes`, `len_entries`.
pub(crate) fn entries<'a>(
    s: Section<'a>,
    tag: &'static str,
    entry: usize,
) -> Result<&'a [u8], RbWaveError> {
    let entry_bytes = s.u32(12)? as usize;
    let count = s.u32(16)? as usize;
    let payload = payload(&s);
    if entry_bytes != entry || payload.len() != count.saturating_mul(entry) {
        return Err(RbWaveError::Shape {
            tag,
            what: format!(
                "{count} entries of {entry_bytes} bytes, payload {} bytes",
                payload.len()
            ),
        });
    }
    Ok(payload)
}

/// [`Section::payload`] with the file's lifetime.
fn payload<'a>(s: &Section<'a>) -> &'a [u8] {
    &s.bytes[s.len_header.min(s.bytes.len())..]
}

pub(crate) fn chunks<const N: usize>(payload: &[u8]) -> Vec<[u8; N]> {
    payload.as_chunks::<N>().0.to_vec()
}

fn fixed<const N: usize, T: Clone + std::fmt::Debug>(
    v: impl Into<Vec<T>>,
    tag: &'static str,
) -> Result<[T; N], RbWaveError> {
    let v: Vec<T> = v.into();
    let len = v.len();
    v.try_into().map_err(|_| RbWaveError::Shape {
        tag,
        what: format!("{len} columns, expected {N}"),
    })
}

/// PWVC: one u16 header field, three big-endian u16 gains.
pub(crate) fn pwvc(s: Section<'_>) -> Result<[u16; 3], RbWaveError> {
    let p = s.payload();
    if p.len() != 6 {
        return Err(RbWaveError::Shape {
            tag: "PWVC",
            what: format!("payload {} bytes, expected 6", p.len()),
        });
    }
    Ok(std::array::from_fn(|i| {
        u16::from_be_bytes([p[2 * i], p[2 * i + 1]])
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn reference_sets() -> Vec<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/rekordbox/ref-rb6-demo/PIONEER/USBANLZ");
        let mut dats = Vec::new();
        for p in std::fs::read_dir(root).unwrap() {
            for d in std::fs::read_dir(p.unwrap().path()).unwrap() {
                dats.push(d.unwrap().path().join("ANLZ0000.DAT"));
            }
        }
        dats.sort();
        dats
    }

    fn files(dat: &Path) -> [Vec<u8>; 3] {
        ["DAT", "EXT", "2EX"].map(|e| std::fs::read(dat.with_extension(e)).unwrap())
    }

    /// The section with `tag` in `bytes`, header included.
    fn section(bytes: &[u8], tag: [u8; 4]) -> Vec<u8> {
        read::sections(bytes)
            .unwrap()
            .into_iter()
            .find(|s| s.tag == tag)
            .unwrap()
            .bytes
            .to_vec()
    }

    /// Read rekordbox's waveforms, write them with our section writers: every
    /// section is byte-identical to rekordbox's, headers included.
    #[test]
    fn reference_waveforms_round_trip_byte_identical() {
        let sets = reference_sets();
        assert_eq!(sets.len(), 2);
        for dat in sets {
            let [d, e, x] = files(&dat);
            let w = read_waveforms(&d, &e, &x).unwrap();
            assert!(!w.pwv3.is_empty());
            let ours = [
                w.dat_sections().unwrap(),
                w.ext_sections().unwrap(),
                w.ex2_sections().unwrap(),
            ];
            let tags = [
                (&d, vec![PWAV, PWV2]),
                (&e, vec![PWV3, PWV5, PWV4]),
                (&x, vec![PWV7, PWV6, PWVC]),
            ];
            for (written, (theirs, tags)) in ours.iter().zip(tags) {
                for (sec, tag) in written.iter().zip(tags) {
                    assert_eq!(
                        sec,
                        &section(theirs, tag),
                        "{} in {}",
                        String::from_utf8_lossy(&tag),
                        dat.display()
                    );
                }
            }
        }
    }

    #[test]
    fn missing_or_misshapen_sections_are_refused() {
        let [d, e, x] = files(&reference_sets()[0]);
        // A .DAT where the .2EX should be: no PWV6.
        assert_eq!(
            read_waveforms(&d, &e, &d),
            Err(RbWaveError::Missing("PWV6"))
        );
        assert!(matches!(
            read_waveforms(&d, &e, b"nope"),
            Err(RbWaveError::Read(ReadError::NotAnlz))
        ));
        // PWV3 one entry short of its payload.
        let mut bad = e.clone();
        let at = read::sections(&e)
            .unwrap()
            .into_iter()
            .find(|s| s.tag == PWV3)
            .unwrap()
            .at;
        let n = u32::from_be_bytes(bad[at + 16..at + 20].try_into().unwrap()) - 1;
        bad[at + 16..at + 20].copy_from_slice(&n.to_be_bytes());
        assert!(matches!(
            read_waveforms(&d, &bad, &x),
            Err(RbWaveError::Shape { tag: "PWV3", .. })
        ));
    }
}
