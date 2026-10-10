//! Events: what Google returns for them, and the content and filters used to create, change and list them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Attendee, ConferenceData, NewAttendee};

/// An event on a calendar.
///
/// An instance that was removed from a recurring series arrives with little
/// more than `id`, `status` and `original_start_time`, which is why most
/// fields are optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    /// `confirmed`, `tentative` or `cancelled`.
    pub status: Option<String>,
    /// The event's page in Google Calendar.
    pub html_link: Option<String>,
    /// The event's title.
    pub summary: Option<String>,
    /// May contain HTML.
    pub description: Option<String>,
    pub location: Option<String>,
    pub start: Option<EventTime>,
    /// Exclusive: an all-day event on the 12th ends on the 13th.
    pub end: Option<EventTime>,
    pub creator: Option<Person>,
    pub organizer: Option<Person>,
    pub attendees: Vec<Attendee>,
    /// True when Google left people out of `attendees`.
    pub attendees_omitted: bool,
    /// The Google Meet link, when the event has one.
    pub hangout_link: Option<String>,
    /// The conference in full: its meeting code and every way to join.
    pub conference_data: Option<ConferenceData>,
    /// Files attached to the event. Google Meet attaches a meeting's
    /// recording, transcript and notes here once they exist.
    pub attachments: Vec<Attachment>,
    /// The `RRULE`, `EXDATE` and similar lines of a recurring event.
    pub recurrence: Vec<String>,
    /// For one instance of a recurring event, the id of the series.
    pub recurring_event_id: Option<String>,
    /// For one instance of a recurring event, when the series put it.
    pub original_start_time: Option<EventTime>,
    /// The event's id across calendar systems. Every instance of a series shares it.
    #[serde(rename = "iCalUID")]
    pub ical_uid: Option<String>,
    /// `default`, `outOfOffice`, `focusTime`, `workingLocation`, `birthday` or `fromGmail`.
    pub event_type: Option<String>,
    /// `transparent` when the event does not make the person busy.
    pub transparency: Option<String>,
    /// `default`, `public`, `private` or `confidential`.
    pub visibility: Option<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
}

/// When an event starts or ends: a time, or a date for an all-day event.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EventTime {
    /// Set for an all-day event: `2026-10-12`.
    pub date: Option<String>,
    /// Set for a timed event, in RFC 3339: `2026-10-12T09:00:00-07:00`. The
    /// offset may be left off when `time_zone` is given.
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

    /// A whole day: `2026-10-12`.
    pub fn all_day(date: impl Into<String>) -> Self {
        Self {
            date: Some(date.into()),
            ..Self::default()
        }
    }
}

/// The person who created or organises an event.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Person {
    pub id: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    /// True when this is the calendar this copy of the event is on. On
    /// `primary` that is the signed-in person.
    #[serde(rename = "self")]
    pub is_self: bool,
}

/// A file attached to an event.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Attachment {
    /// Where the file opens.
    pub file_url: String,
    pub title: Option<String>,
    pub mime_type: Option<String>,
    pub icon_link: Option<String>,
    /// For a Google Drive file, its Drive id.
    pub file_id: Option<String>,
}

/// Which events to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListEvents {
    /// Only events that end after this time. RFC 3339 with its offset: `2026-10-12T00:00:00Z`.
    pub time_min: Option<String>,
    /// Only events that start before this time. RFC 3339 with its offset.
    pub time_max: Option<String>,
    /// Free text, matched against titles, descriptions, locations and the people on an event.
    pub q: Option<String>,
    /// Return each occurrence of a recurring event as its own event, and not the series.
    pub single_events: Option<bool>,
    /// `startTime`, which needs `single_events`, or `updated`. Both are oldest first.
    pub order_by: Option<String>,
    /// Only events changed after this time. RFC 3339 with its offset.
    pub updated_min: Option<String>,
    /// Include cancelled events.
    pub show_deleted: Option<bool>,
    /// The time zone of the times in the answer. Google's default is the calendar's.
    pub time_zone: Option<String>,
    /// How many events per page, at most 2500. Google's default is 250, and
    /// a page may hold fewer than asked for even when more follow.
    pub max_results: Option<u32>,
    /// The `next_cursor` of the page before; absent for the first page.
    pub page_token: Option<String>,
}

/// Which occurrences of a recurring event to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Instances {
    /// Only occurrences that end after this time. RFC 3339 with its offset.
    pub time_min: Option<String>,
    /// Only occurrences that start before this time. RFC 3339 with its offset.
    pub time_max: Option<String>,
    /// Include occurrences that were cancelled.
    pub show_deleted: Option<bool>,
    /// The time zone of the times in the answer. Google's default is the calendar's.
    pub time_zone: Option<String>,
    /// How many occurrences per page, at most 2500. Google's default is 250.
    pub max_results: Option<u32>,
    /// The `next_cursor` of the page before; absent for the first page.
    pub page_token: Option<String>,
}

/// A new event. Google refuses one without a start and an end.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsertEvent {
    /// The event's title.
    pub summary: Option<String>,
    /// May contain HTML.
    pub description: Option<String>,
    pub location: Option<String>,
    pub start: EventTime,
    /// Exclusive: an all-day event on the 12th ends on the 13th.
    pub end: EventTime,
    pub attendees: Option<Vec<NewAttendee>>,
    /// `RRULE` lines that make the event repeat, such as
    /// `RRULE:FREQ=WEEKLY;COUNT=10`. The start and end then need a `timeZone`.
    pub recurrence: Option<Vec<String>>,
    /// Asks Google to create a Google Meet link for the event.
    pub create_meet_link: Option<bool>,
    /// Who is emailed an invitation: `all`, `externalOnly` or `none`. Unset,
    /// Google emails nobody.
    pub send_updates: Option<String>,
}

impl InsertEvent {
    pub fn new(start: EventTime, end: EventTime) -> Self {
        Self {
            start,
            end,
            ..Self::default()
        }
    }

    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Adds one person to the guest list.
    pub fn invite(mut self, email: impl Into<String>) -> Self {
        self.attendees
            .get_or_insert_with(Vec::new)
            .push(NewAttendee::email(email));
        self
    }

    /// Asks Google to create a Google Meet link for the event.
    pub fn with_meet_link(mut self) -> Self {
        self.create_meet_link = Some(true);
        self
    }
}

/// Changes to an event. Only the fields that are set are changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PatchEvent {
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start: Option<EventTime>,
    pub end: Option<EventTime>,
    /// Replaces the whole guest list: anyone left out is uninvited. Give a
    /// guest who stays their `responseStatus`, so their answer is kept.
    pub attendees: Option<Vec<NewAttendee>>,
    /// Asks Google to create a Google Meet link for the event.
    pub create_meet_link: Option<bool>,
    /// Who is emailed about the change: `all`, `externalOnly` or `none`.
    /// Unset, Google emails nobody.
    pub send_updates: Option<String>,
}

/// How to delete an event.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteEvent {
    /// Who is emailed the cancellation: `all`, `externalOnly` or `none`.
    /// Unset, Google emails nobody.
    pub send_updates: Option<String>,
}
