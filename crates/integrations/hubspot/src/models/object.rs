//! CRM records: what HubSpot returns for one, and the options and content
//! used to read, create and change one.
//!
//! One shape serves every object type, because HubSpot's CRM is one API
//! over all of them: a contact, a deal, a note and a record of a custom
//! object differ only in which properties they carry.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::AssociationSpec;
use super::text::{id, nullable};

/// One record of any object type.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Object {
    #[serde(deserialize_with = "id")]
    pub id: String,
    /// The properties that were asked for, beside the few HubSpot always
    /// returns. A property without a value is `null`. HubSpot writes every
    /// value as text, a number and a date included.
    #[serde(deserialize_with = "nullable")]
    pub properties: BTreeMap<String, Option<String>>,
    /// The earlier values of the properties asked for with
    /// `propertiesWithHistory`, each with when it was set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties_with_history: Option<BTreeMap<String, Vec<PropertyValue>>>,
    /// The ids of the associated records asked for with `associations`,
    /// under the name of each object type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub associations: Option<BTreeMap<String, AssociatedIds>>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    /// The address that opens the record in HubSpot, when HubSpot gives one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// One value a property has had.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct PropertyValue {
    pub value: Option<String>,
    /// When the value was set.
    pub timestamp: Option<String>,
    /// What set it: `CRM_UI`, `API`, `IMPORT` and so on.
    pub source_type: Option<String>,
    pub source_id: Option<String>,
    pub source_label: Option<String>,
    pub updated_by_user_id: Option<i64>,
}

/// The records of one object type that a record is associated with.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AssociatedIds {
    #[serde(deserialize_with = "nullable")]
    pub results: Vec<AssociatedId>,
    /// Present when there are more than these, as HubSpot sent it.
    /// `associations.list` reads them all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paging: Option<Value>,
}

/// One associated record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AssociatedId {
    #[serde(deserialize_with = "id")]
    pub id: String,
    /// The kind of association, such as `contact_to_company`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// Which records of an object type to list, and what to return of each.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListObjects {
    /// The properties to return of each record. HubSpot returns a few of its
    /// own choosing when none is named; `properties.list` names them all.
    pub properties: Option<Vec<String>>,
    /// Object types, such as `companies`, whose associated record ids to
    /// return with each record.
    pub associations: Option<Vec<String>>,
    /// Return the archived records and no others.
    pub archived: Option<bool>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most records to return, from 1 to 100. HubSpot returns 10 when not given.
    pub limit: Option<u32>,
}

/// What to return of one record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetObject {
    /// The properties to return. HubSpot returns a few of its own choosing
    /// when none is named; `properties.list` names them all.
    pub properties: Option<Vec<String>>,
    /// Properties to return with every value they have had.
    pub properties_with_history: Option<Vec<String>>,
    /// Object types, such as `companies`, whose associated record ids to return.
    pub associations: Option<Vec<String>>,
    /// The name of a property whose values are unique, such as `email` for a
    /// contact. The record is then found by that value and not by its id.
    pub id_property: Option<String>,
    /// Look among the archived records and no others.
    pub archived: Option<bool>,
}

/// A record to create.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateObject {
    /// The record's properties by their internal names, each value as text:
    /// `{ "email": "ada@example.com" }`. `properties.list` names them.
    pub properties: BTreeMap<String, String>,
    /// Records to associate the new one with. An engagement such as a note
    /// shows on a record's timeline only when it is associated with it.
    pub associations: Option<Vec<NewAssociation>>,
}

/// A record to associate a new record with, and how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewAssociation {
    pub to: ObjectId,
    /// The kinds of association, by HubSpot's type ids: `202` associates a
    /// note with a contact.
    pub types: Vec<AssociationSpec>,
}

/// A record, named by its id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectId {
    pub id: String,
}

/// The properties to change on a record. The rest are left as they are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateObject {
    /// The properties to set, each value as text. An empty text clears the property.
    pub properties: BTreeMap<String, String>,
    /// The name of a property whose values are unique, such as `email` for a
    /// contact. The record is then found by that value and not by its id.
    pub id_property: Option<String>,
}
