//! Persons: the people a company deals with.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;
use super::{Address, CustomFields, SortDirection};

/// A person.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Person {
    pub id: u64,
    pub name: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    /// The user who owns the person.
    pub owner_id: Option<u64>,
    /// The organisation the person belongs to.
    pub org_id: Option<u64>,
    #[serde(default, deserialize_with = "nullable")]
    pub emails: Vec<ContactDetail>,
    #[serde(default, deserialize_with = "nullable")]
    pub phones: Vec<ContactDetail>,
    pub job_title: Option<String>,
    pub postal_address: Option<Address>,
    pub add_time: Option<String>,
    pub update_time: Option<String>,
    pub visible_to: Option<u32>,
    pub is_deleted: Option<bool>,
    #[serde(default, deserialize_with = "nullable")]
    pub label_ids: Vec<u64>,
    /// The fields the company added, each under its key. A list leaves them
    /// out unless it is asked for some; `fields.person_fields` names them.
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "CustomFields::is_empty"
    )]
    pub custom_fields: CustomFields,
}

/// One email address or phone number of a person.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContactDetail {
    /// The address or the number.
    pub value: String,
    /// Whether this is the one to use first.
    pub primary: Option<bool>,
    /// What it is for: `work`, `home`, `mobile`, `other`.
    pub label: Option<String>,
}

/// Which persons to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListPersons {
    /// Only the persons this user owns.
    pub owner_id: Option<u64>,
    /// Only the persons of this organisation.
    pub org_id: Option<u64>,
    /// Only the persons linked to this deal.
    pub deal_id: Option<u64>,
    /// Only the persons a saved filter matches. Pipedrive then ignores the other filters.
    pub filter_id: Option<u64>,
    /// Only persons changed at or after this time, in RFC 3339.
    pub updated_since: Option<String>,
    /// Only persons changed before this time, in RFC 3339.
    pub updated_until: Option<String>,
    /// `id` (the default), `update_time` or `add_time`.
    pub sort_by: Option<String>,
    pub sort_direction: Option<SortDirection>,
    /// The keys of the custom fields to return with each person, at most 15.
    /// None are returned when not given.
    pub custom_fields: Option<Vec<String>>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most persons to return in a page, from 1 to 500. Pipedrive returns 100 when not given.
    pub limit: Option<u32>,
}

/// What to search persons for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchPersons {
    /// The words to look for: at least two characters, or one with `exact_match`.
    pub term: String,
    /// Where to look: any of `name`, `email`, `phone`, `notes` and `custom_fields`. Everywhere when not given.
    pub fields: Option<Vec<String>>,
    /// Only whole matches of the term, whatever their case.
    pub exact_match: Option<bool>,
    /// Only persons of this organisation.
    pub organization_id: Option<u64>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most results to return in a page, from 1 to 100.
    pub limit: Option<u32>,
}

/// A person to create.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreatePerson {
    pub name: String,
    /// The user who owns the person. The signed-in user when not given.
    pub owner_id: Option<u64>,
    /// The organisation the person belongs to.
    pub org_id: Option<u64>,
    pub emails: Option<Vec<ContactDetail>>,
    pub phones: Option<Vec<ContactDetail>>,
    pub visible_to: Option<u32>,
    pub label_ids: Option<Vec<u64>>,
    /// Values for the fields the company added, each under its key.
    pub custom_fields: Option<CustomFields>,
}

/// What to change on a person. A field that is not set is left as it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdatePerson {
    pub name: Option<String>,
    pub owner_id: Option<u64>,
    pub org_id: Option<u64>,
    /// The person's email addresses. The list replaces the one that was there.
    pub emails: Option<Vec<ContactDetail>>,
    /// The person's phone numbers. The list replaces the one that was there.
    pub phones: Option<Vec<ContactDetail>>,
    pub visible_to: Option<u32>,
    /// The person's labels. The list replaces the one that was there.
    pub label_ids: Option<Vec<u64>>,
    /// Values for the fields the company added, each under its key. A value
    /// of `null` clears that field; a key that is not named is left as it is.
    pub custom_fields: Option<CustomFields>,
}
