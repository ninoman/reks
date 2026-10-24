//! A click track that changes tempo, for grid editing with tempo sections
//! (phase 3 tasks 15–18): [`TWO_TEMPO`] is 140 beats at 140 BPM (60 s)
//! then 128 beats at 174 BPM, a DnB-style switch. Each section starts on a
//! downbeat; downbeats are a louder, lower click. Mono `f64`, built from
//! basic IEEE operations like the rest of this crate. Not checked in; tests
//! render it (`cargo run -p reks-fixtures-gen -- tempo <out.wav>` writes it
//! for the hardware check).

use std::io::{Seek, Write};

use crate::sin_turns;

/// A tempo section: `beats` beats at `bpm_x100` / 100 BPM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section {
    pub bpm_x100: u32,
    pub beats: usize,
}

/// 140 BPM for 60 s, then 174 BPM for 128 beats (about 44 s).
pub const TWO_TEMPO: [Section; 2] = [
    Section {
        bpm_x100: 14_000,
        beats: 140,
    },
    Section {
        bpm_x100: 17_400,
        beats: 128,
    },
];

/// Beats per bar.
const BEATS_PER_BAR: usize = 4;
/// Silence before the first beat, ms (a whole number of samples at 44.1 and 48 kHz).
pub const LEAD_MS: u32 = 100;
const CLICK_MS: u32 = 10;
const TAIL_MS: u32 = 500;

/// First sample of every beat of `sections` at `rate`, from [`LEAD_MS`]:
/// each section's beats are exact multiples of its period from its own
/// first beat, rounded to the nearest sample with integer arithmetic. The
/// next section starts one period after the last beat of the one before.
pub fn beat_onsets(rate: u32, sections: &[Section]) -> Vec<usize> {
    let rate = rate as usize;
    let mut out = Vec::new();
    // Section start in samples × bpm_x100 (exact).
    let mut start = LEAD_MS as usize * rate / 1000;
    for s in sections {
        let bpm = s.bpm_x100 as usize;
        // Period = rate × 6000 / bpm_x100 samples.
        let onset = |n: usize| start + (n * rate * 6_000 + bpm / 2) / bpm;
        out.extend((0..s.beats).map(onset));
        start = onset(s.beats);
    }
    out
}

/// Renders `sections` at `rate`: a 10 ms click on every beat (1 kHz on
/// downbeats at 0.6, 2 kHz otherwise at 0.3, linear decay), half a second of
/// silence at the end. Beat `n` starts at [`beat_onsets`]`[n]`; the first
/// beat of every section is a downbeat.
pub fn render_tempo(rate: u32, sections: &[Section]) -> Vec<f64> {
    let onsets = beat_onsets(rate, sections);
    let end = onsets.last().copied().unwrap_or(0) + (TAIL_MS * rate / 1000) as usize;
    let mut out = vec![0.0f64; end];
    let len = (CLICK_MS * rate / 1000) as usize;
    let mut n_in_section = 0;
    let mut section = 0;
    for &at in &onsets {
        if n_in_section == sections[section].beats {
            section += 1;
            n_in_section = 0;
        }
        let down = n_in_section % BEATS_PER_BAR == 0;
        let (hz, amp) = if down { (1_000u64, 0.6) } else { (2_000, 0.3) };
        for (k, slot) in out[at..at + len].iter_mut().enumerate() {
            let turns = (hz * k as u64 % u64::from(rate)) as f64 / f64::from(rate);
            *slot = amp * (1.0 - k as f64 / len as f64) * sin_turns(turns);
        }
        n_in_section += 1;
    }
    out
}

/// Writes [`TWO_TEMPO`] as a 44.1 kHz mono 16-bit WAV.
pub fn write_wav_two_tempo<W: Write + Seek>(writer: W) -> Result<(), hound::Error> {
    let mut wav = hound::WavWriter::new(writer, crate::wav_spec())?;
    for x in render_tempo(crate::SAMPLE_RATE, &TWO_TEMPO) {
        wav.write_sample((x * 32_767.0).round() as i16)?;
    }
    wav.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_switch_on_the_beat() {
        let on = beat_onsets(44_100, &TWO_TEMPO);
        assert_eq!(on.len(), 268);
        // 140 BPM = 18,900 samples a beat; 60 s after the 100 ms lead.
        assert_eq!(on[0], 4_410);
        assert_eq!(on[1] - on[0], 18_900);
        assert_eq!(on[140], 4_410 + 60 * 44_100);
        // 174 BPM = 15,206.9 samples a beat.
        assert_eq!(on[141] - on[140], 15_207);
        assert_eq!(on[267], on[140] + (127 * 44_100 * 6_000 + 8_700) / 17_400);
    }

    #[test]
    fn clicks_sit_on_the_onsets() {
        let x = render_tempo(48_000, &TWO_TEMPO);
        let on = beat_onsets(48_000, &TWO_TEMPO);
        assert_eq!(x.len(), on[267] + 24_000);
        for (n, &at) in on.iter().enumerate() {
            assert_eq!(x[at - 1], 0.0, "beat {n}");
            assert!(x[at + 1] != 0.0, "beat {n}");
        }
        // Downbeats are louder; the 174 section starts on one.
        let peak = |at: usize| x[at..at + 480].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        assert!(peak(on[140]) > 0.5 && peak(on[141]) < 0.35);
    }
}
