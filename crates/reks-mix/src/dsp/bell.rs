//! A peaking bell: SVF with `y = x + m1·band` (Simper's bell form), exact
//! gain at the centre, shared by the 4-band mids and the 3-band MID.

use super::svf::{SvfCoeffs, SvfState};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Bell {
    fc: f64,
    q_boost: f64,
    q_cut: f64,
    coeffs: SvfCoeffs,
    m1: f64,
    state: [SvfState; 2],
}

impl Bell {
    /// Flat. `q_boost` / `q_cut`: the width at half the gain in dB.
    pub(crate) fn new(fc: f64, q_boost: f64, q_cut: f64, sample_rate: f64) -> Self {
        let mut bell = Self {
            fc,
            q_boost,
            q_cut,
            coeffs: SvfCoeffs::new(fc, 1.0, sample_rate),
            m1: 0.0,
            state: [SvfState::default(); 2],
        };
        bell.set_db(0.0, sample_rate);
        bell
    }

    /// Exact `db` at the centre: k = 1/(Q·A), m1 = k·(A² − 1), A = 10^(dB/40).
    pub(crate) fn set_db(&mut self, db: f64, sample_rate: f64) {
        let a = 10f64.powf(db / 40.0);
        let q = if db >= 0.0 { self.q_boost } else { self.q_cut };
        let k = 1.0 / (q * a);
        self.coeffs = SvfCoeffs::new(self.fc, k, sample_rate);
        self.m1 = k * (a * a - 1.0);
    }

    #[inline]
    pub(crate) fn tick(&mut self, ch: usize, x: f64) -> f64 {
        let out = self.state[ch & 1].tick(&self.coeffs, x);
        x + self.m1 * out.band
    }

    pub(crate) fn reset(&mut self) {
        self.state = [SvfState::default(); 2];
    }
}
