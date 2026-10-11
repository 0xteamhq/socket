//! Transcripts of Teams online meetings.

use serde_json::Value;
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::{Paging, Transcript, TranscriptContent};

/// Transcripts of Teams online meetings.
#[derive(Debug, Clone, Copy)]
pub struct Transcripts<'a>(pub(crate) Api<'a>);

impl Transcripts<'_> {
    /// Lists a meeting's transcripts. Empty when transcription was never switched on.
    pub async fn list(&self, meeting: &str, paging: Paging) -> Result<Page<Transcript>> {
        let meeting = self.0.segment("a meeting id", meeting)?;
        let request = RawRequest::get(format!("me/onlineMeetings/{meeting}/transcripts"));
        self.0.page(request, &paging, "transcripts").await
    }

    /// One transcript's details: when it was made, and by whose meeting.
    pub async fn get(&self, meeting: &str, transcript: &str) -> Result<Transcript> {
        let body = self.0.send(RawRequest::get(self.path(meeting, transcript)?)).await?;
        let transcript: Transcript = self.0.decode(body, "a transcript")?;
        if transcript.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "microsoft answered without a transcript"));
        }
        Ok(transcript)
    }

    /// What was said: the transcript as Microsoft wrote it, and the same
    /// thing as entries with the speaker, the start, the end and the words.
    ///
    /// Asks for WebVTT, the format that names the speakers.
    pub async fn content(&self, meeting: &str, transcript: &str) -> Result<TranscriptContent> {
        let path = format!("{}/content", self.path(meeting, transcript)?);
        let request = RawRequest::get(path).with_header("Accept", "text/vtt").as_text();
        let answered = self.0.send(request).await.map_err(|e| match e.kind() {
            // Graph refuses the format that names speakers where an
            // organisation withholds who spoke, and gives no other sign of it
            // than a code this error does not carry.
            ErrorKind::AccessDenied => self.0.error(
                ErrorKind::AccessDenied,
                format!(
                    "{} (if the organisation withholds who spoke, a transcript cannot be read this way yet)",
                    e.message()
                ),
            ),
            _ => e,
        })?;
        match answered {
            Value::String(text) if !text.trim().is_empty() => {
                TranscriptContent::from_vtt(&text).map_err(|e| e.with_provider(self.0.connection.provider().id.clone()))
            }
            _ => Err(self
                .0
                .error(ErrorKind::Decode, "microsoft answered without a transcript")),
        }
    }

    fn path(&self, meeting: &str, transcript: &str) -> Result<String> {
        let meeting = self.0.segment("a meeting id", meeting)?;
        let transcript = self.0.segment("a transcript id", transcript)?;
        Ok(format!("me/onlineMeetings/{meeting}/transcripts/{transcript}"))
    }
}
