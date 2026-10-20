//! Shared test inputs: the rekordbox 7.2.19 reference export (task 1).

use std::path::{Path, PathBuf};

use crate::dump::Dump;

/// Not the OneLibrary key: tests that need the real one read
/// `REKS_ONELIBRARY_KEY` and are `#[ignore]`d.
pub const TEST_KEY: &str = "reks-test-key-not-the-real-one";

pub fn reference_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/onelibrary/ref-7.2.19")
}

/// The reference `exportLibrary.db`, dumped (`dump.json`).
pub fn reference_dump() -> Dump {
    let text = std::fs::read_to_string(reference_root().join("dump.json")).unwrap();
    Dump::from_json(&text).unwrap()
}

/// A temp stick with the reference's `export.pdb` and ANLZ files (no
/// OneLibrary files, as after baken's write).
pub fn reference_stick() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_tree(
        &reference_root().join("PIONEER/USBANLZ"),
        &dir.path().join("PIONEER/USBANLZ"),
    );
    let rb = dir.path().join("PIONEER/rekordbox");
    std::fs::create_dir_all(&rb).unwrap();
    std::fs::copy(
        reference_root().join("PIONEER/rekordbox/export.pdb"),
        rb.join("export.pdb"),
    )
    .unwrap();
    dir
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dst = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &dst);
        } else {
            std::fs::copy(e.path(), dst).unwrap();
        }
    }
}
