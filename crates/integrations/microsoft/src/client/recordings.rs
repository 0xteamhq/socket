//! Recordings of Teams online meetings.

use socketkit_core::{Content, ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::{Download, Paging, Recording};

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
        let path = self.path(meeting, recording)?;
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

    /// The recording itself: the video, as the bytes Graph serves.
    ///
    /// A recording is far larger than the ten megabytes and the thirty
    /// seconds a fetch is given unless told otherwise, so `limits` says how
    /// much the caller is ready to hold in memory and how long it will wait.
    /// A recording over that is refused whole, with `too_large`.
    ///
    /// Graph gives the video only to the meeting's organiser. This is a typed
    /// method only: an operation called by name never returns bytes.
    pub async fn content(&self, meeting: &str, recording: &str, limits: Download) -> Result<Content> {
        let path = format!("{}/content", self.path(meeting, recording)?);
        self.0.fetch(path, &limits).await
    }

    fn path(&self, meeting: &str, recording: &str) -> Result<String> {
        let meeting = self.0.segment("a meeting id", meeting)?;
        let recording = self.0.segment("a recording id", recording)?;
        Ok(format!("me/onlineMeetings/{meeting}/recordings/{recording}"))
    }
}
