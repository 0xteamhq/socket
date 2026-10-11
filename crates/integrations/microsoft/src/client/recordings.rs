//! Recordings of Teams online meetings.

use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::{Paging, Recording};

/// Recordings of Teams online meetings.
#[derive(Debug, Clone, Copy)]
pub struct Recordings<'a>(pub(crate) Api<'a>);

impl Recordings<'_> {
    /// Lists a meeting's recordings. Empty when the meeting was not recorded.
    pub async fn list(&self, meeting: &str, paging: Paging) -> Result<Page<Recording>> {
        let meeting = self.0.segment("a meeting id", meeting)?;
        let request = RawRequest::get(format!("me/onlineMeetings/{meeting}/recordings"));
        self.0.page(request, &paging, "recordings").await
    }

    /// One recording's details, with the address its video is at.
    pub async fn get(&self, meeting: &str, recording: &str) -> Result<Recording> {
        let meeting = self.0.segment("a meeting id", meeting)?;
        let recording = self.0.segment("a recording id", recording)?;
        let path = format!("me/onlineMeetings/{meeting}/recordings/{recording}");
        let recording: Recording = self
            .0
            .decode(self.0.send(RawRequest::get(path)).await?, "a recording")?;
        if recording.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "microsoft answered without a recording"));
        }
        Ok(recording)
    }
}
