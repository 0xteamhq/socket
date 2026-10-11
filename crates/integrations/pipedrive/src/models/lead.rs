//! Leads: possible deals that are not yet in a pipeline.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::CustomFields;
use super::nullable::{nullable, text_or_number};

/// A lead. Its id is a UUID, where every other record's is a number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Lead {
    pub id: String,
    pub title: Option<String>,
    /// The user who owns the lead.
    pub owner_id: Option<u64>,
    pub creator_id: Option<u64>,
    pub person_id: Option<u64>,
    pub organization_id: Option<u64>,
    /// What the lead might be worth.
    pub value: Option<LeadValue>,
    /// The day the lead is expected to close, as `YYYY-MM-DD`.
    pub expected_close_date: Option<String>,
    /// The ids of the lead's labels, which are UUIDs too.
    #[serde(default, deserialize_with = "nullable")]
    pub label_ids: Vec<String>,
    /// Where the lead came from, in words: `API`, `Manually created`, and so on.
    pub source_name: Option<String>,
    pub origin: Option<String>,
    pub is_archived: Option<bool>,
    /// Whether anyone has opened the lead in Pipedrive.
    pub was_seen: Option<bool>,
    pub next_activity_id: Option<u64>,
    pub add_time: Option<String>,
    pub update_time: Option<String>,
    /// Who may see the lead: `1`, `3`, `5` or `7`; the meaning of each depends on the company's plan.
    #[serde(default, deserialize_with = "text_or_number")]
    pub visible_to: Option<String>,
    /// The fields the company added, each under its key. A lead has a deal's
    /// fields, which `fields.deal_fields` names. A list leaves them out.
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "CustomFields::is_empty"
    )]
    pub custom_fields: CustomFields,
}

/// A sum of money on a lead.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LeadValue {
    pub amount: f64,
    /// A three-letter currency code.
    pub currency: String,
}

/// Which leads to list. Archived leads are not listed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListLeads {
    /// Only the leads this user owns.
    pub owner_id: Option<u64>,
    /// Only the leads linked to this person.
    pub person_id: Option<u64>,
    /// Only the leads linked to this organisation.
    pub organization_id: Option<u64>,
    /// Only the leads a saved filter matches. Pipedrive then ignores the other filters.
    pub filter_id: Option<u64>,
    /// Only leads changed at or after this time, in RFC 3339.
    pub updated_since: Option<String>,
    /// A field and a direction, such as `update_time DESC`; several are separated by commas.
    pub sort: Option<String>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most leads to return in a page, from 1 to 500. Pipedrive returns 100 when not given.
    pub limit: Option<u32>,
}

/// What to search leads for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchLeads {
    /// The words to look for: at least two characters, or one with `exact_match`.
    pub term: String,
    /// Where to look: any of `title`, `notes` and `custom_fields`. Everywhere when not given.
    pub fields: Option<Vec<String>>,
    /// Only whole matches of the term, whatever their case.
    pub exact_match: Option<bool>,
    /// Only leads linked to this person.
    pub person_id: Option<u64>,
    /// Only leads linked to this organisation.
    pub organization_id: Option<u64>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most results to return in a page, from 1 to 100.
    pub limit: Option<u32>,
}

/// A lead to create. It has to be linked to a person, an organisation or both.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateLead {
    pub title: String,
    pub person_id: Option<u64>,
    pub organization_id: Option<u64>,
    /// The user who owns the lead. The signed-in user when not given.
    pub owner_id: Option<u64>,
    pub value: Option<LeadValue>,
    /// The day the lead is expected to close, as `YYYY-MM-DD`.
    pub expected_close_date: Option<String>,
    /// The ids of lead labels.
    pub label_ids: Option<Vec<String>>,
    /// Who may see the lead: `1`, `3`, `5` or `7`.
    pub visible_to: Option<String>,
    pub was_seen: Option<bool>,
}

/// What to change on a lead. A field that is not set is left as it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateLead {
    pub title: Option<String>,
    pub person_id: Option<u64>,
    pub organization_id: Option<u64>,
    pub owner_id: Option<u64>,
    pub value: Option<LeadValue>,
    pub expected_close_date: Option<String>,
    /// The lead's labels. The list replaces the one that was there.
    pub label_ids: Option<Vec<String>>,
    pub visible_to: Option<String>,
    pub was_seen: Option<bool>,
    /// Archives the lead, or brings it back.
    pub is_archived: Option<bool>,
}
