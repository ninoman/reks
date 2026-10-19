//! [`AnlzLocator`] over rekordbox's local analysis cache ([`RbCache`]).
//!
//! rbcache matches on the values baken reads from the temp `collection.xml`
//! (`Location`, `TotalTime`, first `TEMPO`), so [`CacheLocator`] builds the
//! same XML the writer hands baken (`rbxml::export`), parses it with baken's
//! own `collection` parser and keeps one [`AnlzQuery`] per track id. The
//! cache is scanned once, when the locator is built.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use reks_core::{rbxml, Library, TrackId};

use super::{AnlzLocator, PlanError, Selection, SourceTrack};
use crate::rbcache::{AnlzQuery, Lookup, Missing, RbCache};

/// Finds each track's analysis in rekordbox's cache.
#[derive(Debug)]
pub struct CacheLocator {
    cache: RbCache,
    queries: HashMap<TrackId, AnlzQuery>,
}

impl CacheLocator {
    /// Locator for the tracks in `collection` (baken's parse of the temp
    /// XML; `TrackID` is our track id).
    pub fn new(cache: RbCache, collection: &baken_export::collection::Library) -> Self {
        let queries = collection
            .tracks
            .iter()
            .filter_map(|t| Some((TrackId::try_from(t.id).ok()?, AnlzQuery::from(t))))
            .collect();
        Self { cache, queries }
    }

    /// Locator for `sel`: exports it as rekordbox XML (in memory), parses
    /// that with baken and indexes the queries. Unknown playlist ids are left
    /// out here; the planner reports them.
    pub fn for_selection(
        lib: &Library,
        sel: &Selection,
        cache: RbCache,
    ) -> Result<Self, PlanError> {
        let known: HashSet<_> = lib.playlists()?.nodes.iter().map(|n| n.id).collect();
        let sel = rbxml::Selection {
            playlists: sel
                .playlists
                .iter()
                .copied()
                .filter(|id| known.contains(id))
                .collect(),
        };
        let xml = match rbxml::export(lib, &sel) {
            Ok(xml) => xml,
            Err(reks_core::Error::EmptySelection) => {
                return Ok(Self {
                    cache,
                    queries: HashMap::new(),
                })
            }
            Err(e) => return Err(e.into()),
        };
        let mut bytes = Vec::new();
        rbxml::write(&xml.collection, &mut bytes)?;
        let parsed = baken_export::collection::Library::parse(&bytes)
            .map_err(|e| PlanError::Xml(format!("{e:#}")))?;
        Ok(Self::new(cache, &parsed))
    }

    /// The cache, e.g. for its roots and file count.
    pub fn cache(&self) -> &RbCache {
        &self.cache
    }

    /// rekordbox's analysis for `track`, or why there is none.
    pub fn lookup(&self, track: &SourceTrack) -> Lookup {
        match self.queries.get(&track.track_id) {
            Some(q) => self.cache.locate(q),
            None => {
                tracing::debug!(track_id = track.track_id, "track not in the exported XML");
                Lookup::Missing(Missing::NotAnalysed)
            }
        }
    }
}

impl AnlzLocator for CacheLocator {
    fn locate(&self, track: &SourceTrack) -> Option<PathBuf> {
        self.lookup(track).found().map(|set| set.dat.clone())
    }

    fn why_missing(&self, track: &SourceTrack) -> String {
        match self.lookup(track) {
            Lookup::Missing(m) => m.to_string(),
            Lookup::Found(_) => "no rekordbox analysis found".to_owned(),
        }
    }
}
