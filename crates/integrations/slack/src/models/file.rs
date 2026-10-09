//! Files shared in the workspace.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A file shared in the workspace.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct File {
    pub id: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub mimetype: Option<String>,
    pub filetype: Option<String>,
    pub size: Option<u64>,
    pub user: Option<String>,
    pub created: Option<i64>,
    pub permalink: Option<String>,
    /// Needs the token to download.
    pub url_private: Option<String>,
}

/// Which files to list. Slack pages this list by number, not by cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListFiles {
    pub channel: Option<String>,
    pub user: Option<String>,
    /// Comma-separated: `images`, `pdfs`, `snippets`, `gdocs`, `zips`, `spaces`, or `all`.
    pub types: Option<String>,
    pub count: Option<u32>,
    pub page: Option<u32>,
}
