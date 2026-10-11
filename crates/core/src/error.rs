use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::provider::ProviderId;

pub type Result<T> = std::result::Result<T, Error>;

/// What went wrong, as a closed set a caller can branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The provider rejected the stored authorization, or none is stored.
    ReconnectRequired,
    /// Known caller, refused: policy or a missing scope.
    AccessDenied,
    NotFound,
    RateLimited,
    InvalidInput,
    /// No registered integration offers the operation, or what was asked
    /// for cannot be handed over in the form asked for.
    Unsupported,
    /// The application set something up wrong.
    Config,
    Transport,
    Decode,
    /// The content is larger than the limit set for the request. Reading
    /// stopped at the limit, if it began at all, and no part of the content
    /// is returned.
    TooLarge,
    Unexpected,
}

impl ErrorKind {
    /// Stable string code. Part of the public API in every language.
    pub fn code(self) -> &'static str {
        match self {
            Self::ReconnectRequired => "reconnect_required",
            Self::AccessDenied => "access_denied",
            Self::NotFound => "not_found",
            Self::RateLimited => "rate_limited",
            Self::InvalidInput => "invalid_input",
            Self::Unsupported => "unsupported",
            Self::Config => "config",
            Self::Transport => "transport",
            Self::Decode => "decode",
            Self::TooLarge => "too_large",
            Self::Unexpected => "unexpected",
        }
    }
}

/// Whether repeating the same call can succeed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retry {
    Never,
    After(Duration),
    Later,
}

pub struct Error {
    kind: ErrorKind,
    provider: Option<ProviderId>,
    retry: Retry,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            provider: None,
            retry: Retry::Never,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_provider(mut self, provider: ProviderId) -> Self {
        self.provider = Some(provider);
        self
    }

    pub fn with_retry(mut self, retry: Retry) -> Self {
        self.retry = retry;
        self
    }

    pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Rewrites the message. Used to strip a credential a provider echoed back.
    pub(crate) fn map_message(mut self, f: impl FnOnce(String) -> String) -> Self {
        self.message = f(self.message);
        self
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn provider(&self) -> Option<&ProviderId> {
        self.provider.as_ref()
    }

    pub fn retry(&self) -> Retry {
        self.retry
    }

    /// Safe to show to an end user.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The form that crosses a process or language boundary. `source` is left behind.
    pub fn to_wire(&self) -> WireError {
        let (retry, retry_after_secs) = match self.retry {
            Retry::Never => ("never", None),
            Retry::Later => ("later", None),
            Retry::After(wait) => ("after", Some(wait.as_secs())),
        };
        WireError {
            code: self.kind.code().to_owned(),
            provider: self.provider.as_ref().map(|p| p.as_str().to_owned()),
            retry: retry.to_owned(),
            retry_after_secs,
            message: self.message.clone(),
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("kind", &self.kind)
            .field("provider", &self.provider)
            .field("retry", &self.retry)
            .field("message", &self.message)
            .field("source", &self.source)
            .finish()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.provider {
            Some(provider) => write!(f, "{} ({}): {}", self.kind.code(), provider, self.message),
            None => write!(f, "{}: {}", self.kind.code(), self.message),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|s| s as &(dyn std::error::Error + 'static))
    }
}

/// An [`Error`] as plain data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireError {
    pub code: String,
    pub provider: Option<String>,
    /// `"never"`, `"later"` or `"after"`.
    pub retry: String,
    pub retry_after_secs: Option<u64>,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_distinct_code() {
        let kinds = [
            ErrorKind::ReconnectRequired,
            ErrorKind::AccessDenied,
            ErrorKind::NotFound,
            ErrorKind::RateLimited,
            ErrorKind::InvalidInput,
            ErrorKind::Unsupported,
            ErrorKind::Config,
            ErrorKind::Transport,
            ErrorKind::Decode,
            ErrorKind::TooLarge,
            ErrorKind::Unexpected,
        ];
        let codes: std::collections::HashSet<_> = kinds.iter().map(|k| k.code()).collect();
        assert_eq!(codes.len(), kinds.len());
        assert_eq!(ErrorKind::ReconnectRequired.code(), "reconnect_required");
        assert_eq!(ErrorKind::RateLimited.code(), "rate_limited");
        assert_eq!(ErrorKind::TooLarge.code(), "too_large");
    }

    #[test]
    fn a_new_error_is_not_retryable_and_has_no_provider() {
        let err = Error::new(ErrorKind::InvalidInput, "channel is required");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(err.retry(), Retry::Never);
        assert!(err.provider().is_none());
        assert_eq!(err.to_string(), "invalid_input: channel is required");
    }

    #[test]
    fn wire_form_carries_code_provider_and_retry_seconds() {
        let err = Error::new(ErrorKind::RateLimited, "slack is throttling requests")
            .with_provider(ProviderId::new("slack").unwrap())
            .with_retry(Retry::After(Duration::from_secs(30)));
        let wire = err.to_wire();
        assert_eq!(wire.code, "rate_limited");
        assert_eq!(wire.provider.as_deref(), Some("slack"));
        assert_eq!(wire.retry, "after");
        assert_eq!(wire.retry_after_secs, Some(30));

        let json = serde_json::to_string(&wire).unwrap();
        let back: WireError = serde_json::from_str(&json).unwrap();
        assert_eq!(back, wire);
    }

    #[test]
    fn wire_form_leaves_the_source_behind() {
        let cause = std::io::Error::other("connection reset: token=abc123");
        let err = Error::new(ErrorKind::Transport, "could not reach github").with_source(cause);
        assert!(std::error::Error::source(&err).is_some());
        let json = serde_json::to_string(&err.to_wire()).unwrap();
        assert!(!json.contains("abc123"), "{json}");
    }
}
