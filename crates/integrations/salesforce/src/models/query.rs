//! Queries in SOQL, and what they return.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Record;

/// How to read the results of a query.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QueryOptions {
    /// The `next_cursor` of the batch before, unchanged; absent for the
    /// first batch. It names Salesforce's place in the results of the query
    /// that made it, so the query is not sent again with it.
    pub cursor: Option<String>,
    /// The most records to return in one batch, from 200 to 2000. Salesforce
    /// returns up to 2000 when not given, and may return fewer than asked.
    /// To get fewer than 200, end the query with `LIMIT`.
    pub batch_size: Option<u32>,
}

/// One batch of a query's results.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QueryResult {
    /// How many records the whole query matches, in this batch and those
    /// after it. For `SELECT COUNT() FROM …` this is the answer, and
    /// `records` is empty.
    #[serde(rename = "totalSize")]
    pub total_size: u64,
    /// True when this batch is the last.
    pub done: bool,
    pub records: Vec<Record>,
    /// Pass this back as `cursor` for the next batch. `None` on the last.
    pub next_cursor: Option<String>,
}
