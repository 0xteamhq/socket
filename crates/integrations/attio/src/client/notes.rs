//! The notes written on records.

use serde_json::{Map, Value, json};
use socketkit_core::{Page, RawRequest, Result};

use super::{Api, asking, filled, set};
use crate::models::{CreateNote, ListNotes, Note, NoteFormat};

/// The notes written on records.
#[derive(Debug, Clone, Copy)]
pub struct Notes<'a>(pub(crate) Api<'a>);

impl Notes<'_> {
    /// Lists the notes of one record, or of every record, with their text.
    pub async fn list(&self, notes: ListNotes) -> Result<Page<Note>> {
        let (object, record) = (notes.parent_object.as_deref(), notes.parent_record_id.as_deref());
        // Attio returns 10 unless told otherwise, and at most 50.
        let window = self.0.window(notes.cursor.as_deref(), notes.limit, 10, 50)?;
        let request = RawRequest::get("notes")
            .with_query("limit", window.limit.to_string())
            .with_query("offset", window.offset.to_string());
        let request = asking(request, "parent_object", filled(object));
        let request = asking(request, "parent_record_id", filled(record));
        Ok(window.page(self.0.all(request, "notes").await?))
    }

    /// Gets one note.
    pub async fn get(&self, note: &str) -> Result<Note> {
        let note = self.0.segment("a note id", note)?;
        self.0.one(RawRequest::get(format!("notes/{note}")), "a note").await
    }

    /// Writes a note on a record. Everyone who can see the record sees it.
    pub async fn create(&self, note: CreateNote) -> Result<Note> {
        self.0.required("`parent_object`", &note.parent_object)?;
        self.0.required("`parent_record_id`", &note.parent_record_id)?;
        let mut data = Map::new();
        data.insert("parent_object".to_owned(), note.parent_object.trim().into());
        data.insert("parent_record_id".to_owned(), note.parent_record_id.trim().into());
        data.insert("title".to_owned(), note.title.into());
        let format = note.format.unwrap_or(NoteFormat::Plaintext);
        data.insert("format".to_owned(), format.as_str().into());
        data.insert("content".to_owned(), note.content.into());
        set(&mut data, "created_at", note.created_at);
        set(&mut data, "meeting_id", note.meeting_id);
        let body = json!({ "data": Value::Object(data) });
        self.0.one(RawRequest::post("notes", body), "a note").await
    }

    /// Deletes a note.
    pub async fn delete(&self, note: &str) -> Result<()> {
        let note = self.0.segment("a note id", note)?;
        self.0.done(RawRequest::new("DELETE", format!("notes/{note}"))).await
    }
}
