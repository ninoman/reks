//! Streaming parser: one pass over quick-xml events, no DOM.

use std::io::BufRead;
use std::str::FromStr;

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::QName;
use quick_xml::{Reader, XmlVersion};

use super::{
    Collection, Extra, NodeKind, PositionMark, Product, RbXmlError, Tempo, XmlNode, XmlTrack,
};

type PResult<T> = std::result::Result<T, RbXmlError>;

/// Parse a rekordbox `collection.xml` from `reader` (UTF-8).
///
/// Unknown elements are skipped with a warning; unknown attributes and
/// unparseable optional values land in the element's `extra` map. Derived
/// counts (`Entries`, `Count`) are checked and then dropped.
pub fn parse(reader: impl BufRead) -> crate::Result<Collection> {
    let mut p = Parser {
        r: Reader::from_reader(reader),
        buf: Vec::new(),
    };
    Ok(p.document()?)
}

/// [`parse`] the file at `path`.
pub fn parse_file(path: &std::path::Path) -> crate::Result<Collection> {
    let f = std::fs::File::open(path).map_err(|source| crate::Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse(std::io::BufReader::new(f))
}

enum Ev {
    /// A start tag; `true` if it has content (`<X>`), `false` if self-closing (`<X/>`).
    Open(BytesStart<'static>, bool),
    End,
    Eof,
}

struct Parser<R> {
    r: Reader<R>,
    buf: Vec<u8>,
}

impl<R: BufRead> Parser<R> {
    fn pos(&self) -> u64 {
        self.r.buffer_position()
    }

    /// Next element-level event; text, comments, PIs and the declaration are ignored.
    fn next(&mut self) -> PResult<Ev> {
        loop {
            self.buf.clear();
            let position = self.r.buffer_position();
            let ev = self
                .r
                .read_event_into(&mut self.buf)
                .map_err(|source| RbXmlError::Xml { position, source })?;
            return Ok(match ev {
                Event::Start(e) => Ev::Open(e.into_owned(), true),
                Event::Empty(e) => Ev::Open(e.into_owned(), false),
                Event::End(_) => Ev::End,
                Event::Eof => Ev::Eof,
                _ => continue,
            });
        }
    }

    /// Skip the rest of an element whose `Start` we just read.
    fn skip(&mut self, e: &BytesStart<'_>) -> PResult<()> {
        let position = self.pos();
        self.buf.clear();
        self.r
            .read_to_end_into(QName(e.name().as_ref()), &mut self.buf)
            .map_err(|source| RbXmlError::Xml { position, source })?;
        Ok(())
    }

    fn skip_if(&mut self, e: &BytesStart<'_>, open: bool) -> PResult<()> {
        if open {
            self.skip(e)
        } else {
            Ok(())
        }
    }

    fn unknown(&mut self, ev: Ev, parent: &str) -> PResult<()> {
        match ev {
            Ev::Open(e, open) => {
                tracing::warn!(element = %name_of(&e), parent, "skipping unknown element");
                self.skip_if(&e, open)
            }
            Ev::End | Ev::Eof => Ok(()),
        }
    }

    fn eof(&self, inside: &str) -> RbXmlError {
        RbXmlError::Structure(format!("file ends inside <{inside}>"))
    }

    fn document(&mut self) -> PResult<Collection> {
        let (root, open) = match self.next()? {
            Ev::Open(e, open) => (e, open),
            Ev::End | Ev::Eof => return Err(RbXmlError::Structure("no root element".into())),
        };
        if root.name().as_ref() != "DJ_PLAYLISTS" {
            return Err(RbXmlError::Structure(format!(
                "root element is <{}>, expected <DJ_PLAYLISTS>",
                name_of(&root)
            )));
        }
        let mut c = Collection::default();
        for (k, v) in self.attrs(&root)? {
            match k.as_str() {
                "Version" => c.version = v,
                _ => {
                    c.extra.insert(k, v);
                }
            }
        }
        if !open {
            return Ok(c);
        }
        loop {
            match self.next()? {
                Ev::End => return Ok(c),
                Ev::Eof => return Err(self.eof("DJ_PLAYLISTS")),
                Ev::Open(e, open) if is(&e, "PRODUCT") => {
                    c.product = self.product(&e)?;
                    self.skip_if(&e, open)?;
                }
                Ev::Open(e, open) if is(&e, "COLLECTION") => {
                    if open {
                        c.tracks = self.collection()?;
                    }
                    check_count("COLLECTION", "Entries", &self.attrs(&e)?, c.tracks.len());
                }
                Ev::Open(e, open) if is(&e, "PLAYLISTS") => {
                    if open {
                        if let Some(root) = self.playlists()? {
                            c.playlists = root;
                        }
                    }
                }
                ev => self.unknown(ev, "DJ_PLAYLISTS")?,
            }
        }
    }

    fn product(&mut self, e: &BytesStart<'_>) -> PResult<Product> {
        let mut p = Product::default();
        for (k, v) in self.attrs(e)? {
            match k.as_str() {
                "Name" => p.name = Some(v),
                "Version" => p.version = Some(v),
                "Company" => p.company = Some(v),
                _ => {
                    p.extra.insert(k, v);
                }
            }
        }
        Ok(p)
    }

    fn collection(&mut self) -> PResult<Vec<XmlTrack>> {
        let mut tracks = Vec::new();
        loop {
            match self.next()? {
                Ev::End => return Ok(tracks),
                Ev::Eof => return Err(self.eof("COLLECTION")),
                Ev::Open(e, open) if is(&e, "TRACK") => {
                    let mut t = self.track(&e)?;
                    if open {
                        self.track_children(&mut t)?;
                    }
                    tracks.push(t);
                }
                ev => self.unknown(ev, "COLLECTION")?,
            }
        }
    }

    fn track(&mut self, e: &BytesStart<'_>) -> PResult<XmlTrack> {
        const EL: &str = "TRACK";
        let position = self.pos();
        let mut t = XmlTrack::default();
        let mut id = None;
        for (k, v) in self.attrs(e)? {
            let x = &mut t.extra;
            match k.as_str() {
                "TrackID" => id = Some(required(EL, "TrackID", v, position)?),
                "Name" => t.name = Some(v),
                "Artist" => t.artist = Some(v),
                "Composer" => t.composer = Some(v),
                "Album" => t.album = Some(v),
                "Grouping" => t.grouping = Some(v),
                "Genre" => t.genre = Some(v),
                "Kind" => t.kind = Some(v),
                "Size" => optional(&mut t.size, EL, k, v, x),
                "TotalTime" => optional(&mut t.total_time, EL, k, v, x),
                "DiscNumber" => optional(&mut t.disc_number, EL, k, v, x),
                "TrackNumber" => optional(&mut t.track_number, EL, k, v, x),
                "Year" => optional(&mut t.year, EL, k, v, x),
                "AverageBpm" => optional_f64(&mut t.average_bpm, EL, k, v, x),
                "DateModified" => t.date_modified = Some(v),
                "DateAdded" => t.date_added = Some(v),
                "BitRate" => optional(&mut t.bit_rate, EL, k, v, x),
                "SampleRate" => optional(&mut t.sample_rate, EL, k, v, x),
                "Comments" => t.comments = Some(v),
                "PlayCount" => optional(&mut t.play_count, EL, k, v, x),
                "LastPlayed" => t.last_played = Some(v),
                "Rating" => optional(&mut t.rating, EL, k, v, x),
                "Location" => t.location = Some(v),
                "Remixer" => t.remixer = Some(v),
                "Tonality" => t.tonality = Some(v),
                "Label" => t.label = Some(v),
                "Mix" => t.mix = Some(v),
                "Colour" => match parse_colour(&v) {
                    Some(c) => t.colour = Some(c),
                    None => keep_raw(EL, k, v, x),
                },
                _ => {
                    x.insert(k, v);
                }
            }
        }
        t.track_id = id.ok_or(RbXmlError::Missing {
            element: EL,
            attr: "TrackID",
            position,
        })?;
        Ok(t)
    }

    fn track_children(&mut self, t: &mut XmlTrack) -> PResult<()> {
        loop {
            match self.next()? {
                Ev::End => return Ok(()),
                Ev::Eof => return Err(self.eof("TRACK")),
                Ev::Open(e, open) if is(&e, "TEMPO") => {
                    t.tempos.push(self.tempo(&e)?);
                    self.skip_if(&e, open)?;
                }
                Ev::Open(e, open) if is(&e, "POSITION_MARK") => {
                    t.position_marks.push(self.position_mark(&e)?);
                    self.skip_if(&e, open)?;
                }
                ev => self.unknown(ev, "TRACK")?,
            }
        }
    }

    fn tempo(&mut self, e: &BytesStart<'_>) -> PResult<Tempo> {
        const EL: &str = "TEMPO";
        let position = self.pos();
        let (mut inizio, mut bpm) = (None, None);
        let mut t = Tempo {
            inizio: 0.0,
            bpm: 0.0,
            metro: None,
            battito: None,
            extra: Extra::new(),
        };
        for (k, v) in self.attrs(e)? {
            match k.as_str() {
                "Inizio" => inizio = Some(required_f64(EL, "Inizio", v, position)?),
                "Bpm" => bpm = Some(required_f64(EL, "Bpm", v, position)?),
                "Metro" => t.metro = Some(v),
                "Battito" => optional(&mut t.battito, EL, k, v, &mut t.extra),
                _ => {
                    t.extra.insert(k, v);
                }
            }
        }
        t.inizio = inizio.ok_or(missing(EL, "Inizio", position))?;
        t.bpm = bpm.ok_or(missing(EL, "Bpm", position))?;
        Ok(t)
    }

    fn position_mark(&mut self, e: &BytesStart<'_>) -> PResult<PositionMark> {
        const EL: &str = "POSITION_MARK";
        let position = self.pos();
        let (mut kind, mut start, mut num) = (None, None, None);
        let mut rgb: [Option<String>; 3] = [None, None, None];
        let mut m = PositionMark {
            name: None,
            kind: 0,
            start: 0.0,
            end: None,
            num: 0,
            rgb: None,
            extra: Extra::new(),
        };
        for (k, v) in self.attrs(e)? {
            match k.as_str() {
                "Name" => m.name = Some(v),
                "Type" => kind = Some(required(EL, "Type", v, position)?),
                "Start" => start = Some(required_f64(EL, "Start", v, position)?),
                "End" => optional_f64(&mut m.end, EL, k, v, &mut m.extra),
                "Num" => num = Some(required(EL, "Num", v, position)?),
                "Red" => rgb[0] = Some(v),
                "Green" => rgb[1] = Some(v),
                "Blue" => rgb[2] = Some(v),
                _ => {
                    m.extra.insert(k, v);
                }
            }
        }
        m.kind = kind.ok_or(missing(EL, "Type", position))?;
        m.start = start.ok_or(missing(EL, "Start", position))?;
        m.num = num.ok_or(missing(EL, "Num", position))?;
        let parsed: Vec<Option<u8>> = rgb
            .iter()
            .map(|c| c.as_deref().and_then(|s| s.parse().ok()))
            .collect();
        if let [Some(r), Some(g), Some(b)] = parsed[..] {
            m.rgb = Some((r, g, b));
        } else {
            for (name, raw) in ["Red", "Green", "Blue"].into_iter().zip(rgb) {
                if let Some(raw) = raw {
                    keep_raw(EL, name.to_owned(), raw, &mut m.extra);
                }
            }
        }
        Ok(m)
    }

    /// Children of `PLAYLISTS`: the first `NODE` is the root folder.
    fn playlists(&mut self) -> PResult<Option<XmlNode>> {
        let mut root = None;
        loop {
            match self.next()? {
                Ev::End => return Ok(root),
                Ev::Eof => return Err(self.eof("PLAYLISTS")),
                Ev::Open(e, open) if is(&e, "NODE") && root.is_none() => {
                    root = Some(self.node(&e, open)?)
                }
                ev => self.unknown(ev, "PLAYLISTS")?,
            }
        }
    }

    fn node(&mut self, e: &BytesStart<'_>, has_children: bool) -> PResult<XmlNode> {
        const EL: &str = "NODE";
        let position = self.pos();
        let attrs = self.attrs(e)?;
        let mut name = None;
        let mut node_type: Option<u8> = None;
        let mut key_type: Option<u8> = None;
        let mut extra = Extra::new();
        for (k, v) in &attrs {
            match k.as_str() {
                "Name" => name = Some(v.clone()),
                "Type" => node_type = Some(required(EL, "Type", v.clone(), position)?),
                "KeyType" => key_type = Some(required(EL, "KeyType", v.clone(), position)?),
                "Count" | "Entries" => {}
                _ => {
                    extra.insert(k.clone(), v.clone());
                }
            }
        }
        let name = name.ok_or(missing(EL, "Name", position))?;
        let kind = match node_type {
            Some(0) => {
                let mut children = Vec::new();
                if has_children {
                    loop {
                        match self.next()? {
                            Ev::End => break,
                            Ev::Eof => return Err(self.eof(EL)),
                            Ev::Open(c, open) if is(&c, "NODE") => {
                                children.push(self.node(&c, open)?)
                            }
                            ev => self.unknown(ev, "NODE")?,
                        }
                    }
                }
                check_count(EL, "Count", &attrs, children.len());
                NodeKind::Folder(children)
            }
            Some(1) => {
                let mut keys = Vec::new();
                if has_children {
                    loop {
                        match self.next()? {
                            Ev::End => break,
                            Ev::Eof => return Err(self.eof(EL)),
                            Ev::Open(c, open) if is(&c, "TRACK") => {
                                let tp = self.pos();
                                let key = self
                                    .attrs(&c)?
                                    .into_iter()
                                    .find(|(k, _)| k == "Key")
                                    .map(|(_, v)| v)
                                    .ok_or(missing("NODE/TRACK", "Key", tp))?;
                                keys.push(key);
                                self.skip_if(&c, open)?;
                            }
                            ev => self.unknown(ev, "NODE")?,
                        }
                    }
                }
                check_count(EL, "Entries", &attrs, keys.len());
                NodeKind::Playlist {
                    key_type: key_type.unwrap_or(0),
                    keys,
                }
            }
            Some(other) => {
                return Err(RbXmlError::Invalid {
                    element: EL,
                    attr: "Type",
                    value: other.to_string(),
                    position,
                })
            }
            None => return Err(missing(EL, "Type", position)),
        };
        Ok(XmlNode { name, kind, extra })
    }

    /// Attributes in document order, unescaped and normalised per XML 1.0.
    fn attrs(&self, e: &BytesStart<'_>) -> PResult<Vec<(String, String)>> {
        let position = self.pos();
        let xml = |source: quick_xml::Error| RbXmlError::Xml { position, source };
        let mut out = Vec::new();
        for a in e.attributes() {
            let a = a.map_err(|err| xml(err.into()))?;
            let key = a.key.as_ref().to_owned();
            let value = a
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(xml)?
                .into_owned();
            out.push((key, value));
        }
        Ok(out)
    }
}

fn is(e: &BytesStart<'_>, name: &str) -> bool {
    e.name().as_ref() == name
}

fn name_of(e: &BytesStart<'_>) -> String {
    e.name().as_ref().to_owned()
}

fn missing(element: &'static str, attr: &'static str, position: u64) -> RbXmlError {
    RbXmlError::Missing {
        element,
        attr,
        position,
    }
}

fn required<T: FromStr>(
    element: &'static str,
    attr: &'static str,
    v: String,
    position: u64,
) -> PResult<T> {
    v.parse().map_err(|_| RbXmlError::Invalid {
        element,
        attr,
        value: v,
        position,
    })
}

fn required_f64(
    element: &'static str,
    attr: &'static str,
    v: String,
    position: u64,
) -> PResult<f64> {
    match v.parse::<f64>() {
        Ok(x) if x.is_finite() => Ok(x),
        _ => Err(RbXmlError::Invalid {
            element,
            attr,
            value: v,
            position,
        }),
    }
}

fn optional<T: FromStr>(
    slot: &mut Option<T>,
    element: &str,
    k: String,
    v: String,
    extra: &mut Extra,
) {
    match v.parse() {
        Ok(x) => *slot = Some(x),
        Err(_) => keep_raw(element, k, v, extra),
    }
}

fn optional_f64(slot: &mut Option<f64>, element: &str, k: String, v: String, extra: &mut Extra) {
    match v.parse::<f64>() {
        Ok(x) if x.is_finite() => *slot = Some(x),
        _ => keep_raw(element, k, v, extra),
    }
}

/// Keep an unparseable known attribute verbatim so a write puts it back.
fn keep_raw(element: &str, k: String, v: String, extra: &mut Extra) {
    tracing::warn!(element, attr = %k, value = %v, "unparseable value kept verbatim");
    extra.insert(k, v);
}

/// `0xRRGGBB` (case-insensitive prefix and digits).
fn parse_colour(v: &str) -> Option<u32> {
    let hex = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X"))?;
    if hex.is_empty() || hex.len() > 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

fn check_count(element: &str, attr: &str, attrs: &[(String, String)], actual: usize) {
    if let Some((_, v)) = attrs.iter().find(|(k, _)| k == attr) {
        if v.parse::<usize>().ok() != Some(actual) {
            tracing::warn!(element, attr, declared = %v, actual, "declared count differs from content");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_forms() {
        assert_eq!(parse_colour("0xFF007F"), Some(0xFF007F));
        assert_eq!(parse_colour("0x00ff00"), Some(0x00FF00));
        assert_eq!(parse_colour("0X0"), Some(0));
        assert_eq!(parse_colour("FF007F"), None);
        assert_eq!(parse_colour("0x"), None);
        assert_eq!(parse_colour("0x1000000"), None);
        assert_eq!(parse_colour("0xZZ"), None);
    }
}
