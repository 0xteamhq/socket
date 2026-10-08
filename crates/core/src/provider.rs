use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};

/// A provider's identifier, such as `"slack"`.
///
/// Lowercase ASCII letters, digits, `-` and `_`; it must start with a letter.
/// It cannot contain `.`, which separates it from the rest of an operation name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        let starts_with_letter = id.chars().next().is_some_and(|c| c.is_ascii_lowercase());
        let all_allowed = id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        if starts_with_letter && all_allowed {
            Ok(Self(id))
        } else {
            Err(Error::new(ErrorKind::Config, format!("invalid provider id {id:?}")))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ProviderId {
    type Error = Error;

    fn try_from(id: String) -> Result<Self> {
        Self::new(id)
    }
}

impl From<ProviderId> for String {
    fn from(id: ProviderId) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_are_lowercase_and_cannot_contain_a_dot() {
        assert!(ProviderId::new("slack").is_ok());
        assert!(ProviderId::new("google-drive_2").is_ok());
        for bad in ["", "Slack", "slack.chat", "2fa", "sla ck", "slack/", "-slack"] {
            let err = ProviderId::new(bad).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Config, "{bad:?}");
        }
    }

    #[test]
    fn a_provider_id_is_validated_when_read_from_data() {
        assert!(serde_json::from_str::<ProviderId>("\"github\"").is_ok());
        assert!(serde_json::from_str::<ProviderId>("\"Git Hub\"").is_err());
    }
}
