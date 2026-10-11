//! Searching the records of any object type.
//!
//! A search is a read that HubSpot takes as a POST, and it has two limits
//! of its own: it returns the first 10,000 matches and no more, and the
//! account may make five searches a second. The rest of the group is in
//! `objects.rs`.

use serde::Serialize;
use serde_json::{Map, Value, json};
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::{Objects, cursor, next_cursor};
use crate::Limit;
use crate::models::{Search, SearchResults};

/// How many matches a search returns before HubSpot stops.
const SEARCH_CEILING: u64 = 10_000;
/// The page HubSpot returns when a search names no size.
const SEARCH_PAGE: u64 = 10;
/// The longest `query` HubSpot takes, in characters.
const LONGEST_QUERY: usize = 3_000;

impl Objects<'_> {
    /// Searches the records of an object type by words, by conditions on
    /// their properties, or both. Changes nothing; HubSpot takes it as a POST.
    ///
    /// A search returns the first 10,000 matches and no more. The page that
    /// reaches the last of them comes back with no cursor, as a last page
    /// does, and `total` says how many matched in all; a cursor past them is
    /// refused.
    pub async fn search(&self, object_type: &str, search: Search) -> Result<SearchResults> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        let path = format!("{}/search", self.path(object_type)?);
        self.0.limit(Some(200), search.limit)?;
        if search
            .query
            .as_deref()
            .is_some_and(|query| query.chars().count() > LONGEST_QUERY)
        {
            return Err(invalid("`query` is at most 3,000 characters"));
        }
        if search.sorts.as_deref().is_some_and(|sorts| sorts.len() > 1) {
            return Err(invalid("`sorts` takes one sort: HubSpot applies no more"));
        }
        // A search's cursor is the number of matches already read.
        let start = match cursor(search.cursor.as_deref()) {
            None => 0,
            Some(place) => place
                .parse::<u64>()
                .map_err(|_| invalid("`cursor` is not a place in a search; pass back `next_cursor` unchanged"))?,
        };
        if start >= SEARCH_CEILING {
            return Err(invalid(
                "hubspot's search returns the first 10,000 matches and no more; narrow the filters, \
                 or sort by a property and filter on the last value read to go on from there",
            ));
        }
        // HubSpot answers 400 to paging past the 10,000th match. So a page
        // that would end past it is asked for only as far as it: the caller
        // gets the matches there are, and HubSpot is never asked for more.
        let left = SEARCH_CEILING - start;
        let page = search.limit.map_or(SEARCH_PAGE, u64::from);
        let mut body = with(json!({}), &Search { cursor: None, ..search });
        if page > left {
            body["limit"] = json!(left);
        }
        if start > 0 {
            body["after"] = json!(start.to_string());
        }
        let answered = match self.0.send(RawRequest::post(path, body)).await {
            Ok(answered) => answered,
            Err(refused) => return Err(self.search_refused(refused, page >= left)),
        };
        let total = answered["total"].as_u64().ok_or_else(|| {
            self.0
                .error(ErrorKind::Decode, "hubspot answered without the number of matches")
        })?;
        // The place after the last match a search returns is no next page.
        let reachable = |next: &String| next.parse::<u64>().map_or(true, |next| next < SEARCH_CEILING);
        Ok(SearchResults {
            total,
            items: self.0.results(&answered, "records")?,
            next_cursor: next_cursor(&answered).filter(reachable),
        })
    }

    /// What to report when HubSpot refused a search. `to_the_last` says the
    /// page asked for reached the 10,000th match.
    ///
    /// Two refusals are a search's own. Neither replaces what HubSpot said:
    /// its documentation shows what neither looks like, so what it said is
    /// kept and what is known of the search is added.
    fn search_refused(&self, refused: socketkit_core::Error, to_the_last: bool) -> socketkit_core::Error {
        let retry = refused.retry();
        let said = refused.message().trim_end_matches('.').to_owned();
        match (refused.kind(), crate::limit_of(&refused)) {
            // HubSpot no longer holds ordinary calls to a limit by the
            // second, so on a search that policy is the search's own.
            (ErrorKind::RateLimited, Some(Limit::Second)) => self
                .0
                .error(
                    ErrorKind::RateLimited,
                    "hubspot allows five searches a second for the whole account, and this one was over; \
                     listing and reading records are not held to that limit",
                )
                .with_retry(retry)
                .with_source(Limit::Second),
            // Which limit it was is not known, so nothing is claimed about
            // the calls that are not searches.
            (ErrorKind::RateLimited, Some(Limit::Unnamed)) => self
                .0
                .error(
                    ErrorKind::RateLimited,
                    format!("{said}; HubSpot did not say which limit, and allows five searches a second for the whole account"),
                )
                .with_retry(retry)
                .with_source(Limit::Unnamed),
            (ErrorKind::InvalidInput, _) if to_the_last => self.0.error(
                ErrorKind::InvalidInput,
                format!("{said}; this page also reaches the 10,000th match, which is the last a search returns"),
            ),
            _ => refused,
        }
    }
}

/// `base` with the set fields of `options` added. Unset fields are left out
/// at every depth, so HubSpot applies its own defaults.
fn with(base: Value, options: &impl Serialize) -> Value {
    let mut merged = match base {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    if let Ok(Value::Object(extra)) = serde_json::to_value(options) {
        merged.extend(extra);
    }
    without_nulls(Value::Object(merged))
}

fn without_nulls(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(name, value)| (name, without_nulls(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(without_nulls).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_fields_are_left_out_at_every_depth() {
        #[derive(Serialize)]
        struct Options {
            query: Option<String>,
            groups: Vec<Inner>,
        }
        #[derive(Serialize)]
        struct Inner {
            value: Option<String>,
            name: String,
        }
        let merged = with(
            json!({ "limit": 5 }),
            &Options {
                query: None,
                groups: vec![Inner {
                    value: None,
                    name: "email".into(),
                }],
            },
        );
        assert_eq!(merged, json!({ "limit": 5, "groups": [{ "name": "email" }] }));
    }
}
