//! The users of a company's Pipedrive.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A user of the company's Pipedrive.
///
/// The `company_…` fields are set only for the signed-in user, by `users.me`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct User {
    pub id: u64,
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    /// Whether the user can sign in.
    pub active_flag: Option<bool>,
    /// Whether this is the user the connection is signed in as.
    pub is_you: Option<bool>,
    pub role_id: Option<u64>,
    pub timezone_name: Option<String>,
    /// The format of dates and numbers the user chose, not their language.
    pub locale: Option<String>,
    pub default_currency: Option<String>,
    pub icon_url: Option<String>,
    pub last_login: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    /// The company the connection is to.
    pub company_id: Option<u64>,
    pub company_name: Option<String>,
    /// The company's own name in its Pipedrive address: `acme` for `acme.pipedrive.com`.
    pub company_domain: Option<String>,
    pub company_country: Option<String>,
}
