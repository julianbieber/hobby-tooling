//! RFC 3339 timestamps in and out: parse one to seconds since the epoch, measure
//! how long ago it was, and render a duration the way a person reads one.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch, now.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parse `YYYY-MM-DDTHH:MM:SS`, with an optional fractional second and a
/// mandatory `Z` or `±HH:MM` offset, into seconds since the Unix epoch. `None`
/// on anything else — this is a fixed-shape reader, not a general RFC 3339
/// parser.
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let (date, time_and_offset) = s.split_once('T')?;

    let mut date_parts = date.splitn(4, '-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() {
        return None;
    }

    let (time, offset_secs) = if let Some(rest) = time_and_offset.strip_suffix('Z') {
        (rest, 0)
    } else {
        let pos = time_and_offset.find(['+', '-'])?;
        let (t, off) = time_and_offset.split_at(pos);
        let sign: i64 = if off.starts_with('-') { -1 } else { 1 };
        let (oh, om) = off[1..].split_once(':')?;
        let oh: i64 = oh.parse().ok()?;
        let om: i64 = om.parse().ok()?;
        (t, sign * (oh * 3600 + om * 60))
    };
    let time = time.split('.').next()?;

    let mut time_parts = time.splitn(4, ':');
    let hour: u32 = time_parts.next()?.parse().ok()?;
    let minute: u32 = time_parts.next()?.parse().ok()?;
    let second: u32 = time_parts.next()?.parse().ok()?;
    if time_parts.next().is_some() {
        return None;
    }

    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }

    let days = days_from_civil(year, month, day);
    let secs_of_day = i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second);
    Some(days * 86400 + secs_of_day - offset_secs)
}

/// Seconds between `timestamp` and now, when it parses. Can come back negative
/// when the writer's clock runs ahead of the reader's.
pub fn seconds_since(timestamp: &str) -> Option<i64> {
    parse_rfc3339(timestamp).map(|then| now_unix() - then)
}

/// A duration rendered for a person: `47s`, `12m`, `1h 12m`, `2d 3h`, dropping a
/// zero trailing unit. A negative duration reads as `0s` rather than claiming a
/// precision two unsynchronised clocks cannot give.
pub fn human(secs: i64) -> String {
    if secs < 60 {
        format!("{}s", secs.max(0))
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        if minutes == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {minutes}m")
        }
    } else {
        let days = secs / 86400;
        let hours = (secs % 86400) / 3600;
        if hours == 0 {
            format!("{days}d")
        } else {
            format!("{days}d {hours}h")
        }
    }
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let year_of_era = y - era * 400;
    let month_p = (i64::from(month) + 9) % 12;
    let day_of_year = (153 * month_p + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146097 + day_of_era - 719468
}

#[cfg(test)]
mod tests {
    use super::*;

    // The epoch itself is the simplest fixed point days_from_civil has to land on.
    #[test]
    fn parses_the_epoch() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
    }

    // A date away from the epoch, checked against its well-known Unix timestamp.
    #[test]
    fn parses_a_known_date() {
        assert_eq!(parse_rfc3339("2024-01-01T00:00:00Z"), Some(1_704_067_200));
    }

    // An offset has to convert to UTC, not just be accepted and ignored.
    #[test]
    fn an_offset_shifts_to_utc() {
        assert_eq!(
            parse_rfc3339("2024-01-01T02:00:00+02:00"),
            Some(1_704_067_200)
        );
        assert_eq!(
            parse_rfc3339("2023-12-31T19:00:00-05:00"),
            Some(1_704_067_200)
        );
    }

    // A fractional second is dropped, not misread as part of the seconds field.
    #[test]
    fn a_fractional_second_is_ignored() {
        assert_eq!(
            parse_rfc3339("2024-01-01T00:00:00.523Z"),
            Some(1_704_067_200)
        );
    }

    // Three different ways a timestamp can be malformed, none of which should panic
    // or silently produce a number.
    #[test]
    fn malformed_strings_return_none() {
        assert_eq!(parse_rfc3339("not a date"), None);
        assert_eq!(parse_rfc3339("2024-01-01 00:00:00Z"), None);
        assert_eq!(parse_rfc3339("2024-13-01T00:00:00Z"), None);
    }

    // The boundaries `human` switches its unit at, plus the 72-minute case a
    // coordinator actually reports.
    #[test]
    fn human_boundaries() {
        assert_eq!(human(59), "59s");
        assert_eq!(human(60), "1m");
        assert_eq!(human(3599), "59m");
        assert_eq!(human(3600), "1h");
        assert_eq!(human(4320), "1h 12m");
        assert_eq!(human(86400), "1d");
        assert_eq!(human(-5), "0s");
    }
}
