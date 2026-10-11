//! The fields a company's deals, persons and organisations have.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::nullable::nullable;

/// One field of a deal, a person or an organisation: Pipedrive's own, or one
/// the company added.
///
/// A custom field's value sits in a record's `custom_fields` under the
/// field's `field_code`, a 40-character key that says nothing by itself.
/// `field_name` is what the company called it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Field {
    /// The field's key. For a custom field, the key its value is under in `custom_fields`.
    pub field_code: String,
    /// The field's name as people see it.
    pub field_name: Option<String>,
    /// The kind of value: `varchar`, `text`, `double`, `monetary`, `date`,
    /// `enum` (one option), `set` (several options), `user`, `org`, `people`,
    /// `address`, and so on.
    pub field_type: Option<String>,
    /// True for a field the company added.
    pub is_custom_field: Option<bool>,
    /// True for a field Pipedrive leaves out of a record unless it is asked for.
    pub is_optional_response_field: Option<bool>,
    pub description: Option<String>,
    /// The choices of an `enum` or `set` field. A record holds the `id` of each chosen one.
    #[serde(default, deserialize_with = "nullable")]
    pub options: Vec<FieldOption>,
    /// The parts of a field that holds more than one value, such as the
    /// amount and currency of a `monetary` field, as Pipedrive sends them.
    #[serde(default, deserialize_with = "nullable")]
    pub subfields: Vec<Value>,
}

/// One choice of an `enum` or `set` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FieldOption {
    /// What a record holds for this choice: a number for a custom field, a
    /// word for some of Pipedrive's own.
    #[serde(default)]
    pub id: Value,
    /// The choice as people see it.
    pub label: Option<String>,
    pub color: Option<String>,
}
