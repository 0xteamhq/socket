//! Recordings of the calls held in a meeting.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::transcript::SentTranscript;
use super::{Actor, Transcript};

/// The id of a call recording.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CallRecordingId {
    pub workspace_id: String,
    pub meeting_id: String,
    pub call_recording_id: String,
}

/// A recording as a list returns it: what it is and whether it is ready,
/// without what was said. `call_recordings.get` returns the transcript.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CallRecordingRow {
    pub id: CallRecordingId,
    /// `processing` until Attio has read the recording, then `completed` or `failed`.
    pub status: Option<String>,
    /// The address that opens the recording in Attio.
    pub web_url: Option<String>,
    pub created_by_actor: Option<Actor>,
    pub created_at: Option<String>,
}

/// One recording, with what was said in it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CallRecording {
    pub id: CallRecordingId,
    /// `processing` until Attio has read the recording, then `completed` or `failed`.
    pub status: Option<String>,
    /// The address that opens the recording in Attio.
    pub web_url: Option<String>,
    pub created_by_actor: Option<Actor>,
    pub created_at: Option<String>,
    /// A signed address the video can be downloaded from for about an hour
    /// after this was read. Only a recording Attio's own recorder made has
    /// one. It works by itself: anyone who holds it can download the video
    /// without a token, so treat it as a secret for as long as it lasts. It
    /// is part of what this read returns, and so ends up wherever the result
    /// does, a log or an agent's context included.
    pub video_url: Option<String>,
    /// What was said. Absent while the recording is being read, and when it
    /// has no transcript.
    pub transcript: Option<Transcript>,
}

/// A recording as Attio sends it, with the transcript in Attio's own shape.
/// The client reads this and turns it into a [`CallRecording`], so that the
/// public type is written and read the same way.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub(crate) struct SentCallRecording {
    id: CallRecordingId,
    status: Option<String>,
    web_url: Option<String>,
    created_by_actor: Option<Actor>,
    created_at: Option<String>,
    video_url: Option<String>,
    transcript: Option<SentTranscript>,
}

impl From<SentCallRecording> for CallRecording {
    fn from(sent: SentCallRecording) -> Self {
        Self {
            id: sent.id,
            status: sent.status,
            web_url: sent.web_url,
            created_by_actor: sent.created_by_actor,
            created_at: sent.created_at,
            video_url: sent.video_url,
            transcript: sent.transcript.map(Transcript::from),
        }
    }
}
