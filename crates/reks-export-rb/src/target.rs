//! Player families an export is checked against.

use serde::{Deserialize, Serialize};

/// Which player family drives the export's format checks (and, later,
/// filesystem warnings). Pick the oldest player the stick must work on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TargetProfile {
    /// CDJ-2000NXS and other players without FLAC/ALAC support.
    Cdj2000Nxs,
    /// CDJ-2000NXS2: FLAC/ALAC up to 96 kHz / 24-bit (R§4).
    Cdj2000Nxs2,
    /// CDJ-3000.
    Cdj3000,
    /// XDJ-XZ: FLAC/ALAC depend on firmware (R§4).
    XdjXz,
    /// Unknown player: the conservative common subset.
    Generic,
}
