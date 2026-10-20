//! `reks history ...`: import the play history players left on a stick
//! (Phase 2.5 task 10), headless. Same shape as `reks rb export`: a dry run
//! first, writes only with `--yes`.
//!
//! ```text
//! reks history scan <stick|backup> --library <library.sqlite> [--hist1-dates] [--json]
//!     print the import dry run (read-only, stick and library)
//! reks history import <stick|backup> <scan flags>
//!                     [--practice <session>]... [--include <session>]... [--yes]
//!     print the dry run with the choices applied; with --yes, import the
//!     sessions into the library and recompute play counts
//! ```
//!
//! `<stick|backup>` is a stick's mount point, any folder standing in for a
//! stick (no volume serial), or a Phase 2 `stick-backups/<serial>/<time>/`
//! folder (recognised by its `manifest.json`; its serial comes from there).
//! `<session>` is a session's stick name (`HISTORY 001`, or `HISTORY 001
//! (OneLibrary)` for `exportLibrary.db`'s) or the start of its fingerprint
//! (at least 8 hex digits). `--practice` imports a new session
//! excluded from play counts, `--include` imports it counted (the default).
//! `--hist1-dates` reads session dates with the UNVERIFIED table-19
//! heuristic (HIST1); without it every session is undated.
//!
//! Both stick libraries are read (Phase 4 task 11): `export.pdb` and, with
//! the OneLibrary key (Settings → Experimental, else `REKS_ONELIBRARY_KEY`),
//! `exportLibrary.db`. Its sessions are listed with `(OneLibrary)` and
//! import with source `onelibrary`. A database that is not read (no key,
//! wrong key, a backup copy that does not match its manifest) is reported
//! and left out; the `export.pdb` sessions still import.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use reks_core::history::{
    self, EntryKind, HistoryImportPlan, HistoryLibrary, ImportChoices, ImportReport, MatchKind,
    PlannedSession, SessionAction, SkipReason,
};
use reks_core::{Library, OneLibraryKey};
use reks_export_onelibrary::history::{read_both, OneLibraryStatus};
use reks_export_rb::history::{HistoryReadOptions, HistorySource, BACKUP_MANIFEST};
use reks_export_rb::onelibrary::resolve_key;
use serde::Serialize;

const USAGE: &str = "usage: reks history scan <stick|backup> --library <library.sqlite> \
[--hist1-dates] [--json]
       reks history import <stick|backup> <scan flags> [--practice <session>]... \
[--include <session>]... [--yes]";

/// Shortest fingerprint prefix a `<session>` may use.
const MIN_FINGERPRINT_PREFIX: usize = 8;

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let (cmd, path, flags) = match args {
        [cmd, path, flags @ ..] if cmd == "scan" => (Command::Scan, path, flags),
        [cmd, path, flags @ ..] if cmd == "import" => (Command::Import, path, flags),
        _ => bail!("{USAGE}"),
    };
    let args = parse_args(flags, cmd)?;
    let source = source(Path::new(path))?;
    let out = execute(&args, &source)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        print!("{}", render(&out));
        if out.report.is_none() && cmd == Command::Import {
            eprintln!("Nothing written. Pass --yes to import.");
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Scan,
    Import,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    library: PathBuf,
    hist1_dates: bool,
    json: bool,
    /// `import`: (selector, practice).
    choices: Vec<(String, bool)>,
    /// `import`: write the library.
    yes: bool,
}

fn parse_args(flags: &[String], cmd: Command) -> anyhow::Result<Args> {
    let mut library = None;
    let mut hist1_dates = false;
    let mut json = false;
    let mut choices = Vec::new();
    let mut yes = false;
    let mut flags = flags.iter();
    while let Some(flag) = flags.next() {
        let mut value = || {
            flags
                .next()
                .with_context(|| format!("{flag} needs a value\n{USAGE}"))
        };
        match flag.as_str() {
            "--library" => library = Some(PathBuf::from(value()?)),
            "--hist1-dates" => hist1_dates = true,
            "--json" => json = true,
            "--practice" if cmd == Command::Import => choices.push((value()?.clone(), true)),
            "--include" if cmd == Command::Import => choices.push((value()?.clone(), false)),
            "--yes" if cmd == Command::Import => yes = true,
            other => bail!("unknown flag {other}\n{USAGE}"),
        }
    }
    Ok(Args {
        library: library.with_context(|| format!("--library is required\n{USAGE}"))?,
        hist1_dates,
        json,
        choices,
        yes,
    })
}

/// What the command reads: a stick (with its serial, if any) or a backup.
#[derive(Debug)]
struct Source {
    src: HistorySource,
    stick_serial: Option<String>,
}

fn source(path: &Path) -> anyhow::Result<Source> {
    if path.join(BACKUP_MANIFEST).is_file() {
        let dir = std::fs::canonicalize(path).with_context(|| format!("{}", path.display()))?;
        return Ok(Source {
            src: HistorySource::Backup(dir),
            stick_serial: None,
        });
    }
    let st = crate::rb::stick(path)?;
    Ok(Source {
        src: HistorySource::Stick(st.dev.mount_point),
        stick_serial: st.serial,
    })
}

/// Output of both commands (also their `--json`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Out {
    source: HistorySource,
    /// The source's `exportLibrary.db`.
    onelibrary: OneLibraryStatus,
    plan: HistoryImportPlan,
    /// Fingerprint → excluded, for every new session: the choice, else the
    /// plan's default.
    excluded: HashMap<String, bool>,
    /// `None` unless `import --yes` ran.
    report: Option<ImportReport>,
}

fn execute(args: &Args, source: &Source) -> anyhow::Result<Out> {
    if !args.library.is_file() {
        bail!("no library at {}", args.library.display());
    }
    let cache_dir = args
        .library
        .parent()
        .unwrap_or(Path::new("."))
        .join("cache");
    let lib = Library::open(&args.library, &cache_dir)?;
    let key = resolve_key(&lib)?;
    let (both, onelibrary) = read_both(
        &source.src,
        &HistoryReadOptions {
            stick_serial: source.stick_serial.clone(),
            hist1_dates: args.hist1_dates,
        },
        key.as_ref().map(OneLibraryKey::expose),
    )?;
    let plan = history::scan(&both, &lib)?;
    let choices = resolve_choices(&plan, &args.choices)?;
    let excluded = plan
        .sessions
        .iter()
        .filter(|s| s.action == SessionAction::New)
        .map(|s| {
            let ex = choices
                .excluded
                .get(&s.fingerprint)
                .copied()
                .unwrap_or(false);
            (s.fingerprint.clone(), ex)
        })
        .collect();
    let report = if args.yes {
        Some(history::import(&plan, &choices, &lib)?)
    } else {
        None
    };
    Ok(Out {
        source: source.src.clone(),
        onelibrary,
        plan,
        excluded,
        report,
    })
}

/// `--practice` / `--include` selectors → choices. Each must name exactly
/// one new session.
fn resolve_choices(
    plan: &HistoryImportPlan,
    wanted: &[(String, bool)],
) -> anyhow::Result<ImportChoices> {
    let mut choices = ImportChoices::default();
    for (sel, practice) in wanted {
        // `HISTORY 001` is export.pdb's, `HISTORY 001 (OneLibrary)` the
        // other library's; a plain name also finds a OneLibrary-only session.
        let mut by_name: Vec<&PlannedSession> = plan
            .sessions
            .iter()
            .filter(|s| shown_name(s) == *sel)
            .collect();
        if by_name.is_empty() {
            by_name = plan
                .sessions
                .iter()
                .filter(|s| s.source_name == *sel)
                .collect();
        }
        let hits = if !by_name.is_empty() {
            by_name
        } else if sel.len() >= MIN_FINGERPRINT_PREFIX {
            let sel = sel.to_ascii_lowercase();
            plan.sessions
                .iter()
                .filter(|s| s.fingerprint.starts_with(&sel))
                .collect()
        } else {
            Vec::new()
        };
        let s = match hits.as_slice() {
            [] => bail!("no session {sel:?} in this history"),
            [s] => *s,
            _ => bail!("{sel:?} names several sessions; use a longer fingerprint"),
        };
        if s.action != SessionAction::New {
            bail!(
                "session {sel:?} is not new ({}); only new sessions take --practice or --include",
                action_text(s)
            );
        }
        if choices
            .excluded
            .insert(s.fingerprint.clone(), *practice)
            .is_some_and(|prev| prev != *practice)
        {
            bail!("session {sel:?} is both --practice and --include");
        }
    }
    Ok(choices)
}

/// The session's stick name, with ` (OneLibrary)` for `exportLibrary.db`'s.
fn shown_name(s: &PlannedSession) -> String {
    match s.library {
        HistoryLibrary::DeviceLibrary => s.source_name.clone(),
        HistoryLibrary::OneLibrary => format!("{} (OneLibrary)", s.source_name),
    }
}

fn action_text(s: &PlannedSession) -> String {
    match s.action {
        SessionAction::New => "new".into(),
        SessionAction::Skip(SkipReason::AlreadyImported { session_id }) => {
            format!("already imported as session {session_id}")
        }
        SessionAction::Skip(SkipReason::DuplicateInSource) => "duplicate on this stick".into(),
        SessionAction::Extend {
            session_id,
            existing_entries,
        } => format!("extends session {session_id} after {existing_entries} entries"),
    }
}

fn render(out: &Out) -> String {
    let mut s = String::new();
    let (what, path) = match &out.source {
        HistorySource::Stick(p) => ("Stick", p),
        HistorySource::Backup(p) => ("Backup", p),
    };
    let p = &out.plan;
    let _ = writeln!(
        s,
        "{what} {} (serial {})",
        path.display(),
        p.volume_serial.as_deref().unwrap_or("unknown")
    );
    let _ = writeln!(
        s,
        "Baseline  {}",
        p.baseline_date.as_deref().unwrap_or("none")
    );
    match &out.onelibrary {
        OneLibraryStatus::Absent => {}
        OneLibraryStatus::Read { sessions } => {
            let _ = writeln!(s, "OneLibrary  exportLibrary.db read: {sessions} sessions");
        }
        OneLibraryStatus::NotRead { reason } => {
            let _ = writeln!(s, "OneLibrary  exportLibrary.db not read: {reason}");
        }
    }
    if p.sessions.is_empty() {
        let _ = writeln!(s, "No play history.");
    }
    let (mut new, mut extended, mut skipped, mut entries, mut unmatched) = (0, 0, 0, 0, 0);
    for x in &p.sessions {
        let _ = writeln!(
            s,
            "  {}  {}  {}  {}",
            shown_name(x),
            x.played_on.as_deref().unwrap_or("undated"),
            &x.fingerprint[..MIN_FINGERPRINT_PREFIX.min(x.fingerprint.len())],
            action_text(x)
        );
        match x.action {
            SessionAction::Skip(_) => {
                skipped += 1;
                continue;
            }
            SessionAction::New => new += 1,
            SessionAction::Extend { .. } => extended += 1,
        }
        let add = x.new_entries();
        let count = |k: MatchKind| {
            add.iter()
                .filter(|e| e.kind == EntryKind::Track && e.matched_by == Some(k))
                .count()
        };
        let dividers = add.iter().filter(|e| e.kind == EntryKind::Divider).count();
        let missing = add
            .iter()
            .filter(|e| e.kind == EntryKind::Track && e.track_id.is_none())
            .count();
        entries += add.len();
        unmatched += missing;
        let _ = writeln!(
            s,
            "      {} {}: {} exact, {} by path, {} by tags, {} unmatched, {} {}",
            add.len(),
            if add.len() == 1 { "entry" } else { "entries" },
            count(MatchKind::Manifest),
            count(MatchKind::PathSuffix),
            count(MatchKind::Metadata),
            missing,
            dividers,
            if dividers == 1 { "divider" } else { "dividers" }
        );
        if let Some(&ex) = out.excluded.get(&x.fingerprint) {
            let why = if ex { "practice" } else { "counted" };
            let _ = writeln!(s, "      {why}");
        }
    }
    let _ = writeln!(
        s,
        "Sessions  {new} new, {extended} extended, {skipped} skipped; \
         {entries} entries, {unmatched} unmatched"
    );
    if let Some(r) = &out.report {
        let _ = writeln!(
            s,
            "Imported  {} new, {} extended, {} skipped; {} entries written, {} unmatched",
            r.created.len(),
            r.extended.len(),
            r.skipped,
            r.entries,
            r.unmatched
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use std::fs;

    use reks_core::{CancelToken, TrackQuery};
    use reks_export_rb::history::{plan_backups, read_history, run_backups, HistoryPolicy};
    use reks_export_rb::testutil::{
        artist_row, history_entry, history_playlist, synthetic_pdb, track_row, T_ARTISTS,
        T_HISTORY_ENTRIES, T_HISTORY_PLAYLISTS, T_TRACKS,
    };

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/audio");

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn flags() {
        let a = parse_args(
            &[
                args("--library lib.sqlite --hist1-dates --json --practice"),
                vec!["HISTORY 001".into()],
                args("--include 0123abcd --yes"),
            ]
            .concat(),
            Command::Import,
        )
        .unwrap();
        assert_eq!(
            a,
            Args {
                library: "lib.sqlite".into(),
                hist1_dates: true,
                json: true,
                choices: vec![("HISTORY 001".into(), true), ("0123abcd".into(), false)],
                yes: true,
            }
        );
        assert!(parse_args(&args("--library x --yes"), Command::Scan).is_err());
        assert!(parse_args(&args("--json"), Command::Scan).is_err());
        assert!(parse_args(&args("--library"), Command::Scan).is_err());
    }

    /// A library with `a.wav`, and a fake stick whose `export.pdb` holds
    /// `HISTORY 001` (a.wav twice, one unknown track) and `HISTORY 002`
    /// (a.wav once).
    fn setup(dir: &Path) -> (PathBuf, PathBuf) {
        let music = dir.join("music");
        fs::create_dir_all(&music).unwrap();
        fs::copy(
            Path::new(FIXTURES).join("sine-click-128.wav"),
            music.join("a.wav"),
        )
        .unwrap();
        let db = dir.join("lib.sqlite");
        let lib = Library::open(&db, &dir.join("cache")).unwrap();
        reks_core::scan(
            std::slice::from_ref(&music),
            &lib,
            &CancelToken::new(),
            |_| {},
        )
        .unwrap();
        let stick = dir.join("stick");
        let pdb = stick.join("PIONEER/rekordbox/export.pdb");
        fs::create_dir_all(pdb.parent().unwrap()).unwrap();
        fs::write(
            &pdb,
            synthetic_pdb(&[
                (
                    T_TRACKS,
                    vec![vec![
                        track_row(5, 1, 30, "Sine", "/Contents/reks/x/a.wav"),
                        track_row(6, 1, 99, "Other", "/Contents/reks/x/other.mp3"),
                    ]],
                ),
                (T_ARTISTS, vec![vec![artist_row(1, "reks")]]),
                (
                    T_HISTORY_PLAYLISTS,
                    vec![vec![
                        history_playlist(1, "HISTORY 001"),
                        history_playlist(2, "HISTORY 002"),
                    ]],
                ),
                (
                    T_HISTORY_ENTRIES,
                    vec![vec![
                        history_entry(5, 1, 1),
                        history_entry(6, 1, 2),
                        history_entry(5, 1, 3),
                        history_entry(5, 2, 1),
                    ]],
                ),
            ]),
        )
        .unwrap();
        (db, stick)
    }

    fn play_count(db: &Path) -> i64 {
        let lib = Library::open(db, &db.parent().unwrap().join("cache")).unwrap();
        lib.tracks(&TrackQuery::default()).unwrap().rows[0].play_count
    }

    fn run_on(db: &Path, path: &Path, cmd: Command, flags: &[String]) -> anyhow::Result<Out> {
        let mut all = vec!["--library".to_owned(), db.to_str().unwrap().to_owned()];
        all.extend_from_slice(flags);
        execute(&parse_args(&all, cmd)?, &source(path)?)
    }

    #[test]
    fn scan_then_import_a_stick_once() {
        let tmp = tempfile::tempdir().unwrap();
        let (db, stick) = setup(tmp.path());
        let pdb = fs::read(stick.join("PIONEER/rekordbox/export.pdb")).unwrap();

        let out = run_on(&db, &stick, Command::Scan, &[]).unwrap();
        let text = render(&out);
        assert!(text.contains("(serial unknown)"), "{text}");
        assert!(
            text.contains("3 entries: 0 exact, 2 by path, 0 by tags, 1 unmatched, 0 dividers"),
            "{text}"
        );
        assert!(text.contains("      counted\n"), "{text}");
        assert!(!text.contains("practice"), "{text}");
        assert!(
            text.contains("Sessions  2 new, 0 extended, 0 skipped; 4 entries, 1 unmatched"),
            "{text}"
        );
        assert!(out.report.is_none());

        // Without --yes nothing is written.
        let flags = vec!["--include".to_owned(), "HISTORY 001".to_owned()];
        let out = run_on(&db, &stick, Command::Import, &flags).unwrap();
        assert!(out.report.is_none());
        assert_eq!(out.excluded.values().filter(|&&ex| !ex).count(), 2);
        assert_eq!(play_count(&db), 0);

        let fp = out.plan.sessions[1].fingerprint[..10].to_owned();
        let mut yes = flags.clone();
        yes.extend(["--practice".to_owned(), fp, "--yes".to_owned()]);
        let out = run_on(&db, &stick, Command::Import, &yes).unwrap();
        let report = out.report.as_ref().unwrap();
        assert_eq!((report.created.len(), report.entries), (2, 4));
        assert!(render(&out).contains("Imported  2 new"), "{}", render(&out));
        // HISTORY 001 counts a.wav twice; HISTORY 002 is practice.
        assert_eq!(play_count(&db), 2);

        // Again: everything is skipped, choices on skipped sessions are refused.
        let out = run_on(&db, &stick, Command::Import, &args("--yes")).unwrap();
        assert_eq!(out.report.as_ref().unwrap().skipped, 2);
        assert!(render(&out).contains("already imported as session"));
        let err = run_on(&db, &stick, Command::Import, &flags).unwrap_err();
        assert!(err.to_string().contains("is not new"), "{err}");
        assert_eq!(play_count(&db), 2);

        assert_eq!(
            fs::read(stick.join("PIONEER/rekordbox/export.pdb")).unwrap(),
            pdb,
            "the stick changed"
        );
    }

    #[test]
    fn bad_selectors() {
        let tmp = tempfile::tempdir().unwrap();
        let (db, stick) = setup(tmp.path());
        for (sel, msg) in [
            ("HISTORY 009", "no session"),
            ("abc", "no session"),
            ("zzzzzzzzzz", "no session"),
        ] {
            let flags = vec!["--practice".to_owned(), sel.to_owned()];
            let err = run_on(&db, &stick, Command::Import, &flags).unwrap_err();
            assert!(err.to_string().contains(msg), "{sel}: {err}");
        }
        let both = ["--practice", "HISTORY 001", "--include", "HISTORY 001"].map(str::to_owned);
        let err = run_on(&db, &stick, Command::Import, &both).unwrap_err();
        assert!(err.to_string().contains("both"), "{err}");
        assert!(run_on(&tmp.path().join("nope.sqlite"), &stick, Command::Scan, &[]).is_err());
    }

    const OL_KEY: &str = "reks-cli-test-key";

    /// An `exportLibrary.db` on `stick` whose `HISTORY 001` plays a.wav twice.
    fn put_onelibrary(stick: &Path) {
        use reks_export_onelibrary::{cipher, schema, CipherProfile, RbVersion};
        let db = stick.join("PIONEER/rekordbox/exportLibrary.db");
        let conn = cipher::open_read_write(&db, OL_KEY, CipherProfile::Sqlcipher4).unwrap();
        schema::create_empty(&conn, RbVersion::LATEST).unwrap();
        conn.execute_batch(
            "INSERT INTO content (content_id, title, path, length)
                 VALUES (1, 'Sine', '/Contents/reks/x/a.wav', 30);
             INSERT INTO history VALUES (1, 0, 'HISTORY 001', 0, 0);
             INSERT INTO history_content VALUES (1, 1, 1), (1, 1, 2);",
        )
        .unwrap();
    }

    fn store_key(db: &Path) {
        let lib = Library::open(db, &db.parent().unwrap().join("cache")).unwrap();
        lib.update_settings(reks_core::SettingsPatch {
            onelibrary_key: Some(reks_core::OneLibraryKey::new(OL_KEY)),
            ..Default::default()
        })
        .unwrap();
    }

    /// Both stick libraries' history, the OneLibrary one only with the key.
    #[test]
    fn reads_exportlibrary_history_with_the_key() {
        let tmp = tempfile::tempdir().unwrap();
        let (db, stick) = setup(tmp.path());
        put_onelibrary(&stick);

        let out = run_on(&db, &stick, Command::Scan, &[]).unwrap();
        let text = render(&out);
        assert!(
            // No key stored (REKS_ONELIBRARY_KEY, if set, is not this one).
            text.contains("OneLibrary  exportLibrary.db not read: "),
            "{text}"
        );
        assert_eq!(out.plan.sessions.len(), 2);

        store_key(&db);
        let out = run_on(&db, &stick, Command::Scan, &[]).unwrap();
        let text = render(&out);
        assert!(text.contains("exportLibrary.db read: 1 sessions"), "{text}");
        assert!(
            text.contains("  HISTORY 001 (OneLibrary)  undated  "),
            "{text}"
        );
        assert!(text.contains("Sessions  3 new, 0 extended"), "{text}");

        let flags = ["--practice", "HISTORY 001 (OneLibrary)", "--yes"].map(str::to_owned);
        let out = run_on(&db, &stick, Command::Import, &flags).unwrap();
        let report = out.report.as_ref().unwrap();
        assert_eq!((report.created.len(), report.entries), (3, 6));
        // export.pdb's HISTORY 001 (twice) and 002 count; the OneLibrary
        // session is practice.
        assert_eq!(play_count(&db), 3);

        let out = run_on(&db, &stick, Command::Import, &args("--yes")).unwrap();
        assert_eq!(out.report.as_ref().unwrap().skipped, 3);
        assert_eq!(play_count(&db), 3);
    }

    /// A Phase 2 backup of the stick reads like the stick: importing from
    /// it first makes the stick's sessions skips.
    #[test]
    fn import_from_a_backup() {
        let tmp = tempfile::tempdir().unwrap();
        let (db, stick) = setup(tmp.path());
        let h = read_history(&stick, None);
        let plan = plan_backups(&stick, &h, HistoryPolicy::BackUp, false).unwrap();
        let backup = run_backups(&plan, &tmp.path().join("data"))
            .unwrap()
            .unwrap();

        let out = run_on(&db, &backup.dir, Command::Import, &args("--yes")).unwrap();
        assert!(matches!(out.source, HistorySource::Backup(_)));
        assert!(render(&out).starts_with("Backup "), "{}", render(&out));
        assert_eq!(out.report.as_ref().unwrap().created.len(), 2);
        // Undated sessions count by default.
        assert!(play_count(&db) > 0);

        let out = run_on(&db, &stick, Command::Scan, &[]).unwrap();
        assert!(out
            .plan
            .sessions
            .iter()
            .all(|s| matches!(s.action, SessionAction::Skip(_))));
    }
}
