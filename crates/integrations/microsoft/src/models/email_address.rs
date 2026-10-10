//! A person or a resource, named by an email address.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// A name and the address that goes with it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EmailAddress {
    pub name: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub address: String,
}

impl EmailAddress {
    /// An address with no name beside it.
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            name: None,
            address: address.into(),
        }
    }
}

/// Someone a message or an event is from: Graph wraps the address once more.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Recipient {
    #[serde(deserialize_with = "nullable")]
    pub email_address: EmailAddress,
}
