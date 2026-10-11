//! Records of any object type, found by a search.
//!
//! This is a method of [`Objects`], whose struct and single-record methods
//! are in `objects.rs`.

use serde_json::json;
use socketkit_core::{Error, ErrorKind, Page, RawRequest, Result};

use super::objects::Objects;
use super::{place, with};
use crate::models::{Filter, Operator, Record, Search};
use crate::unnamed_limit;

/// The most results HubSpot returns for one search, however it is paged.
const MOST_RESULTS: u64 = 10_000;

impl Objects<'_> {
    /// Searches the records of a type: by filters, by words, or both. Changes nothing.
    ///
    /// HubSpot offers a search only as POST. It returns at most 10,000
    /// results for one search, and allows an account about five searches a
    /// second; each of the two is reported as its own error.
    pub async fn search(&self, object_type: &str, search: Search) -> Result<Page<Record>> {
        let path = format!("{}/search", self.0.records(object_type)?);
        self.0.limit(200, search.limit)?;
        self.searchable(&search)?;
        let cursor = place(search.cursor.as_deref());
        // HubSpot's cursor for a search is the number of results before it.
        let reached = cursor.and_then(|cursor| cursor.parse::<u64>().ok());
        if reached.is_some_and(|reached| reached >= MOST_RESULTS) {
            return Err(self.past_the_last_result());
        }
        let mut content = with(json!({}), &search);
        if let Some(fields) = content.as_object_mut() {
            fields.remove("cursor");
            if let Some(cursor) = cursor {
                fields.insert("after".to_owned(), json!(cursor));
            }
        }
        let asked = reached.unwrap_or(0) + u64::from(search.limit.unwrap_or(10));
        match self.0.send(RawRequest::post(path, content)).await {
            Ok(body) => self.0.paged(&body, "records"),
            Err(refused) => Err(self.search_refusal(refused, asked > MOST_RESULTS)),
        }
    }

    /// HubSpot's refusal of a search, as the limit of search it stands for
    /// when it stands for one.
    fn search_refusal(&self, refused: Error, past_the_cap: bool) -> Error {
        let provider = &self.0.connection.provider().id;
        match refused.kind() {
            // A 429 that names neither of the account's general limits is search's own.
            ErrorKind::RateLimited if refused.message() == unnamed_limit(provider) => self
                .0
                .error(
                    ErrorKind::RateLimited,
                    "hubspot allows an account about five searches a second, whatever its other limits, \
                     and they are used up; wait a second and try again",
                )
                .with_retry(refused.retry()),
            // HubSpot's 400 past the cap gives no reason. One that has
            // another cause gives it, so its words are kept and the cap is
            // named beside them.
            ErrorKind::InvalidInput if past_the_cap => self.0.error(
                ErrorKind::InvalidInput,
                format!(
                    "{}. This page reaches past 10,000 results, the most hubspot returns for one search; \
                     if that is the reason, narrow the filters, or sort by a property and filter on the last value that was read",
                    refused.message().trim_end_matches('.')
                ),
            ),
            _ => refused,
        }
    }

    fn past_the_last_result(&self) -> Error {
        self.0.error(
            ErrorKind::InvalidInput,
            "hubspot returns at most 10,000 results for one search, and this page lies past them; \
             narrow the filters, or sort by a property and filter on the last value that was read",
        )
    }

    /// What HubSpot's search refuses, refused before it is called.
    fn searchable(&self, search: &Search) -> Result<()> {
        let invalid = |message: &str| Err(self.0.error(ErrorKind::InvalidInput, message));
        if search.query.as_ref().is_some_and(|query| query.chars().count() > 3000) {
            return invalid("`query` takes at most 3,000 characters");
        }
        let groups = search.filter_groups.as_deref().unwrap_or_default();
        if groups.len() > 5 {
            return invalid("`filterGroups` takes at most 5 groups");
        }
        if groups.iter().any(|group| group.filters.is_empty()) {
            return invalid("every group of `filterGroups` needs at least one filter");
        }
        if groups.iter().any(|group| group.filters.len() > 6) {
            return invalid("a group of `filterGroups` takes at most 6 filters");
        }
        if groups.iter().map(|group| group.filters.len()).sum::<usize>() > 18 {
            return invalid("a search takes at most 18 filters in all");
        }
        for filter in groups.iter().flat_map(|group| &group.filters) {
            self.filter(filter)?;
        }
        let sorts = search.sorts.as_deref().unwrap_or_default();
        if sorts.len() > 1 {
            return invalid("`sorts` takes one sort: HubSpot applies no more");
        }
        if sorts.iter().any(|sort| sort.property_name.trim().is_empty()) {
            return invalid("a sort needs `propertyName`");
        }
        self.0.names("properties", search.properties.as_deref()).map(drop)
    }

    /// A filter names a property, and carries what its operator compares with.
    fn filter(&self, filter: &Filter) -> Result<()> {
        let invalid = |message: &str| Err(self.0.error(ErrorKind::InvalidInput, message));
        if filter.property_name.trim().is_empty() {
            return invalid("every filter needs `propertyName`");
        }
        let has = |value: &Option<String>| value.is_some();
        match filter.operator {
            Operator::HasProperty | Operator::NotHasProperty => Ok(()),
            Operator::In | Operator::NotIn if filter.values.as_ref().is_none_or(Vec::is_empty) => {
                invalid("a filter with `IN` or `NOT_IN` needs `values`")
            }
            Operator::In | Operator::NotIn => Ok(()),
            Operator::Between if !(has(&filter.value) && has(&filter.high_value)) => {
                invalid("a filter with `BETWEEN` needs `value` and `highValue`")
            }
            _ if !has(&filter.value) => invalid("a filter needs `value`"),
            _ => Ok(()),
        }
    }
}
