//! The people in a meeting, and each time one of them was connected.

use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::Api;
use super::meet::{PARTICIPANT, RECORD};
use crate::models::{MeetParticipant, MeetParticipantSession, Paging};

/// The most participants, or sessions, Google returns in one page.
const MOST: u32 = 250;

/// Who was in a meeting.
#[derive(Debug, Clone, Copy)]
pub struct MeetParticipants<'a>(pub(crate) Api<'a>);

impl MeetParticipants<'_> {
    /// Lists who was in a meeting, the latest to join first. Someone who
    /// left and came back is listed once.
    pub async fn list(&self, record: &str, paging: Paging) -> Result<Page<MeetParticipant>> {
        let path = format!("{}/participants", self.0.meet(&[(&RECORD, record)])?);
        let request = self.0.paged(RawRequest::get(path), &paging, "pageSize", MOST)?;
        self.0.page(self.0.send(request).await?, "participants", "participants")
    }

    /// Gets one participant: the name they were shown under, and when they
    /// first joined and last left.
    pub async fn get(&self, record: &str, participant: &str) -> Result<MeetParticipant> {
        let body = self.0.send(RawRequest::get(self.item(record, participant)?)).await?;
        let participant: MeetParticipant = self.0.decode(body, "a participant")?;
        if participant.name.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a participant"));
        }
        Ok(participant)
    }

    /// Lists each time a participant was connected: one session for every
    /// time they joined, from every device, the latest first.
    pub async fn sessions(
        &self,
        record: &str,
        participant: &str,
        paging: Paging,
    ) -> Result<Page<MeetParticipantSession>> {
        let path = format!("{}/participantSessions", self.item(record, participant)?);
        let request = self.0.paged(RawRequest::get(path), &paging, "pageSize", MOST)?;
        self.0.page(
            self.0.send(request).await?,
            "participantSessions",
            "participant sessions",
        )
    }

    /// The address of one participant.
    fn item(&self, record: &str, participant: &str) -> Result<String> {
        self.0.meet(&[(&RECORD, record), (&PARTICIPANT, participant)])
    }
}
