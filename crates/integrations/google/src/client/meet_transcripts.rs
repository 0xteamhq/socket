//! Transcripts of a meeting: what was said, and by whom.
//!
//! Meet returns a transcript in parts. Its entries name each speaker by a
//! reference to a participant, and the participant's name is a third thing
//! to ask for. `read` asks for all three and puts them together.

use std::collections::HashSet;

use serde_json::Value;
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::Api;
use super::meet::{RECORD, TRANSCRIPT};
use crate::models::{
    MeetParticipant, MeetReadTranscript, MeetTranscript, MeetTranscriptContent, MeetTranscriptEntry, Paging,
};

/// The most transcripts, or entries, Google returns in one page.
const MOST: u32 = 100;

/// The most participants Google returns in one page.
const MOST_PARTICIPANTS: u32 = 250;

/// How many entries `read` returns unless it is asked for another number,
/// and the most it can be asked for.
const ENTRIES: u32 = 1_000;
const MOST_ENTRIES: u32 = 10_000;

/// How many pages of participants `read` looks through for the speakers:
/// 10,000 people. A speaker further down the list than that goes unnamed.
const PARTICIPANT_PAGES: u32 = 40;

/// What was said in a meeting.
#[derive(Debug, Clone, Copy)]
pub struct MeetTranscripts<'a>(pub(crate) Api<'a>);

impl MeetTranscripts<'_> {
    /// Lists a meeting's transcripts, the earliest first. Empty when
    /// transcription was never switched on.
    pub async fn list(&self, record: &str, paging: Paging) -> Result<Page<MeetTranscript>> {
        let path = format!("{}/transcripts", self.0.meet(&[(&RECORD, record)])?);
        let request = self.0.paged(RawRequest::get(path), &paging, "pageSize", MOST)?;
        self.0.page(self.0.send(request).await?, "transcripts", "transcripts")
    }

    /// Gets one transcript's details: when it was made, whether its Google
    /// Doc has been written, and which Doc that is.
    pub async fn get(&self, record: &str, transcript: &str) -> Result<MeetTranscript> {
        let path = self.item(record, transcript)?;
        self.transcript(self.0.send(RawRequest::get(path)).await?)
    }

    /// Lists a transcript's entries as Meet returns them, in the order
    /// they were said: the text, the times, the language, and the speaker as
    /// a reference to a participant. Google deletes them 30 days after the
    /// meeting ended.
    pub async fn entries(&self, record: &str, transcript: &str, paging: Paging) -> Result<Page<MeetTranscriptEntry>> {
        let path = self.item(record, transcript)?;
        self.page_of_entries(&path, paging).await
    }

    /// Reads a whole transcript, with each speaker's name filled in from
    /// the meeting's participants.
    ///
    /// It makes several requests: the transcript, each page of its entries,
    /// and pages of the participants until every speaker has been found.
    /// It stops at `max_entries` entries, 1,000 unless another number from 1
    /// to 10,000 is asked for, and says so in `truncated`, so it cannot run
    /// without end or return more than was asked for.
    pub async fn read(
        &self,
        record: &str,
        transcript: &str,
        options: MeetReadTranscript,
    ) -> Result<MeetTranscriptContent> {
        let most = options.max_entries.unwrap_or(ENTRIES);
        if !(1..=MOST_ENTRIES).contains(&most) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                format!("`maxEntries` is from 1 to {MOST_ENTRIES}"),
            ));
        }
        let path = self.item(record, transcript)?;
        let participants = format!("{}/participants", self.0.meet(&[(&RECORD, record)])?);

        let transcript = self.transcript(self.0.send(RawRequest::get(path.as_str())).await?)?;
        let (entries, truncated) = self.entries_up_to(&path, most).await?;
        let speakers = self.speakers(&participants, &entries).await?;
        MeetTranscriptContent::from_entries(transcript, entries, &speakers, truncated)
            .map_err(|e| e.with_provider(self.0.connection.provider().id.clone()))
    }

    /// The first `most` entries of the transcript at `path`, and whether
    /// there are more.
    ///
    /// Google may fill a page with fewer entries than were asked for, so
    /// more pages are read than full ones would take: twice as many, and
    /// two more. Past that the transcript is returned as cut short. A
    /// server that kept answering with one more page would otherwise keep
    /// this going for ever.
    async fn entries_up_to(&self, path: &str, most: u32) -> Result<(Vec<MeetTranscriptEntry>, bool)> {
        let mut entries = Vec::new();
        let mut room = most;
        let mut cursor = None;
        for _ in 0..2 * most.div_ceil(MOST) + 2 {
            let paging = Paging {
                cursor: cursor.take(),
                limit: Some(room.min(MOST)),
            };
            let page = self.page_of_entries(path, paging).await?;
            for entry in page.items {
                if room == 0 {
                    // More than was asked for in this page.
                    return Ok((entries, true));
                }
                entries.push(entry);
                room -= 1;
            }
            match page.next_cursor {
                None => return Ok((entries, false)),
                Some(_) if room == 0 => return Ok((entries, true)),
                next => cursor = next,
            }
        }
        Ok((entries, true))
    }

    /// The participants who speak in `entries`, from the list at
    /// `participants`, which is read a page at a time until each of them
    /// has been found. A meeting where nobody spoke asks for none.
    async fn speakers(&self, participants: &str, entries: &[MeetTranscriptEntry]) -> Result<Vec<MeetParticipant>> {
        let mut wanted: HashSet<&str> = entries
            .iter()
            .filter_map(|entry| entry.participant.as_deref())
            .filter(|participant| !participant.is_empty())
            .collect();
        let mut found = Vec::with_capacity(wanted.len());
        let mut cursor = None;
        for _ in 0..PARTICIPANT_PAGES {
            if wanted.is_empty() {
                break;
            }
            let paging = Paging {
                cursor: cursor.take(),
                limit: Some(MOST_PARTICIPANTS),
            };
            let request = self
                .0
                .paged(RawRequest::get(participants), &paging, "pageSize", MOST_PARTICIPANTS)?;
            let page: Page<MeetParticipant> =
                self.0
                    .page(self.0.send(request).await?, "participants", "participants")?;
            for participant in page.items {
                if wanted.remove(participant.name.as_str()) {
                    found.push(participant);
                }
            }
            match page.next_cursor {
                None => break,
                next => cursor = next,
            }
        }
        Ok(found)
    }

    async fn page_of_entries(&self, path: &str, paging: Paging) -> Result<Page<MeetTranscriptEntry>> {
        let request = RawRequest::get(format!("{path}/entries"));
        let request = self.0.paged(request, &paging, "pageSize", MOST)?;
        self.0
            .page(self.0.send(request).await?, "transcriptEntries", "transcript entries")
    }

    /// The address of one transcript.
    fn item(&self, record: &str, transcript: &str) -> Result<String> {
        self.0.meet(&[(&RECORD, record), (&TRANSCRIPT, transcript)])
    }

    fn transcript(&self, body: Value) -> Result<MeetTranscript> {
        let transcript: MeetTranscript = self.0.decode(body, "a transcript")?;
        if transcript.name.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a transcript"));
        }
        Ok(transcript)
    }
}
