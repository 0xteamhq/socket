//! Mail folders.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// A folder of a mailbox.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MailFolder {
    pub id: String,
    #[serde(deserialize_with = "nullable")]
    pub display_name: String,
    pub parent_folder_id: Option<String>,
    pub child_folder_count: Option<i64>,
    /// Graph counts every kind of item here, not only mail.
    pub unread_item_count: Option<i64>,
    pub total_item_count: Option<i64>,
    #[serde(deserialize_with = "nullable")]
    pub is_hidden: bool,
}

/// Which folders to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListFolders {
    /// List the folders inside this one: a folder id or a well-known name
    /// such as `inbox`. The top of the mailbox when not given.
    pub parent: Option<String>,
    /// Also list folders Outlook hides. Graph leaves them out unless asked.
    pub include_hidden: Option<bool>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most folders to return in the first page. Graph returns 10 when not given.
    pub limit: Option<u32>,
}
