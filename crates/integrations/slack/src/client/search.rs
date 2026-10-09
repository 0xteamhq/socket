//! Searching messages. Needs a user token; Slack does not let a bot search.

use serde_json::json;
use socketkit_core::Result;

use super::{Api, with};
use crate::models::{Search, SearchResults};

/// Searching messages. Needs a user token; Slack does not let a bot search.
#[derive(Debug, Clone, Copy)]
pub struct SearchApi<'a>(pub(crate) Api<'a>);

impl SearchApi<'_> {
    /// Searches messages, using Slack's own query syntax (`in:#channel`, `from:@user`, …).
    pub async fn messages(&self, query: &str, options: Search) -> Result<SearchResults> {
        self.0.required("a search query", query)?;
        let body = self
            .0
            .get("search.messages", with(json!({ "query": query }), &options))
            .await?;
        self.0.field(&body, "messages")
    }
}
