//! Golden tests against rekordbox 7.2.19's export of the fixture tracks
//! (`fixtures/onelibrary/ref-7.2.19`, Phase 4 tasks 3–5): schema, seed rows
//! and every row reks builds from the same `export.pdb`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use reks_export_onelibrary::diff::{diff, DiffOptions, Difference};
use reks_export_onelibrary::dump::Dump;
use reks_export_onelibrary::{build, schema, writer, OneLibraryReader, RbVersion};
use rusqlite::Connection;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/onelibrary/ref-7.2.19")
}

fn reference() -> Dump {
    Dump::from_json(&std::fs::read_to_string(root().join("dump.json")).unwrap()).unwrap()
}

/// Differences other than the ones the writer makes on purpose: the
/// reference's `myTag` holds its user's renamed columns, and
/// `myTagMasterDBID` that user's rekordbox id; reks writes the four default
/// columns and 0.
fn unexpected(diffs: &[Difference]) -> Vec<String> {
    diffs
        .iter()
        .filter(|d| !matches!(d, Difference::Row { table, .. } if table == "myTag"))
        .filter(
            |d| !matches!(d, Difference::Property { column, .. } if column == "myTagMasterDBID"),
        )
        .map(ToString::to_string)
        .collect()
}

#[test]
fn empty_database_has_the_reference_schema_and_seeds() {
    let conn = Connection::open_in_memory().unwrap();
    schema::create_empty(&conn, RbVersion::V7_2_19).unwrap();
    let ours = Dump::read(&conn).unwrap();
    let theirs = reference();
    assert_eq!(ours.schema, theirs.schema);
    for t in ["menuItem", "category", "sort"] {
        assert_eq!(ours.tables[t], theirs.tables[t], "{t}");
    }
    let tags: Vec<_> = ours.tables["myTag"]
        .rows
        .iter()
        .map(|r| r[2].clone())
        .collect();
    assert_eq!(
        tags,
        ["Genre", "Components", "Situation", "Untitled Column"]
    );
}

#[test]
fn rows_from_the_reference_pdb_match_rekordbox_strictly() {
    let pdb = reks_export_rb::pdbread::read_stick(&root()).unwrap();
    // The reference export's date, so dates compare too.
    let lib = build::from_pdb(&pdb, "2026-10-20");
    let mut conn = Connection::open_in_memory().unwrap();
    writer::build_into(&mut conn, RbVersion::V7_2_19, &lib).unwrap();
    let ours = Dump::read(&conn).unwrap();
    let diffs = diff(&ours, &reference(), DiffOptions { strict: true });
    let left = unexpected(&diffs);
    assert!(left.is_empty(), "{left:#?}");
}

/// Opens the real, encrypted reference with `REKS_ONELIBRARY_KEY` and checks
/// `dump.json` is what it holds: the cipher profile (task 2) and the fixture.
#[test]
#[ignore = "needs REKS_ONELIBRARY_KEY"]
fn reference_opens_with_the_key_and_matches_its_dump() {
    let Ok(key) = std::env::var("REKS_ONELIBRARY_KEY") else {
        return;
    };
    let db = root().join("PIONEER/rekordbox/exportLibrary.db");
    let reader = OneLibraryReader::open(&db, &key).unwrap();
    assert_eq!(reader.integrity_check().unwrap(), "ok");
    assert_eq!(reader.dump().unwrap(), reference());
    // The history reader takes rekordbox's file: no sessions on a fresh stick.
    assert!(reader.play_history().unwrap().is_empty());
}
