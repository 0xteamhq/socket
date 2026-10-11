//! Events: what Google returns for them, and the content used to create, change and delete them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{EventAttendee, EventConference, EventInvitee, EventTime};

/// An event on a calendar.
///
/// An instance that was removed from a recurring series arrives with little
/// more than `id`, `status` and `original_start_time`, which is why most
/// fields are optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct CalendarEvent {
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
    pub creator: Option<EventPerson>,
    pub organizer: Option<EventPerson>,
    pub attendees: Vec<EventAttendee>,
    /// True when Google left people out of `attendees`.
    pub attendees_omitted: bool,
    /// False when the organiser hid the guest list: `attendees` then holds
    /// only the calendar's own entry. Google's default is true.
    pub guests_can_see_other_guests: Option<bool>,
    /// The Google Meet link, when the event has one.
    pub hangout_link: Option<String>,
    /// The conference in full: its meeting code and every way to join.
    pub conference_data: Option<EventConference>,
    /// Files attached to the event. Google Meet attaches a meeting's
    /// recording, transcript and notes here once they exist.
    pub attachments: Vec<EventAttachment>,
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

/// The person who created or organises an event.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EventPerson {
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
pub struct EventAttachment {
    /// Where the file opens.
    pub file_url: String,
    pub title: Option<String>,
    pub mime_type: Option<String>,
    pub icon_link: Option<String>,
    /// For a Google Drive file, its Drive id.
    pub file_id: Option<String>,
}

/// A new event. Google refuses one without a start and an end.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventInsert {
    /// The event's title.
    pub summary: Option<String>,
    /// May contain HTML.
    pub description: Option<String>,
    /// Where it takes place, as free text.
    pub location: Option<String>,
    /// When it starts: `dateTime`, or `date` for an all-day event.
    pub start: EventTime,
    /// When it ends. Exclusive: an all-day event on the 12th ends on the 13th.
    pub end: EventTime,
    /// The people to invite.
    pub attendees: Option<Vec<EventInvitee>>,
    /// `RRULE` lines that make the event repeat, such as
    /// `RRULE:FREQ=WEEKLY;COUNT=10`. The start and end then need a `timeZone`.
    pub recurrence: Option<Vec<String>>,
    /// Asks Google to create a Google Meet link for the event.
    pub create_meet_link: Option<bool>,
    /// Who is emailed an invitation: `all`, `externalOnly` or `none`. Unset,
    /// Google emails nobody.
    pub send_updates: Option<String>,
}

impl EventInsert {
    /// An event from `start` to `end`, with nothing else set.
    pub fn between(start: EventTime, end: EventTime) -> Self {
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
            .push(EventInvitee::email(email));
        self
    }

    /// Asks Google to create a Google Meet link for the event.
    pub fn with_meet_link(mut self) -> Self {
        self.create_meet_link = Some(true);
        self
    }
}

/// Changes to an event. What is left unset is left as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EventPatch {
    /// The event's title.
    pub summary: Option<String>,
    /// May contain HTML.
    pub description: Option<String>,
    /// Where it takes place, as free text.
    pub location: Option<String>,
    /// When it starts: `dateTime`, or `date` for an all-day event.
    pub start: Option<EventTime>,
    /// When it ends. Exclusive.
    pub end: Option<EventTime>,
    /// The whole guest list: anyone left out is uninvited. Give a guest who
    /// stays their `responseStatus`, so their answer is kept.
    pub attendees: Option<Vec<EventInvitee>>,
    /// Asks Google to create a Google Meet link for the event.
    pub create_meet_link: Option<bool>,
    /// Who is emailed about the change: `all`, `externalOnly` or `none`.
    /// Unset, Google emails nobody.
    pub send_updates: Option<String>,
}

/// How to delete an event.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EventDelete {
    /// Who is emailed the cancellation: `all`, `externalOnly` or `none`.
    /// Unset, Google emails nobody.
    pub send_updates: Option<String>,
}
