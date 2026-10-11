//! Paging, which every list shares.
//!
//! Attio pages most lists by `limit` and `offset`, in the query or, for the
//! two queries, in the body, and a few by a cursor of its own. A caller sees
//! one thing either way: the `next_cursor` of a page, passed back as `cursor`.

use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::Paging;

/// The page size of a list when the caller names none. Attio's own is as
/// much as 500 records, each with every value it holds, which is more than
/// a caller with a limited context can take in.
pub(super) const A_PAGE: u32 = 50;

/// The page size of a query of records or entries when the caller names
/// none. A row carries every attribute, so fewer of them make a page.
pub(super) const A_PAGE_OF_ROWS: u32 = 25;

/// The most Attio's documentation shows a list to return where it states no
/// limit of its own: the size of the page it returns by default.
pub(super) const MOST: u32 = 500;

/// The part of an offset-paged list one request asks for.
#[derive(Debug, Clone, Copy)]
struct Window {
    limit: u32,
    offset: u64,
}

impl Window {
    /// The cursor of the page after one that returned `returned` items.
    ///
    /// Attio does not say whether more follow. A page as long as was asked
    /// for may have more behind it; a shorter one is the last. So a list
    /// whose length is an exact number of pages ends with an empty page.
    fn next(self, returned: usize) -> Option<String> {
        let returned = u64::try_from(returned).ok()?;
        let full = returned > 0 && returned >= u64::from(self.limit);
        full.then(|| self.offset.checked_add(returned))
            .flatten()
            .map(|next| next.to_string())
    }
}

impl Api<'_> {
    /// The page size to ask for: the caller's, or `unset`. Always sent, so
    /// that what makes a page full is known here and not left to a default
    /// Attio may change.
    fn limit(&self, paging: &Paging, unset: u32, most: u32) -> Result<u32> {
        match paging.limit {
            Some(limit) if !(1..=most).contains(&limit) => {
                Err(self.error(ErrorKind::InvalidInput, format!("`limit` is from 1 to {most}")))
            }
            limit => Ok(limit.unwrap_or(unset.min(most))),
        }
    }

    /// The cursor a caller passed back, if any.
    fn cursor(paging: &Paging) -> Option<&str> {
        paging
            .cursor
            .as_deref()
            .map(str::trim)
            .filter(|cursor| !cursor.is_empty())
    }

    /// Where a page of a list that Attio pages by offset starts. The cursor
    /// of such a list is the offset, written as a number.
    fn window(&self, paging: &Paging, unset: u32, most: u32) -> Result<Window> {
        let limit = self.limit(paging, unset, most)?;
        let offset = match Self::cursor(paging) {
            None => 0,
            Some(cursor) => cursor.parse().map_err(|_| {
                self.error(
                    ErrorKind::InvalidInput,
                    "`cursor` is not a place in this list; pass back `next_cursor` unchanged",
                )
            })?,
        };
        Ok(Window { limit, offset })
    }

    /// One page of a list Attio pages by `limit` and `offset` in the query.
    /// `unset` is the page size when the caller names none, `most` the largest Attio takes.
    pub(super) async fn page<T: DeserializeOwned>(
        &self,
        first: RawRequest,
        paging: &Paging,
        (unset, most): (u32, u32),
        what: &str,
    ) -> Result<Page<T>> {
        let window = self.window(paging, unset, most)?;
        let mut request = first.with_query("limit", window.limit.to_string());
        if window.offset > 0 {
            request = request.with_query("offset", window.offset.to_string());
        }
        let items: Vec<T> = self.one(request, what).await?;
        Ok(Page {
            next_cursor: window.next(items.len()),
            items,
        })
    }

    /// One page of a query, which Attio takes as a POST with `limit` and
    /// `offset` in the body beside the filter. It changes nothing.
    pub(super) async fn query<T: DeserializeOwned>(
        &self,
        path: String,
        mut search: Map<String, Value>,
        paging: &Paging,
        what: &str,
    ) -> Result<Page<T>> {
        let window = self.window(paging, A_PAGE_OF_ROWS, MOST)?;
        search.insert("limit".to_owned(), json!(window.limit));
        if window.offset > 0 {
            search.insert("offset".to_owned(), json!(window.offset));
        }
        let items: Vec<T> = self.one(RawRequest::post(path, Value::Object(search)), what).await?;
        Ok(Page {
            next_cursor: window.next(items.len()),
            items,
        })
    }

    /// `first` for the page a caller asked for, of a list Attio pages by a
    /// cursor of its own. The cursor is sent as a query parameter and as
    /// nothing else, so whatever it holds, it cannot change where the request goes.
    pub(super) fn at_cursor(
        &self,
        first: RawRequest,
        paging: &Paging,
        (unset, most): (u32, u32),
    ) -> Result<RawRequest> {
        let request = first.with_query("limit", self.limit(paging, unset, most)?.to_string());
        Ok(match Self::cursor(paging) {
            Some(cursor) => request.with_query("cursor", cursor),
            None => request,
        })
    }

    /// One page of a list Attio pages by a cursor of its own.
    pub(super) async fn cursor_page<T: DeserializeOwned>(
        &self,
        first: RawRequest,
        paging: &Paging,
        sizes: (u32, u32),
        what: &str,
    ) -> Result<Page<T>> {
        let body = self.send(self.at_cursor(first, paging, sizes)?).await?;
        Ok(Page {
            next_cursor: next_cursor(&body),
            items: self.data(body, what)?,
        })
    }
}

/// Where a cursor-paged list goes on, as Attio states it beside the data.
pub(super) fn next_cursor(body: &Value) -> Option<String> {
    body["pagination"]["next_cursor"]
        .as_str()
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_page_may_have_more_behind_it_and_a_short_one_is_the_last() {
        let window = Window { limit: 25, offset: 50 };
        assert_eq!(window.next(25).as_deref(), Some("75"));
        assert_eq!(window.next(24), None);
        assert_eq!(window.next(0), None);
        // More than was asked for is still a full page, and goes on from what came.
        assert_eq!(window.next(30).as_deref(), Some("80"));
        let first = Window { limit: 1, offset: 0 };
        assert_eq!(first.next(1).as_deref(), Some("1"));
    }

    #[test]
    fn an_offset_too_large_to_go_on_from_ends_the_list() {
        let last = Window {
            limit: 2,
            offset: u64::MAX - 1,
        };
        assert_eq!(last.next(2), None);
        let nearly = Window {
            limit: 2,
            offset: u64::MAX - 2,
        };
        assert_eq!(nearly.next(2), Some(u64::MAX.to_string()));
    }

    #[test]
    fn the_next_cursor_is_read_from_beside_the_data() {
        assert_eq!(
            next_cursor(&json!({ "data": [], "pagination": { "next_cursor": "abc" } })).as_deref(),
            Some("abc")
        );
        for last in [
            json!({ "data": [], "pagination": { "next_cursor": null } }),
            json!({ "data": [], "pagination": { "next_cursor": "" } }),
            json!({ "data": [], "pagination": {} }),
            json!({ "data": [] }),
            json!(null),
        ] {
            assert_eq!(next_cursor(&last), None, "{last}");
        }
    }
}
