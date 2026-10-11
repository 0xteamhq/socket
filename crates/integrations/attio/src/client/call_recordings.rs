//! Recordings of the calls held in a meeting, and what was said in them.

use socketkit_core::{Page, RawRequest, Result};

use super::{A_PAGE, Api};
use crate::models::{CallRecording, CallRecordingRow, Paging, SentCallRecording};

/// Recordings of the calls held in a meeting, and what was said in them.
/// Attio marks this part of its API as beta.
#[derive(Debug, Clone, Copy)]
pub struct CallRecordings<'a>(pub(crate) Api<'a>);

impl CallRecordings<'_> {
    /// Lists a meeting's recordings, at most 200 a page. A row says whether
    /// a recording is ready; `get` returns what was said.
    pub async fn list(&self, meeting: &str, paging: Paging) -> Result<Page<CallRecordingRow>> {
        let request = RawRequest::get(self.recordings(meeting)?);
        self.0
            .cursor_page(request, &paging, (A_PAGE, 200), "call recordings")
            .await
    }

    /// Gets one recording with its transcript: the whole text, and one entry
    /// for each thing said, with the speaker, the start and the end.
    ///
    /// The answer may carry `video_url`, a signed link that downloads the
    /// video without a token for about an hour. Keep it out of logs.
    pub async fn get(&self, meeting: &str, recording: &str) -> Result<CallRecording> {
        let recording = self.0.id("a call recording id", recording)?;
        let request = RawRequest::get(format!("{}/{recording}", self.recordings(meeting)?));
        // Attio's transcript is read in its own shape and turned into ours.
        let sent: SentCallRecording = self.0.one(request, "a call recording").await?;
        let recording = CallRecording::from(sent);
        if recording.id.call_recording_id.is_empty() {
            return Err(self.0.missing("a call recording"));
        }
        Ok(recording)
    }

    fn recordings(&self, meeting: &str) -> Result<String> {
        Ok(format!(
            "meetings/{}/call_recordings",
            self.0.id("a meeting id", meeting)?
        ))
    }
}
