//! Transcripts of a meeting: what Meet returns for one, and its entries.
//!
//! A whole transcript with the speakers named is in `meet_transcript_content.rs`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One transcript of a meeting. A meeting where transcription was switched
/// on twice has two.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetTranscript {
    /// `conferenceRecords/{id}/transcripts/{id}`.
    pub name: String,
    /// `STARTED` while transcribing, `ENDED` once it stopped, and
    /// `FILE_GENERATED` when the Google Doc has been written.
    pub state: Option<String>,
    /// When transcription began. The entries' `startMs` count from here.
    pub start_time: Option<String>,
    /// When transcription stopped.
    pub end_time: Option<String>,
    /// The Google Doc the transcript was saved to. Absent until it has been
    /// written. The Doc stays after the entries are deleted.
    pub docs_destination: Option<MeetDocsDestination>,
}

/// The Google Doc a transcript was saved to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetDocsDestination {
    /// The document's id, as the Docs and Drive methods take it.
    pub document: Option<String>,
    /// The address that opens the document in a browser.
    pub export_uri: Option<String>,
}

/// One thing someone said, as Meet returns it: the speaker is a reference
/// to a participant, not a name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetTranscriptEntry {
    /// `conferenceRecords/{id}/transcripts/{id}/entries/{id}`.
    pub name: String,
    /// Who spoke: `conferenceRecords/{id}/participants/{id}`, as
    /// `meet_participants.get` takes it.
    pub participant: Option<String>,
    /// What was said.
    pub text: String,
    /// The language it was said in, such as `en-US`.
    pub language_code: Option<String>,
    /// When it began, in UTC: `2026-10-12T16:00:04.250Z`.
    pub start_time: Option<String>,
    /// When it ended.
    pub end_time: Option<String>,
}

/// How much of a transcript to read at once.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MeetReadTranscript {
    /// The most entries to return, from 1 to 10000. 1000 when not given. A
    /// transcript with more comes back cut short, with `truncated` set.
    pub max_entries: Option<u32>,
}
