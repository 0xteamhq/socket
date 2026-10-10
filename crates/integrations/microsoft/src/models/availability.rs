//! Availability: when people are free, and when a meeting could be held.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::nullable::nullable;
use super::{Attendee, DateTimeTimeZone, Location, TimeSlot};

/// What a meeting needs, for Graph to suggest times for it. Everything is
/// optional: with nothing set Graph looks at the account's own calendar.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FindMeetingTimes {
    pub attendees: Option<Vec<Attendee>>,
    /// When the meeting may be held.
    pub time_constraint: Option<TimeConstraint>,
    /// How long the meeting is, as an ISO 8601 duration such as `PT1H`. Half an hour when not set.
    pub meeting_duration: Option<String>,
    /// The most suggestions to return.
    pub max_candidates: Option<u32>,
    /// Whether the meeting can go ahead without the account itself.
    pub is_organizer_optional: Option<bool>,
    /// The share of attendees, from 0 to 100, who must be free. Half when not set.
    pub minimum_attendee_percentage: Option<f64>,
    /// Ask Graph to say why it suggests each time.
    pub return_suggestion_reasons: Option<bool>,
}

/// When a meeting may be held.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct TimeConstraint {
    /// `work` (working hours, the default), `personal`, or `unrestricted`.
    pub activity_domain: Option<String>,
    pub time_slots: Vec<TimeSlot>,
}

/// The times Graph suggests for a meeting.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetingTimeSuggestions {
    /// Why there are no suggestions, such as `attendeesUnavailable`. Empty when there are some.
    #[serde(deserialize_with = "nullable")]
    pub empty_suggestions_reason: String,
    #[serde(deserialize_with = "nullable")]
    pub meeting_time_suggestions: Vec<MeetingTimeSuggestion>,
}

/// One time a meeting could be held.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetingTimeSuggestion {
    /// The chance, from 0 to 100, that everyone can attend.
    pub confidence: Option<f64>,
    /// Where this suggestion ranks; 1 is the best.
    pub order: Option<i32>,
    pub organizer_availability: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub attendee_availability: Vec<AttendeeAvailability>,
    pub meeting_time_slot: Option<TimeSlot>,
    #[serde(deserialize_with = "nullable")]
    pub locations: Vec<Location>,
    pub suggestion_reason: Option<String>,
}

/// Whether one attendee is free at a suggested time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AttendeeAvailability {
    /// `free`, `tentative`, `busy`, `oof`, `workingElsewhere` or `unknown`.
    pub availability: Option<String>,
    pub attendee: Option<Attendee>,
}

/// Whose free and busy times to read, and for when.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetSchedule {
    /// The email addresses of people, distribution lists or rooms. Graph reads at most 20 at once.
    pub schedules: Vec<String>,
    pub start_time: DateTimeTimeZone,
    /// Less than 62 days after `start_time`.
    pub end_time: DateTimeTimeZone,
    /// The length in minutes of each slot of `availabilityView`, from 5 to 1440. Half an hour when not set.
    pub availability_view_interval: Option<u32>,
}

/// One person's or room's free and busy times.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ScheduleInformation {
    /// The address that was asked about.
    #[serde(deserialize_with = "nullable")]
    pub schedule_id: String,
    /// One digit per slot: `0` free or working elsewhere, `1` tentative, `2` busy, `3` out of office.
    #[serde(deserialize_with = "nullable")]
    pub availability_view: String,
    #[serde(deserialize_with = "nullable")]
    pub schedule_items: Vec<ScheduleItem>,
    /// The hours the person works, as Graph sent them.
    pub working_hours: Option<Value>,
    /// Why this schedule could not be read, when it could not.
    pub error: Option<ScheduleError>,
}

/// One thing in someone's calendar. What it is about is present only when
/// the account may see it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ScheduleItem {
    /// `free`, `tentative`, `busy`, `oof`, `workingElsewhere` or `unknown`.
    pub status: Option<String>,
    pub start: Option<DateTimeTimeZone>,
    pub end: Option<DateTimeTimeZone>,
    pub subject: Option<String>,
    pub location: Option<String>,
    pub is_private: Option<bool>,
}

/// Why one schedule could not be read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ScheduleError {
    pub message: Option<String>,
    pub response_code: Option<String>,
}
