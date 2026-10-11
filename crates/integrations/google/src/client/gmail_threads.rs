//! Gmail threads: conversations, and the messages in one.

use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, GMAIL, with_query};
use crate::models::{GmailGetThread, GmailListThreads, GmailThread, GmailWireThread, Paging};

const THREADS: &str = "gmail/v1/users/me/threads";

/// The threads of a Gmail mailbox.
#[derive(Debug, Clone, Copy)]
pub struct GmailThreads<'a>(pub(crate) Api<'a>);

impl GmailThreads<'_> {
    /// Lists the threads a search finds, or every thread when nothing is asked for.
    ///
    /// The list is light: each thread is its id and a snippet, without its
    /// messages, and nothing is fetched for each row. Read one with
    /// [`GmailThreads::get`].
    pub async fn list(&self, options: GmailListThreads, paging: Paging) -> Result<Page<GmailThread>> {
        let options = GmailListThreads {
            q: options.q.filter(|q| !q.trim().is_empty()),
            ..options
        };
        if options.label_ids.iter().flatten().any(|id| id.trim().is_empty()) {
            return Err(self.0.error(ErrorKind::InvalidInput, "`labelIds` has a blank id"));
        }
        let request = with_query(RawRequest::get(self.0.on(GMAIL, THREADS)), &options);
        let request = self.0.paged(request, &paging, "maxResults", 500)?;
        let page: Page<GmailThread> = self.0.page(self.0.send(request).await?, "threads", "threads")?;
        if page.items.iter().any(|thread| thread.id.is_empty()) {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a thread"));
        }
        Ok(page)
    }

    /// Gets one thread with its messages, each decoded as
    /// [`crate::GmailMessages::get`] decodes one.
    ///
    /// A long thread with attachments in it is large. Ask for
    /// `GmailFormat::Metadata` to get the headers of every message without
    /// any body.
    pub async fn get(&self, thread: &str, options: GmailGetThread) -> Result<GmailThread> {
        let thread = self.0.segment("a thread id", thread)?;
        let path = self.0.on(GMAIL, &format!("{THREADS}/{thread}"));
        let body = self.0.send(with_query(RawRequest::get(path), &options)).await?;
        let thread = self.0.decode::<GmailWireThread>(body, "a thread")?.read();
        if thread.id.is_empty() || thread.messages.iter().any(|message| message.id.is_empty()) {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a thread"));
        }
        Ok(thread)
    }
}
