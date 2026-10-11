//! The people invited to an event, and their answers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Someone invited to an event, and what they answered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EventAttendee {
    pub email: String,
    pub display_name: Option<String>,
    /// `needsAction`, `declined`, `tentative` or `accepted`.
    pub response_status: String,
    /// The note the attendee left with their answer.
    pub comment: Option<String>,
    /// True when their attendance is optional.
    pub optional: bool,
    /// True for the event's organiser.
    pub organizer: bool,
    /// True for the entry of the calendar this copy of the event is on. On
    /// `primary` that is the signed-in person.
    #[serde(rename = "self")]
    pub is_self: bool,
    /// True for a room or a piece of equipment.
    pub resource: bool,
    /// How many people the attendee is bringing.
    pub additional_guests: Option<u32>,
}

/// Someone to invite to an event.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventInvitee {
    /// The person's email address.
    pub email: String,
    pub display_name: Option<String>,
    /// True when their attendance is optional.
    pub optional: Option<bool>,
    /// Their answer, to set it. Leave unset: someone new has not answered,
    /// and a guest who stays on a list that is being replaced keeps the
    /// answer they gave.
    pub response_status: Option<String>,
}

impl EventInvitee {
    pub fn email(email: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            ..Self::default()
        }
    }
}

/// An answer to an invitation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventResponse {
    /// `accepted`, `declined`, `tentative`, or `needsAction` to take an answer back.
    pub response_status: String,
    /// A note the organiser sees beside the answer.
    pub comment: Option<String>,
    /// Who is emailed about the answer: `all`, `externalOnly` or `none`.
    /// Unset, Google emails nobody.
    pub send_updates: Option<String>,
}

impl EventResponse {
    fn with(response_status: &str) -> Self {
        Self {
            response_status: response_status.to_owned(),
            ..Self::default()
        }
    }

    pub fn accept() -> Self {
        Self::with("accepted")
    }

    pub fn decline() -> Self {
        Self::with("declined")
    }

    pub fn tentative() -> Self {
        Self::with("tentative")
    }

    pub fn comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }
}
