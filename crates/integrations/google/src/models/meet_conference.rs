//! Conference records: one for each time a meeting was held in a Meet space.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One meeting that was held. A space that is met in every week has a
/// conference record for each week.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ConferenceRecord {
    /// `conferenceRecords/{id}`. Pass it as it is wherever a conference
    /// record is asked for.
    pub name: String,
    /// When the meeting began, in UTC: `2026-10-12T16:00:00Z`.
    pub start_time: Option<String>,
    /// When it ended. Absent while it is still going on.
    pub end_time: Option<String>,
    /// When Google deletes this record, with its participants and transcript
    /// entries: 30 days after the meeting ended.
    pub expire_time: Option<String>,
    /// The space it was held in, `spaces/{id}`, as `meet_spaces.get` takes it.
    pub space: Option<String>,
}

/// Which conference records to list. Google lists only the meetings the
/// account organised, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MeetListConferenceRecords {
    /// Only meetings held under this meeting code: `abc-mnop-xyz`, or the
    /// link that ends in it, `https://meet.google.com/abc-mnop-xyz`. Not to
    /// be given with `space`.
    pub meeting_code: Option<String>,
    /// Only meetings held in this space: its name, `spaces/{id}`, or its id.
    /// A meeting code is not a space id; give that as `meetingCode`.
    pub space: Option<String>,
    /// Only meetings that began at or after this time, in RFC 3339:
    /// `2026-10-01T00:00:00Z`.
    pub start_time_min: Option<String>,
    /// Only meetings that began at or before this time, in RFC 3339.
    pub start_time_max: Option<String>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most records to return in one page, from 1 to 100. Google returns 25 when not given.
    pub limit: Option<u32>,
}
