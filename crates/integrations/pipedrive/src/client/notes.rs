//! Notes. Pipedrive offers them in version 1 of its API only.

use serde_json::Value;
use socketkit_core::{Page, RawRequest, Result};

use super::{Api, body, filtered};
use crate::models::{CreateNote, ListNotes, Note, Paging, UpdateNote, preview};

/// Notes written on a deal, a person, an organisation or a lead.
#[derive(Debug, Clone, Copy)]
pub struct Notes<'a>(pub(crate) Api<'a>);

impl Notes<'_> {
    /// Lists notes: all of them, or those on one deal, person, organisation
    /// or lead, or by one user.
    ///
    /// A row carries the beginning of a long note and says so in
    /// `truncated`. `get` returns the whole text.
    pub async fn list(&self, options: ListNotes) -> Result<Page<Note>> {
        let paging = Paging {
            cursor: options.cursor.clone(),
            limit: options.limit,
        };
        let request = filtered(RawRequest::get("v1/notes"), &options);
        let page = self.0.page_by_offset(request, &paging, "notes").await?;
        let notes: Vec<Note> = self.0.decode(Value::Array(page.items), "notes")?;
        Ok(Page {
            items: notes.into_iter().map(preview).collect(),
            next_cursor: page.next_cursor,
        })
    }

    /// Gets one note, with its whole text.
    pub async fn get(&self, note: u64) -> Result<Note> {
        self.0.one(RawRequest::get(format!("v1/notes/{note}")), "a note").await
    }

    /// Writes a note on a deal, a person, an organisation or a lead.
    pub async fn create(&self, note: CreateNote) -> Result<Note> {
        self.0.required("`content`", &note.content)?;
        if note.deal_id.is_none() && note.person_id.is_none() && note.org_id.is_none() && note.lead_id.is_none() {
            return Err(self
                .0
                .invalid("a note needs one of `deal_id`, `person_id`, `org_id` and `lead_id` to be written on"));
        }
        self.0.one(RawRequest::post("v1/notes", body(&note)), "a note").await
    }

    /// Changes a note. What is not set is left as it is; a new `content`
    /// replaces the text that was there.
    pub async fn update(&self, note: u64, change: UpdateNote) -> Result<Note> {
        if change
            .content
            .as_deref()
            .is_some_and(|content| content.trim().is_empty())
        {
            return Err(self
                .0
                .invalid("`content` cannot be empty; delete the note to remove it"));
        }
        // Version 1 changes a note with PUT, and still only what is sent.
        let mut request = self.0.change(format!("v1/notes/{note}"), &change)?;
        request.method = "PUT".to_owned();
        self.0.one(request, "a note").await
    }

    /// Deletes a note.
    pub async fn delete(&self, note: u64) -> Result<()> {
        let request = RawRequest::new("DELETE", format!("v1/notes/{note}"));
        self.0.done(request, "a deleted note").await
    }
}
