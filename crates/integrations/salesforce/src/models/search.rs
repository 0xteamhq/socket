//! Searching the text of records.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Record;

/// Which fields of a record a search looks in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchScope {
    /// Every searchable field. What Salesforce does when nothing is said.
    All,
    /// Name fields only.
    Name,
    /// Email fields only.
    Email,
    /// Phone number fields only.
    Phone,
    /// The fields Salesforce's own sidebar search looks in.
    Sidebar,
}

impl SearchScope {
    /// The scope as Salesforce names it.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Name => "NAME",
            Self::Email => "EMAIL",
            Self::Phone => "PHONE",
            Self::Sidebar => "SIDEBAR",
        }
    }
}

/// One object type to search, and what to return of what is found there.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FindIn {
    /// The object type, such as `Account` or `Invoice__c`.
    pub name: String,
    /// The fields to return of each record of this type, in place of the
    /// search's own `fields`.
    pub fields: Option<Vec<String>>,
    /// The most records of this type to return, in place of the search's own `limit`.
    pub limit: Option<u32>,
}

/// A search for some text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Find {
    /// The text to look for. It is searched for as it stands: a character
    /// Salesforce's search language gives a meaning to, such as `*`, `?`,
    /// `"` or `-`, is taken as that character.
    pub text: String,
    /// The object types to look in. Every searchable type when not given,
    /// and then only the ids of what is found come back.
    pub objects: Option<Vec<FindIn>>,
    /// The fields to return of each record found, for every type in
    /// `objects` that names none of its own. Every one of them has to exist
    /// on each of those types. Only ids when not given.
    pub fields: Option<Vec<String>>,
    /// Which fields to look in. Every searchable field when not given.
    pub within: Option<SearchScope>,
    /// The most records to return of each object type, from 1 to 2000.
    pub limit: Option<u32>,
    /// The most records to return in all, from 1 to 2000.
    pub overall_limit: Option<u32>,
}

/// What a search found.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SearchResult {
    /// The records found, of every type that was searched, the best matches first.
    pub records: Vec<Record>,
}
