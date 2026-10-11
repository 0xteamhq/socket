//! Which events, and which occurrences of one, a list returns.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which events to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EventFilter {
    /// Only events that end after this time. RFC 3339 with its offset: `2026-10-12T00:00:00Z`.
    pub time_min: Option<String>,
    /// Only events that start before this time. RFC 3339 with its offset.
    /// Later than `timeMin` when both are given.
    pub time_max: Option<String>,
    /// Free text, matched against titles, descriptions, locations and the people on an event.
    pub q: Option<String>,
    /// Return each occurrence of a recurring event as its own event, and not the series.
    pub single_events: Option<bool>,
    /// `startTime`, which needs `singleEvents` to be true, or `updated`. Both are oldest first.
    pub order_by: Option<String>,
    /// Only events changed after this time. RFC 3339 with its offset.
    pub updated_min: Option<String>,
    /// Include cancelled events.
    pub show_deleted: Option<bool>,
    /// The time zone of the times in the answer. Google's default is the calendar's.
    pub time_zone: Option<String>,
}

/// Which occurrences of a recurring event to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EventInstancesFilter {
    /// Only occurrences that end after this time. RFC 3339 with its offset.
    pub time_min: Option<String>,
    /// Only occurrences that start before this time. RFC 3339 with its offset.
    pub time_max: Option<String>,
    /// Include occurrences that were cancelled.
    pub show_deleted: Option<bool>,
    /// The time zone of the times in the answer. Google's default is the calendar's.
    pub time_zone: Option<String>,
}
