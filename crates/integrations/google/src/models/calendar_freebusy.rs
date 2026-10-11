//! When calendars are busy.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The window to check.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FreeBusyQuery {
    /// The start of the window. RFC 3339 with its offset: `2026-10-12T00:00:00Z`.
    pub time_min: String,
    /// The end of the window. RFC 3339 with its offset.
    pub time_max: String,
    /// The time zone of the times in the answer. Google's default is UTC.
    pub time_zone: Option<String>,
}

impl FreeBusyQuery {
    pub fn between(time_min: impl Into<String>, time_max: impl Into<String>) -> Self {
        Self {
            time_min: time_min.into(),
            time_max: time_max.into(),
            time_zone: None,
        }
    }
}

/// When each calendar asked about is busy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct FreeBusy {
    pub time_min: Option<String>,
    pub time_max: Option<String>,
    /// One entry per calendar, under the id it was asked about by.
    pub calendars: BTreeMap<String, FreeBusyCalendar>,
}

/// One calendar's busy periods.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct FreeBusyCalendar {
    pub busy: Vec<FreeBusyPeriod>,
    /// Why Google could not answer for this calendar. When this is not empty,
    /// an empty `busy` does not mean the calendar is free.
    pub errors: Vec<FreeBusyError>,
}

/// A stretch of time in which a calendar is busy. One that came without
/// its start or its end is not read as one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FreeBusyPeriod {
    pub start: String,
    /// Exclusive.
    pub end: String,
}

/// Why Google could not report on a calendar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct FreeBusyError {
    pub domain: String,
    /// `notFound` when the calendar does not exist or the account may not
    /// see it; `tooManyCalendarsRequested`; `internalError`; and others
    /// Google may add.
    pub reason: String,
}
