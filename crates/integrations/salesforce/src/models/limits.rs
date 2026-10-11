//! What is left of an organisation's allowances.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One allowance: how much there is, and how much of it is left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct Limit {
    /// The organisation's allowance.
    pub max: i64,
    /// How much of it is left.
    pub remaining: i64,
}

/// An organisation's allowances, by Salesforce's own names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Limits {
    /// API requests in the last 24 hours. Every call through this
    /// integration counts against it, and when none is left every call is
    /// refused until earlier ones are a day old.
    #[serde(rename = "DailyApiRequests")]
    pub daily_api_requests: Limit,
    /// Every other allowance Salesforce reports, such as `DataStorageMB`,
    /// `DailyBulkApiBatches` or `SingleEmail`.
    #[serde(flatten)]
    pub others: BTreeMap<String, Limit>,
}
