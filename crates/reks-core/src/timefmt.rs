//! UTC ISO-8601 timestamps for the library DB (CLAUDE.md: timestamps are UTC
//! ISO-8601 strings). Small enough not to need a date-time crate.

use std::time::{SystemTime, UNIX_EPOCH};

/// `YYYY-MM-DDTHH:MM:SS.mmmZ`, UTC, millisecond precision.
pub fn iso8601_utc(t: SystemTime) -> String {
    let ms = unix_millis(t);
    let (days, ms_of_day) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    let (y, m, d) = civil_from_days(days);
    let secs = ms_of_day / 1000;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        secs / 3600,
        secs / 60 % 60,
        secs % 60,
        ms_of_day % 1000
    )
}

/// Current time as [`iso8601_utc`].
pub fn now_utc() -> String {
    iso8601_utc(SystemTime::now())
}

/// Current UTC date, `YYYY-MM-DD` (the form rekordbox uses for `DateAdded`).
pub fn today_utc() -> String {
    let (y, m, d) = civil_from_days(unix_millis(SystemTime::now()).div_euclid(86_400_000));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Inverse of [`iso8601_utc`]: Unix milliseconds of `YYYY-MM-DDTHH:MM:SS.mmmZ`.
/// `None` for anything else.
pub fn parse_iso8601_utc(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let shape = b"0000-00-00T00:00:00.000Z";
    if b.len() != shape.len()
        || !b.iter().zip(shape).all(|(c, s)| {
            if *s == b'0' {
                c.is_ascii_digit()
            } else {
                c == s
            }
        })
    {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s[r].parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (hh, mm, ss, ms) = (n(11..13)?, n(14..16)?, n(17..19)?, n(20..23)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    let days = days_from_civil(y, m, d);
    Some(((days * 24 + hh) * 60 + mm) * 60_000 + ss * 1000 + ms)
}

/// (year, month, day) → days since 1970-01-01 (Hinnant's `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = y - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn unix_millis(t: SystemTime) -> i64 {
    let ms = |d: std::time::Duration| i64::try_from(d.as_millis()).unwrap_or(i64::MAX);
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => ms(d),
        Err(e) => -ms(e.duration()),
    }
}

/// Days since 1970-01-01 → (year, month, day), proleptic Gregorian.
/// Howard Hinnant's `civil_from_days` (public-domain algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = yoe + era * 400 + i64::from(m <= 2);
    // d and m are in the small ranges noted above.
    (y, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(ms: i64) -> SystemTime {
        if ms >= 0 {
            UNIX_EPOCH + Duration::from_millis(ms as u64)
        } else {
            UNIX_EPOCH - Duration::from_millis(ms.unsigned_abs())
        }
    }

    #[test]
    fn known_instants() {
        assert_eq!(iso8601_utc(at(0)), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601_utc(at(951_782_400_000)), "2000-02-29T00:00:00.000Z");
        assert_eq!(
            iso8601_utc(at(1_792_411_384_567)),
            "2026-10-19T12:03:04.567Z"
        );
        assert_eq!(iso8601_utc(at(-1)), "1969-12-31T23:59:59.999Z");
        assert_eq!(
            iso8601_utc(at(4_107_542_399_000)),
            "2100-02-28T23:59:59.000Z"
        );
    }

    #[test]
    fn parse_inverts_format() {
        for ms in [0, 951_782_400_000, 1_792_411_384_567, 4_107_542_399_000] {
            assert_eq!(parse_iso8601_utc(&iso8601_utc(at(ms))), Some(ms));
        }
        assert_eq!(parse_iso8601_utc("2026-10-19"), None);
        assert_eq!(parse_iso8601_utc("2026-13-19T12:03:04.567Z"), None);
        assert_eq!(parse_iso8601_utc("2026-10-19T12:03:04.567+"), None);
    }

    #[test]
    fn today_is_a_date() {
        let t = today_utc();
        assert_eq!(t.len(), 10);
        assert!(now_utc().starts_with(&t[..4]));
    }
}
