//! Reading the timestamps Meet writes, without a date-time dependency.
//!
//! Google writes a time as RFC 3339 in UTC, with no fraction or with three,
//! six or nine digits of one: `2026-10-12T16:00:00Z`,
//! `2026-10-12T16:00:04.250Z`. A caller's own time may carry an offset
//! instead of `Z`. Nothing here is a model; it is the arithmetic behind
//! `startMs` and `endMs`, and the check a time gets before it goes into a
//! filter.

/// The milliseconds from the start of 1970 (UTC) to `text`, an RFC 3339
/// timestamp. `None` when `text` is not one. Digits after the third of a
/// fraction are dropped.
pub(crate) fn millis(text: &str) -> Option<i64> {
    let (seconds, billionths) = instant(text)?;
    Some(seconds * 1_000 + billionths / 1_000_000)
}

/// The same moment as whole seconds from the start of 1970 (UTC) and the
/// billionths of a second after them, to tell apart two times that fall in
/// one millisecond. Digits after the ninth of a fraction are dropped.
pub(crate) fn instant(text: &str) -> Option<(i64, i64)> {
    let bytes = text.as_bytes();
    // `2026-10-12T16:00:00Z` is the shortest there is.
    if bytes.len() < 20 || !text.is_ascii() {
        return None;
    }
    let number = |from: usize, to: usize| -> Option<i64> {
        let digits = text.get(from..to)?;
        if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok()
    };
    let is = |at: usize, expected: &[u8]| bytes.get(at).is_some_and(|byte| expected.contains(byte));
    if !(is(4, b"-") && is(7, b"-") && is(10, b"Tt") && is(13, b":") && is(16, b":")) {
        return None;
    }
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
    if !(1..=12).contains(&month) || !(1..=days_in(year, month)).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    // The fraction, when there is one, however fine: what is finer than a
    // thousandth is left off.
    let mut at = 19;
    let mut billionths = 0;
    if is(at, b".") {
        let digits: Vec<i64> = bytes[at + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .map(|byte| i64::from(byte - b'0'))
            .collect();
        if digits.is_empty() {
            return None;
        }
        // `.5` is 500 thousandths, which is 500,000,000 billionths.
        for place in 0..9 {
            billionths = billionths * 10 + digits.get(place).copied().unwrap_or(0);
        }
        at += 1 + digits.len();
    }

    // `Z`, or how far ahead of UTC the time was written: `+05:30`.
    let ahead_minutes = match bytes.get(at)? {
        b'Z' | b'z' if at + 1 == bytes.len() => 0,
        sign @ (b'+' | b'-') if at + 6 == bytes.len() && is(at + 3, b":") => {
            let (hours, minutes) = (number(at + 1, at + 3)?, number(at + 4, at + 6)?);
            if hours > 23 || minutes > 59 {
                return None;
            }
            let ahead = hours * 60 + minutes;
            if *sign == b'+' { ahead } else { -ahead }
        }
        _ => return None,
    };

    let seconds = days_from_1970(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second - ahead_minutes * 60;
    Some((seconds, billionths))
}

fn days_in(year: i64, month: i64) -> i64 {
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The days from 1 January 1970 to a date. Counted in cycles of 400 years
/// that begin on 1 March, so that a leap day is the last day of its year.
fn days_from_1970(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let cycle = year.div_euclid(400);
    let year_of_cycle = year.rem_euclid(400);
    let month_from_march = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_cycle = year_of_cycle * 365 + year_of_cycle / 4 - year_of_cycle / 100 + day_of_year;
    cycle * 146_097 + day_of_cycle - 719_468
}

#[cfg(test)]
mod tests {
    use super::{instant, millis};

    #[test]
    fn a_time_is_read_with_and_without_a_fraction() {
        assert_eq!(millis("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(millis("2026-10-12T16:00:00Z"), Some(1_791_820_800_000));
        assert_eq!(millis("2026-10-12T16:00:04.250Z"), Some(1_791_820_804_250));
        // One digit is tenths; digits after the third are dropped, not rounded.
        assert_eq!(millis("2026-10-12T16:00:04.5Z"), Some(1_791_820_804_500));
        assert_eq!(millis("2026-10-12T16:00:04.250999Z"), Some(1_791_820_804_250));
        assert_eq!(millis("2026-10-12T16:00:04.000000001Z"), Some(1_791_820_804_000));
        // Finer than Google is known to write. What is past a thousandth is left off.
        assert_eq!(millis("2026-10-12T16:00:04.2501234567Z"), Some(1_791_820_804_250));
        // Two times in one millisecond are still told apart.
        assert!(instant("2026-10-12T16:00:04.0009Z") > instant("2026-10-12T16:00:04.0001Z"));
        assert_eq!(instant("2026-10-12T16:00:04.000000001+00:00"), Some((1_791_820_804, 1)));
        assert_eq!(millis("2026-10-12t16:00:00z"), Some(1_791_820_800_000));
    }

    #[test]
    fn an_offset_is_taken_off_so_that_two_ways_of_writing_one_moment_agree() {
        let utc = millis("2026-10-12T16:00:00Z");
        assert_eq!(millis("2026-10-12T21:30:00+05:30"), utc);
        assert_eq!(millis("2026-10-12T09:00:00-07:00"), utc);
        assert_eq!(millis("2026-10-13T00:00:00.000+08:00"), utc);
    }

    #[test]
    fn dates_are_counted_across_leap_years_and_before_1970() {
        assert_eq!(millis("2024-02-29T00:00:00Z"), Some(1_709_164_800_000));
        assert_eq!(millis("2024-03-01T00:00:00Z"), Some(1_709_251_200_000));
        assert_eq!(millis("2000-02-29T12:00:00Z"), Some(951_825_600_000));
        assert_eq!(millis("2025-12-31T23:59:59.999Z"), Some(1_767_225_599_999));
        assert_eq!(millis("1969-12-31T23:59:59Z"), Some(-1_000));
        // The last second of a year is one second before the next year.
        let end = millis("2026-12-31T23:59:59Z").unwrap();
        assert_eq!(millis("2027-01-01T00:00:00Z"), Some(end + 1_000));
    }

    #[test]
    fn what_is_not_a_timestamp_is_not_read() {
        for text in [
            "",
            "yesterday",
            "2026-10-12",
            "2026-10-12T16:00:00",
            "2026-10-12 16:00:00Z",
            "2026-10-12T16:00Z",
            "2026-13-01T00:00:00Z",
            "2026-00-10T00:00:00Z",
            "2026-02-29T00:00:00Z",
            "1900-02-29T00:00:00Z",
            "2026-04-31T00:00:00Z",
            "2026-10-12T24:00:00Z",
            "2026-10-12T16:60:00Z",
            "2026-10-12T16:00:60Z",
            "2026-10-12T16:00:00.Z",
            "2026-10-12T16:00:00+0530",
            "2026-10-12T16:00:00+24:00",
            "2026-10-12T16:00:00Zjunk",
            "2026-10-12T16:00:00Z\" OR start_time>=\"",
            "+026-10-12T16:00:00Z",
            "2026-1\u{ff10}-12T16:00:00Z",
        ] {
            assert_eq!(millis(text), None, "{text}");
        }
    }
}
