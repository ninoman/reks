//! Change a library folder: every track path under it gets the new prefix.
//!
//! The dry run looks at a sample of the files at the new place; the change
//! itself is refused when none of the sample is there (wrong folder, drive
//! not mounted) or a new path would collide, and is otherwise one
//! transaction.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, Transaction};

use super::{RemapExample, RemapPlan, RemapReport};
use crate::db::Library;
use crate::error::{Error, Result};
use crate::fileinfo::library_path;
use crate::timefmt;

/// Files checked at the new place.
const SAMPLE: usize = 24;

/// Examples shown in the dry run.
const EXAMPLES: usize = 5;

/// Looks at what changing library folder `root_id` to `new_dir` would do.
/// Writes nothing.
pub fn plan_remap(lib: &Library, root_id: i64, new_dir: &Path) -> Result<RemapPlan> {
    let conn = lib.conn()?;
    build(&conn, root_id, new_dir)
}

/// Changes library folder `root_id` to `new_dir`, rewriting the paths of its
/// tracks (and of library folders inside it) in one transaction. Refuses
/// under the conditions of [`plan_remap`]; nothing is written then.
pub fn remap(lib: &Library, root_id: i64, new_dir: &Path) -> Result<RemapReport> {
    let mut conn = lib.conn()?;
    let tx = conn.transaction()?;
    let plan = build(&tx, root_id, new_dir)?;
    if plan.collisions > 0 {
        return Err(refused(format!(
            "{} tracks would get a path that another track already has",
            plan.collisions
        )));
    }
    if plan.sampled > 0 && plan.sample_found == 0 {
        return Err(refused(format!(
            "none of {} files checked exist under {}; is the drive connected?",
            plan.sampled, plan.new_path
        )));
    }
    let now = timefmt::now_utc();
    let tracks = rewrite(&tx, &plan, &now)?;
    tx.commit()?;
    Ok(RemapReport {
        tracks,
        roots: 1 + plan.nested_roots,
    })
}

fn refused(message: String) -> Error {
    Error::RelocateRefused(message)
}

/// `path` with the prefix `old` replaced by `new`; `None` outside `old`.
fn swap_prefix(path: &str, old: &str, new: &str) -> Option<String> {
    let rest = Path::new(path).strip_prefix(old).ok()?;
    if rest.as_os_str().is_empty() {
        return Some(new.to_owned()); // the folder itself: no trailing separator
    }
    library_path(&Path::new(new).join(rest))
}

fn build(conn: &Connection, root_id: i64, new_dir: &Path) -> Result<RemapPlan> {
    let old: String = conn
        .query_row(
            "SELECT path FROM library_root WHERE id = ?1",
            [root_id],
            |r| r.get(0),
        )
        .map_err(|_| Error::LibraryRootNotFound(root_id))?;
    let abs = std::path::absolute(new_dir).unwrap_or_else(|_| new_dir.to_path_buf());
    if !abs.is_dir() {
        return Err(refused(format!("{} is not a folder", abs.display())));
    }
    let new =
        library_path(&abs).ok_or_else(|| refused("the folder name is not valid UTF-8".into()))?;
    if new == old {
        return Err(refused("that is already this library folder".into()));
    }
    if Path::new(&new).starts_with(&old) {
        return Err(refused("a folder cannot move inside itself".into()));
    }
    let taken: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM library_root WHERE path = ?1)",
        [&new],
        |r| r.get(0),
    )?;
    if taken {
        return Err(refused("that folder is already a library folder".into()));
    }

    let mut moved: Vec<(String, String)> = Vec::new();
    let mut stmt = conn.prepare("SELECT path FROM track ORDER BY path")?;
    let paths = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut staying: HashSet<String> = HashSet::new();
    for p in paths {
        let p = p?;
        match swap_prefix(&p, &old, &new) {
            Some(to) => moved.push((p, to)),
            None => {
                staying.insert(p);
            }
        }
    }
    let collisions = moved.iter().filter(|(_, to)| staying.contains(to)).count();

    let nested_roots = {
        let mut stmt = conn.prepare("SELECT path FROM library_root WHERE id != ?1")?;
        let paths = stmt.query_map([root_id], |r| r.get::<_, String>(0))?;
        let mut n = 0;
        for p in paths {
            n += usize::from(Path::new(&p?).starts_with(&old));
        }
        n
    };

    // Evenly spread, so a half-moved library shows up.
    let step = moved.len().div_ceil(SAMPLE).max(1);
    let sample: Vec<&(String, String)> = moved.iter().step_by(step).collect();
    let sample_found = sample
        .iter()
        .filter(|(_, to)| Path::new(to).exists())
        .count();

    Ok(RemapPlan {
        root_id,
        old_path: old,
        new_path: new,
        tracks: moved.len(),
        nested_roots,
        examples: moved
            .iter()
            .take(EXAMPLES)
            .map(|(from, to)| RemapExample {
                from: from.clone(),
                to: to.clone(),
            })
            .collect(),
        sampled: sample.len(),
        sample_found,
        collisions,
    })
}

/// Writes the new prefix into tracks and roots; returns the track count.
fn rewrite(tx: &Transaction, plan: &RemapPlan, now: &str) -> Result<usize> {
    let (old, new) = (&plan.old_path, &plan.new_path);
    let rows: Vec<(i64, String)> = {
        let mut stmt = tx.prepare("SELECT id, path FROM track")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut n = 0;
    for (id, path) in rows {
        if let Some(to) = swap_prefix(&path, old, new) {
            tx.execute(
                "UPDATE track SET path = ?2, updated_at = ?3 WHERE id = ?1",
                (id, to, now),
            )?;
            n += 1;
        }
    }
    let roots: Vec<(i64, String)> = {
        let mut stmt = tx.prepare("SELECT id, path FROM library_root")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for (id, path) in roots {
        if let Some(to) = swap_prefix(&path, old, new) {
            tx.execute("UPDATE library_root SET path = ?2 WHERE id = ?1", (id, to))?;
        }
    }
    Ok(n)
}
