//! Who joined a Teams online meeting, when, and for how long.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Identity;
use super::nullable::nullable;

/// The attendance of one sitting of a meeting. A meeting that was started
/// three times has three reports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AttendanceReport {
    pub id: String,
    pub meeting_start_date_time: Option<String>,
    pub meeting_end_date_time: Option<String>,
    pub total_participant_count: Option<i32>,
}

/// One person's attendance in a report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AttendanceRecord {
    pub id: Option<String>,
    pub email_address: Option<String>,
    pub identity: Option<Identity>,
    /// `None`, `Attendee`, `Presenter` or `Organizer`.
    pub role: Option<String>,
    pub total_attendance_in_seconds: Option<i32>,
    /// One entry for each time the person joined and left.
    #[serde(deserialize_with = "nullable")]
    pub attendance_intervals: Vec<AttendanceInterval>,
}

/// One stay in a meeting, from joining to leaving.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AttendanceInterval {
    pub join_date_time: Option<String>,
    pub leave_date_time: Option<String>,
    pub duration_in_seconds: Option<i32>,
}
