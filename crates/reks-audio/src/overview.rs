//! Overview waveform: the minimum and maximum sample of every
//! [`FRAMES_PER_COLUMN`] frames, over both channels (Phase 1 task 11).
//!
//! Layout: `[min0, max0, min1, max1, …]`, values in −1..=1. On disk it is
//! the same sequence as little-endian `f32` ([`to_le_bytes`]).

/// Source frames per column.
pub const FRAMES_PER_COLUMN: usize = 1000;

/// Builds an overview one frame at a time.
#[derive(Debug, Default)]
pub struct Overview {
    columns: Vec<f32>,
    min: f32,
    max: f32,
    count: usize,
}

impl Overview {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, l: f32, r: f32) {
        if self.count == 0 {
            self.min = l.min(r);
            self.max = l.max(r);
        } else {
            self.min = self.min.min(l).min(r);
            self.max = self.max.max(l).max(r);
        }
        self.count += 1;
        if self.count == FRAMES_PER_COLUMN {
            self.flush();
        }
    }

    /// The columns; a last partial column counts as a column.
    pub fn finish(mut self) -> Vec<f32> {
        if self.count > 0 {
            self.flush();
        }
        self.columns
    }

    fn flush(&mut self) {
        self.columns.push(self.min.clamp(-1.0, 1.0));
        self.columns.push(self.max.clamp(-1.0, 1.0));
        self.count = 0;
    }
}

/// Serialises columns for the cache file and the UI (`Float32Array`).
pub fn to_le_bytes(columns: &[f32]) -> Vec<u8> {
    columns.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Parses a cache file; `None` when the length is not whole `[min, max]` pairs.
pub fn from_le_bytes(bytes: &[u8]) -> Option<Vec<f32>> {
    if !bytes.len().is_multiple_of(8) {
        return None;
    }
    Some(
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_max_per_column_with_partial_tail() {
        let mut o = Overview::new();
        for i in 0..FRAMES_PER_COLUMN + 10 {
            let v = if i == 5 { 0.5 } else { 0.0 };
            o.push(v, -v);
        }
        o.push(2.0, 0.1); // clamps
        assert_eq!(o.finish(), vec![-0.5, 0.5, 0.0, 1.0]);
    }

    #[test]
    fn empty_input_gives_no_columns() {
        assert!(Overview::new().finish().is_empty());
    }

    #[test]
    fn bytes_round_trip() {
        let cols = vec![-0.25, 0.75, -1.0, 1.0];
        let bytes = to_le_bytes(&cols);
        assert_eq!(bytes.len(), 16);
        assert_eq!(&bytes[..4], &(-0.25f32).to_le_bytes());
        assert_eq!(from_le_bytes(&bytes), Some(cols));
        assert_eq!(from_le_bytes(&bytes[..12]), None);
    }
}
