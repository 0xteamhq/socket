//! Notes: what Attio returns for them, and the content used to write one.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Actor;
use super::nullable::nullable;

/// A note written on a record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Note {
    pub id: NoteId,
    /// The slug of the object the note's record belongs to.
    #[serde(deserialize_with = "nullable")]
    pub parent_object: String,
    #[serde(deserialize_with = "nullable")]
    pub parent_record_id: String,
    #[serde(deserialize_with = "nullable")]
    pub title: String,
    /// The meeting the note was taken in, when it was taken in one.
    pub meeting_id: Option<String>,
    /// The note as plain text.
    #[serde(deserialize_with = "nullable")]
    pub content_plaintext: String,
    /// The note as Markdown, with its headings, lists and links.
    #[serde(deserialize_with = "nullable")]
    pub content_markdown: String,
    /// The members and records mentioned in the note, as Attio sent them.
    #[serde(deserialize_with = "nullable")]
    pub tags: Vec<Value>,
    pub created_by_actor: Option<Actor>,
    pub created_at: Option<String>,
}

/// A note's id, with the workspace it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct NoteId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    #[serde(deserialize_with = "nullable")]
    pub note_id: String,
}

/// Which notes to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListNotes {
    /// The slug or id of an object, to list only the notes on its records.
    pub parent_object: Option<String>,
    /// The id of a record, to list only the notes on it.
    pub parent_record_id: Option<String>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most notes to return, from 1 to 50. 10 when not given.
    pub limit: Option<u32>,
}

impl ListNotes {
    /// The notes of one record.
    pub fn of(parent_object: impl Into<String>, parent_record_id: impl Into<String>) -> Self {
        Self {
            parent_object: Some(parent_object.into()),
            parent_record_id: Some(parent_record_id.into()),
            ..Self::default()
        }
    }
}

/// How the content of a new note is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NoteFormat {
    Plaintext,
    Markdown,
}

impl NoteFormat {
    /// The name Attio knows the format by.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plaintext => "plaintext",
            Self::Markdown => "markdown",
        }
    }
}

/// A new note on a record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateNote {
    /// The slug or id of the object the record belongs to, such as `people`.
    pub parent_object: String,
    /// The id of the record the note is about.
    pub parent_record_id: String,
    pub title: String,
    /// The text of the note.
    pub content: String,
    /// How `content` is written. `plaintext` when not given.
    pub format: Option<NoteFormat>,
    /// When the note was written, in ISO 8601, for a note brought in from
    /// elsewhere. Now when not given.
    pub created_at: Option<String>,
    /// The id of the meeting the note was taken in.
    pub meeting_id: Option<String>,
}

impl CreateNote {
    /// A plain-text note on the record `parent_record_id` of `parent_object`.
    pub fn on(
        parent_object: impl Into<String>,
        parent_record_id: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            parent_object: parent_object.into(),
            parent_record_id: parent_record_id.into(),
            title: title.into(),
            content: content.into(),
            format: None,
            created_at: None,
            meeting_id: None,
        }
    }
}
