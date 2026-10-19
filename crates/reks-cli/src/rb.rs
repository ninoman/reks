//! `reks rb ...`: legacy rekordbox USB export, headless (Phase 2 task 11).
//!
//! ```text
//! reks rb plan <stick> --library <library.sqlite> --playlist <id | Folder/Name>...
//!              [--target nxs|nxs2|3000|xdj-xz|generic] [--missing-analysis skip|generate|without]
//!              [--keep-stale] [--artwork] [--skip-unsupported] [--discard-history]
//!              [--anlz-root <dir>]... [--json]
//!     print the dry run; exits non-zero when the plan is blocked
//! reks rb history <stick> [--json]
//!     list the play history sessions players left on the stick
//! ```
//!
//! `<stick>` is the stick's mount point. Any other directory is used as the
//! stick root (a fake stick for testing) and has no volume serial.
//! `--discard-history` is the explicit confirmation `Discard` needs.
//! `--anlz-root` replaces rekordbox's default analysis folders.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use reks_core::{Library, PlaylistId, PlaylistNode};
use reks_export_rb::device::{self, DeviceInfo, FsKind};
use reks_export_rb::history::{self, HistoryPolicy, HistorySession, StickHistory};
use reks_export_rb::plan::{
    self, human_bytes, AnlzOp, CacheLocator, ExportOptions, ExportPlan, MissingAnalysis, Selection,
};
use reks_export_rb::rbcache::{self, RbCache};
use reks_export_rb::TargetProfile;
use serde::Serialize;

const USAGE: &str = "usage: reks rb plan <stick> --library <library.sqlite> \
--playlist <id|Folder/Name>... [--target nxs|nxs2|3000|xdj-xz|generic] \
[--missing-analysis skip|generate|without] [--keep-stale] [--artwork] [--skip-unsupported] \
[--discard-history] [--anlz-root <dir>]... [--json]\n       reks rb history <stick> [--json]";

pub fn run(args: &[String]) -> anyhow::Result<()> {
    match args {
        [cmd, stick_path, flags @ ..] if cmd == "plan" => {
            let args = parse_plan_args(flags)?;
            let (dev, serial) = stick(Path::new(stick_path))?;
            let plan = build_plan(&args, &dev, serial)?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&plan)?);
            } else {
                print!("{}", render_plan(&plan));
            }
            if plan.is_blocked() {
                bail!("the plan is blocked by {} error(s)", plan.errors.len());
            }
            Ok(())
        }
        [cmd, stick_path, flags @ ..] if cmd == "history" => {
            let json = match flags {
                [] => false,
                [f] if f == "--json" => true,
                _ => bail!("{USAGE}"),
            };
            let (dev, serial) = stick(Path::new(stick_path))?;
            let out = read_stick_history(&dev.mount_point, serial);
            if json {
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                print!("{}", render_history(&dev.mount_point, &out));
            }
            match out.error {
                Some(e) => bail!(e),
                None => Ok(()),
            }
        }
        _ => bail!("{USAGE}"),
    }
}

/// The device for `path`, and its volume serial. A directory that is not a
/// mount point becomes the root of a stand-in device (with the containing
/// volume's file system and free space) and gets no serial.
fn stick(path: &Path) -> anyhow::Result<(DeviceInfo, Option<String>)> {
    let root = std::fs::canonicalize(path).with_context(|| format!("{}", path.display()))?;
    if !root.is_dir() {
        bail!("{} is not a directory", root.display());
    }
    let mut dev = device::device_for_path(&root)?;
    let mount = std::fs::canonicalize(&dev.mount_point).unwrap_or_else(|_| dev.mount_point.clone());
    if mount == root {
        let serial = device::volume_serial(&dev);
        Ok((dev, serial))
    } else {
        eprintln!(
            "note: {} is not a mount point; using it as the stick root (no volume serial)",
            root.display()
        );
        dev.mount_point = root;
        Ok((dev, None))
    }
}

#[derive(Debug, Clone, PartialEq)]
struct PlanArgs {
    library: PathBuf,
    playlists: Vec<String>,
    opts: ExportOptions,
    /// `None`: rekordbox's default analysis folders.
    anlz_roots: Option<Vec<PathBuf>>,
    json: bool,
}

fn parse_plan_args(flags: &[String]) -> anyhow::Result<PlanArgs> {
    let mut library = None;
    let mut playlists = Vec::new();
    let mut opts = ExportOptions::default();
    let mut anlz_roots: Option<Vec<PathBuf>> = None;
    let mut json = false;
    let mut flags = flags.iter();
    while let Some(flag) = flags.next() {
        let mut value = || {
            flags
                .next()
                .with_context(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--library" => library = Some(PathBuf::from(value()?)),
            "--playlist" => playlists.push(value()?.clone()),
            "--target" => {
                opts.target = match value()?.as_str() {
                    "nxs" => TargetProfile::Cdj2000Nxs,
                    "nxs2" => TargetProfile::Cdj2000Nxs2,
                    "3000" => TargetProfile::Cdj3000,
                    "xdj-xz" => TargetProfile::XdjXz,
                    "generic" => TargetProfile::Generic,
                    other => bail!("unknown target {other}"),
                }
            }
            "--missing-analysis" => {
                opts.missing_analysis = match value()?.as_str() {
                    "skip" => MissingAnalysis::Skip,
                    "generate" => MissingAnalysis::Generate,
                    "without" => MissingAnalysis::ExportWithout,
                    other => bail!("unknown --missing-analysis {other}"),
                }
            }
            "--anlz-root" => anlz_roots
                .get_or_insert_with(Vec::new)
                .push(PathBuf::from(value()?)),
            "--keep-stale" => opts.keep_stale = true,
            "--artwork" => opts.artwork = true,
            "--skip-unsupported" => opts.skip_unsupported = true,
            "--discard-history" => {
                opts.history = HistoryPolicy::Discard;
                opts.discard_history_confirmed = true;
            }
            "--json" => json = true,
            other => bail!("unknown flag {other}\n{USAGE}"),
        }
    }
    let Some(library) = library else {
        bail!("--library is required\n{USAGE}");
    };
    if playlists.is_empty() {
        bail!("at least one --playlist is required\n{USAGE}");
    }
    Ok(PlanArgs {
        library,
        playlists,
        opts,
        anlz_roots,
        json,
    })
}

fn build_plan(
    args: &PlanArgs,
    dev: &DeviceInfo,
    serial: Option<String>,
) -> anyhow::Result<ExportPlan> {
    if !args.library.is_file() {
        bail!("no library at {}", args.library.display());
    }
    let cache_dir = args
        .library
        .parent()
        .unwrap_or(Path::new("."))
        .join("cache");
    let lib = Library::open(&args.library, &cache_dir)?;
    let sel = Selection {
        playlists: resolve_playlists(&lib.playlists()?.nodes, &args.playlists)?,
    };
    let roots = args
        .anlz_roots
        .clone()
        .unwrap_or_else(rbcache::default_roots);
    let cache = RbCache::scan(&roots)?;
    eprintln!(
        "rekordbox analysis: {} files under {:?}",
        cache.files_indexed(),
        cache.roots()
    );
    let locator = CacheLocator::for_selection(&lib, &sel, cache)?;
    let sources = plan::load(&lib, &sel)?;
    Ok(plan::plan_sources(
        sources, dev, serial, &args.opts, &locator,
    )?)
}

/// `Folder/Name` path of every node, from the depth-first node list.
fn node_paths(nodes: &[PlaylistNode]) -> HashMap<PlaylistId, String> {
    let mut paths: HashMap<PlaylistId, String> = HashMap::new();
    for n in nodes {
        let path = match n.parent_id.and_then(|p| paths.get(&p)) {
            Some(parent) => format!("{parent}/{}", n.name),
            None => n.name.clone(),
        };
        paths.insert(n.id, path);
    }
    paths
}

/// Playlist ids for `--playlist` values: an id, or a `Folder/Name` path.
fn resolve_playlists(nodes: &[PlaylistNode], wanted: &[String]) -> anyhow::Result<Vec<PlaylistId>> {
    let paths = node_paths(nodes);
    wanted
        .iter()
        .map(|w| {
            if let Ok(id) = w.parse::<PlaylistId>() {
                if paths.contains_key(&id) {
                    return Ok(id);
                }
            }
            let hits: Vec<PlaylistId> = nodes
                .iter()
                .filter(|n| paths.get(&n.id) == Some(w))
                .map(|n| n.id)
                .collect();
            match hits.as_slice() {
                [id] => Ok(*id),
                [] => bail!("no playlist or folder {w:?} in the library"),
                many => bail!("{w:?} names several playlists ({many:?}); pass an id"),
            }
        })
        .collect()
}

fn fs_text(dev: &DeviceInfo) -> String {
    let kind = match &dev.fs {
        FsKind::Fat32 => "FAT32",
        FsKind::ExFat => "exFAT",
        FsKind::Hfs => "HFS+",
        FsKind::Other(name) => name.as_str(),
    };
    format!("{kind}, {}", dev.fs_name)
}

fn render_plan(plan: &ExportPlan) -> String {
    let mut out = String::new();
    let o = &mut out;
    let dev = &plan.device;
    let _ = writeln!(o, "Dry run: nothing has been written.");
    let _ = writeln!(
        o,
        "Stick     {} ({}, label {:?}, {} free, serial {})",
        dev.mount_point.display(),
        fs_text(dev),
        dev.label,
        human_bytes(dev.free_bytes),
        plan.history.volume_serial.as_deref().unwrap_or("unknown")
    );
    let _ = writeln!(o, "Target    {:?}", plan.options.target);
    let on_stick = plan.tracks.iter().filter(|t| t.on_stick).count();
    let _ = writeln!(
        o,
        "Tracks    {} ({} to copy, {on_stick} already on the stick)",
        plan.tracks.len(),
        plan.copies.len()
    );
    let (mut cached, mut generated, mut without) = (0, 0, 0);
    for a in &plan.anlz {
        match a {
            AnlzOp::CopyFromCache { .. } => cached += 1,
            AnlzOp::Generate { .. } => generated += 1,
            AnlzOp::Missing { .. } => without += 1,
        }
    }
    let _ = writeln!(
        o,
        "Analysis  {cached} from rekordbox, {generated} generated, {without} without"
    );

    let _ = writeln!(o, "Playlists");
    let mut depth: HashMap<PlaylistId, usize> = HashMap::new();
    for p in &plan.playlists {
        let d = p
            .parent_id
            .and_then(|id| depth.get(&id))
            .map_or(0, |d| d + 1);
        depth.insert(p.id, d);
        let indent = "  ".repeat(d + 1);
        if p.is_folder {
            let _ = writeln!(o, "{indent}{}/", p.name);
        } else {
            let _ = writeln!(o, "{indent}{} ({} tracks)", p.name, p.track_ids.len());
        }
    }

    let total = |ops: &[plan::FileOp]| human_bytes(ops.iter().map(|f| f.bytes).sum());
    let _ = writeln!(
        o,
        "Copy      {} files, {}",
        plan.copies.len(),
        total(&plan.copies)
    );
    for c in &plan.copies {
        let src = c.src.as_deref().map(|p| p.display().to_string());
        let _ = writeln!(
            o,
            "  {}  <- {}  ({})",
            c.dst_rel,
            src.unwrap_or_default(),
            human_bytes(c.bytes)
        );
    }
    let _ = writeln!(
        o,
        "Delete    {} files, {}",
        plan.deletes.len(),
        total(&plan.deletes)
    );
    for d in &plan.deletes {
        let _ = writeln!(o, "  {}  ({})", d.dst_rel, human_bytes(d.bytes));
    }
    let h = &plan.history;
    let _ = writeln!(
        o,
        "History   {} sessions, {} entries{}{}",
        h.pdb_sessions,
        h.pdb_entries,
        if h.pdb_unreadable {
            "; export.pdb unreadable"
        } else {
            ""
        },
        if h.onelibrary_db {
            "; exportLibrary.db present"
        } else {
            ""
        }
    );
    if !plan.backups.is_empty() {
        let _ = writeln!(
            o,
            "Back up   {} files, {}, to the app data dir before writing",
            plan.backups.len(),
            total(&plan.backups)
        );
        for b in &plan.backups {
            let _ = writeln!(o, "  {}  ({})", b.dst_rel, human_bytes(b.bytes));
        }
    }
    if !plan.warnings.is_empty() {
        let _ = writeln!(o, "Warnings ({})", plan.warnings.len());
        for w in &plan.warnings {
            let _ = writeln!(o, "  - {w}");
        }
    }
    if !plan.errors.is_empty() {
        let _ = writeln!(o, "Errors ({})", plan.errors.len());
        for e in &plan.errors {
            let _ = writeln!(o, "  - {e}");
        }
    }
    let free_after = if plan.bytes_free_after < 0 {
        format!("-{}", human_bytes(plan.bytes_free_after.unsigned_abs()))
    } else {
        human_bytes(plan.bytes_free_after.unsigned_abs())
    };
    let _ = writeln!(
        o,
        "Writes    {}; free after {free_after}",
        human_bytes(plan.bytes_to_write)
    );
    let _ = writeln!(
        o,
        "Result    {}",
        if plan.is_blocked() {
            "blocked"
        } else {
            "ready"
        }
    );
    out
}

/// `reks rb history` output (also its `--json`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryOut {
    history: StickHistory,
    /// `None` when `export.pdb` could not be read.
    sessions: Option<Vec<HistorySession>>,
    error: Option<String>,
}

fn read_stick_history(root: &Path, serial: Option<String>) -> HistoryOut {
    let history = history::read_history(root, serial);
    match history::read_sessions(root) {
        Ok(sessions) => HistoryOut {
            history,
            sessions: Some(sessions),
            error: None,
        },
        Err(e) => HistoryOut {
            history,
            sessions: None,
            error: Some(e.to_string()),
        },
    }
}

fn render_history(root: &Path, out: &HistoryOut) -> String {
    let mut s = String::new();
    let h = &out.history;
    let _ = writeln!(
        s,
        "Stick {} (serial {})",
        root.display(),
        h.volume_serial.as_deref().unwrap_or("unknown")
    );
    match (&out.sessions, &out.error) {
        (_, Some(e)) => {
            let _ = writeln!(s, "export.pdb: {e}");
        }
        (Some(sessions), None) => {
            let _ = writeln!(
                s,
                "export.pdb: {} history sessions, {} entries",
                h.pdb_sessions, h.pdb_entries
            );
            for x in sessions {
                let _ = writeln!(
                    s,
                    "  {:>4}  {}  {} {}",
                    x.id,
                    x.name.as_deref().unwrap_or("(no playlist row)"),
                    x.entries,
                    if x.entries == 1 { "entry" } else { "entries" }
                );
            }
        }
        (None, None) => {}
    }
    if h.onelibrary_db {
        let _ = writeln!(
            s,
            "exportLibrary.db: present; players may have logged history there (not read yet)"
        );
    }
    let _ = writeln!(
        s,
        "{}",
        if h.may_hold_history() {
            "A re-export erases this history; `reks rb plan` plans a backup first."
        } else {
            "No history on this stick."
        }
    );
    s
}

#[cfg(test)]
mod tests {
    use std::fs;

    use reks_core::{CancelToken, PlaylistOp, PlaylistOutcome};
    use reks_export_rb::testutil;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/audio");

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    fn fake_dev(root: &Path) -> DeviceInfo {
        DeviceInfo {
            mount_point: root.to_path_buf(),
            fs: FsKind::Fat32,
            fs_name: "msdos".into(),
            removable: true,
            total_bytes: 32_000_000_000,
            free_bytes: 16_000_000_000,
            label: "REKS".into(),
            os_name: "disk9s1".into(),
        }
    }

    #[test]
    fn plan_flags() {
        let a = parse_plan_args(&args(
            "--library lib.sqlite --playlist Gigs/Friday --playlist 7 --target nxs2 \
             --missing-analysis generate --keep-stale --discard-history --anlz-root /a --json",
        ))
        .unwrap();
        assert_eq!(a.library, PathBuf::from("lib.sqlite"));
        assert_eq!(a.playlists, ["Gigs/Friday", "7"]);
        assert_eq!(a.opts.target, TargetProfile::Cdj2000Nxs2);
        assert_eq!(a.opts.missing_analysis, MissingAnalysis::Generate);
        assert!(a.opts.keep_stale && a.json);
        assert_eq!(a.opts.history, HistoryPolicy::Discard);
        assert!(a.opts.discard_history_confirmed);
        assert_eq!(a.anlz_roots, Some(vec![PathBuf::from("/a")]));

        let d = parse_plan_args(&args("--library l --playlist x")).unwrap();
        assert_eq!(d.opts, ExportOptions::default());
        assert_eq!(d.anlz_roots, None);

        for bad in [
            "--playlist x",
            "--library l",
            "--library l --playlist x --target cdj9",
            "--library l --playlist x --bogus",
            "--library",
        ] {
            assert!(parse_plan_args(&args(bad)).is_err(), "{bad}");
        }
    }

    fn node(
        id: PlaylistId,
        parent_id: Option<PlaylistId>,
        name: &str,
        is_folder: bool,
    ) -> PlaylistNode {
        PlaylistNode {
            id,
            parent_id,
            name: name.into(),
            is_folder,
            depth: u32::from(parent_id.is_some()),
            position: 0,
            entries: 0,
        }
    }

    #[test]
    fn playlists_by_id_or_path() {
        let nodes = [
            node(1, None, "Gigs", true),
            node(2, Some(1), "Friday", false),
            node(3, None, "Friday", false),
            node(4, None, "Dup", false),
            node(5, None, "Dup", false),
        ];
        let ids = resolve_playlists(&nodes, &args("Gigs/Friday Friday Gigs 5")).unwrap();
        assert_eq!(ids, [2, 3, 1, 5]);
        assert!(resolve_playlists(&nodes, &args("Dup")).is_err());
        assert!(resolve_playlists(&nodes, &args("Nope")).is_err());
        assert!(resolve_playlists(&nodes, &args("99")).is_err());
    }

    /// Library `lib.sqlite` with `a.wav` in `Gigs/Friday`, a rekordbox
    /// analysis for it under `USBANLZ`, and a played stick.
    #[test]
    fn plan_and_history_on_a_played_stick() {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("music");
        fs::create_dir_all(&music).unwrap();
        fs::copy(
            Path::new(FIXTURES).join("sine-click-128.wav"),
            music.join("a.wav"),
        )
        .unwrap();
        let db = tmp.path().join("lib.sqlite");
        let lib = Library::open(&db, &tmp.path().join("cache")).unwrap();
        reks_core::scan(
            std::slice::from_ref(&music),
            &lib,
            &CancelToken::new(),
            |_| {},
        )
        .unwrap();
        let track = lib.tracks(&reks_core::TrackQuery::default()).unwrap().rows[0].id;
        let created = |op| match lib.playlist_mutate(op).unwrap() {
            PlaylistOutcome::Done { created_id, .. } => created_id.unwrap(),
            other => panic!("{other:?}"),
        };
        let gigs = created(PlaylistOp::CreateFolder {
            parent_id: None,
            name: "Gigs".into(),
            position: None,
        });
        let friday = created(PlaylistOp::CreatePlaylist {
            parent_id: Some(gigs),
            name: "Friday".into(),
            position: None,
        });
        lib.playlist_mutate(PlaylistOp::AddTracks {
            playlist_id: friday,
            track_ids: vec![track],
            before: None,
            duplicates: reks_core::DuplicateMode::Allow,
        })
        .unwrap();
        drop(lib);

        let anlz = tmp.path().join("USBANLZ");
        put(
            &anlz,
            "000/a/ANLZ0000.DAT",
            &testutil::anlz_dat("?/a.wav", 0, 0, 0),
        );
        let stick_root = tmp.path().join("stick");
        put(
            &stick_root,
            "PIONEER/rekordbox/export.pdb",
            &testutil::played_pdb(),
        );

        let a = parse_plan_args(&[
            "--library".into(),
            db.display().to_string(),
            "--playlist".into(),
            "Gigs/Friday".into(),
            "--anlz-root".into(),
            anlz.display().to_string(),
        ])
        .unwrap();
        let plan = build_plan(&a, &fake_dev(&stick_root), None).unwrap();
        assert!(!plan.is_blocked(), "{:?}", plan.errors);
        let text = render_plan(&plan);
        for want in [
            "Dry run: nothing has been written.",
            "Tracks    1 (1 to copy, 0 already on the stick)",
            "Analysis  1 from rekordbox, 0 generated, 0 without",
            "  Gigs/\n    Friday (1 tracks)\n",
            "/Contents/",
            "History   2 sessions, 3 entries\n",
            "Back up   1 files,",
            "  /PIONEER/rekordbox/export.pdb  (",
            "  - export.pdb holds play history (2 sessions, 3 entries)",
            "  - the OS gave no volume serial",
            "Result    ready",
        ] {
            assert!(text.contains(want), "{want:?} not in\n{text}");
        }
        let json: serde_json::Value = serde_json::to_value(&plan).unwrap();
        assert_eq!(json["history"]["pdbSessions"], 2);
        assert_eq!(json["backups"][0]["reason"], "historyBackup");

        let out = read_stick_history(&stick_root, Some("4993-1CFB".into()));
        let text = render_history(&stick_root, &out);
        assert!(text.contains("serial 4993-1CFB"), "{text}");
        assert!(
            text.contains("export.pdb: 2 history sessions, 3 entries"),
            "{text}"
        );
        assert!(text.contains("     1  HISTORY 001  2 entries\n"), "{text}");
        assert!(text.contains("     2  HISTORY 002  1 entry\n"), "{text}");
        assert!(text.contains("plans a backup first"), "{text}");

        // An unknown playlist is an error, not an empty plan.
        let missing = parse_plan_args(&args(&format!(
            "--library {} --playlist Nope",
            db.display()
        )))
        .unwrap();
        assert!(build_plan(&missing, &fake_dev(&stick_root), None).is_err());
    }

    #[test]
    fn history_on_clean_and_damaged_sticks() {
        let clean = tempfile::tempdir().unwrap();
        let out = read_stick_history(clean.path(), None);
        assert_eq!(out.sessions.as_deref(), Some(&[][..]));
        assert!(render_history(clean.path(), &out).contains("No history on this stick."));

        let bad = tempfile::tempdir().unwrap();
        put(bad.path(), "PIONEER/rekordbox/export.pdb", b"garbage");
        put(bad.path(), "PIONEER/rekordbox/exportLibrary.db", b"sqlite");
        let out = read_stick_history(bad.path(), None);
        assert!(out.error.is_some() && out.sessions.is_none());
        let text = render_history(bad.path(), &out);
        assert!(
            text.contains("export.pdb: cannot read the history"),
            "{text}"
        );
        assert!(text.contains("exportLibrary.db: present"), "{text}");
        assert!(text.contains("erases this history"), "{text}");
        let json = serde_json::to_value(&out).unwrap();
        assert_eq!(json["history"]["pdbUnreadable"], true);
    }
}
