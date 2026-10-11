//! Recordings of a meeting.
//!
//! Meet says where a recording is. The video itself is a file in Drive, and
//! reading it is Drive's to do.

use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::Api;
use super::meet::{RECORD, RECORDING};
use crate::models::{MeetRecording, Paging};

/// The most recordings Google returns in one page.
const MOST: u32 = 100;

/// The recordings of a meeting.
#[derive(Debug, Clone, Copy)]
pub struct MeetRecordings<'a>(pub(crate) Api<'a>);

impl MeetRecordings<'_> {
    /// Lists a meeting's recordings, the earliest first. Empty when the
    /// meeting was not recorded.
    pub async fn list(&self, record: &str, paging: Paging) -> Result<Page<MeetRecording>> {
        let path = format!("{}/recordings", self.0.meet(&[(&RECORD, record)])?);
        let request = self.0.paged(RawRequest::get(path), &paging, "pageSize", MOST)?;
        self.0.page(self.0.send(request).await?, "recordings", "recordings")
    }

    /// Gets one recording: whether its file is ready, and the Drive file
    /// it was saved to.
    pub async fn get(&self, record: &str, recording: &str) -> Result<MeetRecording> {
        let path = self.0.meet(&[(&RECORD, record), (&RECORDING, recording)])?;
        let recording: MeetRecording = self
            .0
            .decode(self.0.send(RawRequest::get(path)).await?, "a recording")?;
        if recording.name.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a recording"));
        }
        Ok(recording)
    }
}
