//! One search across every kind of record. Version 2 of Pipedrive's API.

use socketkit_core::{Page, RawRequest, Result};

use super::{Api, filtered};
use crate::models::{Paging, SearchItems, SearchResult};

/// One search across deals, persons, organisations, leads and the rest.
#[derive(Debug, Clone, Copy)]
pub struct Search<'a>(pub(crate) Api<'a>);

impl Search<'_> {
    /// Searches every kind of record, or the kinds named, best match first.
    ///
    /// A result is enough of a record to tell it from the others; the
    /// record itself is read with its group's `get`.
    pub async fn items(&self, options: SearchItems) -> Result<Page<SearchResult>> {
        self.0.term(&options.term, options.exact_match)?;
        let paging = Paging {
            cursor: options.cursor.clone(),
            limit: options.limit,
        };
        let request = filtered(RawRequest::get("v2/itemSearch"), &options);
        self.0.found(request, &paging, "search results").await
    }
}
