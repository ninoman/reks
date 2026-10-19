//! Writes the WAV fixtures.
//!
//! Usage: `cargo run -p reks-fixtures-gen [-- [hires|float] [<output.wav>]]`
//! Default: the sine+click WAV to `fixtures/audio/sine-click-128.wav`.
//! `hires` (24-bit 96 kHz) and `float` (32-bit float) are the converter's
//! stereo sources; they are not checked in, so they need an output path.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1).peekable();
    let kind = match args.peek().and_then(|a| a.to_str()) {
        Some(k @ ("hires" | "float")) => {
            let k = k.to_owned();
            args.next();
            Some(k)
        }
        _ => None,
    };
    let out = match (args.next(), &kind) {
        (Some(path), _) => PathBuf::from(path),
        (None, None) => {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../audio/sine-click-128.wav")
        }
        (None, Some(k)) => return Err(format!("{k}: give an output path").into()),
    };
    let file = BufWriter::new(File::create(&out)?);
    match kind.as_deref() {
        Some("hires") => reks_fixtures_gen::write_wav_hires(file)?,
        Some(_) => reks_fixtures_gen::write_wav_float(file)?,
        None => reks_fixtures_gen::write_wav(file)?,
    }
    writeln!(io::stderr(), "wrote {}", out.display())?;
    Ok(())
}
