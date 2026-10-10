//! The video meeting attached to an event.
//!
//! This is how a meeting is found again afterwards: `conference_id` is the
//! Google Meet code that Meet's own records are looked up by.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The conference attached to an event, such as a Google Meet meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ConferenceData {
    /// For Google Meet, the meeting code, such as `abc-defg-hij`.
    pub conference_id: Option<String>,
    /// Which product hosts the conference. Unset while one is being created, or when creating it failed.
    pub conference_solution: Option<ConferenceSolution>,
    /// The ways to join: a video link, phone numbers.
    pub entry_points: Vec<EntryPoint>,
    /// Present when the conference was asked for with the event; says how far Google has got.
    pub create_request: Option<ConferenceRequest>,
    /// Extra joining instructions. May contain HTML.
    pub notes: Option<String>,
}

/// The product that hosts a conference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ConferenceSolution {
    pub key: Option<ConferenceSolutionKey>,
    /// What a person sees, such as `Google Meet`.
    pub name: Option<String>,
    pub icon_uri: Option<String>,
}

/// Which kind of conference this is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ConferenceSolutionKey {
    /// `hangoutsMeet` for Google Meet, `addOn` for another provider's.
    #[serde(rename = "type")]
    pub kind: String,
}

/// One way to join a conference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EntryPoint {
    /// `video`, `phone`, `sip` or `more`.
    pub entry_point_type: String,
    /// The link to open or the number to dial: `https:`, `tel:` or `sip:`.
    pub uri: Option<String>,
    pub label: Option<String>,
    pub pin: Option<String>,
    pub meeting_code: Option<String>,
    pub access_code: Option<String>,
    pub passcode: Option<String>,
    pub password: Option<String>,
}

/// A request to create a conference, as Google reports it back.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ConferenceRequest {
    pub request_id: Option<String>,
    pub conference_solution_key: Option<ConferenceSolutionKey>,
    pub status: Option<ConferenceRequestStatus>,
}

/// How far Google has got with creating a conference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ConferenceRequestStatus {
    /// `pending`, `success` or `failure`. While it is `pending` the event has
    /// no link yet; read the event again.
    pub status_code: String,
}
