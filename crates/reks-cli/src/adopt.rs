//! `reks adopt scan`: the dry run of adopting a stick reks did not write
//! (Phase 2.5 task 12), headless. Read-only on the stick and the library.
//! Applying needs a saved stick profile, so it is done in the app.
//!
//! ```text
//! reks adopt scan <stick> --library <library.sqlite> [--json]
//! ```
//!
//! `<stick>` is a stick's mount point (volume serial from the OS) or any
//! folder standing in for one (no serial).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use reks_core::Library;
use reks_export_rb::adopt::{self, AdoptPlan};

const USAGE: &str = "usage: reks adopt scan <stick> --library <library.sqlite> [--json]";

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let [cmd, path, flags @ ..] = args else {
        bail!("{USAGE}");
    };
    if cmd != "scan" {
        bail!("{USAGE}");
    }
    let mut library = None;
    let mut json = false;
    let mut flags = flags.iter();
    while let Some(flag) = flags.next() {
        match flag.as_str() {
            "--library" => {
                library =
                    Some(PathBuf::from(flags.next().with_context(|| {
                        format!("--library needs a value\n{USAGE}")
                    })?));
            }
            "--json" => json = true,
            other => bail!("unknown flag {other}\n{USAGE}"),
        }
    }
    let library = library.with_context(|| format!("--library is required\n{USAGE}"))?;
    let plan = scan(Path::new(path), &library)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        print!("{}", render(&plan));
    }
    Ok(())
}

fn scan(path: &Path, library: &Path) -> anyhow::Result<AdoptPlan> {
    if !library.is_file() {
        bail!("no library at {}", library.display());
    }
    let cache_dir = library.parent().unwrap_or(Path::new(".")).join("cache");
    let lib = Library::open(library, &cache_dir)?;
    let st = crate::rb::stick(path)?;
    Ok(adopt::scan(
        &st.dev.mount_point,
        st.serial.as_deref(),
        &lib,
    )?)
}

fn render(plan: &AdoptPlan) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", plan.summary);
    let _ = writeln!(out, "{}", plan.history_text);
    for n in &plan.notes {
        let _ = writeln!(out, "note: {n}");
    }
    for p in &plan.playlists {
        let _ = writeln!(out, "  {} [pdb {}]: {}", p.path_text, p.pdb_id, p.text);
        for c in &p.link {
            let _ = writeln!(out, "    can link to {} [{}]", c.path_text, c.playlist_id);
        }
    }
    if !plan.unmatched.is_empty() {
        let _ = writeln!(out, "{}", plan.unmatched_text);
        for t in &plan.unmatched {
            let _ = writeln!(
                out,
                "  {} – {} {}",
                t.artist.as_deref().unwrap_or("?"),
                t.title.as_deref().unwrap_or("(track row missing)"),
                t.stick_path.as_deref().unwrap_or("")
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::fs;

    use reks_export_rb::testutil::*;

    use super::*;

    #[test]
    fn scans_a_fake_stick_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("lib.sqlite");
        drop(Library::open(&db, &dir.path().join("cache")).unwrap());
        let stick = dir.path().join("stick");
        let pdb = stick.join("PIONEER/rekordbox/export.pdb");
        fs::create_dir_all(pdb.parent().unwrap()).unwrap();
        let bytes = synthetic_pdb(&[
            (
                T_TRACKS,
                vec![vec![track_row(5, 0, 30, "Sine", "/Contents/x/a.wav")]],
            ),
            (
                T_PLAYLIST_TREE,
                vec![vec![playlist_node(1, 0, 0, false, "Peak")]],
            ),
            (T_PLAYLIST_ENTRIES, vec![vec![playlist_entry(1, 5, 1)]]),
        ]);
        fs::write(&pdb, &bytes).unwrap();
        let plan = scan(&stick, &db).unwrap();
        let text = render(&plan);
        assert!(
            text.contains("1 playlist, 1 track entry: 0 in your library, 1 not"),
            "{text}"
        );
        assert!(
            text.contains("  Peak [pdb 1]: 1 track, 1 not in your library"),
            "{text}"
        );
        assert_eq!(fs::read(&pdb).unwrap(), bytes);
        assert!(run(&["scan".into(), stick.display().to_string()]).is_err());
    }
}
