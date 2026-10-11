//! Properties: the fields an object type has, in this account.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::text::nullable;

/// A property as a row of a list: enough to choose it by.
///
/// `properties.get` returns the rest: its description, and the options of
/// one that takes a value from a list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct PropertySummary {
    /// The internal name, which is what every other call names the property by.
    pub name: String,
    /// The name a person sees in HubSpot.
    #[serde(deserialize_with = "nullable")]
    pub label: String,
    /// The kind of value: `string`, `number`, `date`, `datetime`, `enumeration`, `bool`.
    #[serde(rename = "type", deserialize_with = "nullable")]
    pub kind: String,
    /// How HubSpot shows it: `text`, `textarea`, `select`, `checkbox`, `date` and so on.
    #[serde(deserialize_with = "nullable")]
    pub field_type: String,
    pub group_name: Option<String>,
    /// HubSpot works the value out itself; it cannot be set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calculated: Option<bool>,
    /// No two records may have the same value, so a record can be found by it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_unique_value: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
}

/// One property, with everything HubSpot says about it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Property {
    pub name: String,
    #[serde(deserialize_with = "nullable")]
    pub label: String,
    /// The kind of value: `string`, `number`, `date`, `datetime`, `enumeration`, `bool`.
    #[serde(rename = "type", deserialize_with = "nullable")]
    pub kind: String,
    #[serde(deserialize_with = "nullable")]
    pub field_type: String,
    pub group_name: Option<String>,
    pub description: Option<String>,
    /// The values the property can take, when it takes one from a list.
    /// A record is set to an option's `value`, not to its `label`.
    #[serde(deserialize_with = "nullable")]
    pub options: Vec<PropertyOption>,
    pub calculated: Option<bool>,
    pub has_unique_value: Option<bool>,
    pub hidden: Option<bool>,
    /// One of HubSpot's own properties, and not one the account made.
    pub hubspot_defined: Option<bool>,
    /// The object type the value is the id of, such as `OWNER`.
    pub referenced_object_type: Option<String>,
    pub display_order: Option<i64>,
    /// What may be changed. A property whose `readOnlyValue` is true cannot be set.
    pub modification_metadata: Option<PropertyModification>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub archived: Option<bool>,
}

/// One value a property can take.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct PropertyOption {
    /// What a person sees.
    #[serde(deserialize_with = "nullable")]
    pub label: String,
    /// What a record is set to.
    #[serde(deserialize_with = "nullable")]
    pub value: String,
    pub description: Option<String>,
    pub display_order: Option<i64>,
    pub hidden: Option<bool>,
}

/// What of a property may be changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct PropertyModification {
    pub archivable: Option<bool>,
    pub read_only_definition: Option<bool>,
    pub read_only_options: Option<bool>,
    pub read_only_value: Option<bool>,
}

/// Which properties of an object type to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListProperties {
    /// List the archived properties and no others.
    pub archived: Option<bool>,
}
