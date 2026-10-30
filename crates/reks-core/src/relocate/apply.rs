//! Applying a dry run: relink the sure matches and the user's picks.
//!
//! Files are checked first, outside the database lock (and cancellably); the
//! rows are then updated in one transaction, so a cancel or a failure leaves
//! the library as it was. A row that changed since the dry run is skipped
//! and reported, never overwritten.

use std::collections::HashSet;
use std::path::Path;

use super::{
    FoundFile, RelocateChoices, RelocateEvent, RelocatePlan, RelocateReport, ReviewBasis, Skipped,
    TrackRef,
};
use crate::backup::BackupReason;
use crate::db::Library;
use crate::error::{Error, Result};
use crate::fileinfo;
use crate::scan::CancelToken;
use crate::timefmt;

/// One track and the file it is relinked to.
struct Assignment<'a> {
    track: &'a TrackRef,
    file: &'a FoundFile,
    /// The file's content is known to equal the track's (hash match), so its
    /// size and mtime are stored. Otherwise they stay, and the next scan
    /// refreshes the track from the file.
    verified: bool,
    /// Set by the file check.
    mtime: String,
}

/// Relinks the tracks `plan` found, plus the picks in `choices`.
pub fn apply(
    lib: &Library,
    plan: &RelocatePlan,
    choices: &RelocateChoices,
    cancel: &CancelToken,
    progress: impl Fn(RelocateEvent),
) -> Result<RelocateReport> {
    if plan.cancelled {
        return Err(Error::RelocateRefused(
            "this dry run was stopped early; run it again".into(),
        ));
    }
    let mut report = RelocateReport::default();
    let mut todo = assignments(plan, choices)?;

    // Each file goes to one track.
    let mut used = HashSet::new();
    todo.retain(|a| {
        let fresh = used.insert(a.file.path.as_str());
        if !fresh {
            report
                .skipped
                .push(skip(a, "the file is already used for another track"));
        }
        fresh
    });

    let total = todo.len();
    let mut checked = Vec::with_capacity(total);
    for (n, mut a) in todo.into_iter().enumerate() {
        if cancel.is_cancelled() {
            return Ok(RelocateReport {
                cancelled: true,
                ..RelocateReport::default()
            });
        }
        progress(RelocateEvent::Applying { done: n, total });
        match fileinfo::stat(&a.file.path) {
            Ok((size, mtime)) if size == a.file.size => {
                a.mtime = mtime;
                checked.push(a);
            }
            Ok(_) => report
                .skipped
                .push(skip(&a, "the file changed since the dry run")),
            Err(e) => report
                .skipped
                .push(skip(&a, &format!("cannot read the file: {e}"))),
        }
    }
    if cancel.is_cancelled() {
        return Ok(RelocateReport {
            cancelled: true,
            ..RelocateReport::default()
        });
    }

    if !checked.is_empty() {
        lib.snapshot_before(BackupReason::Relocate);
    }
    {
        let mut conn = lib.conn()?;
        let tx = conn.transaction()?;
        let now = timefmt::now_utc();
        for a in &checked {
            let row: Option<(String, bool)> = tx
                .query_row(
                    "SELECT path, missing FROM track WHERE id = ?1",
                    [a.track.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .ok();
            if row != Some((a.track.path.clone(), true)) {
                report
                    .skipped
                    .push(skip(a, "the track changed since the dry run"));
                continue;
            }
            let taken: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM track WHERE path = ?1 AND id != ?2)",
                (&a.file.path, a.track.id),
                |r| r.get(0),
            )?;
            if taken {
                report
                    .skipped
                    .push(skip(a, "the file is already in the library"));
                continue;
            }
            if a.verified {
                tx.execute(
                    "UPDATE track SET path = ?2, file_mtime = ?3, missing = 0, updated_at = ?4
                     WHERE id = ?1",
                    (a.track.id, &a.file.path, &a.mtime, &now),
                )?;
            } else {
                tx.execute(
                    "UPDATE track SET path = ?2, missing = 0, updated_at = ?3 WHERE id = ?1",
                    (a.track.id, &a.file.path, &now),
                )?;
            }
            report.relinked += 1;
        }
        tx.commit()?;
    }
    progress(RelocateEvent::Applying { done: total, total });

    if choices.add_root && !plan.inside_root {
        report.root = Some(lib.add_library_root(Path::new(&plan.folder))?);
    }
    Ok(report)
}

/// The sure matches and the picks, each checked against the plan.
fn assignments<'a>(
    plan: &'a RelocatePlan,
    choices: &RelocateChoices,
) -> Result<Vec<Assignment<'a>>> {
    let mut out: Vec<Assignment> = plan
        .sure
        .iter()
        .map(|m| Assignment {
            track: &m.track,
            file: &m.file,
            verified: true,
            mtime: String::new(),
        })
        .collect();
    for (id, path) in &choices.picks {
        let picked = plan.review.iter().find_map(|r| {
            let file = r.candidates.iter().find(|c| &c.path == path)?;
            (r.track.id == *id).then_some((r, file))
        });
        let Some((item, file)) = picked else {
            return Err(Error::RelocateRefused(format!(
                "{path} is not an option for track {id} in the dry run"
            )));
        };
        out.push(Assignment {
            track: &item.track,
            file,
            verified: item.basis == ReviewBasis::SameContent,
            mtime: String::new(),
        });
    }
    Ok(out)
}

fn skip(a: &Assignment, reason: &str) -> Skipped {
    Skipped {
        track: a.track.clone(),
        reason: reason.to_owned(),
    }
}
