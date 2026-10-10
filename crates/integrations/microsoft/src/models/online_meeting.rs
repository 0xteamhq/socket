//! Teams online meetings.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::IdentitySet;
use super::nullable::null_as_default;

/// A Teams online meeting. Its `id` is what transcripts, recordings and
/// attendance reports are asked for by.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct OnlineMeeting {
    pub id: String,
    pub subject: Option<String>,
    pub start_date_time: Option<String>,
    pub end_date_time: Option<String>,
    pub creation_date_time: Option<String>,
    /// The link people join with. A calendar event carries the same link.
    pub join_web_url: Option<String>,
    /// `adhoc`, `scheduled`, `recurring`, `broadcast` and so on.
    pub meeting_type: Option<String>,
    /// Whether transcription may be switched on. A transcript exists only if someone did.
    pub allow_transcription: Option<bool>,
    pub allow_recording: Option<bool>,
    pub record_automatically: Option<bool>,
    pub chat_info: Option<ChatInfo>,
    pub participants: Option<MeetingParticipants>,
}

/// The chat that belongs to a meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ChatInfo {
    pub thread_id: Option<String>,
    pub message_id: Option<String>,
    pub reply_chain_message_id: Option<String>,
}

/// Who organised a meeting and who was invited to it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetingParticipants {
    pub organizer: Option<MeetingParticipant>,
    #[serde(deserialize_with = "null_as_default")]
    pub attendees: Vec<MeetingParticipant>,
}

/// One person in a meeting's invitation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetingParticipant {
    /// The person's sign-in name.
    pub upn: Option<String>,
    /// `attendee`, `presenter`, `producer` or `coorganizer`.
    pub role: Option<String>,
    pub identity: Option<IdentitySet>,
}
