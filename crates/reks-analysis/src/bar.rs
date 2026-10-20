//! Bar placement (phase 3 task 12a): which line of a constant grid is beat 1.
//!
//! The tracker flags downbeats all through the track, but the single flag
//! nearest the start of the steady region is a coin toss on many tracks: a
//! small change to the beats moves the region and rotates the bar. Two
//! signals agree with rekordbox's bar far more often (measured on the
//! maintainer's library, docs/analysis-parity.md):
//!
//! - the bar position (line index mod 4) that most tracker downbeats fall
//!   on, as a share of all of them;
//! - the first grid line with a kick: tracks start their kick on beat 1.
//!
//! Each position scores its share of the downbeats, plus [`FIRST_KICK_WEIGHT`]
//! if the first kick is on it; the highest wins. So the first kick decides
//! unless the vote is clear.

use crate::onset::KickPulse;

/// The first kick: the first grid line with a low-band rise of at least
/// this many dB ([`KickPulse::db`]) within [`KICK_SLACK_FRAMES`] of it.
pub const FIRST_KICK_DB: f32 = 10.0;
/// Score of the first kick, against downbeat shares that sum to 1.
pub const FIRST_KICK_WEIGHT: f64 = 0.3;
/// Pulse frames either side of a line searched for its kick (5 ms each).
const KICK_SLACK_FRAMES: i64 = 2;

/// How [`downbeat_line`] picks the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarRule {
    /// The flagged downbeat nearest the steady region's start (version 3).
    Region,
    /// The bar position most downbeats fall on.
    Vote,
    /// [`BarRule::Vote`] plus the first kick.
    VoteKick,
}

impl BarRule {
    pub fn name(self) -> &'static str {
        match self {
            Self::Region => "region",
            Self::Vote => "vote",
            Self::VoteKick => "vote-kick",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Region, Self::Vote, Self::VoteKick]
            .into_iter()
            .find(|r| r.name() == s)
    }
}

/// The lines of a constant grid: `origin + k * beat_len` samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lines {
    pub origin: f64,
    pub beat_len: f64,
}

impl Lines {
    fn index(&self, sample: f64) -> i64 {
        ((sample - self.origin) / self.beat_len).round() as i64
    }

    fn at(&self, index: i64) -> f64 {
        self.origin + index as f64 * self.beat_len
    }
}

/// The grid line to call beat 1: the first line at or after `fallback`'s
/// (the line nearest it) on the bar position `rule` picks. `fallback`'s
/// position wins ties and stands when there is no evidence.
pub fn downbeat_line(
    lines: Lines,
    downbeats: &[i64],
    pulse: Option<&KickPulse>,
    fallback: f64,
    rule: BarRule,
) -> f64 {
    let base = lines.index(fallback);
    let position = |index: i64| (index - base).rem_euclid(4) as usize;
    let mut score = [0.0f64; 4];
    if rule != BarRule::Region && !downbeats.is_empty() {
        let share = 1.0 / downbeats.len() as f64;
        for &d in downbeats {
            score[position(lines.index(d as f64))] += share;
        }
    }
    if rule == BarRule::VoteKick {
        if let Some(index) = pulse.and_then(|p| first_kick(lines, p)) {
            score[position(index)] += FIRST_KICK_WEIGHT;
        }
    }
    // First of equal maxima: position 0 is the fallback's.
    let best = (1..4).fold(0, |b, r| if score[r] > score[b] { r } else { b });
    lines.at(base + best as i64)
}

/// Index of the first line at or after sample 0 with a kick
/// ([`FIRST_KICK_DB`]).
fn first_kick(lines: Lines, pulse: &KickPulse) -> Option<i64> {
    let end = pulse.len_samples();
    let mut index = (-lines.origin / lines.beat_len).ceil() as i64;
    while lines.at(index) < end {
        if pulse.rise_near(lines.at(index), KICK_SLACK_FRAMES) >= FIRST_KICK_DB {
            return Some(index);
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::ANALYSIS_RATE;
    use crate::onset::PULSE_RATE;

    const BEAT: f64 = 22_050.0; // 120 BPM at 44.1 kHz
    const LINES: Lines = Lines {
        origin: 1_000.0,
        beat_len: BEAT,
    };

    /// Downbeats on line `phase + 4k` for `bars` bars from line 0.
    fn downbeats(phase: i64, bars: i64) -> Vec<i64> {
        (0..bars).map(|k| LINES.at(phase + 4 * k) as i64).collect()
    }

    /// A rise curve with a `db` kick on every line from `from` on.
    fn kicks_from(from: i64, db: f32, lines: i64) -> KickPulse {
        let frame = |i: i64| {
            (LINES.at(i) * f64::from(PULSE_RATE) / f64::from(ANALYSIS_RATE)).round() as usize
        };
        let mut q = vec![0u8; frame(lines) + 1];
        for i in from..lines {
            q[frame(i)] = (db * 4.0) as u8;
        }
        KickPulse::from_quarter_db(q)
    }

    #[test]
    fn the_vote_beats_the_downbeat_nearest_the_fallback() {
        // 30 bars on position 2, 5 on position 0 near the fallback.
        let mut d = downbeats(2, 30);
        d.extend(downbeats(0, 5));
        d.sort_unstable();
        let got = downbeat_line(LINES, &d, None, LINES.at(4), BarRule::Vote);
        assert_eq!(got, LINES.at(6));
        // Region: the fallback stands.
        let got = downbeat_line(LINES, &d, None, LINES.at(4), BarRule::Region);
        assert_eq!(got, LINES.at(4));
    }

    #[test]
    fn the_first_kick_decides_a_split_vote() {
        // 55% on position 1, 45% on position 3; the kick starts on line 7.
        let mut d = downbeats(1, 11);
        d.extend(downbeats(3, 9));
        let pulse = kicks_from(7, 14.0, 200);
        let got = downbeat_line(LINES, &d, Some(&pulse), LINES.at(0), BarRule::VoteKick);
        assert_eq!(got, LINES.at(3));
        // Without the kick rule the vote wins.
        let got = downbeat_line(LINES, &d, Some(&pulse), LINES.at(0), BarRule::Vote);
        assert_eq!(got, LINES.at(1));
    }

    #[test]
    fn a_clear_vote_beats_the_first_kick() {
        // 80% on position 2; the kick starts on line 5 (position 1).
        let mut d = downbeats(2, 16);
        d.extend(downbeats(1, 4));
        let pulse = kicks_from(5, 14.0, 200);
        let got = downbeat_line(LINES, &d, Some(&pulse), LINES.at(0), BarRule::VoteKick);
        assert_eq!(got, LINES.at(2));
    }

    #[test]
    fn weak_kicks_do_not_count() {
        // Ties between positions 0 and 3: a kick below the threshold
        // changes nothing; the fallback's position wins the tie.
        let mut d = downbeats(0, 10);
        d.extend(downbeats(3, 10));
        let weak = kicks_from(3, FIRST_KICK_DB - 1.0, 200);
        let got = downbeat_line(LINES, &d, Some(&weak), LINES.at(8), BarRule::VoteKick);
        assert_eq!(got, LINES.at(8));
        let strong = kicks_from(3, FIRST_KICK_DB, 200);
        let got = downbeat_line(LINES, &d, Some(&strong), LINES.at(8), BarRule::VoteKick);
        assert_eq!(got, LINES.at(11));
    }

    #[test]
    fn no_evidence_keeps_the_fallback() {
        let silent = KickPulse::from_quarter_db(vec![0; 1000]);
        for rule in [BarRule::Region, BarRule::Vote, BarRule::VoteKick] {
            let got = downbeat_line(LINES, &[], Some(&silent), LINES.at(9), rule);
            assert_eq!(got, LINES.at(9), "{rule:?}");
        }
        // A fallback between lines snaps to the nearest one.
        let got = downbeat_line(LINES, &[], None, LINES.at(9) + 0.3 * BEAT, BarRule::Vote);
        assert_eq!(got, LINES.at(9));
    }

    #[test]
    fn the_first_kick_ignores_lines_before_the_track() {
        // Lines 0–2 lie before sample 0; line 3 is the first kick.
        let lines = Lines {
            origin: LINES.origin - 3.0 * BEAT,
            beat_len: BEAT,
        };
        let pulse = kicks_from(0, 14.0, 200);
        assert_eq!(first_kick(lines, &pulse), Some(3));
        assert_eq!(first_kick(LINES, &kicks_from(5, 14.0, 200)), Some(5));
        assert_eq!(first_kick(LINES, &kicks_from(5, 9.0, 200)), None);
    }

    #[test]
    fn rule_names_round_trip() {
        for rule in [BarRule::Region, BarRule::Vote, BarRule::VoteKick] {
            assert_eq!(BarRule::parse(rule.name()), Some(rule));
        }
        assert_eq!(BarRule::parse("nearest"), None);
    }
}
