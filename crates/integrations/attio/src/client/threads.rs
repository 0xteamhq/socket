//! Comment threads.

use serde_json::{Map, Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, asking, filled, set};
use crate::models::{Comment, CreateComment, GetThread, ListThreads, Thread};

/// The comment threads on a record or a list entry.
#[derive(Debug, Clone, Copy)]
pub struct Threads<'a>(pub(crate) Api<'a>);

impl Threads<'_> {
    /// Lists the threads on one record or on one list entry, each with its
    /// first comments. A thread that holds more says so in `has_more_comments`.
    pub async fn list(&self, threads: ListThreads) -> Result<Page<Thread>> {
        let (object, record) = (filled(threads.object.as_deref()), filled(threads.record_id.as_deref()));
        let (list, entry) = (filled(threads.list.as_deref()), filled(threads.entry_id.as_deref()));
        self.0.together(("object", object), ("record_id", record))?;
        self.0.together(("list", list), ("entry_id", entry))?;
        if record.is_some() == entry.is_some() {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "name a record, with `object` and `record_id`, or a list entry, with `list` and `entry_id`",
            ));
        }
        // Attio returns 10 unless told otherwise, and at most 50.
        let window = self.0.window(threads.cursor.as_deref(), threads.limit, 10, 50)?;
        let request = RawRequest::get("threads")
            .with_query("limit", window.limit.to_string())
            .with_query("offset", window.offset.to_string());
        let request = asking(request, "object", object);
        let request = asking(request, "record_id", record);
        let request = asking(request, "list", list);
        let request = asking(request, "entry_id", entry);
        Ok(window.page(self.0.all(request, "threads").await?))
    }

    /// Gets one thread and its comments, oldest first. A thread with more
    /// comments than came back carries a `next_cursor` to go on from.
    pub async fn get(&self, thread: &str, comments: GetThread) -> Result<Thread> {
        let thread = self.0.segment("a thread id", thread)?;
        let request = RawRequest::get(format!("threads/{thread}"));
        // Attio returns at most 250 comments at a time.
        let request = self.0.after(request, comments.cursor.as_deref(), comments.limit, 250)?;
        let request = asking(request, "created_after", filled(comments.created_after.as_deref()));
        let (thread, next_cursor) = self.0.one_of_many::<Thread>(request, "a thread").await?;
        Ok(Thread { next_cursor, ..thread })
    }

    /// Writes a comment, as one of the workspace's members: a reply in a
    /// thread, or the first comment on a record or a list entry. Everyone
    /// who can see the record or the entry sees it.
    pub async fn comment(&self, comment: CreateComment) -> Result<Comment> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        if comment.content.trim().is_empty() {
            return Err(invalid("a comment needs `content`"));
        }
        self.0.required("`author`", &comment.author)?;
        let thread = filled(comment.thread_id.as_deref());
        let places = [thread.is_some(), comment.record.is_some(), comment.entry.is_some()];
        if places.into_iter().filter(|given| *given).count() != 1 {
            return Err(invalid(
                "a comment goes in exactly one of `thread_id`, `record` and `entry`",
            ));
        }
        let mut data = Map::new();
        data.insert("format".to_owned(), "plaintext".into());
        data.insert("content".to_owned(), comment.content.as_str().into());
        data.insert(
            "author".to_owned(),
            json!({ "type": "workspace-member", "id": comment.author.trim() }),
        );
        set(&mut data, "created_at", comment.created_at.clone());
        set(&mut data, "thread_id", thread);
        if let Some(record) = &comment.record {
            self.0.required("`record.object`", &record.object)?;
            self.0.required("`record.record_id`", &record.record_id)?;
            data.insert(
                "record".to_owned(),
                json!({ "object": record.object.trim(), "record_id": record.record_id.trim() }),
            );
        }
        if let Some(entry) = &comment.entry {
            self.0.required("`entry.list`", &entry.list)?;
            self.0.required("`entry.entry_id`", &entry.entry_id)?;
            data.insert(
                "entry".to_owned(),
                json!({ "list": entry.list.trim(), "entry_id": entry.entry_id.trim() }),
            );
        }
        let body = json!({ "data": Value::Object(data) });
        self.0.one(RawRequest::post("comments", body), "a comment").await
    }
}
