//! Comment threads on a record or a list entry, and the content of a new comment.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Actor;
use super::nullable::nullable;

/// A thread of comments on a record or a list entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Thread {
    pub id: ThreadId,
    pub created_at: Option<String>,
    /// The comments, oldest first, starting with the one that opened the thread.
    #[serde(deserialize_with = "nullable")]
    pub comments: Vec<Comment>,
    /// In a list of threads, whether the thread holds more comments than
    /// came with it. `threads.get` pages through all of them.
    pub has_more_comments: Option<bool>,
    /// From `threads.get`: the cursor for the comments after these, when there are more.
    pub next_cursor: Option<String>,
}

/// A thread's id, with the workspace it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ThreadId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    #[serde(deserialize_with = "nullable")]
    pub thread_id: String,
}

/// One comment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Comment {
    pub id: CommentId,
    #[serde(deserialize_with = "nullable")]
    pub thread_id: String,
    #[serde(deserialize_with = "nullable")]
    pub content_plaintext: String,
    /// The list entry the comment is on, when it is on one.
    pub entry: Option<CommentedEntry>,
    /// The record the comment is on, when it is on one.
    pub record: Option<CommentedRecord>,
    pub resolved_at: Option<String>,
    pub resolved_by: Option<Actor>,
    pub created_at: Option<String>,
    pub author: Option<Actor>,
}

/// A comment's id, with the workspace it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CommentId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    #[serde(deserialize_with = "nullable")]
    pub comment_id: String,
}

/// The list entry a comment is on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CommentedEntry {
    #[serde(deserialize_with = "nullable")]
    pub entry_id: String,
    #[serde(deserialize_with = "nullable")]
    pub list_id: String,
}

/// The record a comment is on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CommentedRecord {
    #[serde(deserialize_with = "nullable")]
    pub record_id: String,
    #[serde(deserialize_with = "nullable")]
    pub object_id: String,
}

/// Which threads to return: those on one record, or those on one list entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListThreads {
    /// The slug or id of an object. Given together with `record_id`.
    pub object: Option<String>,
    /// The id of the record whose threads to list. Given together with `object`.
    pub record_id: Option<String>,
    /// The slug or id of a list. Given together with `entry_id`.
    pub list: Option<String>,
    /// The id of the list entry whose threads to list. Given together with `list`.
    pub entry_id: Option<String>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most threads to return, from 1 to 50. 10 when not given.
    pub limit: Option<u32>,
}

impl ListThreads {
    /// The threads on one record.
    pub fn on_record(object: impl Into<String>, record_id: impl Into<String>) -> Self {
        Self {
            object: Some(object.into()),
            record_id: Some(record_id.into()),
            ..Self::default()
        }
    }

    /// The threads on one list entry.
    pub fn on_entry(list: impl Into<String>, entry_id: impl Into<String>) -> Self {
        Self {
            list: Some(list.into()),
            entry_id: Some(entry_id.into()),
            ..Self::default()
        }
    }
}

/// Which of a thread's comments to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetThread {
    /// The thread's `next_cursor` from the call before, unchanged; absent for the first comments.
    pub cursor: Option<String>,
    /// The most comments to return, from 1 to 250, which is also what Attio
    /// returns when this is not given.
    pub limit: Option<u32>,
    /// Only the comments made after this time, in ISO 8601.
    pub created_after: Option<String>,
}

/// A new comment: a reply in a thread, or the first comment on a record or
/// a list entry. Exactly one of `thread_id`, `record` and `entry` says where.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateComment {
    /// The text of the comment, as plain text.
    pub content: String,
    /// The id of the workspace member the comment is from. Attio takes no
    /// other author, and shows the comment as theirs.
    pub author: String,
    /// The thread to reply in.
    pub thread_id: Option<String>,
    /// The record to open a thread on.
    pub record: Option<CommentOnRecord>,
    /// The list entry to open a thread on.
    pub entry: Option<CommentOnEntry>,
    /// When the comment was made, in ISO 8601, for a comment brought in from
    /// elsewhere. Now when not given.
    pub created_at: Option<String>,
}

/// The record a new comment is on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CommentOnRecord {
    /// The slug or id of the object the record belongs to.
    pub object: String,
    pub record_id: String,
}

/// The list entry a new comment is on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CommentOnEntry {
    /// The slug or id of the list the entry is on.
    pub list: String,
    pub entry_id: String,
}

impl CreateComment {
    /// A reply in a thread, from the workspace member `author`.
    pub fn reply(thread_id: impl Into<String>, author: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            author: author.into(),
            thread_id: Some(thread_id.into()),
            ..Self::default()
        }
    }
}
