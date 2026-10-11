//! Notes on records.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Actor;
use super::nullable::nullable;

/// The id of a note.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct NoteId {
    pub workspace_id: String,
    pub note_id: String,
}

/// A note, with what it says.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Note {
    pub id: NoteId,
    /// The object of the record the note is on, such as `people`.
    pub parent_object: Option<String>,
    pub parent_record_id: Option<String>,
    pub title: Option<String>,
    /// The meeting the note was taken in, if any.
    pub meeting_id: Option<String>,
    /// What the note says, as plain text with a line feed between lines.
    pub content_plaintext: Option<String>,
    /// The same in Markdown. Images are left out.
    pub content_markdown: Option<String>,
    /// The members and records the note mentions.
    #[serde(deserialize_with = "nullable")]
    pub tags: Vec<NoteTag>,
    pub created_by_actor: Option<Actor>,
    pub created_at: Option<String>,
}

/// A member or a record that a note mentions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct NoteTag {
    /// `workspace-member` or `record`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Set for a member.
    pub workspace_member_id: Option<String>,
    /// Set for a record: the object it belongs to.
    pub object: Option<String>,
    /// Set for a record.
    pub record_id: Option<String>,
}

/// A note as a list returns it: what it is on and what it is called, without
/// what it says. `notes.get` returns the text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct NoteRow {
    pub id: NoteId,
    pub parent_object: Option<String>,
    pub parent_record_id: Option<String>,
    pub title: Option<String>,
    pub meeting_id: Option<String>,
    pub created_by_actor: Option<Actor>,
    pub created_at: Option<String>,
}

/// Which notes to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListNotes {
    /// The object whose notes are wanted, by its slug or id. Every object when not given.
    pub parent_object: Option<String>,
    /// The record whose notes are wanted. It needs `parent_object` beside it.
    pub parent_record_id: Option<String>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most notes to return in one page, from 1 to 50. 50 when not given.
    pub limit: Option<u32>,
}

/// How the text of a new note is written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NoteFormat {
    /// Text as it stands, with a line feed between lines.
    #[default]
    Plaintext,
    /// Markdown: headings to the third level, lists, bold, italic,
    /// strikethrough, `==highlight==` and links.
    Markdown,
}

/// A note to write.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateNote {
    /// The note's title, as plain text.
    pub title: String,
    /// How `content` is written. Plain text when not given.
    pub format: Option<NoteFormat>,
    pub content: String,
    /// When the note was taken, to record one from the past. Now when not
    /// given. Attio refuses a time in the future.
    pub created_at: Option<String>,
    /// The meeting the note was taken in. It has to exist.
    pub meeting_id: Option<String>,
}
