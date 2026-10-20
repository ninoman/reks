//! `reks onelibrary ...`: inspect and compare `exportLibrary.db` files
//! (Phase 4 tasks 7 and 10).
//!
//! ```text
//! reks onelibrary dump <exportLibrary.db> [<out.json>]
//!     every table as JSON (the fixtures' dump.json format)
//! reks onelibrary diff <ours> <theirs> [--strict]
//!     compare two databases (or dump.json files); exits non-zero when they
//!     differ. Ids, master ids and dates are ignored unless --strict.
//! reks onelibrary validate <stick>
//!     check the stick's exportLibrary.db against its export.pdb and ANLZ
//!     files; exits non-zero on findings
//! ```
//!
//! The key comes from `REKS_ONELIBRARY_KEY` (Phase 4 key policy) and is
//! never printed. A database is read from a temporary copy with its `-wal`
//! and `-shm`, so a stick is never written.

use std::path::Path;

use anyhow::{bail, Context};
use reks_export_onelibrary::diff::{diff, DiffOptions};
use reks_export_onelibrary::dump::Dump;
use reks_export_onelibrary::{validate, OneLibraryReader};
use zeroize::Zeroizing;

const USAGE: &str = "usage: reks onelibrary dump <exportLibrary.db> [<out.json>] \
    | diff <ours> <theirs> [--strict] | validate <stick>";

pub fn run(args: &[String]) -> anyhow::Result<()> {
    match args {
        [cmd, db] if cmd == "dump" => {
            print!("{}", read(Path::new(db))?.to_json());
            Ok(())
        }
        [cmd, db, out] if cmd == "dump" => std::fs::write(out, read(Path::new(db))?.to_json())
            .with_context(|| format!("writing {out}")),
        [cmd, ours, theirs, flags @ ..] if cmd == "diff" => {
            let strict = match flags {
                [] => false,
                [f] if f == "--strict" => true,
                _ => bail!("{USAGE}"),
            };
            let diffs = diff(
                &read(Path::new(ours))?,
                &read(Path::new(theirs))?,
                DiffOptions { strict },
            );
            for d in &diffs {
                println!("{d}");
            }
            if !diffs.is_empty() {
                bail!("{} differences", diffs.len());
            }
            println!("no differences");
            Ok(())
        }
        [cmd, stick] if cmd == "validate" => {
            let findings = validate::validate(Path::new(stick), &key()?)?;
            for f in &findings {
                println!("{f}");
            }
            if !findings.is_empty() {
                bail!("{} findings", findings.len());
            }
            println!("exportLibrary.db matches export.pdb");
            Ok(())
        }
        _ => bail!("{USAGE}"),
    }
}

fn key() -> anyhow::Result<Zeroizing<String>> {
    match std::env::var(reks_export_rb::onelibrary::KEY_ENV) {
        Ok(k) if !k.trim().is_empty() => Ok(Zeroizing::new(k.trim().to_owned())),
        _ => bail!(
            "set {} to the OneLibrary key",
            reks_export_rb::onelibrary::KEY_ENV
        ),
    }
}

/// A `.json` dump as is; anything else as an `exportLibrary.db`.
fn read(path: &Path) -> anyhow::Result<Dump> {
    if path.extension().is_some_and(|e| e == "json") {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        return Dump::from_json(&text).with_context(|| format!("parsing {}", path.display()));
    }
    let reader = OneLibraryReader::open(path, &key()?)?;
    Ok(reader.dump()?)
}
