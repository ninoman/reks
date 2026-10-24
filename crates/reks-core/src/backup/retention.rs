//! Which snapshots to keep: the newest `keep_recent`, plus the newest one of
//! each of the last [`WEEKS`] calendar weeks (Monday to Sunday, UTC), so a
//! slow rot (noticed weeks late) still has a clean copy to go back to.

use std::collections::HashSet;

/// Weeks of history the weekly snapshots reach back over.
pub(super) const WEEKS: i64 = 8;

const DAY_MS: i64 = 86_400_000;

/// Monday-based week number; 1970-01-01 was a Thursday.
fn week(ms: i64) -> i64 {
    (ms.div_euclid(DAY_MS) + 3).div_euclid(7)
}

/// Indices into `taken_ms` (Unix milliseconds, any order) of the snapshots to delete.
pub(super) fn expired(taken_ms: &[i64], now_ms: i64, keep_recent: usize) -> Vec<usize> {
    let mut newest_first: Vec<usize> = (0..taken_ms.len()).collect();
    newest_first.sort_by_key(|&i| std::cmp::Reverse(taken_ms[i]));

    let mut keep: HashSet<usize> = newest_first.iter().take(keep_recent).copied().collect();
    let this_week = week(now_ms);
    let mut weeks_seen = HashSet::new();
    for &i in &newest_first {
        let w = week(taken_ms[i]);
        if this_week - w < WEEKS && weeks_seen.insert(w) {
            keep.insert(i);
        }
    }
    (0..taken_ms.len()).filter(|i| !keep.contains(i)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Monday 2026-10-19 00:00 UTC.
    const MONDAY: i64 = 1_792_368_000_000;

    #[test]
    fn weeks_start_on_monday() {
        assert_eq!(week(MONDAY), week(MONDAY + 6 * DAY_MS + DAY_MS - 1));
        assert_eq!(week(MONDAY) - 1, week(MONDAY - 1));
    }

    #[test]
    fn keeps_the_newest_n() {
        let now = MONDAY + 30 * DAY_MS;
        // Fifteen daily snapshots in the last fifteen days, all in the past
        // two to three weeks: the weekly rule adds only a few of them.
        let taken: Vec<i64> = (0..15).map(|d| now - d * DAY_MS).collect();
        let dead = expired(&taken, now, 10);
        for i in 0..10 {
            assert!(!dead.contains(&i), "recent {i} kept");
        }
        assert!(!dead.is_empty());
    }

    #[test]
    fn keeps_one_per_week_for_eight_weeks() {
        let now = MONDAY + 70 * DAY_MS;
        // Two snapshots a week for 12 weeks, oldest first.
        let taken: Vec<i64> = (0..24).map(|i| now - (23 - i) * 3 * DAY_MS - 1).collect();
        let dead = expired(&taken, now, 1);
        let kept: Vec<i64> = (0..taken.len())
            .filter(|i| !dead.contains(i))
            .map(|i| taken[i])
            .collect();
        let mut weeks: Vec<i64> = kept.iter().map(|&t| week(t)).collect();
        weeks.sort();
        weeks.dedup();
        assert_eq!(weeks.len(), kept.len(), "one per week: {kept:?}");
        assert!(kept.len() <= WEEKS as usize);
        assert!(weeks.iter().all(|w| week(now) - w < WEEKS));
        assert!(kept.contains(&taken[23]), "the newest stays");
    }

    #[test]
    fn nothing_to_prune_in_a_small_set() {
        assert!(expired(&[MONDAY, MONDAY + 1], MONDAY + 2, 10).is_empty());
        assert!(expired(&[], MONDAY, 10).is_empty());
    }

    #[test]
    fn ancient_snapshots_beyond_the_count_go() {
        let now = MONDAY + 400 * DAY_MS;
        let taken = [now - 300 * DAY_MS, now - 1];
        assert_eq!(expired(&taken, now, 1), vec![0]);
    }
}
