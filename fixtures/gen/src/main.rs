//! Writes the sine+click WAV fixture.
//!
//! Usage: `cargo run -p reks-fixtures-gen [-- <output.wav>]`
//! Default output: `fixtures/audio/sine-click-128.wav`.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../audio/sine-click-128.wav"),
        PathBuf::from,
    );
    let file = BufWriter::new(File::create(&out)?);
    reks_fixtures_gen::write_wav(file)?;
    writeln!(io::stderr(), "wrote {}", out.display())?;
    Ok(())
}
