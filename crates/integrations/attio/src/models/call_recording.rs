//! Call recordings: the recording of a meeting, and what was said in it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Actor;
use super::nullable::nullable;

/// A recording of a meeting. One that is read on its own carries the
/// transcript; a list of recordings does not.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CallRecording {
    pub id: CallRecordingId,
    /// `processing`, `completed` or `failed`.
    pub status: Option<String>,
    /// The address that opens the recording in Attio.
    pub web_url: Option<String>,
    /// The address of the video, for a recording Attio's own recorder made;
    /// absent for one that was brought in through the API. The address stops
    /// working an hour after it was given; reading the recording again
    /// gives a fresh one.
    pub video_url: Option<String>,
    /// What was said. Absent in a list, and while the recording is processed.
    pub transcript: Option<Transcript>,
    pub created_by_actor: Option<Actor>,
    pub created_at: Option<String>,
}

/// A call recording's id, with the meeting and the workspace it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CallRecordingId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    #[serde(deserialize_with = "nullable")]
    pub meeting_id: String,
    #[serde(deserialize_with = "nullable")]
    pub call_recording_id: String,
}

/// What was said in a recording.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Transcript {
    /// One entry for each thing said, in order.
    #[serde(deserialize_with = "nullable")]
    pub segments: Vec<TranscriptSegment>,
    /// The whole transcript as text, each line with its time and speaker.
    #[serde(deserialize_with = "nullable")]
    pub raw_transcript: String,
}

/// One thing said: who said it, and from when to when.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TranscriptSegment {
    #[serde(deserialize_with = "nullable")]
    pub speech: String,
    /// Seconds from the start of the recording.
    #[serde(deserialize_with = "nullable")]
    pub start_time: f64,
    /// Seconds from the start of the recording.
    #[serde(deserialize_with = "nullable")]
    pub end_time: f64,
    pub speaker: Option<Speaker>,
}

/// Who said something.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Speaker {
    pub name: Option<String>,
}
