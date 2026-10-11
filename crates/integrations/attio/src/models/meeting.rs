//! Meetings: calendar events Attio knows of, and the records they concern.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Actor;
use super::nullable::nullable;

/// The id of a meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct MeetingId {
    pub workspace_id: String,
    pub meeting_id: String,
}

/// A meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Meeting {
    pub id: MeetingId,
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub is_all_day: bool,
    pub start: Option<MeetingTime>,
    pub end: Option<MeetingTime>,
    #[serde(deserialize_with = "nullable")]
    pub participants: Vec<Participant>,
    /// The records the meeting concerns: the people in it, their companies.
    #[serde(deserialize_with = "nullable")]
    pub linked_records: Vec<MeetingRecord>,
    pub created_at: Option<String>,
    pub created_by_actor: Option<Actor>,
}

/// When a meeting starts or ends: a time, or for an all-day meeting, a date.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct MeetingTime {
    /// Set for a meeting at a time of day.
    pub datetime: Option<String>,
    /// The zone the meeting was made in, when Attio knows it.
    pub timezone: Option<String>,
    /// Set for an all-day meeting.
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

/// A record a meeting concerns.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct MeetingRecord {
    pub object_slug: Option<String>,
    pub object_id: Option<String>,
    pub record_id: Option<String>,
}

/// The order of a list of meetings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MeetingSort {
    /// Earliest first. Attio's order when none is asked for.
    StartAsc,
    StartDesc,
}

impl MeetingSort {
    /// The value Attio takes for it.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::StartAsc => "start_asc",
            Self::StartDesc => "start_desc",
        }
    }
}

/// Which meetings to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListMeetings {
    /// The object of a record the meetings concern. It needs `linked_record_id` beside it.
    pub linked_object: Option<String>,
    /// A record the meetings concern. It needs `linked_object` beside it.
    pub linked_record_id: Option<String>,
    /// Email addresses of people in the meetings. A meeting with any of
    /// them is returned, as is one that concerns the record named above.
    pub participants: Option<Vec<String>>,
    pub sort: Option<MeetingSort>,
    /// Only meetings that end at or after this time, in ISO 8601.
    pub ends_from: Option<String>,
    /// Only meetings that start before this time, in ISO 8601.
    pub starts_before: Option<String>,
    /// The zone in which the two times above are read for all-day meetings. UTC when not given.
    pub timezone: Option<String>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most meetings to return in one page, from 1 to 200. 50 when not given.
    pub limit: Option<u32>,
}
