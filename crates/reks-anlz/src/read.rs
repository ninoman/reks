//! Minimal ANLZ reader: splits a file into tagged sections and decodes the
//! few fields export validation needs (`PPTH` path, `PQTZ` beat count,
//! `PCOB`/`PCO2` cue counts). Layout from crate-digger `anlz.ksy` /
//! djl-analysis "Analysis Files" (R§4): big-endian; a `PMAI` file header,
//! then sections of `tag[4]`, `len_header: u32`, `len_tag: u32` (whole
//! section), header fields, payload.
//!
//! Why not `rekordcrate::anlz` (D8): 0.3.0 accepts only cue types 0 and 2 and
//! fails on type 1, the point cue rekordbox and baken write, so it can't read
//! a cue list from a real export. Section framing is all we need here.

/// File header tag.
pub const PMAI: [u8; 4] = *b"PMAI";
/// Audio file path.
pub const PPTH: [u8; 4] = *b"PPTH";
/// Beat grid.
pub const PQTZ: [u8; 4] = *b"PQTZ";
/// Cue list (`.DAT`).
pub const PCOB: [u8; 4] = *b"PCOB";
/// Extended cue list (`.EXT`).
pub const PCO2: [u8; 4] = *b"PCO2";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    #[error("not an ANLZ file (no PMAI header)")]
    NotAnlz,
    #[error("section at byte {at} is truncated or has bad lengths")]
    BadSection { at: usize },
    #[error("{tag} section at byte {at} is too short")]
    ShortSection { tag: String, at: usize },
    #[error("PPTH path is not valid UTF-16")]
    BadPath,
}

/// One section, borrowed from the file bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section<'a> {
    pub tag: [u8; 4],
    /// Byte offset of the section in the file.
    pub at: usize,
    pub len_header: usize,
    /// The whole section, header included.
    pub bytes: &'a [u8],
}

impl Section<'_> {
    fn u32_at(&self, at: usize) -> Result<u32, ReadError> {
        self.bytes
            .get(at..at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| self.short())
    }

    fn u16_at(&self, at: usize) -> Result<u16, ReadError> {
        self.bytes
            .get(at..at + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .ok_or_else(|| self.short())
    }

    fn short(&self) -> ReadError {
        ReadError::ShortSection {
            tag: String::from_utf8_lossy(&self.tag).into_owned(),
            at: self.at,
        }
    }
}

/// The sections of an ANLZ file, in file order.
pub fn sections(bytes: &[u8]) -> Result<Vec<Section<'_>>, ReadError> {
    if bytes.get(..4) != Some(&PMAI[..]) {
        return Err(ReadError::NotAnlz);
    }
    let be32 = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let header_len = be32(4).ok_or(ReadError::NotAnlz)?;
    let file_len = be32(8).ok_or(ReadError::NotAnlz)?;
    if header_len < 12 || header_len > file_len || file_len > bytes.len() {
        return Err(ReadError::NotAnlz);
    }
    let mut out = Vec::new();
    let mut at = header_len;
    while at < file_len {
        let bad = ReadError::BadSection { at };
        let tag: [u8; 4] = bytes
            .get(at..at + 4)
            .and_then(|b| b.try_into().ok())
            .ok_or(bad.clone())?;
        let len_header = be32(at + 4).ok_or(bad.clone())?;
        let len_tag = be32(at + 8).ok_or(bad.clone())?;
        if len_header < 12 || len_tag < len_header || at + len_tag > file_len {
            return Err(bad);
        }
        out.push(Section {
            tag,
            at,
            len_header,
            bytes: &bytes[at..at + len_tag],
        });
        at += len_tag;
    }
    Ok(out)
}

/// What export validation reads from one ANLZ file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    /// `PPTH`: the stick path of the audio file (`/Contents/...`).
    pub path: Option<String>,
    /// Beats across `PQTZ` sections.
    pub beats: u32,
    /// Cues across `PCOB` lists (hot and memory), `None` without `PCOB`.
    pub cues: Option<u32>,
    /// Cues across `PCO2` lists, `None` without `PCO2`.
    pub extended_cues: Option<u32>,
}

/// Reads [`Summary`] from ANLZ file bytes.
pub fn summary(bytes: &[u8]) -> Result<Summary, ReadError> {
    let mut s = Summary::default();
    for section in sections(bytes)? {
        match section.tag {
            PPTH => {
                // len_path at 12, UTF-16BE path with a trailing NUL after the header.
                let len = section.u32_at(12)? as usize;
                let raw = section
                    .bytes
                    .get(section.len_header..section.len_header + len)
                    .ok_or_else(|| section.short())?;
                let units: Vec<u16> = raw
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&c| u16::from_be_bytes(c))
                    .take_while(|&u| u != 0)
                    .collect();
                s.path = Some(String::from_utf16(&units).map_err(|_| ReadError::BadPath)?);
            }
            // len_beats at 20.
            PQTZ => s.beats += section.u32_at(20)?,
            // list type at 12, unknown u16 at 16, len_cues u16 at 18.
            PCOB => *s.cues.get_or_insert(0) += u32::from(section.u16_at(18)?),
            // list type at 12, len_cues u16 at 16.
            PCO2 => *s.extended_cues.get_or_insert(0) += u32::from(section.u16_at(16)?),
            _ => {}
        }
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(tag: &[u8; 4], header_fields: &[u8], payload: &[u8]) -> Vec<u8> {
        let len_header = 12 + header_fields.len();
        let len_tag = len_header + payload.len();
        let mut out = tag.to_vec();
        out.extend((len_header as u32).to_be_bytes());
        out.extend((len_tag as u32).to_be_bytes());
        out.extend(header_fields);
        out.extend(payload);
        out
    }

    fn file(sections: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = sections.concat();
        let len_header = 28usize;
        let mut out = PMAI.to_vec();
        out.extend((len_header as u32).to_be_bytes());
        out.extend(((len_header + body.len()) as u32).to_be_bytes());
        out.resize(len_header, 0);
        out.extend(body);
        out
    }

    fn ppth(path: &str) -> Vec<u8> {
        let mut payload: Vec<u8> = path.encode_utf16().flat_map(u16::to_be_bytes).collect();
        payload.extend([0, 0]);
        section(&PPTH, &(payload.len() as u32).to_be_bytes(), &payload)
    }

    fn pqtz(beats: u32) -> Vec<u8> {
        let mut fields = vec![0u8; 8];
        fields.extend(beats.to_be_bytes());
        section(&PQTZ, &fields, &vec![0u8; 8 * beats as usize])
    }

    fn pcob(list_type: u32, cues: u16) -> Vec<u8> {
        let mut fields = list_type.to_be_bytes().to_vec();
        fields.extend(0u16.to_be_bytes());
        fields.extend(cues.to_be_bytes());
        fields.extend(0u32.to_be_bytes());
        section(&PCOB, &fields, &vec![0u8; 0x38 * usize::from(cues)])
    }

    fn pco2(list_type: u32, cues: u16) -> Vec<u8> {
        let mut fields = list_type.to_be_bytes().to_vec();
        fields.extend(cues.to_be_bytes());
        fields.extend(0u16.to_be_bytes());
        section(&PCO2, &fields, &[])
    }

    #[test]
    fn reads_path_grid_and_cues() {
        let bytes = file(&[
            ppth("/Contents/Ärtist/x.mp3"),
            pqtz(3),
            pcob(1, 2),
            pcob(0, 3),
            section(b"PWAV", &[0; 8], &[1, 2, 3]),
        ]);
        let s = summary(&bytes).unwrap();
        assert_eq!(s.path.as_deref(), Some("/Contents/Ärtist/x.mp3"));
        assert_eq!(s.beats, 3);
        assert_eq!(s.cues, Some(5));
        assert_eq!(s.extended_cues, None);
        assert_eq!(sections(&bytes).unwrap().len(), 5);
    }

    #[test]
    fn reads_extended_cues() {
        let s = summary(&file(&[ppth("/a.mp3"), pco2(0, 1), pco2(1, 4)])).unwrap();
        assert_eq!(s.extended_cues, Some(5));
        assert_eq!(s.cues, None);
        assert_eq!(s.beats, 0);
    }

    #[test]
    fn rejects_damage() {
        assert_eq!(summary(b"nope"), Err(ReadError::NotAnlz));
        let mut bytes = file(&[ppth("/a.mp3"), pqtz(2)]);
        let n = bytes.len();
        // File length past the end.
        bytes[8..12].copy_from_slice(&((n + 1) as u32).to_be_bytes());
        assert_eq!(summary(&bytes), Err(ReadError::NotAnlz));
        // A section longer than the file.
        let mut bytes = file(&[ppth("/a.mp3")]);
        bytes[28 + 8..28 + 12].copy_from_slice(&1000u32.to_be_bytes());
        assert!(matches!(
            summary(&bytes),
            Err(ReadError::BadSection { at: 28 })
        ));
        // A PQTZ without its beat count field.
        let bytes = file(&[section(&PQTZ, &[0; 4], &[])]);
        assert!(matches!(
            summary(&bytes),
            Err(ReadError::ShortSection { .. })
        ));
    }
}
