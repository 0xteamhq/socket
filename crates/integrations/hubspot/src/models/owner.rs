//! Owners: the people and queues a record can be assigned to.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::text::{id, nullable};

/// Someone a record can be assigned to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Owner {
    /// What a record's `hubspot_owner_id` property is set to.
    #[serde(deserialize_with = "id")]
    pub id: String,
    pub email: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    /// `PERSON` or `QUEUE`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// The id of the HubSpot user behind the owner. It is not what a record
    /// is assigned to; `id` is.
    pub user_id: Option<i64>,
    #[serde(deserialize_with = "nullable")]
    pub teams: Vec<OwnerTeam>,
    pub archived: Option<bool>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// A team an owner is in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct OwnerTeam {
    #[serde(deserialize_with = "id")]
    pub id: String,
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    /// Whether it is the owner's main team.
    pub primary: Option<bool>,
}

/// Which owners to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListOwners {
    /// Return only the owner with this email address.
    pub email: Option<String>,
    /// List the archived owners and no others.
    pub archived: Option<bool>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most owners to return. HubSpot returns 100 when not given.
    pub limit: Option<u32>,
}

/// How to find one owner.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetOwner {
    /// Which id was given: the owner's own when not given.
    pub id_property: Option<OwnerIdProperty>,
    /// Look among the archived owners and no others.
    pub archived: Option<bool>,
}

/// Which of an owner's two ids is meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum OwnerIdProperty {
    /// The owner's own id, which records are assigned to.
    #[serde(rename = "id")]
    Id,
    /// The id of the HubSpot user behind the owner.
    #[serde(rename = "userId")]
    UserId,
}
