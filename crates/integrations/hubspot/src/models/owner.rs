//! Owners: the users and queues records are assigned to.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::{id, nullable};

/// Someone a record can be assigned to. A record's `hubspot_owner_id` holds the `id`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Owner {
    #[serde(deserialize_with = "id")]
    pub id: String,
    pub email: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    /// The id of the HubSpot user behind the owner. It is not the owner's id.
    pub user_id: Option<i64>,
    pub user_id_including_inactive: Option<i64>,
    /// `PERSON` or `QUEUE`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub teams: Vec<Team>,
    pub archived: Option<bool>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// A team an owner belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Team {
    #[serde(deserialize_with = "id")]
    pub id: String,
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    /// Whether this is the owner's main team.
    pub primary: Option<bool>,
}

/// Which owners to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListOwners {
    /// Only the owner with this email address.
    pub email: Option<String>,
    /// List the archived owners instead of the live ones.
    pub archived: Option<bool>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most owners to return in a page, from 1 to 500. HubSpot returns 100 when not given.
    pub limit: Option<u32>,
}

/// How to find one owner.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetOwner {
    /// What the id given is: the owner's id when not given.
    pub id_property: Option<OwnerIdProperty>,
    /// Read an archived owner.
    pub archived: Option<bool>,
}

/// What an owner is looked up by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum OwnerIdProperty {
    /// The owner's id, which is what `hubspot_owner_id` holds.
    #[serde(rename = "id")]
    Id,
    /// The id of the HubSpot user behind the owner.
    #[serde(rename = "userId")]
    UserId,
}
