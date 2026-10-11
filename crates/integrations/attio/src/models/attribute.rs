//! Attributes: the fields of an object or of a list, as the workspace defined them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;
use serde_json::Value;

/// Whether an attribute belongs to an object or to a list. Attio writes it
/// as the first part of the path, and so it is written here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Objects,
    Lists,
}

impl Target {
    /// The part of the path that names it.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Objects => "objects",
            Self::Lists => "lists",
        }
    }
}

/// The id of an attribute.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AttributeId {
    pub workspace_id: String,
    /// The object or the list the attribute belongs to.
    pub object_id: String,
    pub attribute_id: String,
}

/// One field of an object or of a list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Attribute {
    pub id: AttributeId,
    pub title: Option<String>,
    pub description: Option<String>,
    /// The name the attribute goes by in a record's values, a filter and a sort.
    pub api_slug: Option<String>,
    /// What the attribute holds: `text`, `number`, `checkbox`, `currency`,
    /// `date`, `timestamp`, `rating`, `status`, `select`, `record-reference`,
    /// `actor-reference`, `location`, `domain`, `email-address`,
    /// `phone-number`, `interaction` or `personal-name`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub is_system_attribute: bool,
    /// False for an attribute Attio fills in itself, which cannot be written.
    #[serde(deserialize_with = "nullable")]
    pub is_writable: bool,
    #[serde(deserialize_with = "nullable")]
    pub is_required: bool,
    /// True when no two records may hold the same value. Only such an
    /// attribute can be what `records.assert` matches on.
    #[serde(deserialize_with = "nullable")]
    pub is_unique: bool,
    /// True when the attribute holds several values at once.
    #[serde(deserialize_with = "nullable")]
    pub is_multiselect: bool,
    #[serde(deserialize_with = "nullable")]
    pub is_default_value_enabled: bool,
    #[serde(deserialize_with = "nullable")]
    pub is_archived: bool,
    /// The value a new record starts with, in Attio's own shape.
    pub default_value: Option<Value>,
    /// The attribute on the other object that mirrors this one, for a
    /// reference that runs both ways.
    pub relationship: Option<Relationship>,
    pub created_at: Option<String>,
    pub config: Option<AttributeConfig>,
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

/// Settings that only some types of attribute have.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AttributeConfig {
    pub currency: Option<CurrencyConfig>,
    pub record_reference: Option<RecordReferenceConfig>,
}

/// How a currency attribute is kept and shown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CurrencyConfig {
    /// The ISO 4217 code a value has when none is written with it, such as `USD`.
    pub default_currency_code: Option<String>,
    /// `code`, `name`, `narrowSymbol` or `symbol`.
    pub display_type: Option<String>,
}

/// Which records a reference may point to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct RecordReferenceConfig {
    /// The objects whose records may be referred to. Any object when absent or empty.
    pub allowed_object_ids: Option<Vec<String>>,
}

/// Which attributes of an object or of a list to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListAttributes {
    /// Whether archived attributes are returned too. They are not unless this is `true`.
    pub show_archived: Option<bool>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most attributes to return in one page, from 1 to 500. 50 when not given.
    pub limit: Option<u32>,
}

/// Whether what was archived is returned too.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ShowArchived {
    /// Archived options or statuses are returned only when this is `true`.
    pub show_archived: Option<bool>,
}

/// The id of one option of a select attribute.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SelectOptionId {
    pub workspace_id: String,
    pub object_id: String,
    pub attribute_id: String,
    pub option_id: String,
}

/// One thing a select attribute can be set to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SelectOption {
    pub id: SelectOptionId,
    /// The option's name, which is also what is written to choose it.
    pub title: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub is_archived: bool,
}

/// The id of one status of a status attribute.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct StatusId {
    pub workspace_id: String,
    pub object_id: String,
    pub attribute_id: String,
    pub status_id: String,
}

/// One stage a status attribute can be at, such as a stage of a pipeline.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Status {
    pub id: StatusId,
    /// The status's name, which is also what is written to choose it.
    pub title: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub is_archived: bool,
    #[serde(deserialize_with = "nullable")]
    pub celebration_enabled: bool,
    /// How long a record is meant to stay at this status, as an ISO 8601 duration.
    pub target_time_in_status: Option<String>,
}
