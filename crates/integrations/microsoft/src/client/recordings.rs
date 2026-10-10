//! Recordings of Teams online meetings.

use socketkit_core::{ErrorKind, Page, Result};

use super::Api;
use crate::models::{Paging, Recording};

/// Recordings of Teams online meetings.
#[derive(Debug, Clone, Copy)]
pub struct Recordings<'a>(pub(crate) Api<'a>);

impl Recordings<'_> {
    /// Lists a meeting's recordings. Empty when the meeting was not recorded.
    pub async fn list(&self, meeting: &str, paging: Paging) -> Result<Page<Recording>> {
        let meeting = self.0.segment("a meeting", meeting)?;
        let path = format!("me/onlineMeetings/{meeting}/recordings");
        self.0.list(&path, &paging, "recordings").await
    }

    /// One recording's details, with the address its video is at.
    pub async fn get(&self, meeting: &str, recording: &str) -> Result<Recording> {
        let meeting = self.0.segment("a meeting", meeting)?;
        let recording = self.0.segment("a recording", recording)?;
        let path = format!("me/onlineMeetings/{meeting}/recordings/{recording}");
        let body = self.0.get(&path, &[]).await?;
        let recording: Recording = self.0.decode(body, "a recording")?;
        if recording.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "microsoft answered with a recording that has no id"));
        }
        Ok(recording)
    }
}
