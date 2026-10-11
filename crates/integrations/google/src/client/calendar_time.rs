//! Times as the Calendar API reads them. The calendar groups share this.

use socketkit_core::{ErrorKind, Result};

use super::Api;

/// Checks a moment in time before it is sent. `name` is the field it was
/// given in: `timeMin`.
///
/// Google reads these as RFC 3339 and refuses one without an offset from
/// UTC. Nothing else about the time is checked here: Google does that, and
/// says what is wrong in its own words.
pub(super) fn instant(api: &Api<'_>, name: &str, time: &str) -> Result<()> {
    if has_offset(time) {
        return Ok(());
    }
    Err(api.error(
        ErrorKind::InvalidInput,
        format!("`{name}` needs a time with its offset, such as 2026-10-12T09:00:00Z or 2026-10-12T09:00:00-07:00"),
    ))
}

/// Whether `time` is a date and a time that ends in an offset from UTC: `Z`,
/// or hours and minutes after a sign.
///
/// The time is sent as it was given, so it is judged as it was given: space
/// around it, or in place of the `T`, is not something Google is known to
/// read, and is refused here where the reason can be said.
pub(super) fn has_offset(time: &str) -> bool {
    let Some((date, clock)) = time.split_once(['T', 't']) else {
        return false;
    };
    let spaced = |part: &str| part.chars().any(char::is_whitespace);
    if date.is_empty() || !clock.is_ascii() || spaced(date) || spaced(clock) {
        return false;
    }
    if let Some(clock) = clock.strip_suffix(['Z', 'z']) {
        return !clock.is_empty();
    }
    // `+hh:mm` or `-hh:mm`, after at least an hour.
    let Some(at) = clock.len().checked_sub(6).filter(|at| *at >= 2) else {
        return false;
    };
    let offset = &clock.as_bytes()[at..];
    matches!(offset[0], b'+' | b'-')
        && offset[3] == b':'
        && [1, 2, 4, 5].iter().all(|digit| offset[*digit].is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::has_offset;

    #[test]
    fn a_time_has_an_offset_when_it_ends_in_z_or_in_signed_hours_and_minutes() {
        for time in [
            "2026-10-12T09:00:00Z",
            "2026-10-12T09:00:00z",
            "2026-10-12t09:00:00Z",
            "2026-10-12T09:00:00.000Z",
            "2026-10-12T09:00:00-07:00",
            "2026-10-12T09:00:00+05:30",
            "2026-10-12T09:00:00.123456+00:00",
        ] {
            assert!(has_offset(time), "{time}");
        }
    }

    #[test]
    fn a_time_without_an_offset_is_told_apart_from_one_with_it() {
        for time in [
            "",
            " ",
            "2026-10-12",
            // The dashes of a date are not an offset.
            "2026-10-12T09:00:00",
            "2026-10-12T09:00:00.000",
            "2026-10-12T09:00",
            "2026-10-12 09:00:00",
            // Sent as it is given, so it has to be right as it is given.
            " 2026-10-12T09:00:00Z",
            "2026-10-12T09:00:00Z ",
            "2026-10-12T09:00:00Z\n",
            "2026-10-12 09:00:00Z",
            "2026-10-12T09:00:00 +02:00",
            "2026-10-12T",
            "2026-10-12TZ",
            "T09:00:00Z",
            "2026-10-12T09:00:00+0700",
            "2026-10-12T09:00:00-7:00",
            "2026-10-12T09:00:00 Europe/Zurich",
            "tomorrow at nine",
            "2026-10-12T09:00:00+ab:cd",
            // Not ASCII, so not a time.
            "2026-10-12T09:00:00é07:00",
        ] {
            assert!(!has_offset(time), "{time:?}");
        }
    }
}
