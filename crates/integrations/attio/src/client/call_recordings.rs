//! The recordings of a meeting.

use socketkit_core::{Page, RawRequest, Result};

use super::Api;
use crate::models::{CallRecording, Paging};

/// The recordings of a meeting, with what was said. Attio marks this part
/// of its API as beta.
#[derive(Debug, Clone, Copy)]
pub struct CallRecordings<'a>(pub(crate) Api<'a>);

impl CallRecordings<'_> {
    /// Lists a meeting's recordings, without their transcripts. Empty when
    /// the meeting was not recorded.
    pub async fn list(&self, meeting: &str, paging: Paging) -> Result<Page<CallRecording>> {
        let meeting = self.0.segment("a meeting id", meeting)?;
        let request = RawRequest::get(format!("meetings/{meeting}/call_recordings"));
        // Attio returns 50 unless told otherwise, and at most 200.
        let request = self.0.after(request, paging.cursor.as_deref(), paging.limit, 200)?;
        self.0.cursor_page(request, "call recordings").await
    }

    /// Gets one recording, with its transcript: each thing said, by whom and
    /// when, and the whole as text.
    pub async fn get(&self, meeting: &str, call_recording: &str) -> Result<CallRecording> {
        let meeting = self.0.segment("a meeting id", meeting)?;
        let recording = self.0.segment("a call recording id", call_recording)?;
        let request = RawRequest::get(format!("meetings/{meeting}/call_recordings/{recording}"));
        self.0.one(request, "a call recording").await
    }
}
