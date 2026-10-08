use std::fmt;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

/// A secret value that never appears in `Debug` output and has no `Display`.
///
/// It does serialize as its plain value: that is how an application's token
/// store persists a [`TokenSet`]. Encrypting at rest is the store's job.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The plain value. Call this only where the secret is sent to its provider.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(***)")
    }
}

/// What a provider issued for one connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: SecretString,
    pub refresh_token: Option<SecretString>,
    /// `None` means the provider gave no expiry; the token is used until rejected.
    pub expires_at: Option<SystemTime>,
    pub scopes: Vec<String>,
}

impl TokenSet {
    /// A token with no refresh token, no expiry and no recorded scopes.
    pub fn bearer(access_token: impl Into<String>) -> Self {
        Self {
            access_token: SecretString::new(access_token),
            refresh_token: None,
            expires_at: None,
            scopes: Vec::new(),
        }
    }

    /// True when the token expires at or before `now + skew`.
    pub fn is_expired(&self, now: SystemTime, skew: Duration) -> bool {
        match self.expires_at {
            None => false,
            Some(expires_at) => expires_at <= now + skew,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_a_secret() {
        let tokens = TokenSet {
            access_token: SecretString::new("xoxb-access"),
            refresh_token: Some(SecretString::new("xoxe-refresh")),
            expires_at: None,
            scopes: vec!["chat:write".into()],
        };
        let shown = format!("{tokens:?} {:#?}", tokens.access_token);
        assert!(!shown.contains("xoxb-access"), "{shown}");
        assert!(!shown.contains("xoxe-refresh"), "{shown}");
        assert!(shown.contains("chat:write"));
    }

    #[test]
    fn a_token_set_round_trips_through_json_for_the_store() {
        let tokens = TokenSet {
            access_token: SecretString::new("a"),
            refresh_token: Some(SecretString::new("r")),
            expires_at: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)),
            scopes: vec!["repo".into()],
        };
        let json = serde_json::to_string(&tokens).unwrap();
        let back: TokenSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back, tokens);
        assert_eq!(back.access_token.expose(), "a");
    }

    #[test]
    fn expiry_respects_the_skew_and_its_boundary() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let mut tokens = TokenSet::bearer("a");
        assert!(
            !tokens.is_expired(now, Duration::from_secs(60)),
            "no expiry never expires"
        );

        tokens.expires_at = Some(now + Duration::from_secs(61));
        assert!(!tokens.is_expired(now, Duration::from_secs(60)));
        tokens.expires_at = Some(now + Duration::from_secs(60));
        assert!(
            tokens.is_expired(now, Duration::from_secs(60)),
            "exactly at the skew counts as expired"
        );
        tokens.expires_at = Some(now - Duration::from_secs(1));
        assert!(tokens.is_expired(now, Duration::ZERO));
    }
}
