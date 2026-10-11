//! Comments on a page or a block.

use serde_json::{Map, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::{Comment, CreateComment, Paging};

/// Comments on a page or a block.
#[derive(Debug, Clone, Copy)]
pub struct Comments<'a>(pub(crate) Api<'a>);

impl Comments<'_> {
    /// Lists the comments on a page or a block that are not resolved.
    /// Notion does not return resolved comments.
    pub async fn list(&self, block: &str, paging: Paging) -> Result<Page<Comment>> {
        let block = self.0.id("a page or block id", block)?;
        let request = self.0.listing("comments".to_owned(), &paging)?;
        let body = self.0.send(request.with_query("block_id", block)).await?;
        self.0.list(body, "comments")
    }

    /// Adds a comment to a page or a block, or a reply to a thread.
    /// Everyone who can see the page sees it.
    pub async fn create(&self, comment: CreateComment) -> Result<Comment> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        let parent = comment.parent.as_ref();
        let targets = [
            parent.and_then(|parent| parent.page_id.as_ref()),
            parent.and_then(|parent| parent.block_id.as_ref()),
            comment.discussion_id.as_ref(),
        ];
        if targets.iter().flatten().count() != 1 {
            return Err(invalid(
                "a comment goes on one thing: give `parent.page_id`, `parent.block_id` or `discussion_id`",
            ));
        }
        let written = comment.rich_text.as_ref().is_some_and(|runs| !runs.is_empty());
        let marked = comment.markdown.as_ref().is_some_and(|text| !text.trim().is_empty());
        if written == marked {
            return Err(invalid("a comment says something once: give `rich_text` or `markdown`"));
        }
        let mut body = Map::new();
        body.extend(comment.parent.map(|parent| ("parent".to_owned(), json!(parent))));
        body.extend(
            comment
                .discussion_id
                .map(|thread| ("discussion_id".to_owned(), json!(thread))),
        );
        body.extend(
            comment
                .rich_text
                .filter(|_| written)
                .map(|runs| ("rich_text".to_owned(), json!(runs))),
        );
        body.extend(
            comment
                .markdown
                .filter(|_| marked)
                .map(|text| ("markdown".to_owned(), json!(text))),
        );
        let created = self.0.send(RawRequest::post("comments", body.into())).await?;
        self.0.object(created, "comment")
    }
}
