//! The people in a meeting, and each time one of them was connected.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Someone who was in a meeting. Exactly one of `signedinUser`,
/// `anonymousUser` and `phoneUser` says who.
///
/// A person who left and came back is still one participant, with a session
/// for each time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetParticipant {
    /// `conferenceRecords/{id}/participants/{id}`. A transcript entry names
    /// its speaker by this.
    pub name: String,
    /// When the participant first joined.
    pub earliest_start_time: Option<String>,
    /// When the participant left for the last time. Absent while still there.
    pub latest_end_time: Option<String>,
    /// Someone signed in to a Google account, or a meeting room's device.
    pub signedin_user: Option<MeetSignedinUser>,
    /// Someone who joined without signing in, under a name they typed.
    pub anonymous_user: Option<MeetAnonymousUser>,
    /// Someone who dialled in by phone.
    pub phone_user: Option<MeetPhoneUser>,
}

impl MeetParticipant {
    /// The name the participant was shown under, whichever kind of
    /// participant it is. `None` when Google withholds it.
    pub fn display_name(&self) -> Option<&str> {
        let signed_in = self.signedin_user.as_ref().map(|user| &user.display_name);
        let anonymous = self.anonymous_user.as_ref().map(|user| &user.display_name);
        let phone = self.phone_user.as_ref().map(|user| &user.display_name);
        [signed_in, anonymous, phone]
            .into_iter()
            .flatten()
            .flatten()
            .map(|name| name.trim())
            .find(|name| !name.is_empty())
    }
}

/// A participant signed in to a Google account.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetSignedinUser {
    /// `users/{id}`, the same id the Admin SDK and the People API use.
    pub user: Option<String>,
    /// The person's first and last name, or the name an administrator gave
    /// a room's device.
    pub display_name: Option<String>,
}

/// A participant who was not signed in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetAnonymousUser {
    /// The name the person typed when joining. Nobody checked it.
    pub display_name: Option<String>,
}

/// A participant who dialled in by phone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetPhoneUser {
    /// The phone number, with part of it hidden.
    pub display_name: Option<String>,
}

/// One time a participant was connected, from one device.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetParticipantSession {
    /// `conferenceRecords/{id}/participants/{id}/participantSessions/{id}`.
    pub name: String,
    /// When the participant joined.
    pub start_time: Option<String>,
    /// When the participant left. Absent while still connected.
    pub end_time: Option<String>,
}
