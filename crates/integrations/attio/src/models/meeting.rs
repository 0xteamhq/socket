//! Meetings: what Attio knows of them from calendars and integrations.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Actor;
use super::nullable::nullable;

/// A meeting, with who was invited and the records it is linked to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Meeting {
    pub id: MeetingId,
    #[serde(deserialize_with = "nullable")]
    pub title: String,
    #[serde(deserialize_with = "nullable")]
    pub description: String,
    #[serde(deserialize_with = "nullable")]
    pub is_all_day: bool,
    pub start: Option<MeetingTime>,
    pub end: Option<MeetingTime>,
    #[serde(deserialize_with = "nullable")]
    pub participants: Vec<Participant>,
    #[serde(deserialize_with = "nullable")]
    pub linked_records: Vec<MeetingRecord>,
    pub created_by_actor: Option<Actor>,
    pub created_at: Option<String>,
}

/// A meeting's id, with the workspace it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct MeetingId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    #[serde(deserialize_with = "nullable")]
    pub meeting_id: String,
}

/// When a meeting starts or ends: a time with its zone, or a date for a
/// meeting that lasts all day.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct MeetingTime {
    /// The time, in ISO 8601. Absent for an all-day meeting.
    pub datetime: Option<String>,
    /// The zone the time was set in, such as `Europe/London`.
    pub timezone: Option<String>,
    /// The date, for an all-day meeting.
    pub date: Option<String>,
}

/// Someone invited to a meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Participant {
    /// `accepted`, `tentative`, `declined` or `pending`.
    pub status: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub is_organizer: bool,
    pub email_address: Option<String>,
    pub name: Option<String>,
}

/// A record a meeting is linked to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct MeetingRecord {
    pub object_slug: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub object_id: String,
    #[serde(deserialize_with = "nullable")]
    pub record_id: String,
}

/// The order meetings are listed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MeetingSort {
    StartAsc,
    StartDesc,
}

impl MeetingSort {
    /// The name Attio knows the order by.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StartAsc => "start_asc",
            Self::StartDesc => "start_desc",
        }
    }
}

/// Which meetings to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListMeetings {
    /// The slug or id of an object, to list the meetings linked to one of
    /// its records. Given together with `linked_record_id`.
    pub linked_object: Option<String>,
    /// The id of the record whose meetings to list. Given together with `linked_object`.
    pub linked_record_id: Option<String>,
    /// Email addresses: meetings that any of them was invited to. With a
    /// linked record as well, meetings that match either are returned.
    pub participants: Option<Vec<String>>,
    /// Only meetings that end at or after this time, in ISO 8601.
    pub ends_from: Option<String>,
    /// Only meetings that start before this time, in ISO 8601.
    pub starts_before: Option<String>,
    /// The zone `ends_from` and `starts_before` are read in for all-day
    /// meetings. UTC when not given.
    pub timezone: Option<String>,
    /// The order. Earliest first when not given.
    pub sort: Option<MeetingSort>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most meetings to return, from 1 to 200. Attio returns 50 when not given.
    pub limit: Option<u32>,
}
