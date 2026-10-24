//! The dry run: walk the chosen folder and match missing tracks to files.
//!
//! 1. Content: a file the same size as a track is hashed (only then); an equal
//!    `content_hash` is a sure match. Several equal files → the user picks.
//! 2. Otherwise a file with the track's normalised file name, or artist and
//!    title, and a duration within a second is a *candidate*, never relinked
//!    without a pick.
//!
//! Nothing is written. Hashing and reading are the slow parts: they only run
//! for files that can match, report progress and stop on cancel.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use walkdir::WalkDir;

use super::matching::{durations_agree, name_key, tag_key, MAX_CANDIDATES};
use super::{
    FoundFile, RelocateEvent, RelocatePlan, RelocateProblem, RelocateScope, ReviewBasis,
    ReviewItem, SureMatch, TrackRef,
};
use crate::db::Library;
use crate::error::Result;
use crate::fileinfo::{content_hash, library_path};
use crate::scan::{CancelToken, TrackId};

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "flac", "wav", "wave", "aif", "aiff"];

/// A missing track's row, as far as matching needs it.
struct Missing {
    id: TrackId,
    path: String,
    file_size: i64,
    content_hash: Option<String>,
    duration_ms: Option<i64>,
    artist: Option<String>,
    title: Option<String>,
}

impl Missing {
    fn track_ref(&self) -> TrackRef {
        let name = Path::new(&self.path)
            .file_name()
            .map_or(self.path.clone(), |n| n.to_string_lossy().into_owned());
        let label = match (self.artist.as_deref(), self.title.as_deref()) {
            (Some(a), Some(t)) => format!("{a} – {t}"),
            (None, Some(t)) => t.to_owned(),
            _ => name,
        };
        TrackRef {
            id: self.id,
            path: self.path.clone(),
            label,
        }
    }
}

/// An audio file of the folder that the library does not have.
struct Candidate {
    path: String,
    size: i64,
}

/// Looks for the missing tracks in `folder`. A cancelled plan comes back
/// with `cancelled` set and is not meant to be applied.
pub fn plan(
    lib: &Library,
    scope: &RelocateScope,
    folder: &Path,
    cancel: &CancelToken,
    progress: impl Fn(RelocateEvent),
) -> Result<RelocatePlan> {
    let (missing, in_library) = load(lib, scope)?;
    let folder = std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf());
    let mut plan = RelocatePlan {
        folder: library_path(&folder).unwrap_or_else(|| folder.to_string_lossy().into_owned()),
        looked_for: missing.len(),
        ..RelocatePlan::default()
    };
    plan.inside_root = lib
        .library_roots()?
        .iter()
        .any(|r| Path::new(&plan.folder).starts_with(&r.path));

    let files = walk(&folder, &in_library, cancel, &progress, &mut plan);
    plan.files_seen = files.len();
    if cancel.is_cancelled() {
        plan.cancelled = true;
        return Ok(plan);
    }

    let mut ctx = Ctx {
        files: &files,
        cancel,
        progress: &progress,
        claimed: HashSet::new(),
        durations: HashMap::new(),
    };
    let mut unresolved = Vec::new();
    ctx.match_content(&missing, &mut plan, &mut unresolved);
    if !ctx.cancel.is_cancelled() {
        ctx.match_possible(unresolved, &mut plan);
    }
    plan.cancelled = cancel.is_cancelled();
    Ok(plan)
}

/// The tracks to look for, and every path the library already has (those
/// files are not candidates).
fn load(lib: &Library, scope: &RelocateScope) -> Result<(Vec<Missing>, HashSet<String>)> {
    let conn = lib.conn()?;
    let wanted: Option<HashSet<TrackId>> = match scope {
        RelocateScope::AllMissing => None,
        RelocateScope::Tracks { ids } => Some(ids.iter().copied().collect()),
    };
    let mut stmt = conn.prepare(
        "SELECT id, path, file_size, content_hash, duration_ms, artist, title, missing
         FROM track ORDER BY id",
    )?;
    let mut missing = Vec::new();
    let mut in_library = HashSet::new();
    let rows = stmt.query_map([], |r| {
        Ok((
            Missing {
                id: r.get(0)?,
                path: r.get(1)?,
                file_size: r.get(2)?,
                content_hash: r.get(3)?,
                duration_ms: r.get(4)?,
                artist: r.get(5)?,
                title: r.get(6)?,
            },
            r.get::<_, bool>(7)?,
        ))
    })?;
    for row in rows {
        let (t, is_missing) = row?;
        in_library.insert(t.path.clone());
        if is_missing && wanted.as_ref().is_none_or(|w| w.contains(&t.id)) {
            missing.push(t);
        }
    }
    Ok((missing, in_library))
}

/// Audio files under `folder` that the library has no row for, sorted by path.
fn walk(
    folder: &Path,
    in_library: &HashSet<String>,
    cancel: &CancelToken,
    progress: &impl Fn(RelocateEvent),
    plan: &mut RelocatePlan,
) -> Vec<Candidate> {
    let mut files = Vec::new();
    let entries = WalkDir::new(folder)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !is_hidden(e));
    for entry in entries {
        if cancel.is_cancelled() {
            return files;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                plan.problems.push(RelocateProblem {
                    path: e.path().map_or_else(
                        || folder.to_string_lossy().into_owned(),
                        |p| p.to_string_lossy().into_owned(),
                    ),
                    message: e.to_string(),
                });
                continue;
            }
        };
        if !entry.file_type().is_file() || !is_audio(entry.path()) {
            continue;
        }
        let Some(path) = library_path(entry.path()) else {
            plan.problems.push(RelocateProblem {
                path: entry.path().to_string_lossy().into_owned(),
                message: "file name is not valid UTF-8".into(),
            });
            continue;
        };
        if in_library.contains(&path) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        files.push(Candidate {
            path,
            size: i64::try_from(meta.len()).unwrap_or(i64::MAX),
        });
        progress(RelocateEvent::Walking { found: files.len() });
    }
    files
}

struct Ctx<'a, P: Fn(RelocateEvent)> {
    files: &'a [Candidate],
    cancel: &'a CancelToken,
    progress: &'a P,
    /// Indexes into `files` that are some track's sure match.
    claimed: HashSet<usize>,
    /// Probed durations by file index (`None`: unreadable).
    durations: HashMap<usize, Option<i64>>,
}

impl<P: Fn(RelocateEvent)> Ctx<'_, P> {
    fn found(&self, i: usize, track: &Missing, duration_ms: Option<i64>) -> FoundFile {
        FoundFile {
            path: self.files[i].path.clone(),
            size: self.files[i].size,
            duration_ms,
            same_size: self.files[i].size == track.file_size,
        }
    }

    /// Step 1. Pushes tracks without a content match onto `unresolved`.
    fn match_content<'m>(
        &mut self,
        missing: &'m [Missing],
        plan: &mut RelocatePlan,
        unresolved: &mut Vec<&'m Missing>,
    ) {
        let sizes: HashSet<i64> = missing
            .iter()
            .filter(|t| t.content_hash.is_some())
            .map(|t| t.file_size)
            .collect();
        let to_hash: Vec<usize> = (0..self.files.len())
            .filter(|&i| sizes.contains(&self.files[i].size))
            .collect();
        let mut hashes: HashMap<usize, String> = HashMap::new();
        for (n, &i) in to_hash.iter().enumerate() {
            if self.cancel.is_cancelled() {
                return;
            }
            match content_hash(Path::new(&self.files[i].path)) {
                Ok(h) => {
                    hashes.insert(i, h);
                }
                Err(e) => plan.problems.push(RelocateProblem {
                    path: self.files[i].path.clone(),
                    message: format!("cannot read file: {e}"),
                }),
            }
            (self.progress)(RelocateEvent::Hashing {
                done: n + 1,
                total: to_hash.len(),
            });
        }

        for t in missing {
            let hits: Vec<usize> = match &t.content_hash {
                Some(h) => to_hash
                    .iter()
                    .copied()
                    .filter(|i| {
                        self.files[*i].size == t.file_size
                            && hashes.get(i) == Some(h)
                            && !self.claimed.contains(i)
                    })
                    .collect(),
                None => Vec::new(),
            };
            match hits.as_slice() {
                [] => unresolved.push(t),
                [i] => {
                    self.claimed.insert(*i);
                    plan.sure.push(SureMatch {
                        track: t.track_ref(),
                        file: self.found(*i, t, t.duration_ms),
                    });
                }
                _ => plan.review.push(ReviewItem {
                    track: t.track_ref(),
                    basis: ReviewBasis::SameContent,
                    candidates: hits
                        .iter()
                        .take(MAX_CANDIDATES)
                        .map(|&i| self.found(i, t, t.duration_ms))
                        .collect(),
                }),
            }
        }
    }

    /// Step 2: candidates by name or tags and length, for the rest.
    fn match_possible(&mut self, unresolved: Vec<&Missing>, plan: &mut RelocatePlan) {
        if unresolved.is_empty() {
            return;
        }
        let pool: Vec<usize> = (0..self.files.len())
            .filter(|i| !self.claimed.contains(i))
            .collect();
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for &i in &pool {
            if let Some(k) = name_key(&self.files[i].path) {
                by_name.entry(k).or_default().push(i);
            }
        }
        // Tags cost a read per file, so only when a track has artist and title.
        let mut by_tags: HashMap<String, Vec<usize>> = HashMap::new();
        if unresolved
            .iter()
            .any(|t| tag_key(t.artist.as_deref(), t.title.as_deref()).is_some())
        {
            for (n, &i) in pool.iter().enumerate() {
                if self.cancel.is_cancelled() {
                    return;
                }
                if let Ok(tags) = reks_audio::tags(Path::new(&self.files[i].path)) {
                    if let Some(k) = tag_key(tags.artist.as_deref(), tags.title.as_deref()) {
                        by_tags.entry(k).or_default().push(i);
                    }
                }
                (self.progress)(RelocateEvent::Reading {
                    done: n + 1,
                    total: pool.len(),
                });
            }
        }

        for t in unresolved {
            if self.cancel.is_cancelled() {
                return;
            }
            let mut idx: Vec<usize> = Vec::new();
            if let Some(v) = name_key(&t.path).and_then(|k| by_name.get(&k)) {
                idx.extend(v);
            }
            if let Some(v) =
                tag_key(t.artist.as_deref(), t.title.as_deref()).and_then(|k| by_tags.get(&k))
            {
                idx.extend(v);
            }
            idx.sort_unstable();
            idx.dedup();
            let mut found: Vec<FoundFile> = idx
                .into_iter()
                .filter_map(|i| {
                    let d = self.duration(i);
                    durations_agree(d, t.duration_ms).then(|| self.found(i, t, d))
                })
                .collect();
            found.sort_by(|a, b| b.same_size.cmp(&a.same_size).then(a.path.cmp(&b.path)));
            found.truncate(MAX_CANDIDATES);
            if found.is_empty() {
                plan.not_found.push(t.track_ref());
            } else {
                plan.review.push(ReviewItem {
                    track: t.track_ref(),
                    basis: ReviewBasis::Possible,
                    candidates: found,
                });
            }
        }
    }

    fn duration(&mut self, i: usize) -> Option<i64> {
        let path = &self.files[i].path;
        *self.durations.entry(i).or_insert_with(|| {
            reks_audio::probe(Path::new(path))
                .ok()
                .and_then(|p| i64::try_from(p.duration_ms).ok())
        })
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

/// Dot-files and dot-folders, as the scan skips them.
fn is_hidden(e: &walkdir::DirEntry) -> bool {
    e.file_name().to_str().is_some_and(|n| n.starts_with('.'))
}
