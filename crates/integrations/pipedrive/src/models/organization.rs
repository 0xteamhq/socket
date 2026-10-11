//! Organisations: the companies a company deals with.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;
use super::{Address, CustomFields, SortDirection};

/// An organisation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Organization {
    pub id: u64,
    pub name: Option<String>,
    /// The user who owns the organisation.
    pub owner_id: Option<u64>,
    pub address: Option<Address>,
    pub website: Option<String>,
    pub linkedin: Option<String>,
    /// The id of one of the choices of the `industry` field; `fields.organization_fields` lists them.
    pub industry: Option<u64>,
    pub annual_revenue: Option<f64>,
    pub employee_count: Option<u64>,
    pub add_time: Option<String>,
    pub update_time: Option<String>,
    pub visible_to: Option<u32>,
    pub is_deleted: Option<bool>,
    #[serde(default, deserialize_with = "nullable")]
    pub label_ids: Vec<u64>,
    /// The fields the company added, each under its key. A list leaves them
    /// out unless it is asked for some; `fields.organization_fields` names them.
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "CustomFields::is_empty"
    )]
    pub custom_fields: CustomFields,
}

/// Which organisations to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListOrganizations {
    /// Only the organisations this user owns.
    pub owner_id: Option<u64>,
    /// Only the organisations a saved filter matches. Pipedrive then ignores `owner_id`.
    pub filter_id: Option<u64>,
    /// Only organisations changed at or after this time, in RFC 3339.
    pub updated_since: Option<String>,
    /// Only organisations changed before this time, in RFC 3339.
    pub updated_until: Option<String>,
    /// `id` (the default), `update_time` or `add_time`.
    pub sort_by: Option<String>,
    pub sort_direction: Option<SortDirection>,
    /// The keys of the custom fields to return with each organisation, at
    /// most 15. None are returned when not given.
    pub custom_fields: Option<Vec<String>>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most organisations to return in a page, from 1 to 500. Pipedrive returns 100 when not given.
    pub limit: Option<u32>,
}

/// What to search organisations for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchOrganizations {
    /// The words to look for: at least two characters, or one with `exact_match`.
    pub term: String,
    /// Where to look: any of `name`, `address`, `notes` and `custom_fields`. Everywhere when not given.
    pub fields: Option<Vec<String>>,
    /// Only whole matches of the term, whatever their case.
    pub exact_match: Option<bool>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most results to return in a page, from 1 to 100.
    pub limit: Option<u32>,
}

/// An organisation to create.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateOrganization {
    pub name: String,
    /// The user who owns the organisation. The signed-in user when not given.
    pub owner_id: Option<u64>,
    pub address: Option<Address>,
    pub website: Option<String>,
    pub linkedin: Option<String>,
    /// The id of one of the choices of the `industry` field.
    pub industry: Option<u64>,
    pub annual_revenue: Option<u64>,
    pub employee_count: Option<u64>,
    pub visible_to: Option<u32>,
    pub label_ids: Option<Vec<u64>>,
    /// Values for the fields the company added, each under its key.
    pub custom_fields: Option<CustomFields>,
}

/// What to change on an organisation. A field that is not set is left as it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateOrganization {
    pub name: Option<String>,
    pub owner_id: Option<u64>,
    pub address: Option<Address>,
    pub website: Option<String>,
    pub linkedin: Option<String>,
    pub industry: Option<u64>,
    pub annual_revenue: Option<u64>,
    pub employee_count: Option<u64>,
    pub visible_to: Option<u32>,
    /// The organisation's labels. The list replaces the one that was there.
    pub label_ids: Option<Vec<u64>>,
    /// Values for the fields the company added, each under its key. A value
    /// of `null` clears that field; a key that is not named is left as it is.
    pub custom_fields: Option<CustomFields>,
}
