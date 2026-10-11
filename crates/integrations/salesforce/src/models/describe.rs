//! What an organisation's objects are, and which fields each has.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// One object type, as the list of all of them describes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct SObjectSummary {
    /// The API name, which every other call takes: `Account`, `Invoice__c`.
    pub name: String,
    /// What people call it.
    #[serde(deserialize_with = "nullable")]
    pub label: String,
    #[serde(deserialize_with = "nullable")]
    pub label_plural: String,
    /// The three characters every id of this type starts with.
    pub key_prefix: Option<String>,
    /// True for an object the organisation defined itself.
    #[serde(deserialize_with = "nullable")]
    pub custom: bool,
    /// Whether the account may query it, search it, and create, change or
    /// delete its records.
    #[serde(deserialize_with = "nullable")]
    pub queryable: bool,
    #[serde(deserialize_with = "nullable")]
    pub searchable: bool,
    #[serde(deserialize_with = "nullable")]
    pub createable: bool,
    #[serde(deserialize_with = "nullable")]
    pub updateable: bool,
    #[serde(deserialize_with = "nullable")]
    pub deletable: bool,
}

/// Which object types to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListSObjects {
    /// Only types whose API name or label contains this text, whatever its
    /// case. An organisation has hundreds of types, so say what is looked for.
    pub contains: Option<String>,
    /// `true` for only the types the organisation defined itself, `false`
    /// for only Salesforce's own.
    pub custom: Option<bool>,
}

/// One object type in full: its fields and what refers to it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Describe {
    pub name: String,
    #[serde(deserialize_with = "nullable")]
    pub label: String,
    #[serde(deserialize_with = "nullable")]
    pub label_plural: String,
    pub key_prefix: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub custom: bool,
    #[serde(deserialize_with = "nullable")]
    pub queryable: bool,
    #[serde(deserialize_with = "nullable")]
    pub searchable: bool,
    #[serde(deserialize_with = "nullable")]
    pub createable: bool,
    #[serde(deserialize_with = "nullable")]
    pub updateable: bool,
    #[serde(deserialize_with = "nullable")]
    pub deletable: bool,
    /// The fields the account can see. One hidden from it by field-level
    /// security is not listed at all.
    #[serde(deserialize_with = "nullable")]
    pub fields: Vec<Field>,
    /// The object types that refer to this one, each of which a subquery can read.
    #[serde(deserialize_with = "nullable")]
    pub child_relationships: Vec<ChildRelationship>,
    /// The record types the organisation has defined for this object.
    #[serde(deserialize_with = "nullable")]
    pub record_type_infos: Vec<RecordType>,
}

/// One field of an object type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Field {
    /// The API name, as a query and a record's fields write it.
    pub name: String,
    #[serde(deserialize_with = "nullable")]
    pub label: String,
    /// Salesforce's name for the kind of value: `string`, `picklist`,
    /// `reference`, `currency`, `datetime`, `boolean` and so on.
    #[serde(rename = "type", deserialize_with = "nullable")]
    pub field_type: String,
    /// The longest text it holds; 0 for a field that is not text.
    pub length: Option<i64>,
    /// For a number: how many digits in all, and how many after the point.
    pub precision: Option<i64>,
    pub scale: Option<i64>,
    /// False for a field that must have a value. Together with
    /// `defaultedOnCreate` and `createable` it says what a new record needs:
    /// a field that is createable, not nillable and not defaulted is required.
    #[serde(deserialize_with = "nullable")]
    pub nillable: bool,
    #[serde(deserialize_with = "nullable")]
    pub defaulted_on_create: bool,
    /// Whether the account may set it on a new record, and change it later.
    #[serde(deserialize_with = "nullable")]
    pub createable: bool,
    #[serde(deserialize_with = "nullable")]
    pub updateable: bool,
    /// Whether a query may filter on it, and sort by it.
    #[serde(deserialize_with = "nullable")]
    pub filterable: bool,
    #[serde(deserialize_with = "nullable")]
    pub sortable: bool,
    /// True for a field the organisation defined itself.
    #[serde(deserialize_with = "nullable")]
    pub custom: bool,
    /// True for a field Salesforce works out, such as a formula.
    #[serde(deserialize_with = "nullable")]
    pub calculated: bool,
    /// True for a field that holds another system's id for the record, which
    /// `records.get_by_external_id` and `records.upsert` can address it by.
    #[serde(deserialize_with = "nullable")]
    pub external_id: bool,
    #[serde(deserialize_with = "nullable")]
    pub unique: bool,
    /// True for a field a record can be looked up or upserted by.
    #[serde(deserialize_with = "nullable")]
    pub id_lookup: bool,
    /// True for the field that holds the record's name.
    #[serde(deserialize_with = "nullable")]
    pub name_field: bool,
    /// For a `reference`: the object types it may point to.
    #[serde(deserialize_with = "nullable")]
    pub reference_to: Vec<String>,
    /// For a `reference`: the name a query reaches the other record by,
    /// such as `Owner` for `OwnerId`.
    pub relationship_name: Option<String>,
    /// For a `picklist`: the values it takes.
    #[serde(deserialize_with = "nullable")]
    pub picklist_values: Vec<PicklistValue>,
    /// The help the organisation wrote for whoever fills the field in.
    pub inline_help_text: Option<String>,
}

/// One value of a picklist.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct PicklistValue {
    /// What is stored, and what a record is written with.
    #[serde(deserialize_with = "nullable")]
    pub value: String,
    /// What people see, when it differs.
    pub label: Option<String>,
    /// False for a value that was retired: old records hold it, new ones cannot.
    #[serde(deserialize_with = "nullable")]
    pub active: bool,
    #[serde(deserialize_with = "nullable")]
    pub default_value: bool,
}

/// An object type that refers to the one described.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ChildRelationship {
    /// The object type that refers to this one.
    #[serde(rename = "childSObject", deserialize_with = "nullable")]
    pub child_sobject: String,
    /// Its field that holds the reference.
    #[serde(deserialize_with = "nullable")]
    pub field: String,
    /// The name a subquery reads those records by, such as `Contacts`.
    /// Absent when the relationship cannot be queried from this side.
    #[serde(rename = "relationshipName")]
    pub relationship_name: Option<String>,
    /// True when deleting a record of this type deletes those that refer to it.
    #[serde(rename = "cascadeDelete", deserialize_with = "nullable")]
    pub cascade_delete: bool,
}

/// One record type of an object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct RecordType {
    /// The id a record's `RecordTypeId` field takes.
    #[serde(deserialize_with = "nullable")]
    pub record_type_id: String,
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    pub developer_name: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub active: bool,
    /// Whether the account may use it.
    #[serde(deserialize_with = "nullable")]
    pub available: bool,
    /// True for the one a new record gets when none is named.
    #[serde(deserialize_with = "nullable")]
    pub default_record_type_mapping: bool,
    /// True for the stand-in every object has when it defines no record types.
    #[serde(deserialize_with = "nullable")]
    pub master: bool,
}
