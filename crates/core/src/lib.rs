//! Core of Socket: providers, credentials, token storage and operations.
//!
//! This crate knows nothing about any specific service.

mod error;
mod provider;
mod secret;

pub use error::{Error, ErrorKind, Result, Retry, WireError};
pub use provider::ProviderId;
pub use secret::{SecretString, TokenSet};
