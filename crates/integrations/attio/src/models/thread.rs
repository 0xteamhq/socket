//! Threads of comments on a record or on a list entry.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Actor;
use super::nullable::nullable;

/// The id of a thread.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ThreadId {
    pub workspace_id: String,
    pub thread_id: String,
}

/// The id of a comment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CommentId {
    pub workspace_id: String,
    pub comment_id: String,
}

/// One comment of a thread.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Comment {
    pub id: CommentId,
    pub thread_id: Option<String>,
    /// What was said, as plain text. Someone mentioned reads as their email address.
    pub content_plaintext: Option<String>,
    /// The list entry the thread is on. Absent for a thread on a record.
    pub entry: Option<CommentEntry>,
    /// The record the thread is on, or the record behind the entry it is on.
    pub record: Option<CommentRecord>,
    /// When the thread was marked resolved, if it was.
    pub resolved_at: Option<String>,
    pub resolved_by: Option<Actor>,
    pub created_at: Option<String>,
    pub author: Option<Actor>,
}

/// The list entry a thread is on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CommentEntry {
    pub entry_id: Option<String>,
    pub list_id: Option<String>,
}

/// The record a thread is on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CommentRecord {
    pub record_id: Option<String>,
    pub object_id: Option<String>,
}

/// A thread with its comments, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Thread {
    pub id: ThreadId,
    pub created_at: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub comments: Vec<Comment>,
    /// Where the comments go on, when the thread has more than this page
    /// holds; `null` when these are the last.
    pub next_cursor: Option<String>,
}

/// A thread as a list returns it: where it is and how much was said, without
/// the comments. `threads.get` returns them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ThreadRow {
    pub id: ThreadId,
    pub created_at: Option<String>,
    /// How many comments Attio returned with the thread. It returns at most
    /// 80 with a list, so a longer thread counts 80 here.
    pub comment_count: u32,
    /// The record the thread is on, or the record behind the entry it is on.
    pub record: Option<CommentRecord>,
    /// The list entry the thread is on. Absent for a thread on a record.
    pub entry: Option<CommentEntry>,
}

impl From<Thread> for ThreadRow {
    fn from(thread: Thread) -> Self {
        // Every comment of a thread is on the same record or entry.
        let first = thread.comments.first();
        Self {
            comment_count: u32::try_from(thread.comments.len()).unwrap_or(u32::MAX),
            record: first.and_then(|comment| comment.record.clone()),
            entry: first.and_then(|comment| comment.entry.clone()),
            id: thread.id,
            created_at: thread.created_at,
        }
    }
}

/// Which threads to return: those on one record, or those on one list entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListThreads {
    /// The object of the record, by its slug or id. It needs `record_id` beside it.
    pub object: Option<String>,
    /// The record whose threads are wanted. It needs `object` beside it.
    pub record_id: Option<String>,
    /// The list of the entry, by its slug or id. It needs `entry_id` beside it.
    pub list: Option<String>,
    /// The entry whose threads are wanted. It needs `list` beside it.
    pub entry_id: Option<String>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most threads to return in one page, from 1 to 50. 50 when not given.
    pub limit: Option<u32>,
}

/// A record to start a thread on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OnRecord {
    /// The object the record belongs to, by its slug or id.
    pub object: String,
    pub record_id: String,
}

/// A list entry to start a thread on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OnEntry {
    /// The list the entry is on, by its slug or id.
    pub list: String,
    pub entry_id: String,
}

/// A comment to write. It goes in exactly one place: an existing thread, a
/// new thread on a record, or a new thread on a list entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateComment {
    /// The thread to reply in.
    pub thread_id: Option<String>,
    /// The record to start a thread on.
    pub record: Option<OnRecord>,
    /// The list entry to start a thread on.
    pub entry: Option<OnEntry>,
    /// What to say, as plain text with a line feed between lines. A member's
    /// email address in the text mentions them.
    pub content: String,
    /// When it was said, to record a comment from the past. Now when not
    /// given. Attio refuses a time in the future.
    pub created_at: Option<String>,
}
