//! The pure part of profile status: what the profile wants on the stick
//! against what the stick holds (read from it, or as the last sync left it),
//! and the bytes each profile entry frees. No I/O; [`super::profile_status`]
//! gathers the inputs.

use std::collections::{HashMap, HashSet};

use reks_core::{PlaylistId, SyncedPlaylist, TrackId};
use serde::Serialize;

/// A playlist the profile puts on the stick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Wanted {
    pub id: PlaylistId,
    /// Folder names, then the playlist name, root first.
    pub path: Vec<String>,
    /// Library entries in order.
    pub entries: Vec<TrackId>,
}

/// How one stick playlist compares with the profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SyncState {
    /// The stick holds this playlist with exactly these entries, in order.
    InSync,
    /// The stick holds an older version. `added`/`removed` count entries
    /// (library side minus stick side and back, duplicates counted);
    /// `reordered` is set when the same entries are in another order;
    /// `moved_from` is the stick path when the playlist was renamed or moved.
    Changed {
        added: u32,
        removed: u32,
        reordered: bool,
        moved_from: Option<Vec<String>>,
    },
    /// In the profile, not on the stick yet.
    New,
    /// On the stick, no longer in the profile (unticked, or deleted from the
    /// library): the next sync removes it.
    WillRemove,
}

/// One row of [`compare`]: a wanted playlist, or a stick playlist to remove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Compared {
    /// The library playlist, when known.
    pub playlist_id: Option<PlaylistId>,
    /// The wanted path, or the stick path for [`SyncState::WillRemove`].
    pub path: Vec<String>,
    /// Entries: the wanted count, or the stick's for `WillRemove`.
    pub tracks: u32,
    pub state: SyncState,
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Compares `wanted` (in profile tree order) with `have` (in stick order).
///
/// A stick playlist pairs with the wanted playlist it was written from
/// (`playlist_id`), else with an unpaired wanted playlist at the same path.
/// Rows: every wanted playlist in order, then the unpaired stick playlists as
/// [`SyncState::WillRemove`]. A stick entry that is no library track (`None`)
/// always counts as removed.
pub(crate) fn compare(wanted: &[Wanted], have: &[SyncedPlaylist]) -> Vec<Compared> {
    let mut used = vec![false; have.len()];
    let mut pair: Vec<Option<usize>> = vec![None; wanted.len()];
    for (w, slot) in wanted.iter().zip(pair.iter_mut()) {
        if let Some(h) = (0..have.len()).find(|&h| !used[h] && have[h].playlist_id == Some(w.id)) {
            used[h] = true;
            *slot = Some(h);
        }
    }
    for (w, slot) in wanted.iter().zip(pair.iter_mut()) {
        if slot.is_some() {
            continue;
        }
        if let Some(h) = (0..have.len()).find(|&h| !used[h] && have[h].path == w.path) {
            used[h] = true;
            *slot = Some(h);
        }
    }

    let mut out: Vec<Compared> = wanted
        .iter()
        .zip(&pair)
        .map(|(w, h)| Compared {
            playlist_id: Some(w.id),
            path: w.path.clone(),
            tracks: count(w.entries.len()),
            state: match h {
                None => SyncState::New,
                Some(h) => state_of(w, &have[*h]),
            },
        })
        .collect();
    out.extend(
        have.iter()
            .zip(&used)
            .filter(|(_, used)| !**used)
            .map(|(h, _)| Compared {
                playlist_id: h.playlist_id,
                path: h.path.clone(),
                tracks: count(h.entries.len()),
                state: SyncState::WillRemove,
            }),
    );
    out
}

fn state_of(w: &Wanted, h: &SyncedPlaylist) -> SyncState {
    let same_entries = w.entries.len() == h.entries.len()
        && w.entries
            .iter()
            .zip(&h.entries)
            .all(|(a, b)| Some(*a) == *b);
    let moved_from = (h.path != w.path).then(|| h.path.clone());
    if same_entries && moved_from.is_none() {
        return SyncState::InSync;
    }
    // Multiset difference both ways.
    let mut left: HashMap<TrackId, i64> = HashMap::new();
    for &t in &w.entries {
        *left.entry(t).or_default() += 1;
    }
    let mut removed = 0usize;
    for e in &h.entries {
        match e.and_then(|t| left.get_mut(&t).filter(|n| **n > 0)) {
            Some(n) => *n -= 1,
            None => removed += 1,
        }
    }
    let added: i64 = left.values().sum();
    let added = usize::try_from(added).unwrap_or(0);
    SyncState::Changed {
        added: count(added),
        removed: count(removed),
        reordered: !same_entries && added == 0 && removed == 0,
        moved_from,
    }
}

/// Planned audio bytes for `entries` (each a profile entry's tracks): every
/// track once, however many playlists hold it.
pub(crate) fn planned_bytes(entries: &[HashSet<TrackId>], size: &HashMap<TrackId, u64>) -> u64 {
    let all: HashSet<TrackId> = entries.iter().flatten().copied().collect();
    all.iter().map(|t| size.get(t).copied().unwrap_or(0)).sum()
}

/// For each entry, the bytes unticking it alone frees: its tracks that no
/// other entry holds. Equals `planned_bytes(all) - planned_bytes(all but it)`.
pub(crate) fn frees(entries: &[HashSet<TrackId>], size: &HashMap<TrackId, u64>) -> Vec<u64> {
    let mut holders: HashMap<TrackId, usize> = HashMap::new();
    for e in entries {
        for &t in e {
            *holders.entry(t).or_default() += 1;
        }
    }
    entries
        .iter()
        .map(|e| {
            e.iter()
                .filter(|t| holders.get(t) == Some(&1))
                .map(|t| size.get(t).copied().unwrap_or(0))
                .sum()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn wanted(id: PlaylistId, path: &[&str], entries: &[TrackId]) -> Wanted {
        Wanted {
            id,
            path: path.iter().map(|s| (*s).to_owned()).collect(),
            entries: entries.to_vec(),
        }
    }

    fn have(id: Option<PlaylistId>, path: &[&str], entries: &[Option<TrackId>]) -> SyncedPlaylist {
        SyncedPlaylist {
            playlist_id: id,
            path: path.iter().map(|s| (*s).to_owned()).collect(),
            entries: entries.to_vec(),
        }
    }

    fn states(rows: &[Compared]) -> Vec<&SyncState> {
        rows.iter().map(|r| &r.state).collect()
    }

    #[test]
    fn each_state() {
        let w = [
            wanted(1, &["Set", "Peak"], &[1, 2, 3]),
            wanted(2, &["Set", "Chill"], &[4, 5]),
            wanted(3, &["Fresh"], &[6]),
            wanted(4, &["Renamed"], &[7, 8]),
            wanted(5, &["Same path"], &[9]),
        ];
        let h = [
            have(Some(1), &["Set", "Peak"], &[Some(1), Some(2), Some(3)]),
            have(Some(2), &["Set", "Chill"], &[Some(5), Some(4), None]),
            have(Some(4), &["Old name"], &[Some(8), Some(7)]),
            have(None, &["Same path"], &[Some(9)]),
            have(Some(99), &["Gone"], &[Some(1)]),
        ];
        let rows = compare(&w, &h);
        assert_eq!(
            states(&rows),
            [
                &SyncState::InSync,
                &SyncState::Changed {
                    added: 0,
                    removed: 1,
                    reordered: false,
                    moved_from: None
                },
                &SyncState::New,
                &SyncState::Changed {
                    added: 0,
                    removed: 0,
                    reordered: true,
                    moved_from: Some(vec!["Old name".into()])
                },
                &SyncState::InSync,
                &SyncState::WillRemove,
            ]
        );
        assert_eq!(rows[5].path, ["Gone"]);
        assert_eq!(rows[5].playlist_id, Some(99));
        assert_eq!(rows[5].tracks, 1);
    }

    #[test]
    fn duplicate_entries_count() {
        let rows = compare(
            &[wanted(1, &["A"], &[1, 1, 2])],
            &[have(Some(1), &["A"], &[Some(1), Some(2), Some(2)])],
        );
        assert_eq!(
            rows[0].state,
            SyncState::Changed {
                added: 1,
                removed: 1,
                reordered: false,
                moved_from: None
            }
        );
    }

    #[test]
    fn id_wins_over_path() {
        // Playlist 2 took the name of playlist 1, which moved: each keeps its own.
        let rows = compare(
            &[wanted(1, &["B"], &[1]), wanted(2, &["A"], &[2])],
            &[
                have(Some(2), &["A"], &[Some(2)]),
                have(Some(1), &["A"], &[Some(1)]),
            ],
        );
        assert_eq!(rows[1].state, SyncState::InSync);
        assert!(
            matches!(&rows[0].state, SyncState::Changed { moved_from: Some(p), .. } if p == &["A"])
        );
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn frees_counts_shared_tracks_once() {
        let size: HashMap<TrackId, u64> = [(1, 10), (2, 20), (3, 40)].into();
        let e = [HashSet::from([1, 2]), HashSet::from([2, 3]), HashSet::new()];
        assert_eq!(planned_bytes(&e, &size), 70);
        assert_eq!(frees(&e, &size), [10, 40, 0]);
    }

    /// A stick playlist made from `entries` by dropping the positions in
    /// `drop` and inserting `extra` fresh tracks (ids from 1000) at `at`.
    fn edited(entries: &[TrackId], drop: &[bool], extra: &[usize]) -> Vec<Option<TrackId>> {
        let mut out: Vec<Option<TrackId>> = entries
            .iter()
            .zip(drop.iter().chain(std::iter::repeat(&false)))
            .filter(|(_, d)| !**d)
            .map(|(t, _)| Some(*t))
            .collect();
        for (i, &at) in extra.iter().enumerate() {
            let at = at % (out.len() + 1);
            out.insert(at, Some(1000 + i as TrackId));
        }
        out
    }

    proptest! {
        /// Library edits (entries dropped from the stick's copy = added on the
        /// library side, fresh stick entries = removed, playlists unticked or
        /// added, renames) give exact counts, and every playlist on either
        /// side appears exactly once.
        #[test]
        fn status_counts_match_the_edits(
            lists in prop::collection::vec(
                (
                    prop::collection::vec(0i64..30, 0..12),   // library entries
                    prop::collection::vec(any::<bool>(), 0..12),  // dropped on the stick
                    prop::collection::vec(0usize..20, 0..4),  // extra stick entries
                    0u8..4,                                   // 0 synced, 1 renamed, 2 new, 3 unticked
                ),
                0..10,
            )
        ) {
            let mut w = Vec::new();
            let mut h = Vec::new();
            let mut expect = Vec::new();
            for (i, (entries, drop, extra, kind)) in lists.iter().enumerate() {
                let id = i as PlaylistId + 1;
                let path = vec![format!("P{i}")];
                let stick = edited(entries, drop, extra);
                let dropped = entries.len() + extra.len() - stick.len();
                match kind {
                    0 | 1 => {
                        w.push(Wanted { id, path: path.clone(), entries: entries.clone() });
                        let stick_path = if *kind == 1 { vec![format!("old {i}")] } else { path };
                        h.push(SyncedPlaylist { playlist_id: Some(id), path: stick_path, entries: stick });
                        expect.push(Some((dropped, extra.len(), *kind == 1)));
                    }
                    2 => {
                        w.push(Wanted { id, path, entries: entries.clone() });
                        expect.push(None);
                    }
                    _ => h.push(SyncedPlaylist { playlist_id: Some(id), path, entries: stick }),
                }
            }
            let rows = compare(&w, &h);
            let removes = lists.iter().filter(|l| l.3 == 3).count();
            prop_assert_eq!(rows.len(), w.len() + removes);
            for (row, exp) in rows.iter().zip(&expect) {
                match (exp, &row.state) {
                    (None, SyncState::New) => {}
                    (Some((0, 0, false)), SyncState::InSync) => {}
                    (Some((a, r, moved)), SyncState::Changed { added, removed, moved_from, .. }) => {
                        prop_assert_eq!(*added as usize, *a);
                        prop_assert_eq!(*removed as usize, *r);
                        prop_assert_eq!(moved_from.is_some(), *moved);
                    }
                    (exp, got) => prop_assert!(false, "expected {:?}, got {:?}", exp, got),
                }
                prop_assert_eq!(row.tracks as usize, w.iter().find(|x| Some(x.id) == row.playlist_id).unwrap().entries.len());
            }
            prop_assert!(rows[w.len()..].iter().all(|r| r.state == SyncState::WillRemove));
        }

        /// "Frees X MB" is the planned size with the entry minus without it.
        #[test]
        fn frees_is_the_planned_difference(
            entries in prop::collection::vec(prop::collection::hash_set(0i64..25, 0..10), 0..6),
            sizes in prop::collection::vec(0u64..1_000_000, 25),
        ) {
            let size: HashMap<TrackId, u64> =
                sizes.iter().enumerate().map(|(i, s)| (i as TrackId, *s)).collect();
            let all = planned_bytes(&entries, &size);
            let freed = frees(&entries, &size);
            for (i, f) in freed.iter().enumerate() {
                let mut rest = entries.clone();
                rest.remove(i);
                prop_assert_eq!(*f, all - planned_bytes(&rest, &size));
            }
            let unique: HashSet<TrackId> = entries.iter().flatten().copied().collect();
            prop_assert_eq!(all, unique.iter().map(|t| size[t]).sum::<u64>());
        }
    }
}
