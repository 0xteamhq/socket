//! Paging, which Pipedrive does in two ways: a cursor in version 2 of its
//! API and an offset in version 1. A caller sees one: [`Paging`] in, and a
//! [`Page`] with a `next_cursor` out.
//!
//! This is part of the shared access to the API, not a group of methods; it
//! has its own file only to keep `mod.rs` short.

use serde::de::DeserializeOwned;
use serde_json::Value;
use socketkit_core::{Error, ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::Paging;

/// What the cursor of a list paged by offset starts with.
///
/// Version 1 has no cursor of its own, only the number of items to skip. A
/// bare number would read as a cursor of either kind, so this one says what
/// it is: handed to a list of the other kind, it is refused and not misread.
const OFFSET: &str = "offset:";

/// The largest page Pipedrive returns from a list, and from a search.
const LIST: u32 = 500;
const SEARCH: u32 = 100;

impl Api<'_> {
    /// One page of a version 2 list: the items under `data`.
    pub(super) async fn page<T: DeserializeOwned>(
        &self,
        first: RawRequest,
        paging: &Paging,
        what: &str,
    ) -> Result<Page<T>> {
        let body = self.send(self.by_cursor(first, paging, LIST)?, what).await?;
        Ok(Page {
            items: self.items(body["data"].clone(), what)?,
            next_cursor: next_cursor(&body),
        })
    }

    /// One page of a search: the results under `data.items`.
    pub(super) async fn found<T: DeserializeOwned>(
        &self,
        first: RawRequest,
        paging: &Paging,
        what: &str,
    ) -> Result<Page<T>> {
        let body = self.send(self.by_cursor(first, paging, SEARCH)?, what).await?;
        // A search always answers with a list, if an empty one. An answer
        // without it has the wrong shape, and is not "nothing found".
        let items = match body["data"]["items"].clone() {
            items @ Value::Array(_) => self.decode(items, what)?,
            _ => return Err(self.error(ErrorKind::Decode, format!("pipedrive answered without {what}"))),
        };
        Ok(Page {
            items,
            next_cursor: next_cursor(&body),
        })
    }

    /// One page of a version 1 list, as Pipedrive sent its rows.
    ///
    /// The rows are returned unread, so that a group can put a version 1
    /// record into the shape of version 2 before it is read.
    pub(super) async fn page_by_offset(&self, first: RawRequest, paging: &Paging, what: &str) -> Result<Page<Value>> {
        self.limit(paging, LIST)?;
        let start = match cursor(paging) {
            None => None,
            Some(cursor) => Some(offset_of(cursor).ok_or_else(|| self.not_a_cursor())?),
        };
        let mut request = first;
        if let Some(start) = start {
            request = request.with_query("start", start.to_string());
        }
        if let Some(limit) = paging.limit {
            request = request.with_query("limit", limit.to_string());
        }
        let body = self.send(request, what).await?;
        let pagination = &body["additional_data"]["pagination"];
        let next_cursor = if pagination["more_items_in_collection"] == true {
            // Pipedrive names where the next page starts. Without that, a
            // list that says it goes on would end here in silence.
            let next = pagination["next_start"].as_u64().ok_or_else(|| {
                self.error(
                    ErrorKind::Decode,
                    format!("pipedrive said there are more {what} without saying where they start"),
                )
            })?;
            Some(format!("{OFFSET}{next}"))
        } else {
            None
        };
        Ok(Page {
            items: self.items(body["data"].clone(), what)?,
            next_cursor,
        })
    }

    /// The rows of a list. Version 1 writes `null` for a list with nothing
    /// in it, so that is read as none from either version.
    fn items<T: DeserializeOwned>(&self, rows: Value, what: &str) -> Result<Vec<T>> {
        match rows {
            Value::Null => Ok(Vec::new()),
            rows @ Value::Array(_) => self.decode(rows, what),
            _ => Err(self.error(ErrorKind::Decode, format!("pipedrive answered without {what}"))),
        }
    }

    /// `first` with the place and size of the page asked for, in version 2.
    ///
    /// A cursor comes back from the caller, so it is not trusted to be what
    /// Pipedrive sent. It is only ever sent as the value of the `cursor`
    /// parameter, percent-encoded whole, so nothing in it can become a host,
    /// a path or another parameter: the page is requested at the address
    /// this crate built for the list.
    fn by_cursor(&self, first: RawRequest, paging: &Paging, most: u32) -> Result<RawRequest> {
        self.limit(paging, most)?;
        let mut request = first;
        if let Some(cursor) = cursor(paging) {
            if cursor.starts_with(OFFSET) || cursor.chars().any(char::is_whitespace) {
                return Err(self.not_a_cursor());
            }
            request = request.with_query("cursor", cursor);
        }
        if let Some(limit) = paging.limit {
            request = request.with_query("limit", limit.to_string());
        }
        Ok(request)
    }

    fn limit(&self, paging: &Paging, most: u32) -> Result<()> {
        if paging.limit.is_some_and(|limit| !(1..=most).contains(&limit)) {
            return Err(self.invalid(format!("`limit` is from 1 to {most}")));
        }
        Ok(())
    }

    fn not_a_cursor(&self) -> Error {
        self.invalid(
            "`cursor` is not a place in this list; pass back `next_cursor` unchanged, to the method it came from",
        )
    }
}

/// The cursor a caller gave, if it holds anything.
fn cursor(paging: &Paging) -> Option<&str> {
    paging.cursor.as_deref().map(str::trim).filter(|c| !c.is_empty())
}

/// Where the next page of a version 2 list starts. `null` on the last page.
fn next_cursor(body: &Value) -> Option<String> {
    body["additional_data"]["next_cursor"]
        .as_str()
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_owned)
}

/// The number of items a version 1 cursor skips, if it is one: `offset:`
/// and then digits, and nothing else. A sign, a space or a second number is
/// not a number Pipedrive gave.
fn offset_of(cursor: &str) -> Option<u64> {
    let digits = cursor.strip_prefix(OFFSET)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offset_cursor_is_its_prefix_and_digits_and_nothing_else() {
        assert_eq!(offset_of("offset:0"), Some(0));
        assert_eq!(offset_of("offset:200"), Some(200));
        for forged in [
            "",
            "200",
            "offset:",
            "offset:-1",
            "offset:+1",
            "offset: 1",
            "offset:1 ",
            "offset:1.5",
            "offset:1&limit=500",
            "offset:1/../../users",
            "OFFSET:1",
            "offset:٣",
            "offset:99999999999999999999999",
            "eyJpZCI6MX0",
        ] {
            assert_eq!(offset_of(forged), None, "{forged:?}");
        }
    }

    #[test]
    fn a_blank_cursor_is_the_first_page() {
        let paging = |cursor: &str| Paging {
            cursor: Some(cursor.to_owned()),
            limit: None,
        };
        assert_eq!(cursor(&paging("  ")), None);
        assert_eq!(cursor(&paging(" eyJpZCI6MX0 ")), Some("eyJpZCI6MX0"));
        assert_eq!(cursor(&Paging::default()), None);
    }
}
