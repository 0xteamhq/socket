//! Properties: the fields of an object type.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// One field of an object type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Property {
    /// The internal name, which is how the property is named everywhere in the API.
    pub name: String,
    /// The name a person sees.
    #[serde(deserialize_with = "nullable")]
    pub label: String,
    /// How the value is stored: `string`, `number`, `date`, `datetime`,
    /// `enumeration`, `bool` and so on.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// How the value is edited in HubSpot: `text`, `textarea`, `select`,
    /// `checkbox`, `date`, `number` and so on.
    pub field_type: Option<String>,
    pub description: Option<String>,
    /// The group the property is shown in.
    pub group_name: Option<String>,
    /// The values the property may take, for an `enumeration`. Empty otherwise.
    #[serde(deserialize_with = "nullable")]
    pub options: Vec<PropertyOption>,
    /// Whether HubSpot works the value out itself.
    pub calculated: Option<bool>,
    /// Whether no two records may share a value.
    pub has_unique_value: Option<bool>,
    pub hidden: Option<bool>,
    /// Whether the property is one of HubSpot's own and not made in the account.
    pub hubspot_defined: Option<bool>,
    /// The object type the value refers to, such as `OWNER`.
    pub referenced_object_type: Option<String>,
    pub display_order: Option<i64>,
    pub form_field: Option<bool>,
    /// Whether the options are kept outside the property, as owners are.
    pub external_options: Option<bool>,
    /// What may be changed about the property and its value.
    pub modification_metadata: Option<ModificationMetadata>,
    pub archived: Option<bool>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// One value an `enumeration` property may take.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct PropertyOption {
    /// The name a person sees.
    #[serde(deserialize_with = "nullable")]
    pub label: String,
    /// The value to write when setting the property.
    #[serde(deserialize_with = "nullable")]
    pub value: String,
    pub description: Option<String>,
    pub display_order: Option<i64>,
    pub hidden: Option<bool>,
}

/// What may be changed about a property.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ModificationMetadata {
    /// Whether the value on a record cannot be set through the API.
    pub read_only_value: Option<bool>,
    pub read_only_definition: Option<bool>,
    pub read_only_options: Option<bool>,
    pub archivable: Option<bool>,
}

/// Which properties of an object type to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListProperties {
    /// List the archived properties instead of the live ones.
    pub archived: Option<bool>,
}
