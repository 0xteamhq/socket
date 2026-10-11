//! Search: finding pages and data sources by title.

use serde_json::{Map, json};
use socketkit_core::{Page, Result};

use super::Api;
use crate::models::{PageOrDataSource, Paging, SearchQuery};

/// Search: finding pages and data sources by title.
#[derive(Debug, Clone, Copy)]
pub struct Search<'a>(pub(crate) Api<'a>);

impl Search<'_> {
    /// Searches the titles of the pages and data sources shared with the
    /// integration. Changes nothing; Notion offers it only as a POST.
    pub async fn run(&self, search: SearchQuery) -> Result<Page<PageOrDataSource>> {
        let mut body = Map::new();
        if let Some(query) = &search.query {
            body.insert("query".to_owned(), json!(query));
        }
        if let Some(filter) = &search.filter {
            // Notion can filter on one thing only, the kind of object, and
            // has to be told so.
            let mut written = Map::new();
            if let Some(value) = filter.value {
                written.insert("property".to_owned(), json!("object"));
                written.insert("value".to_owned(), json!(value));
            }
            if let Some(in_trash) = filter.in_trash {
                written.insert("in_trash".to_owned(), json!(in_trash));
            }
            if !written.is_empty() {
                body.insert("filter".to_owned(), written.into());
            }
        }
        if let Some(sort) = &search.sort {
            // The time of the last edit is the only thing Notion sorts a search by.
            let sort = json!({ "timestamp": "last_edited_time", "direction": sort.direction });
            body.insert("sort".to_owned(), sort);
        }
        let paging = Paging {
            cursor: search.cursor,
            limit: search.limit,
        };
        let request = self.0.asking("search".to_owned(), body, &paging)?;
        self.0.list(self.0.send(request).await?, "search results")
    }
}
