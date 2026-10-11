//! When an event starts or ends.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// When an event starts or ends: a time, or a date for an all-day event.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EventTime {
    /// Set for an all-day event: `2026-10-12`.
    pub date: Option<String>,
    /// Set for a timed event, in RFC 3339: `2026-10-12T09:00:00-07:00`. The
    /// offset may be left off only when `timeZone` is given.
    pub date_time: Option<String>,
    /// An IANA name such as `Europe/Zurich`. A recurring event needs it.
    pub time_zone: Option<String>,
}

impl EventTime {
    /// A time, in RFC 3339 with its offset: `2026-10-12T09:00:00-07:00`.
    pub fn at(date_time: impl Into<String>) -> Self {
        Self {
            date_time: Some(date_time.into()),
            ..Self::default()
        }
    }

    /// A time as a clock in `time_zone` shows it: `2026-10-12T09:00:00` in
    /// `Europe/Zurich`. This is the form a recurring event needs.
    pub fn in_zone(date_time: impl Into<String>, time_zone: impl Into<String>) -> Self {
        Self {
            date_time: Some(date_time.into()),
            time_zone: Some(time_zone.into()),
            ..Self::default()
        }
    }

    /// A whole day: `2026-10-12`.
    pub fn all_day(date: impl Into<String>) -> Self {
        Self {
            date: Some(date.into()),
            ..Self::default()
        }
    }
}
