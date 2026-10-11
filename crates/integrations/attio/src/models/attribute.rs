//! Attributes: the fields of an object or a list, with the options and statuses to choose from.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::nullable::nullable;

/// Where an attribute is defined: on an object, or on a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttributeTarget {
    Objects,
    Lists,
}

impl AttributeTarget {
    /// The name Attio's addresses use for it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Objects => "objects",
            Self::Lists => "lists",
        }
    }
}

/// A field of an object or a list.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Attribute {
    pub id: AttributeId,
    #[serde(deserialize_with = "nullable")]
    pub title: String,
    pub description: Option<String>,
    /// The name values are read and written by.
    #[serde(deserialize_with = "nullable")]
    pub api_slug: String,
    /// `text`, `number`, `checkbox`, `currency`, `date`, `timestamp`,
    /// `rating`, `status`, `select`, `record-reference`, `actor-reference`,
    /// `location`, `domain`, `email-address`, `phone-number`,
    /// `personal-name` or `interaction`.
    #[serde(rename = "type", deserialize_with = "nullable")]
    pub kind: String,
    #[serde(deserialize_with = "nullable")]
    pub is_system_attribute: bool,
    /// Whether the API may write it. Attio fills some in itself.
    #[serde(deserialize_with = "nullable")]
    pub is_writable: bool,
    #[serde(deserialize_with = "nullable")]
    pub is_required: bool,
    /// Whether no two records may hold the same value. Such an attribute can
    /// be matched on by `records.assert`.
    #[serde(deserialize_with = "nullable")]
    pub is_unique: bool,
    /// Whether it may hold several values at once.
    #[serde(deserialize_with = "nullable")]
    pub is_multiselect: bool,
    #[serde(deserialize_with = "nullable")]
    pub is_default_value_enabled: bool,
    #[serde(deserialize_with = "nullable")]
    pub is_archived: bool,
    /// The value a new record starts with, as Attio sent it.
    pub default_value: Option<Value>,
    /// The attribute on the other object that mirrors this one, for a
    /// reference that is kept in both directions.
    pub relationship: Option<Relationship>,
    /// What only some kinds have, as Attio sent it: a currency's code, the
    /// objects a reference may point to.
    pub config: Option<Value>,
    pub created_at: Option<String>,
}

/// An attribute's id, with what it is defined on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AttributeId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    /// The object or the list it is defined on.
    #[serde(deserialize_with = "nullable")]
    pub object_id: String,
    #[serde(deserialize_with = "nullable")]
    pub attribute_id: String,
}

/// The other side of a reference between two objects.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Relationship {
    pub id: AttributeId,
    pub object_slug: Option<String>,
    pub title: Option<String>,
    pub api_slug: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub is_multiselect: bool,
}

/// Which attributes of an object or a list to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListAttributes {
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most attributes to return. Every attribute when not given.
    pub limit: Option<u32>,
    /// Whether to include attributes that were archived. Attio leaves them out unless asked.
    pub show_archived: Option<bool>,
}

/// Whether to include what was archived.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ShowArchived {
    /// Whether to include options or statuses that were archived. Attio
    /// leaves them out unless asked.
    pub show_archived: Option<bool>,
}

/// One of the choices of a select attribute.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SelectOption {
    pub id: SelectOptionId,
    /// What a value is written as.
    #[serde(deserialize_with = "nullable")]
    pub title: String,
    #[serde(deserialize_with = "nullable")]
    pub is_archived: bool,
}

/// A select option's id, with the attribute it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SelectOptionId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    #[serde(deserialize_with = "nullable")]
    pub object_id: String,
    #[serde(deserialize_with = "nullable")]
    pub attribute_id: String,
    #[serde(deserialize_with = "nullable")]
    pub option_id: String,
}

/// One of the stages of a status attribute, such as a deal's stage.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Status {
    pub id: StatusId,
    /// What a value is written as.
    #[serde(deserialize_with = "nullable")]
    pub title: String,
    #[serde(deserialize_with = "nullable")]
    pub is_archived: bool,
    #[serde(deserialize_with = "nullable")]
    pub celebration_enabled: bool,
    /// How long a record is meant to stay in this stage, as an ISO 8601 duration.
    pub target_time_in_status: Option<String>,
}

/// A status's id, with the attribute it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct StatusId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    #[serde(deserialize_with = "nullable")]
    pub object_id: String,
    #[serde(deserialize_with = "nullable")]
    pub attribute_id: String,
    #[serde(deserialize_with = "nullable")]
    pub status_id: String,
}
