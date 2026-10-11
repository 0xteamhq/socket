//! Core of Socket: providers, credentials, token storage, transport,
//! authorisation and operations.
//!
//! This crate knows nothing about any specific service.

mod auth;
mod error;
mod http;
mod oauth;
mod operation;
mod provider;
mod secret;
mod socket;
mod store;

pub use auth::{Authorization, OAuthClient, PendingAuthorization, standard_token_response};
pub use error::{Error, ErrorKind, Result, Retry, WireError};
pub use http::{
    Classifier, Content, ContentRequest, Page, RawRequest, RawResponse, RetryPolicy, StandardClassifier,
    provider_message,
};
pub use oauth::{AuthorizationRequest, CodeGrant, Grant, OAuthContext, OAuthFlow, StandardOAuth};
pub use operation::{
    Access, Account, Connection, Effect, Integration, OperationInfo, Resource, identity_operation, resolve_input,
    resolve_operation, schema_of, to_output,
};
pub use provider::{
    ApiKeySpec, AuthScheme, ClientAuth, ContentHost, KeyPlacement, OAuth2Spec, ProviderId, ProviderSpec,
};
pub use secret::{SecretString, TokenSet};
pub use socket::{Socket, SocketBuilder};
pub use store::{ConnectionKey, MemoryTokenStore, TokenStore};
