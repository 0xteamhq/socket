//! Search results, from one kind of record or from all of them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// One search result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SearchResult {
    /// How well the item matches; higher is better.
    pub result_score: Option<f64>,
    pub item: SearchItem,
}

/// What a search found: enough of a record to tell it from the others. The
/// record itself is read with its group's `get`.
///
/// Which fields are set depends on the kind of record: a deal has a `title`
/// and a `stage`, a person a `name` and `emails`. A list that is empty is
/// not written, so that a deal does not read as a record with no emails.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SearchItem {
    pub id: ItemId,
    /// The kind of record: `deal`, `person`, `organization`, `lead`, and so on.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// The title of a deal or a lead.
    pub title: Option<String>,
    /// The name of a person or an organisation.
    pub name: Option<String>,
    pub value: Option<f64>,
    pub currency: Option<String>,
    /// The status of a deal.
    pub status: Option<String>,
    pub owner: Option<Linked>,
    pub stage: Option<Linked>,
    pub person: Option<Linked>,
    pub organization: Option<Linked>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Vec::is_empty")]
    pub emails: Vec<String>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Vec::is_empty")]
    pub phones: Vec<String>,
    /// The address of an organisation, on one line.
    pub address: Option<String>,
    /// The values of the custom fields the term was found in.
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Vec::is_empty")]
    pub custom_fields: Vec<String>,
    /// The notes the term was found in.
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    pub is_archived: Option<bool>,
}

/// A record's id: a number, or a UUID for a lead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ItemId {
    Number(u64),
    Text(String),
}

/// A record a search result is linked to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Linked {
    pub id: Option<u64>,
    pub name: Option<String>,
}

/// A kind of record a search can look through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    Deal,
    Person,
    Organization,
    Lead,
    Product,
    File,
    MailAttachment,
    Project,
}

/// What to search every kind of record for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchItems {
    /// The words to look for: at least two characters, or one with `exact_match`.
    pub term: String,
    /// The kinds of record to look through. Every kind when not given.
    pub item_types: Option<Vec<ItemType>>,
    /// Where to look: any of `title`, `name`, `email`, `phone`, `address`,
    /// `notes`, `custom_fields`, `code` and `description`. Everywhere when not given.
    pub fields: Option<Vec<String>>,
    /// Only whole matches of the term, whatever their case.
    pub exact_match: Option<bool>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most results to return in a page, from 1 to 100.
    pub limit: Option<u32>,
}
