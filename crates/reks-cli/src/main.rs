//! `reks` command-line interface. Scaffold only: prints versions.

fn main() -> anyhow::Result<()> {
    println!(
        "reks-cli {} (reks-core {})",
        env!("CARGO_PKG_VERSION"),
        reks_core::VERSION
    );
    Ok(())
}
