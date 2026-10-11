//! Records of any object type: what HubSpot returns for them, and the
//! content and options used to read, create and change them.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::AssociationType;
use super::nullable::{id, nullable};

/// A record of any object type: a contact, a company, a deal, a ticket, a
/// note, a call, a meeting, an email, a task, or a record of a custom object.
///
/// HubSpot returns only the properties that were asked for, beside a few it
/// always adds for the type. It writes every value as a string, and `null`
/// where a property that was asked for has no value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Record {
    #[serde(deserialize_with = "id")]
    pub id: String,
    #[serde(deserialize_with = "nullable")]
    pub properties: BTreeMap<String, Option<String>>,
    /// When the record was created, in ISO 8601.
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// Whether the record is in the recycling bin.
    #[serde(deserialize_with = "nullable")]
    pub archived: bool,
    pub archived_at: Option<String>,
    /// The records this one is associated with, by object type. Present only
    /// when associations were asked for, and only for the types that have any.
    pub associations: Option<BTreeMap<String, AssociatedRecords>>,
}

/// The records of one object type that a record is associated with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AssociatedRecords {
    #[serde(deserialize_with = "nullable")]
    pub results: Vec<AssociatedRecord>,
    /// Set when there are more than HubSpot returns beside a record. The rest
    /// are read with `associations.list`.
    pub paging: Option<MoreAssociated>,
}

/// One associated record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AssociatedRecord {
    #[serde(deserialize_with = "id")]
    pub id: String,
    /// The kind of association, such as `contact_to_company`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// Where the associated records that were not returned begin.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct MoreAssociated {
    pub next: Option<NextAssociated>,
}

/// The cursor to give `associations.list` for the associated records that follow.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct NextAssociated {
    #[serde(deserialize_with = "nullable")]
    pub after: String,
}

/// Which records of a type to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListObjects {
    /// The properties to return, by their internal names. HubSpot returns
    /// only these, beside a few it always adds; `properties.list` names them all.
    pub properties: Option<Vec<String>>,
    /// The object types to return the ids of associated records for, such as `companies`.
    pub associations: Option<Vec<String>>,
    /// List the records in the recycling bin instead of the live ones.
    pub archived: Option<bool>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most records to return in a page, from 1 to 100. HubSpot returns 10 when not given.
    pub limit: Option<u32>,
}

/// What to return of one record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetObject {
    /// The properties to return, by their internal names.
    pub properties: Option<Vec<String>>,
    /// The object types to return the ids of associated records for.
    pub associations: Option<Vec<String>>,
    /// Read the record from the recycling bin.
    pub archived: Option<bool>,
    /// The name of a property with unique values, such as `email` for a
    /// contact, when the record is named by that value and not by its id.
    pub id_property: Option<String>,
}

/// A new record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateObject {
    /// The record's properties, by their internal names. Every value is
    /// written as a string, as HubSpot stores it: `"1500"`, `"true"`, a date
    /// in ISO 8601.
    pub properties: BTreeMap<String, String>,
    /// Records to associate the new one with.
    pub associations: Option<Vec<NewAssociation>>,
}

impl CreateObject {
    /// A record with these properties and no associations.
    pub fn with<N: Into<String>, V: Into<String>>(properties: impl IntoIterator<Item = (N, V)>) -> Self {
        Self {
            properties: named(properties),
            associations: None,
        }
    }
}

/// An existing record to associate a new one with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewAssociation {
    pub to: RecordId,
    /// The kinds of association to make. HubSpot's own kinds are listed in
    /// its documentation; a note to a contact is 202.
    pub types: Vec<AssociationType>,
}

/// A record, named by its id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecordId {
    #[serde(deserialize_with = "id")]
    pub id: String,
}

/// What to change on a record. A property that is not named is left as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateObject {
    /// The properties to set, by their internal names, each as a string. An
    /// empty string clears a property.
    pub properties: BTreeMap<String, String>,
    /// The name of a property with unique values, when the record is named
    /// by that value and not by its id.
    pub id_property: Option<String>,
}

impl UpdateObject {
    /// A change to these properties of a record named by its id.
    pub fn with<N: Into<String>, V: Into<String>>(properties: impl IntoIterator<Item = (N, V)>) -> Self {
        Self {
            properties: named(properties),
            id_property: None,
        }
    }
}

fn named<N: Into<String>, V: Into<String>>(properties: impl IntoIterator<Item = (N, V)>) -> BTreeMap<String, String> {
    properties
        .into_iter()
        .map(|(name, value)| (name.into(), value.into()))
        .collect()
}
