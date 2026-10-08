# Phase 1A: Core Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Rust crate, `socketkit-core`, in which an application registers integrations, keeps tokens in its own store, and invokes any operation by name with JSON.

**Architecture:** A Cargo workspace with one crate. The crate holds plain-data types (errors, secrets, provider definitions, operation descriptions), two object-safe traits the outside world implements (`TokenStore` by the application, `Integration` by each service), and a `Socket` handle that checks registrations when it is built and dispatches `invoke` by operation name. There is no HTTP and no OAuth in this plan; those are plans 1B and 1C in [the roadmap](../../roadmap.md).

**Tech Stack:** Rust (edition 2024, minimum 1.85), `serde`, `serde_json`, `url`, `async-trait`; `tokio` in tests and examples only.

**Spec:** `docs/superpowers/specs/2026-10-08-socket-project-design.md` (sections 5.1, 5.2, 5.5, 5.6, 5.9 and 11). Read it before starting.

Every code block in this plan was compiled, tested, linted and formatted together before the plan was written (Rust 1.96, 28 tests passing). If a step fails as written, suspect a transcription slip before suspecting the design.

## Global Constraints

- Edition 2024, `rust-version = "1.85"`.
- Licence `MIT OR Apache-2.0`.
- `unsafe_code = "forbid"` in every library crate.
- `socketkit-core` depends on no other workspace crate.
- No library code reads an environment variable, writes a file, spawns a process, or opens a network connection.
- No ambient runtime: `tokio` is a dev-dependency of `socketkit-core`, never a normal dependency. Library code does not call `tokio::spawn`.
- Traits the outside world implements (`TokenStore`, `Integration`) are object-safe, `Send + Sync`, take owned parameters and return `Result`. No `impl Trait` or borrowed parameters on them.
- Public data types have no lifetimes or generics, and derive `serde` where they cross the by-name layer.
- No type that holds a secret prints it in `Debug`. No such type implements `Display`.
- Every `ErrorKind` has a stable string code; the codes are public API.
- An operation name is `<provider id>.<rest>`.
- Formatting is `cargo fmt` with `max_width = 120`. `cargo clippy --workspace --all-targets -- -D warnings` must pass before every commit.
- Commit messages are plain: no co-author trailer and no tool attribution.
- Work on the branch `phase-1a-core-foundation`. Do not push or open a pull request; that needs a local Cubic review first and is the owner's call.

## Review Focus

Each line names an input the spec implies and the test that pins it.

1. **A URL that looks like an allowed host** (`slack.com.evil.test`, `slack.com@evil.test`, plain `http`, an unlisted subdomain) must not receive credentials. Pinned in Task 4, `credentials_go_only_to_an_exact_allowed_host_over_https`.
2. **A secret reaching a log or another process** through `Debug` or a serialised error. Pinned in Task 3, `debug_never_shows_a_secret`, and Task 2, `wire_form_leaves_the_source_behind`.
3. **A caller passing a name that is not a known operation, or input that is not a JSON object** (wrong case, trailing space, `null`, an array) must get an error, and the integration must not run. Pinned in Task 7, `an_unknown_operation_is_unsupported_and_runs_nothing` and `input_that_is_not_an_object_is_refused_before_the_integration_runs`.
4. **A tenant that never connected, or a store that is down**, must produce `reconnect_required` or the store's own error, never a call with empty credentials. Pinned in Task 7, `a_tenant_with_no_stored_tokens_must_reconnect` and `a_failing_store_surfaces_its_own_error`.
5. **Provider data written by hand with a misspelt field, a bad URL or an invalid id** must be rejected when read, not ignored. Pinned in Task 4, `a_spec_with_a_misspelt_field_or_a_bad_url_is_rejected`, and Task 2, `a_provider_id_is_validated_when_read_from_data`.

## File Structure

| File | Responsibility |
| --- | --- |
| `Cargo.toml` | Workspace: members, shared package fields, shared dependency versions, lints |
| `rustfmt.toml`, `.gitignore` | Formatting width; ignore `target/` |
| `crates/core/Cargo.toml` | The `socketkit-core` package |
| `crates/core/src/lib.rs` | Module list and re-exports only |
| `crates/core/src/error.rs` | `Error`, `ErrorKind`, `Retry`, `WireError`, `Result` |
| `crates/core/src/provider.rs` | `ProviderId`, `ProviderSpec`, `AuthScheme` and its parts, the host allowlist check |
| `crates/core/src/secret.rs` | `SecretString`, `TokenSet` |
| `crates/core/src/store.rs` | `ConnectionKey`, `TokenStore`, `MemoryTokenStore` |
| `crates/core/src/operation.rs` | `Effect`, `OperationInfo`, `Connection`, `Integration` |
| `crates/core/src/socket.rs` | `Socket`, `SocketBuilder`, `invoke` |
| `crates/core/tests/invoke.rs` | The by-name layer tested through the public API |
| `crates/core/examples/invoke_by_name.rs` | A runnable program showing the whole of this plan |
| `.github/workflows/ci.yml` | Format, lint and test on every push |
| `README.md` | What the repository is and how to run the example |

Unit tests live in a `#[cfg(test)] mod tests` at the bottom of the file they test.

---

### Task 1: Workspace and an empty core crate

**Files:**
- Create: `Cargo.toml`, `rustfmt.toml`, `.gitignore`, `crates/core/Cargo.toml`, `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: a workspace in which `cargo test -p socketkit-core` runs. Later tasks add modules to `crates/core/src/lib.rs`.

- [ ] **Step 1: Create the branch**

```bash
git checkout -b phase-1a-core-foundation
```

- [ ] **Step 2: Write the workspace files**

`Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/core"]

[workspace.package]
version = "0.0.0"
edition = "2024"
rust-version = "1.85"
license = "MIT OR Apache-2.0"
repository = "https://github.com/0xteamhq/socket"

[workspace.dependencies]
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt"] }
url = { version = "2", features = ["serde"] }

[workspace.lints.rust]
unsafe_code = "forbid"
missing_debug_implementations = "warn"

[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
```

`rustfmt.toml`:

```toml
max_width = 120
```

`.gitignore`:

```
/target
```

- [ ] **Step 3: Write the core crate**

`crates/core/Cargo.toml`:

```toml
[package]
name = "socketkit-core"
description = "Core of Socket: providers, credentials, token storage and operations."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
url.workspace = true

[dev-dependencies]
tokio.workspace = true

[lints]
workspace = true
```

`crates/core/src/lib.rs`:

```rust
//! Core of Socket: providers, credentials, token storage and operations.
//!
//! This crate knows nothing about any specific service.
```

- [ ] **Step 4: Verify it builds and lints**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`
Expected: all three succeed with no warnings. `Cargo.lock` is created.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock rustfmt.toml .gitignore crates/core
git commit -m "chore: add workspace and empty socketkit-core crate"
```

### Task 2: Errors and provider ids

**Files:**
- Create: `crates/core/src/error.rs`, `crates/core/src/provider.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `ProviderId::new(impl Into<String>) -> Result<ProviderId>`, `ProviderId::as_str(&self) -> &str`, `Display` for `ProviderId`; `Error::new(ErrorKind, impl Into<String>) -> Error`, `.with_provider(ProviderId)`, `.with_retry(Retry)`, `.with_source(E)`, `.kind()`, `.provider() -> Option<&ProviderId>`, `.retry()`, `.message() -> &str`, `.to_wire() -> WireError`; `ErrorKind::code(self) -> &'static str`; `Retry::{Never, After(Duration), Later}`; `WireError { code, provider, retry, retry_after_secs, message }`; `type Result<T> = std::result::Result<T, Error>`.

- [ ] **Step 1: Write the failing tests**

The two types refer to each other (an error names its provider; an invalid id is an error), so they land together. Create `crates/core/src/error.rs` containing only:

```rust
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
            ErrorKind::Unexpected,
        ];
        let codes: std::collections::HashSet<_> = kinds.iter().map(|k| k.code()).collect();
        assert_eq!(codes.len(), kinds.len());
        assert_eq!(ErrorKind::ReconnectRequired.code(), "reconnect_required");
        assert_eq!(ErrorKind::RateLimited.code(), "rate_limited");
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
```

Create `crates/core/src/provider.rs` containing only:

```rust
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
```

Set `crates/core/src/lib.rs` to:

```rust
//! Core of Socket: providers, credentials, token storage and operations.
//!
//! This crate knows nothing about any specific service.

mod error;
mod provider;

pub use error::{Error, ErrorKind, Result, Retry, WireError};
pub use provider::ProviderId;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p socketkit-core`
Expected: FAIL to compile, with errors such as `cannot find type ErrorKind in this scope` and `failed to resolve: use of undeclared type ProviderId`.

- [ ] **Step 3: Write the implementation**

`crates/core/src/error.rs`. Put it above the tests module, at the top of the file.

```rust
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
    /// No registered integration offers the operation.
    Unsupported,
    /// The application set something up wrong.
    Config,
    Transport,
    Decode,
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
```

`crates/core/src/provider.rs`. Put it above the tests module, at the top of the file.

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p socketkit-core && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`
Expected: PASS, 6 tests, with no clippy warnings and no formatting diff.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): add error model and provider ids"
```

### Task 3: Secrets and token sets

**Files:**
- Create: `crates/core/src/secret.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `SecretString::new(impl Into<String>)`, `SecretString::expose(&self) -> &str`; `TokenSet { access_token: SecretString, refresh_token: Option<SecretString>, expires_at: Option<SystemTime>, scopes: Vec<String> }`, `TokenSet::bearer(impl Into<String>) -> TokenSet`, `TokenSet::is_expired(&self, now: SystemTime, skew: Duration) -> bool`.

- [ ] **Step 1: Write the failing tests**

Create `crates/core/src/secret.rs` containing only:

```rust
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
```

Set `crates/core/src/lib.rs` to:

```rust
//! Core of Socket: providers, credentials, token storage and operations.
//!
//! This crate knows nothing about any specific service.

mod error;
mod provider;
mod secret;

pub use error::{Error, ErrorKind, Result, Retry, WireError};
pub use provider::ProviderId;
pub use secret::{SecretString, TokenSet};
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p socketkit-core`
Expected: FAIL to compile, with `cannot find type TokenSet in this scope`.

- [ ] **Step 3: Write the implementation**

`crates/core/src/secret.rs`. Put it above the tests module, at the top of the file.

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p socketkit-core && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`
Expected: PASS, 9 tests, with no clippy warnings and no formatting diff.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): add secret string and token set"
```

### Task 4: Provider definitions and the host allowlist

**Files:**
- Modify: `crates/core/src/provider.rs`, `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `ProviderId`, `Error`, `ErrorKind`, `Result` from Task 2.
- Produces: `ProviderSpec { id: ProviderId, display_name: String, api_base: Url, allowed_hosts: Vec<String>, auth: AuthScheme }`, `ProviderSpec::allows_host(&self, &Url) -> bool`, `ProviderSpec::validate(&self) -> Result<()>`; `AuthScheme::{OAuth2(OAuth2Spec), ApiKey(ApiKeySpec)}`; `OAuth2Spec { authorize_url: Url, token_url: Url, default_scopes: Vec<String>, scope_separator: String, pkce: bool }`; `ApiKeySpec { placement: KeyPlacement }`; `KeyPlacement::{Header { name: String, prefix: Option<String> }, Query { name: String }, Basic}`. All derive `Serialize` and `Deserialize`.

- [ ] **Step 1: Write the failing tests**

In the tests module of `crates/core/src/provider.rs`, add this helper and these five tests. Put the helper directly under `use super::*;` and leave the two tests from Task 2 in place:

```rust
    fn slack() -> ProviderSpec {
        ProviderSpec {
            id: ProviderId::new("slack").unwrap(),
            display_name: "Slack".into(),
            api_base: Url::parse("https://slack.com/api/").unwrap(),
            allowed_hosts: vec!["slack.com".into()],
            auth: AuthScheme::OAuth2(OAuth2Spec {
                authorize_url: Url::parse("https://slack.com/oauth/v2/authorize").unwrap(),
                token_url: Url::parse("https://slack.com/api/oauth.v2.access").unwrap(),
                default_scopes: vec!["chat:write".into()],
                scope_separator: ",".into(),
                pkce: false,
            }),
        }
    }

    #[test]
    fn a_spec_round_trips_through_json() {
        let spec = slack();
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["auth"]["type"], "oauth2");
        let back: ProviderSpec = serde_json::from_value(json).unwrap();
        assert_eq!(back, spec);
    }

    #[test]
    fn an_api_key_spec_reads_from_data() {
        let spec: ProviderSpec = serde_json::from_str(
            r#"{
                "id": "sendgrid",
                "display_name": "SendGrid",
                "api_base": "https://api.sendgrid.com/v3/",
                "allowed_hosts": ["api.sendgrid.com"],
                "auth": { "type": "api_key", "placement": { "in": "header", "name": "Authorization", "prefix": "Bearer " } }
            }"#,
        )
        .unwrap();
        assert_eq!(
            spec.auth,
            AuthScheme::ApiKey(ApiKeySpec {
                placement: KeyPlacement::Header {
                    name: "Authorization".into(),
                    prefix: Some("Bearer ".into())
                }
            })
        );
        spec.validate().unwrap();
    }

    #[test]
    fn a_spec_with_a_misspelt_field_or_a_bad_url_is_rejected() {
        let misspelt = r#"{"id":"x","display_name":"X","api_base":"https://x.test/","allowed_host":["x.test"],"auth":{"type":"api_key","placement":{"in":"basic"}}}"#;
        assert!(serde_json::from_str::<ProviderSpec>(misspelt).is_err());
        let bad_url = r#"{"id":"x","display_name":"X","api_base":"not a url","allowed_hosts":["x.test"],"auth":{"type":"api_key","placement":{"in":"basic"}}}"#;
        assert!(serde_json::from_str::<ProviderSpec>(bad_url).is_err());
    }

    #[test]
    fn credentials_go_only_to_an_exact_allowed_host_over_https() {
        let spec = slack();
        let allows = |u: &str| spec.allows_host(&Url::parse(u).unwrap());
        assert!(allows("https://slack.com/api/chat.postMessage"));
        assert!(allows("https://SLACK.com/api/"), "host comparison ignores case");
        assert!(allows("https://slack.com:443/api/"));
        assert!(!allows("http://slack.com/api/"), "plain http is refused");
        assert!(!allows("https://slack.com.evil.test/api/"));
        assert!(!allows("https://evil-slack.com/"));
        assert!(!allows("https://files.slack.com/"), "a subdomain must be listed itself");
        assert!(
            !allows("https://slack.com@evil.test/"),
            "userinfo does not change the host"
        );
    }

    #[test]
    fn validate_rejects_specs_that_could_leak_or_never_work() {
        slack().validate().unwrap();

        let mut no_hosts = slack();
        no_hosts.allowed_hosts.clear();
        assert_eq!(no_hosts.validate().unwrap_err().kind(), ErrorKind::Config);

        let mut base_elsewhere = slack();
        base_elsewhere.api_base = Url::parse("https://example.test/").unwrap();
        assert_eq!(base_elsewhere.validate().unwrap_err().kind(), ErrorKind::Config);

        let mut http_token_url = slack();
        if let AuthScheme::OAuth2(oauth) = &mut http_token_url.auth {
            oauth.token_url = Url::parse("http://slack.com/api/oauth.v2.access").unwrap();
        }
        let err = http_token_url.validate().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Config);
        assert_eq!(err.provider().map(ProviderId::as_str), Some("slack"));
    }
```

Set `crates/core/src/lib.rs` to:

```rust
//! Core of Socket: providers, credentials, token storage and operations.
//!
//! This crate knows nothing about any specific service.

mod error;
mod provider;
mod secret;

pub use error::{Error, ErrorKind, Result, Retry, WireError};
pub use provider::{ApiKeySpec, AuthScheme, KeyPlacement, OAuth2Spec, ProviderId, ProviderSpec};
pub use secret::{SecretString, TokenSet};
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p socketkit-core`
Expected: FAIL to compile, with `cannot find type ProviderSpec in this scope`.

- [ ] **Step 3: Write the implementation**

In `crates/core/src/provider.rs`, add `use url::Url;` on the line after `use serde::{Deserialize, Serialize};`. Then insert this between the `impl From<ProviderId> for String` block and the tests module:

```rust
/// A service's identity and how it authenticates. Plain data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSpec {
    pub id: ProviderId,
    pub display_name: String,
    /// Overridable for self-hosted instances.
    pub api_base: Url,
    /// The only hosts that may receive this provider's credentials.
    pub allowed_hosts: Vec<String>,
    pub auth: AuthScheme,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthScheme {
    #[serde(rename = "oauth2")]
    OAuth2(OAuth2Spec),
    ApiKey(ApiKeySpec),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OAuth2Spec {
    pub authorize_url: Url,
    pub token_url: Url,
    pub default_scopes: Vec<String>,
    /// Usually a space. Some providers use a comma.
    pub scope_separator: String,
    pub pkce: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiKeySpec {
    pub placement: KeyPlacement,
}

/// Where an API key goes on a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "in", rename_all = "snake_case", deny_unknown_fields)]
pub enum KeyPlacement {
    Header { name: String, prefix: Option<String> },
    Query { name: String },
    Basic,
}

impl ProviderSpec {
    /// True when credentials for this provider may be sent to `url`.
    ///
    /// The scheme must be `https` and the host must equal an allowed host,
    /// compared without regard to case. A subdomain of an allowed host is not allowed.
    pub fn allows_host(&self, url: &Url) -> bool {
        if url.scheme() != "https" {
            return false;
        }
        let Some(host) = url.host_str() else {
            return false;
        };
        self.allowed_hosts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
    }

    /// Checks the rules a spec must meet before it is registered.
    pub fn validate(&self) -> Result<()> {
        let fail = |message: String| Err(Error::new(ErrorKind::Config, message).with_provider(self.id.clone()));
        if self.allowed_hosts.is_empty() {
            return fail(format!("provider {} has no allowed hosts", self.id));
        }
        if !self.allows_host(&self.api_base) {
            return fail(format!(
                "provider {} has an api_base outside its allowed hosts",
                self.id
            ));
        }
        if let AuthScheme::OAuth2(oauth) = &self.auth {
            if oauth.authorize_url.scheme() != "https" || oauth.token_url.scheme() != "https" {
                return fail(format!("provider {} has an OAuth endpoint that is not https", self.id));
            }
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p socketkit-core && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`
Expected: PASS, 14 tests, with no clippy warnings and no formatting diff.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): add provider definitions with host allowlist"
```

### Task 5: Token store

**Files:**
- Create: `crates/core/src/store.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `ProviderId` and `Result` (Task 2), `TokenSet` (Task 3).
- Produces: `ConnectionKey { provider: ProviderId, tenant: String }`, `ConnectionKey::new(ProviderId, impl Into<String>)`; `trait TokenStore: Send + Sync` with `async fn load(&self, key: ConnectionKey) -> Result<Option<TokenSet>>`, `async fn save(&self, key: ConnectionKey, tokens: TokenSet) -> Result<()>`, `async fn delete(&self, key: ConnectionKey) -> Result<()>`; `MemoryTokenStore::new()`.

- [ ] **Step 1: Write the failing tests**

Create `crates/core/src/store.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn key(provider: &str, tenant: &str) -> ConnectionKey {
        ConnectionKey::new(ProviderId::new(provider).unwrap(), tenant)
    }

    #[tokio::test]
    async fn save_then_load_returns_the_tokens_and_delete_removes_them() {
        let store = MemoryTokenStore::new();
        assert_eq!(store.load(key("slack", "acme")).await.unwrap(), None);

        store.save(key("slack", "acme"), TokenSet::bearer("one")).await.unwrap();
        store.save(key("slack", "acme"), TokenSet::bearer("two")).await.unwrap();
        let loaded = store.load(key("slack", "acme")).await.unwrap().unwrap();
        assert_eq!(loaded.access_token.expose(), "two", "save replaces");

        store.delete(key("slack", "acme")).await.unwrap();
        assert_eq!(store.load(key("slack", "acme")).await.unwrap(), None);
        store.delete(key("slack", "acme")).await.unwrap();
    }

    #[tokio::test]
    async fn tenants_and_providers_are_isolated() {
        let store = MemoryTokenStore::new();
        store
            .save(key("slack", "acme"), TokenSet::bearer("acme-slack"))
            .await
            .unwrap();
        assert_eq!(store.load(key("slack", "globex")).await.unwrap(), None);
        assert_eq!(store.load(key("github", "acme")).await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_store_is_usable_as_a_shared_trait_object() {
        let store: Arc<dyn TokenStore> = Arc::new(MemoryTokenStore::new());
        let other = Arc::clone(&store);
        other.save(key("github", "u1"), TokenSet::bearer("t")).await.unwrap();
        assert!(store.load(key("github", "u1")).await.unwrap().is_some());
    }
}
```

Set `crates/core/src/lib.rs` to:

```rust
//! Core of Socket: providers, credentials, token storage and operations.
//!
//! This crate knows nothing about any specific service.

mod error;
mod provider;
mod secret;
mod store;

pub use error::{Error, ErrorKind, Result, Retry, WireError};
pub use provider::{ApiKeySpec, AuthScheme, KeyPlacement, OAuth2Spec, ProviderId, ProviderSpec};
pub use secret::{SecretString, TokenSet};
pub use store::{ConnectionKey, MemoryTokenStore, TokenStore};
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p socketkit-core`
Expected: FAIL to compile, with `cannot find type MemoryTokenStore in this scope`.

- [ ] **Step 3: Write the implementation**

`crates/core/src/store.rs`. Put it above the tests module, at the top of the file.

```rust
use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::provider::ProviderId;
use crate::secret::TokenSet;

/// Which stored authorization: one provider, for one of the application's tenants.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionKey {
    pub provider: ProviderId,
    /// Opaque to Socket: a user id, a workspace id.
    pub tenant: String,
}

impl ConnectionKey {
    pub fn new(provider: ProviderId, tenant: impl Into<String>) -> Self {
        Self {
            provider,
            tenant: tenant.into(),
        }
    }
}

/// Where tokens live. The application implements this; Socket never persists.
///
/// Methods take owned values and return `Result` so the trait can be
/// implemented by an object in another language. Socket holds no lock while
/// calling a store, so an implementation may be slow or call back into Socket.
#[async_trait]
pub trait TokenStore: Send + Sync {
    async fn load(&self, key: ConnectionKey) -> Result<Option<TokenSet>>;
    async fn save(&self, key: ConnectionKey, tokens: TokenSet) -> Result<()>;
    async fn delete(&self, key: ConnectionKey) -> Result<()>;
}

/// A store that keeps tokens in memory. For command-line tools, examples and tests.
#[derive(Debug, Default)]
pub struct MemoryTokenStore {
    tokens: Mutex<HashMap<ConnectionKey, TokenSet>>,
}

impl MemoryTokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<ConnectionKey, TokenSet>> {
        // A poisoned lock only means another thread panicked mid-insert; the map is still usable.
        self.tokens.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[async_trait]
impl TokenStore for MemoryTokenStore {
    async fn load(&self, key: ConnectionKey) -> Result<Option<TokenSet>> {
        Ok(self.lock().get(&key).cloned())
    }

    async fn save(&self, key: ConnectionKey, tokens: TokenSet) -> Result<()> {
        self.lock().insert(key, tokens);
        Ok(())
    }

    async fn delete(&self, key: ConnectionKey) -> Result<()> {
        self.lock().remove(&key);
        Ok(())
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p socketkit-core && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`
Expected: PASS, 17 tests, with no clippy warnings and no formatting diff.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): add token store trait and in-memory store"
```

### Task 6: Operations and the integration trait

**Files:**
- Create: `crates/core/src/operation.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `ProviderSpec` (Task 4), `TokenSet` (Task 3), `ConnectionKey` (Task 5), `Result` (Task 2).
- Produces: `Effect::{Read, Write, Destructive}`; `OperationInfo { name: String, description: String, input_schema: Value, output_schema: Value, effect: Effect, required_scopes: Vec<String> }`; `Connection { key: ConnectionKey, tokens: TokenSet }`; `trait Integration: Send + Sync` with `fn provider(&self) -> ProviderSpec`, `fn operations(&self) -> Vec<OperationInfo>`, `async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/core/src/operation.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_info_serializes_with_snake_case_effect() {
        let info = OperationInfo {
            name: "slack.chat.post_message".into(),
            description: "Post a message to a channel.".into(),
            input_schema: serde_json::json!({ "type": "object", "required": ["channel", "text"] }),
            output_schema: serde_json::json!({ "type": "object" }),
            effect: Effect::Write,
            required_scopes: vec!["chat:write".into()],
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["effect"], "write");
        assert_eq!(json["input_schema"]["required"][0], "channel");
        let back: OperationInfo = serde_json::from_value(json).unwrap();
        assert_eq!(back, info);
        assert_eq!(serde_json::to_value(Effect::Destructive).unwrap(), "destructive");
    }
}
```

Set `crates/core/src/lib.rs` to:

```rust
//! Core of Socket: providers, credentials, token storage and operations.
//!
//! This crate knows nothing about any specific service.

mod error;
mod operation;
mod provider;
mod secret;
mod store;

pub use error::{Error, ErrorKind, Result, Retry, WireError};
pub use operation::{Connection, Effect, Integration, OperationInfo};
pub use provider::{ApiKeySpec, AuthScheme, KeyPlacement, OAuth2Spec, ProviderId, ProviderSpec};
pub use secret::{SecretString, TokenSet};
pub use store::{ConnectionKey, MemoryTokenStore, TokenStore};
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p socketkit-core`
Expected: FAIL to compile, with `cannot find type OperationInfo in this scope`.

- [ ] **Step 3: Write the implementation**

`crates/core/src/operation.rs`. Put it above the tests module, at the top of the file.

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Result;
use crate::provider::ProviderSpec;
use crate::secret::TokenSet;
use crate::store::ConnectionKey;

/// What an operation does to the provider's data. A host uses it to decide
/// which calls need approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Read,
    Write,
    Destructive,
}

/// An operation's description of itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationInfo {
    /// `"<provider id>.<rest>"`, for example `"slack.chat.post_message"`.
    pub name: String,
    pub description: String,
    /// JSON Schema of the input object.
    pub input_schema: Value,
    /// JSON Schema of the output.
    pub output_schema: Value,
    pub effect: Effect,
    pub required_scopes: Vec<String>,
}

/// One stored authorization, loaded and ready to use.
#[derive(Debug, Clone)]
pub struct Connection {
    pub key: ConnectionKey,
    pub tokens: TokenSet,
}

/// One service's operations. Implemented once per integration crate.
#[async_trait]
pub trait Integration: Send + Sync {
    fn provider(&self) -> ProviderSpec;

    /// Every operation this integration offers.
    fn operations(&self) -> Vec<OperationInfo>;

    /// Runs the operation called `operation` with `input`, a JSON object.
    /// `operation` is always one of the names returned by [`Integration::operations`].
    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value>;
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p socketkit-core && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`
Expected: PASS, 18 tests, with no clippy warnings and no formatting diff.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): add operation descriptions and integration trait"
```

### Task 7: The Socket handle and invoke by name

**Files:**
- Create: `crates/core/src/socket.rs`, `crates/core/tests/invoke.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `TokenStore`, `ConnectionKey` (Task 5); `Integration`, `Connection`, `OperationInfo` (Task 6); `ProviderId`, `ProviderSpec::validate` (Tasks 2 and 4); `Error`, `ErrorKind` (Task 2).
- Produces: `Socket::builder(store: Arc<dyn TokenStore>) -> SocketBuilder`; `SocketBuilder::integration(self, Arc<dyn Integration>) -> SocketBuilder`; `SocketBuilder::build(self) -> Result<Socket>`; `Socket::operations(&self) -> Vec<OperationInfo>`, sorted by name; `Socket::invoke(&self, key: ConnectionKey, operation: String, input: Value) -> Result<Value>`. Error kinds: unknown operation is `Unsupported`; an operation of another provider, or input that is not an object, is `InvalidInput`; no stored tokens is `ReconnectRequired`; a bad registration is `Config`.

- [ ] **Step 1: Write the failing tests**

These tests use only the public API, so they go in an integration test. Create `crates/core/tests/invoke.rs`:

```rust
//! The by-name layer, exercised the way an application uses it.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use serde_json::{Value, json};
use socketkit_core::{
    ApiKeySpec, AuthScheme, Connection, ConnectionKey, Effect, Error, ErrorKind, Integration, KeyPlacement,
    MemoryTokenStore, OperationInfo, ProviderId, ProviderSpec, Result, Socket, TokenSet, TokenStore,
};

/// A stand-in integration: `<id>.echo` returns its input and the tenant it ran for.
struct Echo {
    id: &'static str,
    operations: Vec<&'static str>,
    calls: AtomicUsize,
}

impl Echo {
    fn new(id: &'static str, operations: &[&'static str]) -> Arc<Self> {
        Arc::new(Self {
            id,
            operations: operations.to_vec(),
            calls: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl Integration for Echo {
    fn provider(&self) -> ProviderSpec {
        ProviderSpec {
            id: ProviderId::new(self.id).unwrap(),
            display_name: self.id.to_uppercase(),
            api_base: format!("https://api.{}.test/", self.id).parse().unwrap(),
            allowed_hosts: vec![format!("api.{}.test", self.id)],
            auth: AuthScheme::ApiKey(ApiKeySpec {
                placement: KeyPlacement::Basic,
            }),
        }
    }

    fn operations(&self) -> Vec<OperationInfo> {
        self.operations
            .iter()
            .map(|name| OperationInfo {
                name: (*name).to_owned(),
                description: "Return the input.".into(),
                input_schema: json!({ "type": "object" }),
                output_schema: json!({ "type": "object" }),
                effect: Effect::Read,
                required_scopes: Vec::new(),
            })
            .collect()
    }

    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(json!({
            "operation": operation,
            "tenant": connection.key.tenant,
            "token_len": connection.tokens.access_token.expose().len(),
            "input": input,
        }))
    }
}

/// A store whose every call fails, as a database outage would.
struct BrokenStore;

#[async_trait]
impl TokenStore for BrokenStore {
    async fn load(&self, _key: ConnectionKey) -> Result<Option<TokenSet>> {
        Err(Error::new(ErrorKind::Unexpected, "store is down"))
    }
    async fn save(&self, _key: ConnectionKey, _tokens: TokenSet) -> Result<()> {
        Err(Error::new(ErrorKind::Unexpected, "store is down"))
    }
    async fn delete(&self, _key: ConnectionKey) -> Result<()> {
        Err(Error::new(ErrorKind::Unexpected, "store is down"))
    }
}

fn key(provider: &str, tenant: &str) -> ConnectionKey {
    ConnectionKey::new(ProviderId::new(provider).unwrap(), tenant)
}

async fn socket_with_slack_connected() -> (Socket, Arc<Echo>) {
    let store = Arc::new(MemoryTokenStore::new());
    store
        .save(key("slack", "acme"), TokenSet::bearer("xoxb-123"))
        .await
        .unwrap();
    let slack = Echo::new("slack", &["slack.echo", "slack.auth.test"]);
    let github = Echo::new("github", &["github.echo"]);
    let socket = Socket::builder(store)
        .integration(slack.clone())
        .integration(github)
        .build()
        .unwrap();
    (socket, slack)
}

#[tokio::test]
async fn invoke_runs_the_named_operation_with_the_stored_tokens() {
    let (socket, slack) = socket_with_slack_connected().await;
    let out = socket
        .invoke(key("slack", "acme"), "slack.echo".into(), json!({ "text": "hi" }))
        .await
        .unwrap();
    assert_eq!(out["operation"], "slack.echo");
    assert_eq!(out["tenant"], "acme");
    assert_eq!(out["token_len"], 8);
    assert_eq!(out["input"]["text"], "hi");
    assert_eq!(slack.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn operations_lists_every_integration_sorted_by_name() {
    let (socket, _) = socket_with_slack_connected().await;
    let names: Vec<_> = socket.operations().into_iter().map(|o| o.name).collect();
    assert_eq!(names, ["github.echo", "slack.auth.test", "slack.echo"]);
}

#[tokio::test]
async fn an_unknown_operation_is_unsupported_and_runs_nothing() {
    let (socket, slack) = socket_with_slack_connected().await;
    for name in ["slack.nope", "slack", "", "SLACK.ECHO", "slack.echo "] {
        let err = socket
            .invoke(key("slack", "acme"), name.into(), json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unsupported, "{name:?}");
    }
    assert_eq!(slack.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_operation_cannot_run_on_another_providers_connection() {
    let (socket, slack) = socket_with_slack_connected().await;
    let err = socket
        .invoke(key("slack", "acme"), "github.echo".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(slack.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn input_that_is_not_an_object_is_refused_before_the_integration_runs() {
    let (socket, slack) = socket_with_slack_connected().await;
    for input in [json!(null), json!("text"), json!([1, 2]), json!(3)] {
        let err = socket
            .invoke(key("slack", "acme"), "slack.echo".into(), input)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }
    assert_eq!(slack.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_tenant_with_no_stored_tokens_must_reconnect() {
    let (socket, slack) = socket_with_slack_connected().await;
    let err = socket
        .invoke(key("slack", "globex"), "slack.echo".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ReconnectRequired);
    assert_eq!(err.provider().map(ProviderId::as_str), Some("slack"));
    assert_eq!(slack.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_failing_store_surfaces_its_own_error() {
    let socket = Socket::builder(Arc::new(BrokenStore))
        .integration(Echo::new("slack", &["slack.echo"]))
        .build()
        .unwrap();
    let err = socket
        .invoke(key("slack", "acme"), "slack.echo".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unexpected);
    assert_eq!(err.message(), "store is down");
}

#[tokio::test]
async fn build_refuses_duplicate_providers_duplicate_names_and_foreign_names() {
    let store = || Arc::new(MemoryTokenStore::new());
    let kind = |r: Result<Socket>| r.unwrap_err().kind();

    let twice = Socket::builder(store())
        .integration(Echo::new("slack", &["slack.echo"]))
        .integration(Echo::new("slack", &["slack.other"]));
    assert_eq!(kind(twice.build()), ErrorKind::Config);

    let duplicate_name = Socket::builder(store()).integration(Echo::new("slack", &["slack.echo", "slack.echo"]));
    assert_eq!(kind(duplicate_name.build()), ErrorKind::Config);

    for foreign in ["github.echo", "echo", "slack.", "slackecho", "slack"] {
        let builder = Socket::builder(store()).integration(Echo::new("slack", &[foreign]));
        assert_eq!(kind(builder.build()), ErrorKind::Config, "{foreign:?}");
    }
}

#[tokio::test]
async fn a_socket_with_no_integrations_builds_and_supports_nothing() {
    let socket = Socket::builder(Arc::new(MemoryTokenStore::new())).build().unwrap();
    assert!(socket.operations().is_empty());
    let err = socket
        .invoke(key("slack", "acme"), "slack.echo".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unsupported);
}

#[tokio::test]
async fn a_socket_is_shared_across_tasks() {
    let (socket, slack) = socket_with_slack_connected().await;
    let socket = Arc::new(socket);
    let mut handles = Vec::new();
    for n in 0..8 {
        let socket = Arc::clone(&socket);
        handles.push(tokio::spawn(async move {
            socket
                .invoke(key("slack", "acme"), "slack.echo".into(), json!({ "n": n }))
                .await
                .unwrap()
        }));
    }
    for (n, handle) in handles.into_iter().enumerate() {
        assert_eq!(handle.await.unwrap()["input"]["n"], n);
    }
    assert_eq!(slack.calls.load(Ordering::SeqCst), 8);
}
```

Create `crates/core/src/socket.rs` as an empty file.

```rust
// filled in step 3
```

Set `crates/core/src/lib.rs` to:

```rust
//! Core of Socket: providers, credentials, token storage and operations.
//!
//! This crate knows nothing about any specific service.

mod error;
mod operation;
mod provider;
mod secret;
mod socket;
mod store;

pub use error::{Error, ErrorKind, Result, Retry, WireError};
pub use operation::{Connection, Effect, Integration, OperationInfo};
pub use provider::{ApiKeySpec, AuthScheme, KeyPlacement, OAuth2Spec, ProviderId, ProviderSpec};
pub use secret::{SecretString, TokenSet};
pub use socket::{Socket, SocketBuilder};
pub use store::{ConnectionKey, MemoryTokenStore, TokenStore};
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p socketkit-core`
Expected: FAIL to compile, with `unresolved imports socketkit_core::Socket` (the `pub use socket::...` line in `lib.rs` fails first).

- [ ] **Step 3: Write the implementation**

Replace `crates/core/src/socket.rs` with:

```rust
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use serde_json::Value;

use crate::error::{Error, ErrorKind, Result};
use crate::operation::{Connection, Integration, OperationInfo};
use crate::provider::ProviderId;
use crate::store::{ConnectionKey, TokenStore};

/// The handle an application builds once and shares.
pub struct Socket {
    store: Arc<dyn TokenStore>,
    integrations: HashMap<ProviderId, Arc<dyn Integration>>,
    /// Operation name to the provider that owns it.
    owners: HashMap<String, ProviderId>,
    /// Every operation, sorted by name.
    operations: Vec<OperationInfo>,
}

impl fmt::Debug for Socket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut providers: Vec<_> = self.integrations.keys().collect();
        providers.sort();
        f.debug_struct("Socket")
            .field("providers", &providers)
            .field("operations", &self.operations.len())
            .finish_non_exhaustive()
    }
}

impl Socket {
    pub fn builder(store: Arc<dyn TokenStore>) -> SocketBuilder {
        SocketBuilder {
            store,
            integrations: Vec::new(),
        }
    }

    /// Every operation of every registered integration, sorted by name.
    pub fn operations(&self) -> Vec<OperationInfo> {
        self.operations.clone()
    }

    /// Runs the operation called `operation` on the connection `key`.
    ///
    /// `input` must be a JSON object. The store is asked for the connection's
    /// tokens on every call; nothing is cached here.
    pub async fn invoke(&self, key: ConnectionKey, operation: String, input: Value) -> Result<Value> {
        let Some(owner) = self.owners.get(&operation) else {
            return Err(Error::new(
                ErrorKind::Unsupported,
                format!("no operation named {operation:?}"),
            ));
        };
        if *owner != key.provider {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "operation {operation:?} belongs to {owner}, but the connection is for {}",
                    key.provider
                ),
            ));
        }
        if !input.is_object() {
            return Err(
                Error::new(ErrorKind::InvalidInput, "operation input must be a JSON object")
                    .with_provider(owner.clone()),
            );
        }
        let Some(tokens) = self.store.load(key.clone()).await? else {
            return Err(Error::new(
                ErrorKind::ReconnectRequired,
                format!("no stored connection for {owner}"),
            )
            .with_provider(owner.clone()));
        };
        let integration = &self.integrations[owner];
        integration.invoke(Connection { key, tokens }, operation, input).await
    }
}

/// Collects integrations and checks them before a [`Socket`] exists.
pub struct SocketBuilder {
    store: Arc<dyn TokenStore>,
    integrations: Vec<Arc<dyn Integration>>,
}

impl fmt::Debug for SocketBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SocketBuilder")
            .field("integrations", &self.integrations.len())
            .finish_non_exhaustive()
    }
}

impl SocketBuilder {
    pub fn integration(mut self, integration: Arc<dyn Integration>) -> Self {
        self.integrations.push(integration);
        self
    }

    /// Fails with [`ErrorKind::Config`] when a provider spec is invalid, a
    /// provider is registered twice, or an operation name is duplicated or
    /// does not start with its provider's id and a dot.
    pub fn build(self) -> Result<Socket> {
        let mut integrations = HashMap::new();
        let mut owners = HashMap::new();
        let mut operations = Vec::new();

        for integration in self.integrations {
            let spec = integration.provider();
            spec.validate()?;
            let id = spec.id;
            let config = |message: String| Error::new(ErrorKind::Config, message).with_provider(id.clone());

            let prefix = format!("{id}.");
            for info in integration.operations() {
                if info.name.len() <= prefix.len() || !info.name.starts_with(&prefix) {
                    return Err(config(format!(
                        "operation {:?} must be named {prefix}<name>",
                        info.name
                    )));
                }
                if owners.insert(info.name.clone(), id.clone()).is_some() {
                    return Err(config(format!("operation {:?} is registered twice", info.name)));
                }
                operations.push(info);
            }
            if integrations.insert(id.clone(), integration).is_some() {
                return Err(config(format!("provider {id} is registered twice")));
            }
        }

        operations.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Socket {
            store: self.store,
            integrations,
            owners,
            operations,
        })
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p socketkit-core && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`
Expected: PASS, 18 unit tests and 10 tests in `tests/invoke.rs`, with no clippy warnings and no formatting diff.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): add Socket handle with invoke by name"
```

### Task 8: A runnable example, the README and CI

**Files:**
- Create: `crates/core/examples/invoke_by_name.rs`, `README.md`, `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: the whole public API of `socketkit-core` from Tasks 2 to 7.
- Produces: `cargo run -p socketkit-core --example invoke_by_name`, which CI runs. This is the acceptance check for the plan.

- [ ] **Step 1: Write the example**

`crates/core/examples/invoke_by_name.rs`:

```rust
//! Registers one integration and calls an operation by name with JSON.
//! No network: the integration answers from memory.
//!
//! Run with `cargo run -p socketkit-core --example invoke_by_name`.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use socketkit_core::{
    ApiKeySpec, AuthScheme, Connection, ConnectionKey, Effect, Error, ErrorKind, Integration, KeyPlacement,
    MemoryTokenStore, OperationInfo, ProviderId, ProviderSpec, Result, Socket, TokenSet, TokenStore,
};

struct Greeter;

#[async_trait]
impl Integration for Greeter {
    fn provider(&self) -> ProviderSpec {
        ProviderSpec {
            id: ProviderId::new("greeter").expect("valid id"),
            display_name: "Greeter".into(),
            api_base: "https://api.greeter.test/".parse().expect("valid url"),
            allowed_hosts: vec!["api.greeter.test".into()],
            auth: AuthScheme::ApiKey(ApiKeySpec {
                placement: KeyPlacement::Header {
                    name: "Authorization".into(),
                    prefix: Some("Bearer ".into()),
                },
            }),
        }
    }

    fn operations(&self) -> Vec<OperationInfo> {
        vec![OperationInfo {
            name: "greeter.hello".into(),
            description: "Greet someone by name.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "name": { "type": "string" } },
                "required": ["name"]
            }),
            output_schema: json!({ "type": "object", "properties": { "greeting": { "type": "string" } } }),
            effect: Effect::Read,
            required_scopes: Vec::new(),
        }]
    }

    async fn invoke(&self, connection: Connection, _operation: String, input: Value) -> Result<Value> {
        let Some(name) = input.get("name").and_then(Value::as_str) else {
            return Err(Error::new(ErrorKind::InvalidInput, "name is required"));
        };
        Ok(json!({ "greeting": format!("Hello, {name}, from tenant {}", connection.key.tenant) }))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let store = Arc::new(MemoryTokenStore::new());
    let key = ConnectionKey::new(ProviderId::new("greeter")?, "acme");
    store.save(key.clone(), TokenSet::bearer("demo-key")).await?;

    let socket = Socket::builder(store).integration(Arc::new(Greeter)).build()?;

    for operation in socket.operations() {
        println!("{} [{:?}] {}", operation.name, operation.effect, operation.description);
    }

    let output = socket
        .invoke(key.clone(), "greeter.hello".into(), json!({ "name": "Ada" }))
        .await?;
    println!("{output}");

    let refused = socket.invoke(key, "greeter.hello".into(), json!({})).await.unwrap_err();
    println!(
        "{}",
        serde_json::to_string(&refused.to_wire()).expect("wire errors serialize")
    );
    Ok(())
}
```

- [ ] **Step 2: Run it and check the output**

Run: `cargo run -q -p socketkit-core --example invoke_by_name`
Expected, exactly these three lines:

```
greeter.hello [Read] Greet someone by name.
{"greeting":"Hello, Ada, from tenant acme"}
{"code":"invalid_input","provider":null,"retry":"never","retry_after_secs":null,"message":"name is required"}
```

- [ ] **Step 3: Write the README**

`README.md`:

````markdown
# Socket

An open-source library that gives a product the connection, authorisation and operation layers for external services. The application keeps its own credentials; nothing is hosted.

Status: early. The core types and call-by-name exist. HTTP, OAuth and real integrations are next; see [the roadmap](docs/roadmap.md).

- [Vision](docs/vision.md)
- [Design](docs/superpowers/specs/2026-10-08-socket-project-design.md)
- [Catalogue](docs/catalogue.md)

## Try it

```sh
cargo run -p socketkit-core --example invoke_by_name
```

It registers one integration, lists its operations, invokes one by name with JSON, and prints an error in the form other languages will receive.

## Develop

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
````

- [ ] **Step 4: Write the CI workflow**

`.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
  pull_request:

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
      - run: cargo run -p socketkit-core --example invoke_by_name

  msrv:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.85
      - run: cargo check --workspace --all-targets
```

- [ ] **Step 5: Run the full check locally**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: PASS, 28 tests, no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/core/examples README.md .github
git commit -m "docs: add invoke-by-name example, README and CI"
```

## Done when

- `cargo test --workspace` passes with 28 tests.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass.
- The example prints the three expected lines.
- `crates/core/Cargo.toml` lists `tokio` only under `[dev-dependencies]`.

## What this plan leaves for the next ones

- HTTP, the host allowlist applied to real requests, retry, pagination and the generic request: plan 1B.
- The OAuth flow, refresh, and `Socket` refreshing an expired token before `invoke`: plan 1C. Until then `invoke` passes stored tokens through without checking expiry.
- Real providers, `Identity`, `Resolve`, the conformance suite and the facade crate: plan 1D.
