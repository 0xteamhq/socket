//! Notes on records.

use serde_json::json;
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{A_PAGE, Api, with};
use crate::models::{CreateNote, ListNotes, Note, NoteFormat, NoteRow, Paging};

/// Notes on records.
#[derive(Debug, Clone, Copy)]
pub struct Notes<'a>(pub(crate) Api<'a>);

impl Notes<'_> {
    /// Lists the notes on one record, on the records of one object, or on
    /// everything, at most 50 a page. A row says what a note is on and what
    /// it is called; `get` returns what it says.
    pub async fn list(&self, options: ListNotes) -> Result<Page<NoteRow>> {
        let object = self
            .0
            .optional_id("a parent object", options.parent_object.as_deref())?;
        let record = self
            .0
            .optional_id("a parent record id", options.parent_record_id.as_deref())?;
        let mut request = RawRequest::get("notes");
        match (object, record) {
            // A record id by itself does not say which object's record it is.
            (None, Some(_)) => {
                return Err(self.0.error(
                    ErrorKind::InvalidInput,
                    "`parent_record_id` needs `parent_object` beside it",
                ));
            }
            (Some(object), record) => {
                request = request.with_query("parent_object", object);
                if let Some(record) = record {
                    request = request.with_query("parent_record_id", record);
                }
            }
            (None, None) => {}
        }
        let paging = Paging {
            cursor: options.cursor,
            limit: options.limit,
        };
        self.0.page(request, &paging, (A_PAGE, 50), "notes").await
    }

    /// Gets one note, with what it says as plain text and as Markdown.
    pub async fn get(&self, note: &str) -> Result<Note> {
        let note = self.0.id("a note id", note)?;
        self.note(RawRequest::get(format!("notes/{note}"))).await
    }

    /// Writes a note on a record.
    pub async fn create(&self, parent_object: &str, parent_record: &str, note: CreateNote) -> Result<Note> {
        let placed = json!({
            "parent_object": self.0.id("a parent object", parent_object)?,
            "parent_record_id": self.0.id("a parent record id", parent_record)?,
            // Attio has no default for the format, so the plain one is named.
            "format": note.format.unwrap_or(NoteFormat::Plaintext),
        });
        if let Some(meeting) = &note.meeting_id {
            self.0.id("a meeting id", meeting)?;
        }
        let body = json!({ "data": with(placed, &note) });
        self.note(RawRequest::post("notes", body)).await
    }

    /// Deletes a note.
    pub async fn delete(&self, note: &str) -> Result<()> {
        let note = self.0.id("a note id", note)?;
        self.0.send(RawRequest::new("DELETE", format!("notes/{note}"))).await?;
        Ok(())
    }

    async fn note(&self, request: RawRequest) -> Result<Note> {
        let note: Note = self.0.one(request, "a note").await?;
        if note.id.note_id.is_empty() {
            return Err(self.0.missing("a note"));
        }
        Ok(note)
    }
}
