//! Writer: rekordbox's layout (2-space indent, attribute order of the spec),
//! derived counts recomputed, numbers formatted so a re-parse is exact.

use std::fmt::Write as _;
use std::io::{BufWriter, Write};

use super::{Collection, Extra, NodeKind, PositionMark, RbXmlError, Tempo, XmlNode, XmlTrack};

/// Write `c` as rekordbox XML (UTF-8).
///
/// Known attributes come first in rekordbox's order, then `extra` attributes
/// sorted by name. `Entries`/`Count` are computed from the content.
pub fn write(c: &Collection, w: impl Write) -> crate::Result<()> {
    let mut w = BufWriter::new(w);
    write_inner(c, &mut w).map_err(RbXmlError::Io)?;
    w.flush().map_err(RbXmlError::Io)?;
    Ok(())
}

fn write_inner(c: &Collection, w: &mut impl Write) -> std::io::Result<()> {
    w.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\n")?;

    let mut e = El::new(0, "DJ_PLAYLISTS");
    e.attr("Version", &c.version).extra(&c.extra);
    e.open(w)?;

    let p = &c.product;
    let mut e = El::new(1, "PRODUCT");
    e.opt("Name", p.name.as_deref())
        .opt("Version", p.version.as_deref())
        .opt("Company", p.company.as_deref())
        .extra(&p.extra);
    e.empty(w)?;

    let mut e = El::new(1, "COLLECTION");
    e.attr("Entries", &c.tracks.len().to_string());
    e.open(w)?;
    for t in &c.tracks {
        track(t, w)?;
    }
    close(w, 1, "COLLECTION")?;

    El::new(1, "PLAYLISTS").open(w)?;
    node(&c.playlists, 2, w)?;
    close(w, 1, "PLAYLISTS")?;

    close(w, 0, "DJ_PLAYLISTS")
}

fn track(t: &XmlTrack, w: &mut impl Write) -> std::io::Result<()> {
    let mut e = El::new(2, "TRACK");
    e.attr("TrackID", &t.track_id.to_string())
        .opt("Name", t.name.as_deref())
        .opt("Artist", t.artist.as_deref())
        .opt("Composer", t.composer.as_deref())
        .opt("Album", t.album.as_deref())
        .opt("Grouping", t.grouping.as_deref())
        .opt("Genre", t.genre.as_deref())
        .opt("Kind", t.kind.as_deref())
        .num("Size", t.size)
        .num("TotalTime", t.total_time)
        .num("DiscNumber", t.disc_number)
        .num("TrackNumber", t.track_number)
        .num("Year", t.year)
        .opt("AverageBpm", t.average_bpm.map(bpm).as_deref())
        .opt("DateModified", t.date_modified.as_deref())
        .opt("DateAdded", t.date_added.as_deref())
        .num("BitRate", t.bit_rate)
        .num("SampleRate", t.sample_rate)
        .opt("Comments", t.comments.as_deref())
        .num("PlayCount", t.play_count)
        .opt("LastPlayed", t.last_played.as_deref())
        .num("Rating", t.rating)
        .opt("Location", t.location.as_deref())
        .opt("Remixer", t.remixer.as_deref())
        .opt("Tonality", t.tonality.as_deref())
        .opt("Label", t.label.as_deref())
        .opt("Mix", t.mix.as_deref())
        .opt("Colour", t.colour.map(|c| format!("0x{c:06X}")).as_deref())
        .extra(&t.extra);
    if t.tempos.is_empty() && t.position_marks.is_empty() {
        return e.empty(w);
    }
    e.open(w)?;
    for tp in &t.tempos {
        tempo(tp, w)?;
    }
    for m in &t.position_marks {
        position_mark(m, w)?;
    }
    close(w, 2, "TRACK")
}

fn tempo(t: &Tempo, w: &mut impl Write) -> std::io::Result<()> {
    let mut e = El::new(3, "TEMPO");
    e.attr("Inizio", &seconds(t.inizio))
        .attr("Bpm", &bpm(t.bpm))
        .opt("Metro", t.metro.as_deref())
        .num("Battito", t.battito)
        .extra(&t.extra);
    e.empty(w)
}

fn position_mark(m: &PositionMark, w: &mut impl Write) -> std::io::Result<()> {
    let mut e = El::new(3, "POSITION_MARK");
    e.opt("Name", m.name.as_deref())
        .attr("Type", &m.kind.to_string())
        .attr("Start", &seconds(m.start))
        .opt("End", m.end.map(seconds).as_deref())
        .attr("Num", &m.num.to_string());
    if let Some((r, g, b)) = m.rgb {
        e.num("Red", Some(r))
            .num("Green", Some(g))
            .num("Blue", Some(b));
    }
    e.extra(&m.extra);
    e.empty(w)
}

fn node(n: &XmlNode, depth: usize, w: &mut impl Write) -> std::io::Result<()> {
    let mut e = El::new(depth, "NODE");
    match &n.kind {
        NodeKind::Folder(children) => {
            e.attr("Type", "0")
                .attr("Name", &n.name)
                .attr("Count", &children.len().to_string())
                .extra(&n.extra);
            if children.is_empty() {
                return e.empty(w);
            }
            e.open(w)?;
            for c in children {
                node(c, depth + 1, w)?;
            }
        }
        NodeKind::Playlist { key_type, keys } => {
            e.attr("Name", &n.name)
                .attr("Type", "1")
                .attr("KeyType", &key_type.to_string())
                .attr("Entries", &keys.len().to_string())
                .extra(&n.extra);
            if keys.is_empty() {
                return e.empty(w);
            }
            e.open(w)?;
            for k in keys {
                let mut t = El::new(depth + 1, "TRACK");
                t.attr("Key", k);
                t.empty(w)?;
            }
        }
    }
    close(w, depth, "NODE")
}

fn close(w: &mut impl Write, depth: usize, name: &str) -> std::io::Result<()> {
    writeln!(w, "{:indent$}</{name}>", "", indent = depth * 2)
}

/// One start tag being assembled.
struct El {
    buf: String,
}

impl El {
    fn new(depth: usize, name: &str) -> Self {
        let mut buf = String::with_capacity(256);
        buf.extend(std::iter::repeat_n(' ', depth * 2));
        buf.push('<');
        buf.push_str(name);
        Self { buf }
    }

    fn attr(&mut self, k: &str, v: &str) -> &mut Self {
        self.buf.push(' ');
        self.buf.push_str(k);
        self.buf.push_str("=\"");
        escape_into(&mut self.buf, v);
        self.buf.push('"');
        self
    }

    fn opt(&mut self, k: &str, v: Option<&str>) -> &mut Self {
        if let Some(v) = v {
            self.attr(k, v);
        }
        self
    }

    fn num<T: std::fmt::Display>(&mut self, k: &str, v: Option<T>) -> &mut Self {
        if let Some(v) = v {
            let _ = write!(self.buf, " {k}=\"{v}\"");
        }
        self
    }

    fn extra(&mut self, extra: &Extra) -> &mut Self {
        for (k, v) in extra {
            self.attr(k, v);
        }
        self
    }

    fn open(&mut self, w: &mut impl Write) -> std::io::Result<()> {
        self.buf.push_str(">\n");
        w.write_all(self.buf.as_bytes())
    }

    fn empty(&mut self, w: &mut impl Write) -> std::io::Result<()> {
        self.buf.push_str("/>\n");
        w.write_all(self.buf.as_bytes())
    }
}

/// Escape an attribute value for a double-quoted attribute.
///
/// Tab, CR and LF become character references so XML attribute-value
/// normalisation on read doesn't turn them into spaces. Other C0 controls
/// are not allowed in XML 1.0 and are dropped.
fn escape_into(out: &mut String, v: &str) {
    for ch in v.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c if (c as u32) < 0x20 => {
                tracing::warn!(
                    char = c as u32,
                    "dropping control character not allowed in XML"
                );
            }
            c => out.push(c),
        }
    }
}

/// Seconds with rekordbox's 3 decimals, unless that would lose precision.
fn seconds(x: f64) -> String {
    fixed(x, 3)
}

/// BPM with rekordbox's 2 decimals, unless that would lose precision.
fn bpm(x: f64) -> String {
    fixed(x, 2)
}

/// `{:.dp}` when it parses back to exactly `x`; otherwise Rust's shortest
/// round-trip form, so `parse(write(c)) == c`.
fn fixed(x: f64, dp: usize) -> String {
    let s = format!("{x:.dp$}");
    if s.parse::<f64>() == Ok(x) {
        s
    } else {
        format!("{x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_decimals() {
        assert_eq!(seconds(0.0), "0.000");
        assert_eq!(seconds(16.875), "16.875");
        assert_eq!(seconds(0.0251), "0.0251");
        assert_eq!(bpm(128.0), "128.00");
        assert_eq!(bpm(127.99), "127.99");
        assert_eq!(bpm(174.001), "174.001");
        for x in [0.1, 1.0 / 3.0, 123.456_789, 1e-7, 86_400.123] {
            assert_eq!(seconds(x).parse::<f64>().unwrap(), x);
        }
    }

    #[test]
    fn escaping() {
        let mut s = String::new();
        escape_into(&mut s, "a&b<c>d\"e'f\tg\nh\ri\u{1}j ü");
        assert_eq!(s, "a&amp;b&lt;c&gt;d&quot;e&apos;f&#9;g&#10;h&#13;ij ü");
    }
}
