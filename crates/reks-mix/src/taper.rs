//! Knob tapers: every mixer control arrives as a 0..=1 value, like a knob or
//! fader position, and is mapped here to dB, Hz, a gain or a curve.
//!
//! Ranges follow the Xone:92 user guide, the EQ the Xone:96 guide
//! (docs/research.md §9.1); the shape of
//! each law between its end points is our choice and marked UNVERIFIED in
//! `docs/mixer-dsp.md`. Pure functions, safe to call on the audio thread.

/// Clamps a control value to 0..=1; NaN or infinity becomes `neutral`, the
/// position that leaves the signal alone, so a bad value never kills a channel.
pub fn unit(value: f32, neutral: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        neutral
    }
}

/// dB to linear amplitude.
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Maximum boost of the HI and LO shelves (Xone:96, R§9.1).
pub const SHELF_BOOST_DB: f32 = 6.0;
/// Maximum boost of the HI MID and LO MID bands (Xone:96, R§9.1).
pub const MID_BOOST_DB: f32 = 10.0;
/// Full cut of the HI MID and LO MID bands (Xone:96, R§9.1).
pub const MID_CUT_DB: f32 = -27.0;

/// HI MID / LO MID gain in dB: linear in dB on each side of noon,
/// −27 dB fully anticlockwise, 0 dB at noon, +10 dB fully clockwise.
pub fn mid_db(value: f32) -> f32 {
    let v = unit(value, 0.5);
    if v >= 0.5 {
        MID_BOOST_DB * (2.0 * v - 1.0)
    } else {
        MID_CUT_DB * (1.0 - 2.0 * v)
    }
}

/// HI / LO shelf amplitude: 2.0 (+6 dB) fully clockwise, 1.0 at noon, 0.0
/// (the kill, where the band becomes a 12 dB/oct filter) fully anticlockwise.
/// Boost is linear in dB; cut is cubic in amplitude so it reaches the kill
/// smoothly (−18 dB at nine o'clock, −36 dB at 7:30).
pub fn shelf_gain(value: f32) -> f32 {
    let v = unit(value, 0.5);
    if v >= 0.5 {
        db_to_gain(SHELF_BOOST_DB * (2.0 * v - 1.0))
    } else {
        (2.0 * v).powi(3)
    }
}

/// LEVEL trim range in dB (20 dB in total, R§9.1). The split around noon is
/// UNVERIFIED; ±10 dB keeps unity at noon.
pub const TRIM_RANGE_DB: f32 = 20.0;

/// LEVEL trim in dB: −10 dB fully anticlockwise, 0 dB at noon, +10 dB fully
/// clockwise, linear in dB.
pub fn trim_db(value: f32) -> f32 {
    TRIM_RANGE_DB * (unit(value, 0.5) - 0.5)
}

/// Channel fader amplitude: cubic, so 0 dB at the top, −18 dB at half travel
/// and silence at the bottom. UNVERIFIED shape (the guide gives none).
pub fn fader_gain(value: f32) -> f32 {
    unit(value, 0.0).powi(3)
}

/// Master level amplitude: same law as the channel fader, never above 0 dB so
/// the limiter is a safety net, not a tone control.
pub fn master_gain(value: f32) -> f32 {
    fader_gain(value)
}

/// CUE LEVEL amplitude: the fader law, at most 0 dB; the cue limiter
/// protects the headphones.
pub fn cue_level_gain(value: f32) -> f32 {
    fader_gain(value)
}

/// CUE MIX as `(cue, master)` gains: equal power, cue only at 0, master only
/// at 1, both at −3 dB at noon. The guide gives no law: UNVERIFIED.
pub fn cue_mix_gains(value: f32) -> (f32, f32) {
    let a = std::f32::consts::FRAC_PI_2 * unit(value, 0.0);
    (a.cos(), a.sin())
}

/// Filter FREQ range in Hz (R§9.1).
pub const FILTER_MIN_HZ: f32 = 30.0;
pub const FILTER_MAX_HZ: f32 = 20_000.0;

/// Filter cutoff (−3 dB point) in Hz: exponential, so equal knob travel is an
/// equal musical interval (about 0.94 octaves per tenth of travel).
pub fn filter_freq_hz(value: f32) -> f32 {
    FILTER_MIN_HZ * (FILTER_MAX_HZ / FILTER_MIN_HZ).powf(unit(value, 1.0))
}

/// Resonance Q at MILD: Butterworth, so the cutoff is the −3 dB point.
pub const RESONANCE_MIN_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// Resonance Q at WILD: a +20 dB peak. Halved from Q 20 (+26 dB) on the
/// maintainer's ear (2026-10-21): too much at maximum. UNVERIFIED (the guide
/// gives no figure).
pub const RESONANCE_MAX_Q: f32 = 10.0;

/// Filter resonance as Q from MILD to WILD: exponential in the square of the
/// knob, so the peak in dB grows with v², not v. The first half of travel
/// adds under 3 dB (Q 1.37 at noon) and the knob gets busy only near WILD;
/// the plain exponential was too sensitive early on.
pub fn resonance_q(value: f32) -> f32 {
    let v = unit(value, 0.0);
    RESONANCE_MIN_Q * (RESONANCE_MAX_Q / RESONANCE_MIN_Q).powf(v * v)
}

/// Crossfader travel over which a side fades in at the sharpest curve, as a
/// fraction of the full travel (1.5 mm of a 60 mm fader). UNVERIFIED.
pub const XFADE_MIN_WIDTH: f32 = 1.0 / 40.0;

/// Crossfader curve as the fraction of travel a side needs to reach full
/// level: 1.0 (progressive, fades over the whole travel) at 0, down to
/// [`XFADE_MIN_WIDTH`] (near-instant cut) at 1. Exponential in between.
pub fn crossfader_width(curve: f32) -> f32 {
    XFADE_MIN_WIDTH.powf(unit(curve, 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn unit_clamps_and_replaces_non_finite_values() {
        assert_eq!(unit(-1.0, 0.5), 0.0);
        assert_eq!(unit(2.0, 0.5), 1.0);
        assert_eq!(unit(f32::NAN, 0.5), 0.5);
        assert_eq!(unit(f32::INFINITY, 0.25), 0.25);
    }

    #[test]
    fn mid_taper_spans_minus_27_to_plus_10_with_noon_flat() {
        assert!(close(mid_db(0.0), -27.0, 1e-5));
        assert!(close(mid_db(0.25), -13.5, 1e-5));
        assert_eq!(mid_db(0.5), 0.0);
        assert!(close(mid_db(0.75), 5.0, 1e-5));
        assert!(close(mid_db(1.0), 10.0, 1e-5));
        assert_eq!(mid_db(f32::NAN), 0.0);
    }

    #[test]
    fn shelf_taper_runs_from_kill_through_unity_to_plus_6_db() {
        assert_eq!(shelf_gain(0.0), 0.0);
        assert!(close(shelf_gain(0.25), 0.125, 1e-6));
        assert_eq!(shelf_gain(0.5), 1.0);
        assert!(close(shelf_gain(1.0), db_to_gain(6.0), 1e-6));
        // Monotonic all the way.
        let mut last = -1.0;
        for i in 0..=100 {
            let g = shelf_gain(i as f32 / 100.0);
            assert!(g > last);
            last = g;
        }
    }

    #[test]
    fn trim_taper_spans_20_db_with_unity_at_noon() {
        assert!(close(trim_db(0.0), -10.0, 1e-5));
        assert_eq!(trim_db(0.5), 0.0);
        assert!(close(trim_db(1.0), 10.0, 1e-5));
        assert!(close(trim_db(1.0) - trim_db(0.0), TRIM_RANGE_DB, 1e-5));
    }

    #[test]
    fn fader_taper_is_unity_at_top_and_silent_at_bottom() {
        assert_eq!(fader_gain(1.0), 1.0);
        assert_eq!(fader_gain(0.0), 0.0);
        let half_db = 20.0 * fader_gain(0.5).log10();
        assert!(close(half_db, -18.06, 0.01));
        assert_eq!(fader_gain(f32::NAN), 0.0);
    }

    #[test]
    fn master_taper_never_exceeds_unity() {
        for i in 0..=100 {
            assert!(master_gain(i as f32 / 100.0) <= 1.0);
        }
        assert_eq!(master_gain(1.0), 1.0);
    }

    #[test]
    fn cue_level_and_mix_tapers() {
        assert_eq!(cue_level_gain(1.0), 1.0);
        assert_eq!(cue_level_gain(0.0), 0.0);
        assert_eq!(cue_mix_gains(0.0), (1.0, 0.0));
        let (c, m) = cue_mix_gains(1.0);
        assert!(c.abs() < 1e-6 && m == 1.0);
        let (c, m) = cue_mix_gains(0.5);
        assert!(close(c, m, 1e-6) && close(20.0 * c.log10(), -3.01, 0.01));
        assert_eq!(cue_mix_gains(f32::NAN), (1.0, 0.0));
    }

    #[test]
    fn filter_freq_taper_is_exponential_from_30_hz_to_20_khz() {
        assert!(close(filter_freq_hz(0.0), 30.0, 1e-3));
        assert!(close(filter_freq_hz(1.0), 20_000.0, 0.5));
        // Geometric mean at noon.
        assert!(close(
            filter_freq_hz(0.5),
            (30.0f32 * 20_000.0).sqrt(),
            0.05
        ));
    }

    #[test]
    fn resonance_taper_runs_from_butterworth_to_wild() {
        assert!(close(
            resonance_q(0.0),
            std::f32::consts::FRAC_1_SQRT_2,
            1e-6
        ));
        assert!(close(resonance_q(1.0), RESONANCE_MAX_Q, 1e-3));
        // Gentle to noon (peak dB grows with v²), monotonic all the way.
        assert!(close(resonance_q(0.5), 1.37, 0.01));
        let mut last = 0.0;
        for i in 0..=100 {
            let q = resonance_q(i as f32 / 100.0);
            assert!(q > last);
            last = q;
        }
    }

    #[test]
    fn crossfader_width_runs_from_full_travel_to_near_instant() {
        assert_eq!(crossfader_width(0.0), 1.0);
        assert!(close(crossfader_width(1.0), XFADE_MIN_WIDTH, 1e-6));
        assert!(crossfader_width(0.5) < 1.0 && crossfader_width(0.5) > XFADE_MIN_WIDTH);
    }
}
