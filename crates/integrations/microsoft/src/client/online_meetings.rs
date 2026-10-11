//! Teams online meetings.

use socketkit_core::{ErrorKind, RawRequest, Result};

use super::{Api, named};
use crate::models::OnlineMeeting;

/// Teams online meetings.
#[derive(Debug, Clone, Copy)]
pub struct OnlineMeetings<'a>(pub(crate) Api<'a>);

impl OnlineMeetings<'_> {
    /// One online meeting by its id.
    pub async fn get(&self, meeting: &str) -> Result<OnlineMeeting> {
        let meeting = self.0.segment("a meeting id", meeting)?;
        let body = self
            .0
            .send(named(RawRequest::get(format!("me/onlineMeetings/{meeting}"))))
            .await?;
        self.meeting(self.0.decode(body, "an online meeting")?)
    }

    /// The online meeting behind a join link, as found on a calendar event
    /// (`onlineMeeting.joinUrl`). Give the link exactly as the event has it.
    ///
    /// This is how a meeting's id is found, and the id is what its
    /// transcripts, recordings and attendance are asked for by.
    pub async fn find_by_join_url(&self, join_url: &str) -> Result<OnlineMeeting> {
        self.0.required("a join URL", join_url)?;
        // OData writes a quote inside a string as two.
        let filter = format!("JoinWebUrl eq '{}'", join_url.trim().replace('\'', "''"));
        let request = named(RawRequest::get("me/onlineMeetings")).with_query("$filter", filter);
        let found: Vec<OnlineMeeting> = self.0.list(&self.0.send(request).await?, "online meetings")?;
        // The link is not repeated: it lets anyone who holds it into the meeting.
        let first = found.into_iter().next().ok_or_else(|| {
            self.0.error(
                ErrorKind::NotFound,
                "no online meeting this account can read has that join URL",
            )
        })?;
        self.meeting(first)
    }

    fn meeting(&self, meeting: OnlineMeeting) -> Result<OnlineMeeting> {
        if meeting.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "microsoft answered without an online meeting"));
        }
        Ok(meeting)
    }
}
