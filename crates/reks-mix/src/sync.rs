//! Beat sync: a follower's speed for the next block, from the leader's beat.
//!
//! Tempo match: the follower plays as many beats per output frame as the
//! leader, from both decks' local grid tempo, so dynamic grids are followed
//! beat by beat. Phase: the follower's beat (or place in the bar, for bar
//! sync) is compared with the leader's at the same output frame. A small
//! error is pulled in by bending the speed, at most [`MAX_BEND`], with time
//! constant [`PULL_SECS`]; an error over [`JUMP_BEATS`] (sync just engaged,
//! the leader jumped, a nudge was released) is fixed by a jump instead. Pure
//! functions; the graph calls them once per block on the audio thread.

use crate::beatmap::{wrap, BeatMap};

/// Time constant of the phase pull: an error halves in about 0.35 s.
const PULL_SECS: f64 = 0.5;
/// Largest speed bend the phase pull uses (±1%: below what most hear as pitch).
const MAX_BEND: f64 = 0.01;
/// Phase error, in beats, above which the follower jumps into phase
/// (1/32 beat: 15 ms at 128 BPM).
pub(crate) const JUMP_BEATS: f64 = 1.0 / 32.0;

/// The leader as a follower sees it at the start of a block.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lead {
    /// Beats the leader advances per output frame.
    pub beats_per_frame: f64,
    /// Beat number and place in the bar now; `None` while it is stopped
    /// (followers then match its tempo only).
    pub phase: Option<(f64, (f64, u8))>,
}

impl Lead {
    /// `step`: source frames per output frame; `None` without a grid.
    pub fn new(beats: &BeatMap, position: f64, step: f64, playing: bool) -> Option<Self> {
        let period = beats.period_at(position)?;
        let phase = if playing {
            beats.beat_at(position).zip(beats.bar_at(position))
        } else {
            None
        };
        Some(Self {
            beats_per_frame: step / period,
            phase,
        })
    }
}

/// A follower deck's side of the sum.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Follower<'a> {
    pub beats: &'a BeatMap,
    pub position: f64,
    /// Source frames per output frame at speed 1.
    pub rate_ratio: f64,
    pub playing: bool,
    pub bar: bool,
    /// Pitch bend held; phase correction pauses while it is not 1.0.
    pub nudge: f64,
}

/// What a follower does in the next block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Plan {
    Speed(f64),
    /// Jump to this source frame, then play at the speed.
    Jump {
        to: f64,
        speed: f64,
    },
}

/// Phase error in beats (positive: the follower is ahead), wrapped into half
/// a beat, or half a bar with bar sync when both bars are the same length;
/// with the span it was wrapped in.
pub(crate) fn phase_error(me: (f64, (f64, u8)), lead: (f64, (f64, u8)), bar: bool) -> (f64, f64) {
    let ((beat, (in_bar, bpb)), (lead_beat, (lead_in_bar, lead_bpb))) = (me, lead);
    if bar && bpb == lead_bpb {
        let span = f64::from(bpb);
        (wrap(in_bar - lead_in_bar, span), span)
    } else {
        (wrap(beat - lead_beat, 1.0), 1.0)
    }
}

/// The follower's plan; `None` when it has no grid to sync with.
/// `engine_rate` is in frames per second.
pub(crate) fn follow(lead: &Lead, me: &Follower<'_>, engine_rate: f64) -> Option<Plan> {
    let period = me.beats.period_at(me.position)?;
    // Source frames per output frame that match the leader's beat rate.
    let to_speed = |beats_per_frame: f64| beats_per_frame * period / me.rate_ratio;
    let matched = to_speed(lead.beats_per_frame);
    if me.nudge != 1.0 {
        return Some(Plan::Speed(matched * me.nudge));
    }
    let (Some(lead_phase), true) = (lead.phase, me.playing) else {
        return Some(Plan::Speed(matched));
    };
    let mine = me
        .beats
        .beat_at(me.position)
        .zip(me.beats.bar_at(me.position))?;
    let (error, span) = phase_error(mine, lead_phase, me.bar);
    if error.abs() > JUMP_BEATS {
        // In phase, and not before the track starts: a deck started from
        // its first frame is often less than a beat into its grid.
        let mut target = mine.0 - error;
        let start = me.beats.beat_at(0.0)?;
        if target < start {
            target += ((start - target) / span).ceil() * span;
        }
        let to = me.beats.frame_at(target)?;
        return Some(Plan::Jump { to, speed: matched });
    }
    // Close the error over PULL_SECS: that many fewer beats per frame.
    let pull = to_speed(lead.beats_per_frame - error / (PULL_SECS * engine_rate));
    Some(Plan::Speed(pull.clamp(
        matched * (1.0 - MAX_BEND),
        matched * (1.0 + MAX_BEND),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beatmap::GridAnchor;

    const RATE: f64 = 48_000.0;

    fn grid(bpm: f64, frame: i64) -> BeatMap {
        BeatMap::new(
            &[GridAnchor {
                frame,
                bpm,
                beat_in_bar: 1,
                beats_per_bar: 4,
            }],
            48_000,
        )
    }

    fn follower(beats: &BeatMap, position: f64) -> Follower<'_> {
        Follower {
            beats,
            position,
            rate_ratio: 1.0,
            playing: true,
            bar: false,
            nudge: 1.0,
        }
    }

    #[test]
    fn matches_tempo_from_the_grids() {
        let lead_map = grid(128.0, 0);
        let mine = grid(125.0, 0);
        let lead = Lead::new(&lead_map, 0.0, 1.0, false).unwrap();
        let plan = follow(&lead, &follower(&mine, 0.0), RATE).unwrap();
        assert_eq!(plan, Plan::Speed(128.0 / 125.0));
        // A leader at +5% and a follower at another rate.
        let lead = Lead::new(&lead_map, 0.0, 1.05, false).unwrap();
        let me = Follower {
            rate_ratio: 44_100.0 / 48_000.0,
            ..follower(&mine, 0.0)
        };
        let Plan::Speed(s) = follow(&lead, &me, RATE).unwrap() else {
            panic!("speed");
        };
        assert!((s - 1.05 * 128.0 / 125.0 * 48_000.0 / 44_100.0).abs() < 1e-12);
    }

    #[test]
    fn small_errors_bend_the_speed_large_ones_jump() {
        let map = grid(120.0, 0); // 24,000 frames a beat
        let lead = Lead::new(&map, 240_000.0, 1.0, true).unwrap();
        // 240 frames ahead: 0.01 beat, under the jump threshold.
        let Plan::Speed(s) = follow(&lead, &follower(&map, 240_240.0), RATE).unwrap() else {
            panic!("speed");
        };
        assert!((0.99..1.0).contains(&s), "{s}");
        // Behind: faster, and the bend is capped.
        let Plan::Speed(s) = follow(&lead, &follower(&map, 239_500.0), RATE).unwrap() else {
            panic!("speed");
        };
        assert!(s > 1.0 && s <= 1.01, "{s}");
        // A third of a beat ahead: jump back by it.
        let plan = follow(&lead, &follower(&map, 488_000.0), RATE).unwrap();
        assert_eq!(
            plan,
            Plan::Jump {
                to: 480_000.0,
                speed: 1.0
            }
        );
        // In phase on another beat: nothing to fix.
        assert_eq!(
            follow(&lead, &follower(&map, 48_000.0), RATE),
            Some(Plan::Speed(1.0))
        );
    }

    #[test]
    fn bar_sync_matches_the_downbeat() {
        let map = grid(120.0, 0);
        let lead = Lead::new(&map, 240_000.0, 1.0, true).unwrap(); // beat 10: 3rd of its bar
        let me = Follower {
            bar: true,
            ..follower(&map, 48_000.0) // beat 2: also 3rd
        };
        assert_eq!(follow(&lead, &me, RATE), Some(Plan::Speed(1.0)));
        let me = Follower {
            bar: true,
            ..follower(&map, 72_000.0) // beat 3: one beat ahead in the bar
        };
        assert_eq!(
            follow(&lead, &me, RATE),
            Some(Plan::Jump {
                to: 48_000.0,
                speed: 1.0
            })
        );
        assert_eq!(
            phase_error((3.0, (3.0, 4)), (1.0, (1.0, 4)), true),
            (-2.0, 4.0)
        );
        assert_eq!(
            phase_error((3.0, (3.0, 4)), (1.0, (1.0, 3)), true),
            (0.0, 1.0)
        );
    }

    #[test]
    fn a_jump_never_lands_before_the_track() {
        let map = grid(120.0, 0);
        let lead = Lead::new(&map, 240_000.0, 1.0, true).unwrap(); // bar position 2
                                                                   // A follower whose first beat is 0.2 beats into the track, at its
                                                                   // start: bar position 3.8, a bar later than the in-phase beat 2.
        let late = grid(120.0, 4_800);
        let me = Follower {
            bar: true,
            ..follower(&late, 0.0)
        };
        assert_eq!(
            follow(&lead, &me, RATE),
            Some(Plan::Jump {
                to: 4_800.0 + 2.0 * 24_000.0,
                speed: 1.0
            })
        );
        let Some(Plan::Jump { to, .. }) = follow(&lead, &follower(&late, 0.0), RATE) else {
            panic!("jump");
        };
        assert!((to - 4_800.0).abs() < 1e-6, "{to}");
    }

    #[test]
    fn nudge_and_stopped_decks_only_match_tempo() {
        let map = grid(120.0, 0);
        let lead = Lead::new(&map, 240_000.0, 1.0, true).unwrap();
        let me = Follower {
            nudge: 1.04,
            ..follower(&map, 250_000.0)
        };
        assert_eq!(follow(&lead, &me, RATE), Some(Plan::Speed(1.04)));
        let me = Follower {
            playing: false,
            ..follower(&map, 250_000.0)
        };
        assert_eq!(follow(&lead, &me, RATE), Some(Plan::Speed(1.0)));
        let stopped = Lead::new(&map, 240_000.0, 1.0, false).unwrap();
        assert_eq!(
            follow(&stopped, &follower(&map, 250_000.0), RATE),
            Some(Plan::Speed(1.0))
        );
        assert_eq!(
            follow(&lead, &follower(&BeatMap::default(), 0.0), RATE),
            None
        );
        assert!(Lead::new(&BeatMap::default(), 0.0, 1.0, true).is_none());
    }
}
