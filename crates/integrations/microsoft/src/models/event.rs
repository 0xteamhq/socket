//! Events: what Graph returns for them, and the content used to create, change and answer them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::nullable::{given, nullable};
use super::{DateTimeTimeZone, EmailAddress, ItemBody, Recipient, TimeSlot};

/// An event in a calendar.
///
/// `start` and `end` are in UTC, because Socket asks Graph for that. The zone
/// the event was made in is kept beside them, in `original_start_time_zone`
/// and `original_end_time_zone`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    #[serde(deserialize_with = "nullable")]
    pub subject: String,
    /// The first characters of the body, as plain text.
    #[serde(deserialize_with = "nullable")]
    pub body_preview: String,
    pub body: Option<ItemBody>,
    pub start: Option<DateTimeTimeZone>,
    pub end: Option<DateTimeTimeZone>,
    /// The zone the start was set in, such as `Pacific Standard Time`.
    pub original_start_time_zone: Option<String>,
    pub original_end_time_zone: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub is_all_day: bool,
    #[serde(deserialize_with = "nullable")]
    pub is_cancelled: bool,
    /// Whether the account itself organised the event.
    #[serde(deserialize_with = "nullable")]
    pub is_organizer: bool,
    pub organizer: Option<Recipient>,
    #[serde(deserialize_with = "nullable")]
    pub attendees: Vec<Attendee>,
    /// The account's own answer to the invitation.
    pub response_status: Option<ResponseStatus>,
    pub location: Option<Location>,
    #[serde(deserialize_with = "nullable")]
    pub is_online_meeting: bool,
    /// `teamsForBusiness`, `skypeForBusiness`, `skypeForConsumer` or `unknown`.
    pub online_meeting_provider: Option<String>,
    /// How to join, when the event is an online meeting. For a Teams meeting
    /// the join link is also how its transcript is found.
    pub online_meeting: Option<OnlineMeetingInfo>,
    /// The id of the series this event is one occurrence of.
    pub series_master_id: Option<String>,
    /// `singleInstance`, `occurrence`, `exception` or `seriesMaster`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// `free`, `tentative`, `busy`, `oof`, `workingElsewhere` or `unknown`.
    pub show_as: Option<String>,
    /// The address that opens the event in Outlook on the web.
    pub web_link: Option<String>,
    /// The same on every attendee's copy of the event, where `id` is not.
    #[serde(rename = "iCalUId")]
    pub ical_uid: Option<String>,
    /// How the event repeats, as Graph sent it.
    pub recurrence: Option<Value>,
}

/// Someone invited to an event, and what they answered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Attendee {
    /// `required`, `optional` or `resource`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub email_address: EmailAddress,
    /// Their answer. Graph fills it in. It is left out of what is sent, so a
    /// list that was read can be sent back with a change.
    pub status: Option<ResponseStatus>,
}

impl Attendee {
    /// Someone who has to be there.
    pub fn required(address: impl Into<String>) -> Self {
        Self::of_kind("required", address)
    }

    /// Someone who may come.
    pub fn optional(address: impl Into<String>) -> Self {
        Self::of_kind("optional", address)
    }

    fn of_kind(kind: &str, address: impl Into<String>) -> Self {
        Self {
            kind: Some(kind.into()),
            email_address: EmailAddress::new(address),
            status: None,
        }
    }
}

/// An answer to an invitation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ResponseStatus {
    /// `none`, `organizer`, `tentativelyAccepted`, `accepted`, `declined` or `notResponded`.
    pub response: Option<String>,
    /// When the answer was given, in UTC.
    pub time: Option<String>,
}

/// Where an event takes place.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Location {
    #[serde(deserialize_with = "nullable")]
    pub display_name: String,
    /// `default`, `conferenceRoom`, `homeAddress` and so on.
    pub location_type: Option<String>,
    /// The address of a room's own mailbox.
    pub location_email_address: Option<String>,
}

impl Location {
    pub fn named(display_name: impl Into<String>) -> Self {
        Self {
            display_name: display_name.into(),
            ..Self::default()
        }
    }
}

/// How to join an online meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct OnlineMeetingInfo {
    pub join_url: Option<String>,
    pub conference_id: Option<String>,
    pub toll_number: Option<String>,
}

/// A new event. Only its times are required.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateEvent {
    pub subject: Option<String>,
    pub body: Option<ItemBody>,
    pub start: DateTimeTimeZone,
    pub end: DateTimeTimeZone,
    pub location: Option<Location>,
    /// Graph sends each of them an invitation.
    pub attendees: Option<Vec<Attendee>>,
    pub is_all_day: Option<bool>,
    /// Make it an online meeting. The event that comes back carries the join
    /// link in `onlineMeeting.joinUrl`.
    pub is_online_meeting: Option<bool>,
    /// Which service hosts the online meeting. `teamsForBusiness` when an
    /// online meeting is asked for and this is not set.
    pub online_meeting_provider: Option<String>,
    /// `free`, `tentative`, `busy`, `oof` or `workingElsewhere`.
    pub show_as: Option<String>,
    /// `normal`, `personal`, `private` or `confidential`.
    pub sensitivity: Option<String>,
    /// How the event repeats, in Graph's own shape.
    pub recurrence: Option<Value>,
    /// An id of the caller's choosing. Graph does not create a second event
    /// for one it has already seen, which makes trying again safe.
    pub transaction_id: Option<String>,
}

impl CreateEvent {
    /// An event from `start` to `end`, with nothing else set.
    pub fn between(start: DateTimeTimeZone, end: DateTimeTimeZone) -> Self {
        Self {
            start,
            end,
            ..Self::default()
        }
    }
}

/// What to change on an existing event. What is left unset is left as it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEvent {
    pub subject: Option<String>,
    /// For an online meeting, keep the part of the body that holds the join
    /// details, or the meeting stops being one.
    pub body: Option<ItemBody>,
    pub start: Option<DateTimeTimeZone>,
    pub end: Option<DateTimeTimeZone>,
    pub location: Option<Location>,
    /// The whole list: an attendee left out is removed from the event.
    pub attendees: Option<Vec<Attendee>>,
    pub is_all_day: Option<bool>,
    pub is_online_meeting: Option<bool>,
    pub online_meeting_provider: Option<String>,
    pub show_as: Option<String>,
    pub sensitivity: Option<String>,
    /// How the event repeats, in Graph's own shape. `null` makes it stop
    /// repeating; leaving this out leaves it as it is.
    #[serde(default, deserialize_with = "given")]
    pub recurrence: Option<Value>,
}

/// An answer to an invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventResponse {
    Accept,
    TentativelyAccept,
    Decline,
}

/// What goes with an answer to an invitation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RespondToEvent {
    /// Text the organiser sees with the answer.
    pub comment: Option<String>,
    /// Whether the organiser is told. Graph tells them unless this is `false`.
    pub send_response: Option<bool>,
    /// Another time to suggest, with a decline or a tentative acceptance.
    pub proposed_new_time: Option<TimeSlot>,
}

/// What goes with cancelling a meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CancelEvent {
    /// Text the attendees see with the cancellation.
    pub comment: Option<String>,
}
