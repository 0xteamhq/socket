//! Teams online meetings.

use socketkit_core::{ErrorKind, Result};

use super::Api;
use crate::models::OnlineMeeting;

/// Teams online meetings.
#[derive(Debug, Clone, Copy)]
pub struct OnlineMeetings<'a>(pub(crate) Api<'a>);

impl OnlineMeetings<'_> {
    /// One online meeting by its id.
    pub async fn get(&self, meeting: &str) -> Result<OnlineMeeting> {
        let meeting = self.0.segment("a meeting", meeting)?;
        let body = self.0.get(&format!("me/onlineMeetings/{meeting}"), &[]).await?;
        self.meeting(self.0.decode(body, "an online meeting")?)
    }

    /// The online meeting behind a join link, as found on a calendar event
    /// (`onlineMeeting.joinUrl`). Give the link exactly as the event has it.
    ///
    /// This is how a meeting's id is found, and the id is what its
    /// transcripts, recordings and attendance are asked for by.
    pub async fn find_by_join_url(&self, join_url: &str) -> Result<OnlineMeeting> {
        if join_url.trim().is_empty() {
            return Err(self.0.error(ErrorKind::InvalidInput, "a join URL is required"));
        }
        // OData writes a quote inside a string as two.
        let filter = format!("JoinWebUrl eq '{}'", join_url.trim().replace('\'', "''"));
        let body = self.0.get("me/onlineMeetings", &[("$filter", &filter)]).await?;
        let Some(found) = body.get("value").and_then(|meetings| meetings.as_array()) else {
            return Err(self.0.error(
                ErrorKind::Decode,
                "microsoft answered without a list of online meetings",
            ));
        };
        let Some(first) = found.first() else {
            return Err(self.0.error(
                ErrorKind::NotFound,
                "no online meeting this account can read has that join URL",
            ));
        };
        self.meeting(self.0.decode(first.clone(), "an online meeting")?)
    }

    fn meeting(&self, meeting: OnlineMeeting) -> Result<OnlineMeeting> {
        if meeting.id.is_empty() {
            return Err(self.0.error(
                ErrorKind::Decode,
                "microsoft answered with an online meeting that has no id",
            ));
        }
        Ok(meeting)
    }
}
