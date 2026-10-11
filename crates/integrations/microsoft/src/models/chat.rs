//! Chats: one-to-one, group and meeting conversations outside a channel.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A chat.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Chat {
    pub id: String,
    /// The chat's name. A one-to-one chat has none.
    pub topic: Option<String>,
    /// `oneOnOne`, `group` or `meeting`.
    pub chat_type: Option<String>,
    pub created_date_time: Option<String>,
    pub last_updated_date_time: Option<String>,
    /// The address that opens the chat in Teams.
    pub web_url: Option<String>,
    pub tenant_id: Option<String>,
}

/// The kind of chat to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ChatType {
    /// A chat between two people.
    #[serde(rename = "oneOnOne")]
    OneOnOne,
    /// A chat among several people.
    #[serde(rename = "group")]
    Group,
}

/// A chat to create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateChat {
    pub chat_type: ChatType,
    /// A name for a group chat. A one-to-one chat takes none.
    pub topic: Option<String>,
    /// The people in the chat, each by their directory id or sign-in name.
    /// The account that creates the chat has to be one of them.
    pub members: Vec<String>,
}
