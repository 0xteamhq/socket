//! Deals.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;
use super::{CustomFields, SortDirection};

/// A deal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Deal {
    pub id: u64,
    pub title: Option<String>,
    /// The user who owns the deal.
    pub owner_id: Option<u64>,
    pub person_id: Option<u64>,
    pub org_id: Option<u64>,
    pub pipeline_id: Option<u64>,
    pub stage_id: Option<u64>,
    pub value: Option<f64>,
    /// The currency of `value`, as a three-letter code.
    pub currency: Option<String>,
    /// `open`, `won`, `lost` or `deleted`.
    pub status: Option<String>,
    /// How likely the deal is to be won, as a percentage.
    pub probability: Option<f64>,
    /// The day the deal is expected to close, as `YYYY-MM-DD`.
    pub expected_close_date: Option<String>,
    pub add_time: Option<String>,
    pub update_time: Option<String>,
    /// When the deal last moved to another stage.
    pub stage_change_time: Option<String>,
    pub close_time: Option<String>,
    pub won_time: Option<String>,
    pub lost_time: Option<String>,
    pub lost_reason: Option<String>,
    /// Who may see the deal; the meaning of each number depends on the company's plan.
    pub visible_to: Option<u32>,
    pub is_deleted: Option<bool>,
    pub is_archived: Option<bool>,
    #[serde(default, deserialize_with = "nullable")]
    pub label_ids: Vec<u64>,
    /// How the deal was created.
    pub origin: Option<String>,
    /// The fields the company added, each under its key. A list leaves them
    /// out unless it is asked for some; `fields.deal_fields` names them.
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "CustomFields::is_empty"
    )]
    pub custom_fields: CustomFields,
}

/// Where a deal stands: what a deal can be given, and what a search can ask for.
// Pipedrive's own create and update take a fourth status, which removes the
// deal. It is not among these, so that a change to a deal can never remove
// one: that is `delete` and nothing else. This is said in a comment and not
// in the documentation, which becomes the schema an agent reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DealStatus {
    Open,
    Won,
    Lost,
}

/// Which deals a list returns, by where they stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DealStatusFilter {
    Open,
    Won,
    Lost,
    /// Deleted in the last 30 days.
    Deleted,
}

/// Which deals to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListDeals {
    /// Only the deals this user owns.
    pub owner_id: Option<u64>,
    /// Only the deals linked to this person.
    pub person_id: Option<u64>,
    /// Only the deals linked to this organisation.
    pub org_id: Option<u64>,
    /// Only the deals in this pipeline.
    pub pipeline_id: Option<u64>,
    /// Only the deals in this stage.
    pub stage_id: Option<u64>,
    /// Only deals with one of these statuses. Every deal that is not deleted when not given.
    pub status: Option<Vec<DealStatusFilter>>,
    /// Only the deals a saved filter matches. Pipedrive then ignores the other filters.
    pub filter_id: Option<u64>,
    /// Only deals changed at or after this time, in RFC 3339: `2026-10-01T00:00:00Z`.
    pub updated_since: Option<String>,
    /// Only deals changed before this time, in RFC 3339.
    pub updated_until: Option<String>,
    /// `id` (the default), `update_time` or `add_time`.
    pub sort_by: Option<String>,
    pub sort_direction: Option<SortDirection>,
    /// The keys of the custom fields to return with each deal, at most 15.
    /// None are returned when not given.
    pub custom_fields: Option<Vec<String>>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most deals to return in a page, from 1 to 500. Pipedrive returns 100 when not given.
    pub limit: Option<u32>,
}

/// What to search deals for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchDeals {
    /// The words to look for: at least two characters, or one with `exact_match`.
    pub term: String,
    /// Where to look: any of `title`, `notes` and `custom_fields`. Everywhere when not given.
    pub fields: Option<Vec<String>>,
    /// Only whole matches of the term, whatever their case.
    pub exact_match: Option<bool>,
    /// Only deals linked to this person.
    pub person_id: Option<u64>,
    /// Only deals linked to this organisation.
    pub organization_id: Option<u64>,
    /// Only deals with this status.
    pub status: Option<DealStatus>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most results to return in a page, from 1 to 100.
    pub limit: Option<u32>,
}

/// A deal to create.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateDeal {
    pub title: String,
    pub value: Option<f64>,
    /// The currency of `value`, as a three-letter code. The company's default when not given.
    pub currency: Option<String>,
    /// The user who owns the deal. The signed-in user when not given.
    pub owner_id: Option<u64>,
    pub person_id: Option<u64>,
    pub org_id: Option<u64>,
    /// The pipeline the deal goes in. The company's default when not given.
    pub pipeline_id: Option<u64>,
    /// The stage the deal starts in. The pipeline's first when not given.
    pub stage_id: Option<u64>,
    /// `open` when not given.
    pub status: Option<DealStatus>,
    /// How likely the deal is to be won, as a percentage.
    pub probability: Option<f64>,
    /// The day the deal is expected to close, as `YYYY-MM-DD`.
    pub expected_close_date: Option<String>,
    /// Why the deal was lost. Only with the status `lost`.
    pub lost_reason: Option<String>,
    pub visible_to: Option<u32>,
    pub label_ids: Option<Vec<u64>>,
    /// Values for the fields the company added, each under its key.
    pub custom_fields: Option<CustomFields>,
}

/// What to change on a deal. A field that is not set is left as it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateDeal {
    pub title: Option<String>,
    pub value: Option<f64>,
    pub currency: Option<String>,
    pub owner_id: Option<u64>,
    pub person_id: Option<u64>,
    pub org_id: Option<u64>,
    pub pipeline_id: Option<u64>,
    /// The stage to move the deal to.
    pub stage_id: Option<u64>,
    /// `won` and `lost` close the deal; `open` reopens it.
    pub status: Option<DealStatus>,
    pub probability: Option<f64>,
    pub expected_close_date: Option<String>,
    /// Why the deal was lost. Only with the status `lost`.
    pub lost_reason: Option<String>,
    pub visible_to: Option<u32>,
    /// The deal's labels. The list replaces the one that was there.
    pub label_ids: Option<Vec<u64>>,
    /// Values for the fields the company added, each under its key. A value
    /// of `null` clears that field; a key that is not named is left as it is.
    pub custom_fields: Option<CustomFields>,
}
