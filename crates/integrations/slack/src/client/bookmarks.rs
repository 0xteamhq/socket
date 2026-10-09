//! Bookmarks at the top of a channel.

use serde_json::json;
use socketkit_core::{ErrorKind, Result};

use super::Api;
use crate::models::Bookmark;

/// Bookmarks at the top of a channel.
#[derive(Debug, Clone, Copy)]
pub struct Bookmarks<'a>(pub(crate) Api<'a>);

impl Bookmarks<'_> {
    /// Adds a link bookmark to a channel. `emoji` is optional, written `:book:`.
    pub async fn add(&self, channel: &str, title: &str, link: &str, emoji: Option<&str>) -> Result<Bookmark> {
        self.0.required("a channel", channel)?;
        self.0.required("a title", title)?;
        self.0.required("a link", link)?;
        let mut arguments = json!({ "channel_id": channel, "title": title, "type": "link", "link": link });
        if let Some(emoji) = emoji {
            arguments["emoji"] = json!(emoji);
        }
        let body = self.0.post("bookmarks.add", arguments).await?;
        let bookmark: Bookmark = self.0.field(&body, "bookmark")?;
        if bookmark.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered with a bookmark that has no id"));
        }
        Ok(bookmark)
    }

    /// A channel's bookmarks.
    pub async fn list(&self, channel: &str) -> Result<Vec<Bookmark>> {
        self.0.required("a channel", channel)?;
        let body = self.0.post("bookmarks.list", json!({ "channel_id": channel })).await?;
        self.0.field(&body, "bookmarks")
    }

    /// Removes a bookmark.
    pub async fn remove(&self, channel: &str, bookmark: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0.required("a bookmark", bookmark)?;
        self.0
            .post(
                "bookmarks.remove",
                json!({ "channel_id": channel, "bookmark_id": bookmark }),
            )
            .await
            .map(drop)
    }
}
