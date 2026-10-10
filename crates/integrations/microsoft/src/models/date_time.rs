//! A point in time as Graph writes it, and a stretch between two of them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// A date and time, and the zone it is in. Both are always given: a time
/// without its zone would have to be guessed at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DateTimeTimeZone {
    /// The date and time without an offset: `2026-10-12T09:00:00`. Graph
    /// answers with seven decimal places: `2026-10-12T09:00:00.0000000`.
    #[serde(deserialize_with = "nullable")]
    pub date_time: String,
    /// The zone `date_time` is in, as a Windows or IANA name such as `UTC`
    /// or `Pacific Standard Time`. It is `UTC` in everything Socket reads,
    /// because it asks Graph for UTC.
    #[serde(deserialize_with = "nullable")]
    pub time_zone: String,
}

impl DateTimeTimeZone {
    /// A time in UTC, written `2026-10-12T09:00:00`.
    pub fn utc(date_time: impl Into<String>) -> Self {
        Self::in_zone(date_time, "UTC")
    }

    /// A time in the zone named, such as `Pacific Standard Time`.
    pub fn in_zone(date_time: impl Into<String>, time_zone: impl Into<String>) -> Self {
        Self {
            date_time: date_time.into(),
            time_zone: time_zone.into(),
        }
    }

    /// Whether both the time and its zone are there.
    pub(crate) fn is_set(&self) -> bool {
        !self.date_time.trim().is_empty() && !self.time_zone.trim().is_empty()
    }
}

/// From one time to another.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimeSlot {
    pub start: DateTimeTimeZone,
    pub end: DateTimeTimeZone,
}
