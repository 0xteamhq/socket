//! A whole transcript, with each speaker named.
//!
//! The shape is the one a Teams transcript has in the Microsoft crate
//! (`text`, and `entries` with `speaker`, `startMs`, `endMs` and `text`), so a
//! caller reads both the same way. Meet's own fields are kept beside it.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use socketkit_core::{Error, ErrorKind, Result};

use super::meet_time::millis;
use super::{MeetParticipant, MeetTranscript, MeetTranscriptEntry};

/// What was said in a meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetTranscriptContent {
    /// The transcript as lines a person can read, one for each entry:
    /// `Ada Lovelace: Shall we begin?`. A line whose speaker is not known
    /// has only what was said.
    pub text: String,
    /// The same transcript, one entry for each thing someone said, in the
    /// order Meet returns them: by when they began.
    pub entries: Vec<MeetTranscriptContentEntry>,
    /// True when the transcript has more entries than were asked for, and
    /// `text` and `entries` stop early. Ask again with a larger `maxEntries`,
    /// or page through `meet_transcripts.entries`.
    pub truncated: bool,
    /// The transcript itself: when it began, and the Google Doc it was saved to.
    pub transcript: MeetTranscript,
}

/// One thing someone said, with the speaker's name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetTranscriptContentEntry {
    /// Who spoke: the name they were shown under in the meeting. Absent
    /// when Meet does not say. Someone who joined without signing in chose
    /// their own name, and nobody checked it.
    pub speaker: Option<String>,
    /// When it began, in milliseconds from the transcript's `startTime`.
    pub start_ms: i64,
    /// When it ended, in milliseconds from the transcript's `startTime`.
    pub end_ms: i64,
    /// What was said.
    pub text: String,
    /// When it began, as Meet wrote it: `2026-10-12T16:00:04.250Z`.
    pub start_time: Option<String>,
    /// When it ended, as Meet wrote it.
    pub end_time: Option<String>,
    /// The language it was said in, such as `en-US`.
    pub language_code: Option<String>,
    /// The speaker as Meet names them:
    /// `conferenceRecords/{id}/participants/{id}`.
    pub participant: Option<String>,
}

/// `text` on one line: every run of spaces and line breaks is one space.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl MeetTranscriptContent {
    /// Puts a transcript together from what Meet returns in parts: the
    /// transcript, its entries, and the meeting's participants.
    ///
    /// Each entry names its speaker by a participant's `name`; the display
    /// name comes from that participant, whichever kind it is. A participant
    /// who left and came back is one participant, so has one name. An entry
    /// whose participant is not among `participants` has no `speaker`.
    ///
    /// A name is written on one line, and so is what was said in `text`, so
    /// that nothing in a name or in speech can pass for another person's line.
    ///
    /// A time that cannot be read is an error: an entry left out would
    /// return a transcript that looks whole and is not. The error never
    /// repeats what was said.
    pub fn from_entries(
        transcript: MeetTranscript,
        entries: Vec<MeetTranscriptEntry>,
        participants: &[MeetParticipant],
        truncated: bool,
    ) -> Result<Self> {
        let unreadable =
            |what: String| Error::new(ErrorKind::Decode, format!("google sent {what} that could not be read"));
        let began = transcript
            .start_time
            .as_deref()
            .and_then(millis)
            .ok_or_else(|| unreadable("a transcript with a `startTime`".to_owned()))?;
        let names: HashMap<&str, String> = participants
            .iter()
            .filter_map(|participant| Some((participant.name.as_str(), one_line(participant.display_name()?))))
            .collect();

        let mut lines = Vec::with_capacity(entries.len());
        let mut said = Vec::with_capacity(entries.len());
        for (at, entry) in entries.into_iter().enumerate() {
            let offset = |time: &Option<String>, field: &str| {
                time.as_deref()
                    .and_then(millis)
                    .map(|time| time - began)
                    .ok_or_else(|| {
                        unreadable(format!(
                            "a transcript entry with a `{field}`, at `transcriptEntries[{at}]`,"
                        ))
                    })
            };
            let start_ms = offset(&entry.start_time, "startTime")?;
            let end_ms = offset(&entry.end_time, "endTime")?;
            let speaker = entry
                .participant
                .as_deref()
                .and_then(|participant| names.get(participant))
                .cloned();
            lines.push(match &speaker {
                Some(speaker) => format!("{speaker}: {}", one_line(&entry.text)),
                None => one_line(&entry.text),
            });
            said.push(MeetTranscriptContentEntry {
                speaker,
                start_ms,
                end_ms,
                text: entry.text,
                start_time: entry.start_time,
                end_time: entry.end_time,
                language_code: entry.language_code,
                participant: entry.participant,
            });
        }
        Ok(Self {
            text: lines.join("\n"),
            entries: said,
            truncated,
            transcript,
        })
    }
}
