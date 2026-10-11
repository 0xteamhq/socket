//! Threads of comments on a record or on a list entry.

use serde_json::json;
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{A_PAGE, Api, next_cursor, with};
use crate::models::{Comment, CreateComment, ListThreads, Paging, Thread, ThreadRow};

/// Threads of comments on a record or on a list entry.
#[derive(Debug, Clone, Copy)]
pub struct Threads<'a>(pub(crate) Api<'a>);

impl Threads<'_> {
    /// Lists the threads on one record or on one list entry, at most 50 a
    /// page. A row says where a thread is and how much was said; `get`
    /// returns the comments.
    pub async fn list(&self, options: ListThreads) -> Result<Page<ThreadRow>> {
        let object = self.0.optional_id("an object", options.object.as_deref())?;
        let record = self.0.optional_id("a record id", options.record_id.as_deref())?;
        let list = self.0.optional_id("a list", options.list.as_deref())?;
        let entry = self.0.optional_id("an entry id", options.entry_id.as_deref())?;
        let request = match (object, record, list, entry) {
            (Some(object), Some(record), None, None) => RawRequest::get("threads")
                .with_query("object", object)
                .with_query("record_id", record),
            (None, None, Some(list), Some(entry)) => RawRequest::get("threads")
                .with_query("list", list)
                .with_query("entry_id", entry),
            _ => {
                return Err(self.0.error(
                    ErrorKind::InvalidInput,
                    "name a record with `object` and `record_id`, or a list entry with `list` and `entry_id`",
                ));
            }
        };
        let paging = Paging {
            cursor: options.cursor,
            limit: options.limit,
        };
        let page: Page<Thread> = self.0.page(request, &paging, (A_PAGE, 50), "threads").await?;
        Ok(Page {
            items: page.items.into_iter().map(ThreadRow::from).collect(),
            next_cursor: page.next_cursor,
        })
    }

    /// Gets one thread with its comments, oldest first, at most 250 a page.
    /// A longer thread says where its comments go on, in `next_cursor`.
    pub async fn get(&self, thread: &str, paging: Paging) -> Result<Thread> {
        let thread = self.0.id("a thread id", thread)?;
        let request = self
            .0
            .at_cursor(RawRequest::get(format!("threads/{thread}")), &paging, (A_PAGE, 250))?;
        let body = self.0.send(request).await?;
        let next_cursor = next_cursor(&body);
        let thread: Thread = self.0.data(body, "a thread")?;
        if thread.id.thread_id.is_empty() {
            return Err(self.0.missing("a thread"));
        }
        Ok(Thread { next_cursor, ..thread })
    }

    /// Writes a comment as `author`, a workspace member: a reply in a
    /// thread, or the first comment of a new thread on a record or on a
    /// list entry. Everyone who follows the record is told.
    pub async fn comment(&self, author: &str, comment: CreateComment) -> Result<Comment> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        let author = self.0.id("an author's workspace member id", author)?;
        if comment.content.trim().is_empty() {
            return Err(invalid("a comment needs `content`"));
        }
        match (&comment.thread_id, &comment.record, &comment.entry) {
            (Some(thread), None, None) => {
                self.0.id("a thread id", thread)?;
            }
            (None, Some(record), None) => {
                self.0.id("an object", &record.object)?;
                self.0.id("a record id", &record.record_id)?;
            }
            (None, None, Some(entry)) => {
                self.0.id("a list", &entry.list)?;
                self.0.id("an entry id", &entry.entry_id)?;
            }
            _ => {
                return Err(invalid(
                    "a comment goes in exactly one of `thread_id`, `record` and `entry`",
                ));
            }
        }
        // Attio takes plain text only, and a comment has to have an author.
        let written = json!({ "format": "plaintext", "author": { "type": "workspace-member", "id": author } });
        let body = json!({ "data": with(written, &comment) });
        let comment: Comment = self.0.one(RawRequest::post("comments", body), "a comment").await?;
        if comment.id.comment_id.is_empty() {
            return Err(self.0.missing("a comment"));
        }
        Ok(comment)
    }
}
