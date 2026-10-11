//! Meeting spaces: the place a meeting code or a link leads to.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A place where meetings are held. One meeting at a time can be going on
/// in it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetSpace {
    /// `spaces/{id}`. The id is Google's own and does not change; a
    /// conference record names its space by this.
    pub name: String,
    /// The link people join by: `https://meet.google.com/abc-mnop-xyz`.
    pub meeting_uri: Option<String>,
    /// The code at the end of the link. It can come to mean another space
    /// once it has gone unused for about a year, so keep `name` and not this.
    pub meeting_code: Option<String>,
    /// How the space is set up.
    pub config: Option<MeetSpaceConfig>,
    /// The meeting going on in the space now. Absent when there is none.
    pub active_conference: Option<MeetActiveConference>,
}

/// How a space is set up.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetSpaceConfig {
    /// Who can join without asking: `OPEN`, `TRUSTED` or `RESTRICTED`.
    pub access_type: Option<String>,
    /// Where the meeting can be joined from: `ALL`, or `CREATOR_APP_ONLY`.
    pub entry_point_access: Option<String>,
    /// Whether the hosts moderate the meeting: `ON` or `OFF`.
    pub moderation: Option<String>,
    /// Whether an attendance report is made: `GENERATE_REPORT` or `DO_NOT_GENERATE`.
    pub attendance_report_generation_type: Option<String>,
    /// What the space records by itself when a meeting begins.
    pub artifact_config: Option<MeetArtifactConfig>,
}

/// What a space records without being asked. Each is `ON` or `OFF`; someone
/// in the meeting can still switch a recording or a transcript on by hand.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetArtifactConfig {
    pub recording_config: Option<MeetRecordingConfig>,
    pub transcription_config: Option<MeetTranscriptionConfig>,
    pub smart_notes_config: Option<MeetSmartNotesConfig>,
}

/// Whether a space records every meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetRecordingConfig {
    pub auto_recording_generation: Option<String>,
}

/// Whether a space transcribes every meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetTranscriptionConfig {
    pub auto_transcription_generation: Option<String>,
}

/// Whether Gemini takes notes in every meeting of a space.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetSmartNotesConfig {
    pub auto_smart_notes_generation: Option<String>,
}

/// The meeting going on in a space.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetActiveConference {
    /// `conferenceRecords/{id}`, as the conference record methods take it.
    pub conference_record: Option<String>,
}
