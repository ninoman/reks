//! rekordbox track colours: the XML `Colour` RGB value ↔ the colour name we
//! store in `track.color`.
//!
//! rekordbox has eight fixed track colours. We store the lowercase names in
//! the order of the PDB colour ids 1–8 (Phase 2 export maps name → id). The
//! XML spec calls four of them Rose, Lemon, Turquoise and Violet; we use the
//! rekordbox UI names instead.
//!
//! RGB values: pyrekordbox's XML format page (docs/research.md §7). A real
//! rekordbox 6.8.6 export confirms red, green, aqua and blue; pink, orange,
//! yellow and purple are UNVERIFIED against rekordbox output.

/// `(name, 0xRRGGBB)` in PDB colour-id order (id = index + 1).
pub const TRACK_COLOURS: [(&str, u32); 8] = [
    ("pink", 0xFF007F),
    ("red", 0xFF0000),
    ("orange", 0xFFA500),
    ("yellow", 0xFFFF00),
    ("green", 0x00FF00),
    ("aqua", 0x25FDE9),
    ("blue", 0x0000FF),
    ("purple", 0x660099),
];

/// Colour name for an XML `Colour` value; `None` if it isn't one of the
/// eight rekordbox colours (exact match only).
pub fn colour_name(rgb: u32) -> Option<&'static str> {
    TRACK_COLOURS
        .iter()
        .find(|(_, v)| *v == rgb)
        .map(|(name, _)| *name)
}

/// XML `Colour` value for a stored colour name (case-insensitive).
pub fn colour_rgb(name: &str) -> Option<u32> {
    TRACK_COLOURS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, rgb)| *rgb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_colour() {
        for (name, rgb) in TRACK_COLOURS {
            assert_eq!(colour_name(rgb), Some(name));
            assert_eq!(colour_rgb(name), Some(rgb));
        }
        assert_eq!(colour_rgb("Aqua"), Some(0x25FDE9));
    }

    #[test]
    fn unknown_values() {
        assert_eq!(colour_name(0x123456), None);
        assert_eq!(colour_name(0), None);
        assert_eq!(colour_rgb("turquoise"), None);
    }
}
