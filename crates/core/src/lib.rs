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
