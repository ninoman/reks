//! Where a value in the library came from, and rekordbox cue kinds: the
//! typed forms of the `*_source` and `cue.kind` columns. The stored and
//! serialised strings are the columns' own, so the database and the UI see
//! no change.

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};

/// `track.key_source`, `track.bpm_source`, `beatgrid.source`, `cue.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSource {
    /// Read from the file's tags (key and BPM only).
    Tag,
    /// From a rekordbox XML import.
    Rekordbox,
    /// Written by reks's analyser.
    Analysis,
    /// Set or edited by the user in reks.
    Manual,
    /// A plugged-in stick's own ANLZ grid; shown, never stored.
    Stick,
}

impl DataSource {
    const ALL: [Self; 5] = [
        Self::Tag,
        Self::Rekordbox,
        Self::Analysis,
        Self::Manual,
        Self::Stick,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tag => "tag",
            Self::Rekordbox => "rekordbox",
            Self::Analysis => "analysis",
            Self::Manual => "manual",
            Self::Stick => "stick",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// `track.analysis_source`: what the analysis cache entry was made from (D10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisSource {
    /// reks's analyser ran on the file.
    Reks,
    /// rekordbox's analysis was mapped into the cache.
    Rekordbox,
}

impl AnalysisSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reks => "reks",
            Self::Rekordbox => "rekordbox",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        [Self::Reks, Self::Rekordbox]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

/// rekordbox's cue `Type` (`cue.kind`, the XML's `POSITION_MARK Type`).
/// [`crate::track::Cue::kind`] keeps the raw number, so a type reks doesn't
/// know still round-trips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CueKind {
    Cue,
    FadeIn,
    FadeOut,
    Load,
    Loop,
}

impl CueKind {
    pub const fn code(self) -> i64 {
        match self {
            Self::Cue => 0,
            Self::FadeIn => 1,
            Self::FadeOut => 2,
            Self::Load => 3,
            Self::Loop => 4,
        }
    }

    pub fn from_code(code: i64) -> Option<Self> {
        [
            Self::Cue,
            Self::FadeIn,
            Self::FadeOut,
            Self::Load,
            Self::Loop,
        ]
        .into_iter()
        .find(|k| k.code() == code)
    }
}

macro_rules! text_column {
    ($t:ty) => {
        impl ToSql for $t {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::from(self.as_str()))
            }
        }

        impl FromSql for $t {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                let s = value.as_str()?;
                Self::parse(s).ok_or_else(|| {
                    FromSqlError::Other(format!("unknown {}: {s:?}", stringify!($t)).into())
                })
            }
        }
    };
}

text_column!(DataSource);
text_column!(AnalysisSource);

impl ToSql for CueKind {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.code()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn sources_round_trip_through_sql_and_json() {
        let c = Connection::open_in_memory().unwrap();
        for s in DataSource::ALL {
            let back: DataSource = c.query_row("SELECT ?1", [s], |r| r.get(0)).unwrap();
            assert_eq!(back, s);
            assert_eq!(
                serde_json::to_string(&s).unwrap(),
                format!("\"{}\"", s.as_str())
            );
        }
        for s in [AnalysisSource::Reks, AnalysisSource::Rekordbox] {
            let back: AnalysisSource = c.query_row("SELECT ?1", [s], |r| r.get(0)).unwrap();
            assert_eq!(back, s);
        }
        let unknown: rusqlite::Result<DataSource> = c.query_row("SELECT 'other'", [], |r| r.get(0));
        assert!(unknown.is_err());
    }

    #[test]
    fn cue_kinds_are_rekordbox_types() {
        assert_eq!(CueKind::Cue.code(), 0);
        assert_eq!(CueKind::Loop.code(), 4);
        for code in 0..=4 {
            assert_eq!(CueKind::from_code(code).map(CueKind::code), Some(code));
        }
        assert_eq!(CueKind::from_code(5), None);
    }
}
