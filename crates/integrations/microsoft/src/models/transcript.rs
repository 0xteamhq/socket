//! Transcripts of Teams online meetings.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use socketkit_core::Result;

use super::{IdentitySet, webvtt};

/// One transcript of a meeting. A meeting transcribed twice has two.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Transcript {
    pub id: String,
    pub meeting_id: Option<String>,
    pub call_id: Option<String>,
    /// The same value on the recording this transcript was made from.
    pub content_correlation_id: Option<String>,
    pub created_date_time: Option<String>,
    pub end_date_time: Option<String>,
    pub transcript_content_url: Option<String>,
    pub meeting_organizer: Option<IdentitySet>,
}

/// What was said in a meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct TranscriptContent {
    /// The transcript exactly as Microsoft sent it, in WebVTT.
    pub text: String,
    /// The same transcript, one entry for each thing someone said.
    pub entries: Vec<TranscriptEntry>,
}

/// One thing someone said.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct TranscriptEntry {
    /// Who spoke. Absent when the transcript does not say.
    pub speaker: Option<String>,
    /// When it began, in milliseconds from the start of the transcript.
    /// Negative when transcription was switched on mid-conversation.
    pub start_ms: i64,
    /// When it ended, in milliseconds from the start of the transcript.
    pub end_ms: i64,
    pub text: String,
}

impl TranscriptContent {
    /// Reads a WebVTT transcript as Teams writes it: one cue for each thing
    /// said, with the speaker in a voice tag (`<v Ada Lovelace>…</v>`).
    ///
    /// Notes, styles and cue identifiers are skipped, and markup inside a cue
    /// is removed. A cue whose timing cannot be read, or text that is not a
    /// cue at all, is an error: leaving it out would return a transcript that
    /// looks whole and is not. The error never repeats what was said.
    ///
    /// The text is read once, line by line, and nothing of it is held but
    /// the cue being read.
    pub fn from_vtt(text: &str) -> Result<Self> {
        Ok(Self {
            text: text.to_owned(),
            entries: webvtt::entries(text)?,
        })
    }
}
