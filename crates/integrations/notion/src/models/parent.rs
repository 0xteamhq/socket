//! Where a page, a block, a database or a comment lives.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What something is inside of. One of the ids is set, or `workspace`.
///
/// To say where a new page goes, set `page_id` or `data_source_id` and leave
/// the rest out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Parent {
    /// Which of the fields below is set: `page_id`, `data_source_id` and so
    /// on. Notion writes it; it need not be sent.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_id: Option<String>,
    /// A data source of a database: the table a page is a row of.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    /// Set when it sits at the top of the workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<bool>,
}
