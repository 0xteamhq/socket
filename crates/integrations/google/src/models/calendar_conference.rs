//! The video meeting attached to an event.
//!
//! This is how a meeting is found again afterwards: `conference_id` is the
//! Google Meet code that Meet's own records are looked up by.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The conference attached to an event, such as a Google Meet meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EventConference {
    /// For Google Meet, the meeting code, such as `abc-defg-hij`.
    pub conference_id: Option<String>,
    /// Which product hosts the conference. Unset while one is being created, or when creating it failed.
    pub conference_solution: Option<EventConferenceSolution>,
    /// The ways to join: a video link, phone numbers.
    pub entry_points: Vec<EventEntryPoint>,
    /// Present when the conference was asked for with the event; says how far Google has got.
    pub create_request: Option<EventConferenceRequest>,
    /// Extra joining instructions. May contain HTML.
    pub notes: Option<String>,
}

/// The product that hosts a conference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EventConferenceSolution {
    pub key: Option<EventConferenceSolutionKey>,
    /// What a person sees, such as `Google Meet`.
    pub name: Option<String>,
    pub icon_uri: Option<String>,
}

/// Which kind of conference this is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct EventConferenceSolutionKey {
    /// `hangoutsMeet` for Google Meet, `addOn` for another provider's.
    #[serde(rename = "type")]
    pub kind: String,
}

/// One way to join a conference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EventEntryPoint {
    /// `video`, `phone`, `sip` or `more`.
    pub entry_point_type: String,
    /// The link to open or the number to dial: `https:`, `tel:` or `sip:`.
    pub uri: Option<String>,
    pub label: Option<String>,
    pub pin: Option<String>,
    pub meeting_code: Option<String>,
    pub access_code: Option<String>,
    pub passcode: Option<String>,
    /// The country or region a phone number is for, as two letters: `US`.
    pub region_code: Option<String>,
    pub password: Option<String>,
}

/// A request to create a conference, as Google reports it back.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EventConferenceRequest {
    pub request_id: Option<String>,
    pub conference_solution_key: Option<EventConferenceSolutionKey>,
    pub status: Option<EventConferenceRequestStatus>,
}

/// How far Google has got with creating a conference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EventConferenceRequestStatus {
    /// `pending`, `success` or `failure`. While it is `pending` the event has
    /// no link yet; read the event again.
    pub status_code: String,
}
