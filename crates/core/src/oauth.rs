//! The OAuth steps an integration may customise.
//!
//! [`OAuthFlow`] has a working default for every step, so a provider that
//! follows the standard implements nothing. A provider that differs overrides
//! only the step where it differs.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use serde_json::Value;
use url::Url;

use crate::auth::{self, OAuthClient};
use crate::error::{Error, ErrorKind, Result, Retry};
use crate::http::{RawResponse, Transport, shortened};
use crate::provider::{ClientAuth, OAuth2Spec, ProviderId, ProviderSpec};
use crate::secret::{SecretString, TokenSet};

/// What Socket asks for when it needs the URL to send a person to.
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    /// The signed state. It must appear, unchanged, as the `state` parameter of the URL.
    pub state: String,
    pub scopes: Vec<String>,
    /// The PKCE challenge (S256), when the provider uses PKCE.
    pub pkce_challenge: Option<String>,
}

/// What the provider sent to the callback, once Socket has checked the state.
#[derive(Debug, Clone)]
pub struct CodeGrant {
    pub code: String,
    pub pkce_verifier: Option<SecretString>,
}

/// Which grant a token response answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grant {
    /// An authorization code. A refusal means: start connecting again.
    Code,
    /// A refresh token. A refusal means: the person must reconnect.
    Refresh,
}

/// What a step of the flow is given to work with: the provider's definition,
/// the application's OAuth app, and a way to reach the token endpoint.
#[derive(Clone)]
pub struct OAuthContext {
    spec: Arc<ProviderSpec>,
    settings: OAuth2Spec,
    client: OAuthClient,
    transport: Transport,
}

impl fmt::Debug for OAuthContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthContext")
            .field("provider", &self.spec.id)
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}

impl OAuthContext {
    pub(crate) fn new(
        spec: Arc<ProviderSpec>,
        settings: OAuth2Spec,
        client: OAuthClient,
        transport: Transport,
    ) -> Self {
        Self {
            spec,
            settings,
            client,
            transport,
        }
    }

    pub fn provider(&self) -> &ProviderSpec {
        &self.spec
    }

    /// The provider's OAuth settings: its URLs, scopes and how the client authenticates.
    pub fn settings(&self) -> &OAuth2Spec {
        &self.settings
    }

    /// The application's own OAuth app.
    pub fn client(&self) -> &OAuthClient {
        &self.client
    }

    /// Posts `form` to the provider's token endpoint, adding the client's id
    /// and secret the way the provider's definition says.
    ///
    /// This is the only way a step can send the client secret anywhere, and
    /// it goes to the token endpoint of the definition, which must be one of
    /// the provider's allowed hosts. It is never retried: a code or a rotating
    /// refresh token can be spent only once.
    pub async fn post_token(&self, form: Vec<(String, String)>) -> Result<RawResponse> {
        let mut pairs: Vec<(&str, &str)> = form
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let basic = match self.settings.client_auth {
            ClientAuth::Basic => Some((self.client.client_id.as_str(), self.client.client_secret.expose())),
            ClientAuth::Body => {
                pairs.push(("client_id", self.client.client_id.as_str()));
                pairs.push(("client_secret", self.client.client_secret.expose()));
                None
            }
        };
        self.transport
            .post_form(&self.spec, &self.settings.token_url, &pairs, basic)
            .await
    }

    /// Reads the token endpoint's answer: the body when tokens were granted,
    /// or the error a caller can act on.
    ///
    /// A declined code or refresh token is the person's to fix. Throttling, a
    /// rejected OAuth client and a failing provider are not, and are reported
    /// as what they are so that nobody is told to reconnect for them.
    pub fn granted(&self, response: RawResponse, grant: Grant) -> Result<Value> {
        let provider = &self.spec.id;
        match token_outcome(provider, &response)? {
            None => Ok(response.body),
            Some(code) => Err(match grant {
                Grant::Code => Error::new(
                    ErrorKind::InvalidInput,
                    format!("{provider} refused the authorization code ({code}); start the connection again"),
                ),
                Grant::Refresh => Error::new(
                    ErrorKind::ReconnectRequired,
                    format!("{provider} no longer accepts the stored authorization"),
                ),
            }
            .with_provider(provider.clone())),
        }
    }
}

/// The steps of the OAuth authorization-code flow.
///
/// Socket keeps for itself the parts that must not vary: signing and checking
/// the state, generating the PKCE pair, refreshing one at a time per
/// connection, and saving tokens. Each method here is one step in between,
/// with a default that follows the standard.
#[async_trait]
pub trait OAuthFlow: Send + Sync {
    /// Builds the URL the person is sent to. It must carry `request.state`
    /// unchanged as its `state` parameter; Socket refuses a URL that does not.
    fn authorization_url(&self, context: OAuthContext, request: AuthorizationRequest) -> Result<Url> {
        Ok(auth::authorization_url(
            context.settings(),
            context.client(),
            &request.state,
            &request.scopes,
            request.pkce_challenge.as_deref(),
        ))
    }

    /// Exchanges an authorization code for tokens.
    async fn exchange_code(&self, context: OAuthContext, grant: CodeGrant) -> Result<TokenSet> {
        let mut form = vec![
            ("grant_type".to_owned(), "authorization_code".to_owned()),
            ("code".to_owned(), grant.code),
            ("redirect_uri".to_owned(), context.client().redirect_uri.to_string()),
        ];
        if let Some(verifier) = &grant.pkce_verifier {
            form.push(("code_verifier".to_owned(), verifier.expose().to_owned()));
        }
        let now = SystemTime::now();
        let response = context.post_token(form).await?;
        let body = context.granted(response, Grant::Code)?;
        self.parse_token_response(context.provider().id.clone(), body, now)
    }

    /// Exchanges a refresh token for new tokens. `current` is the connection's
    /// stored tokens; what the provider does not send again is kept from them.
    async fn refresh(&self, context: OAuthContext, current: TokenSet) -> Result<TokenSet> {
        let provider = context.provider().id.clone();
        let Some(refresh_token) = current.refresh_token.clone() else {
            return Err(Error::new(
                ErrorKind::ReconnectRequired,
                format!("the authorization for {provider} has expired"),
            )
            .with_provider(provider));
        };
        let form = vec![
            ("grant_type".to_owned(), "refresh_token".to_owned()),
            ("refresh_token".to_owned(), refresh_token.expose().to_owned()),
        ];
        let now = SystemTime::now();
        let response = context.post_token(form).await?;
        let body = context.granted(response, Grant::Refresh)?;
        let mut fresh = self.parse_token_response(provider, body, now)?;
        // Providers that do not rotate omit the refresh token and often the scopes.
        if fresh.refresh_token.is_none() {
            fresh.refresh_token = current.refresh_token;
        }
        if fresh.scopes.is_empty() {
            fresh.scopes = current.scopes;
        }
        Ok(fresh)
    }

    /// Reads a granted token response. `now` is the moment it arrived.
    fn parse_token_response(&self, provider: ProviderId, raw: Value, now: SystemTime) -> Result<TokenSet> {
        auth::standard_token_response(&provider, &raw, now)
    }
}

/// The flow for a provider that follows the standard in every step.
#[derive(Debug, Clone, Copy, Default)]
pub struct StandardOAuth;

impl OAuthFlow for StandardOAuth {}

/// Reads the token endpoint's answer.
///
/// `Ok(None)` means tokens were granted. `Ok(Some(code))` means the provider
/// declined this code or refresh token, which only the person can fix.
/// Everything else is an error that is not the person's to fix.
fn token_outcome(provider: &ProviderId, response: &RawResponse) -> Result<Option<String>> {
    let error = |kind, message: String| Error::new(kind, message).with_provider(provider.clone());
    let refusal = auth::grant_refusal(&response.body).map(|code| shortened(&code, 80));
    let client_rejected = matches!(refusal.as_deref(), Some("invalid_client" | "unauthorized_client"));
    if response.status == 429 {
        let retry = match response
            .header("retry-after")
            .and_then(|v| v.trim().parse::<u64>().ok())
        {
            Some(secs) => Retry::After(Duration::from_secs(secs)),
            None => Retry::Later,
        };
        return Err(error(ErrorKind::RateLimited, format!("{provider} is rate limiting requests")).with_retry(retry));
    }
    if response.status == 401 || client_rejected {
        return Err(error(
            ErrorKind::Config,
            format!("{provider} rejected the application's OAuth client; check its client id and secret"),
        ));
    }
    match response.status {
        200..=299 => Ok(refusal),
        408 | 500..=599 => Err(error(
            ErrorKind::Unexpected,
            format!("{provider} returned HTTP {}", response.status),
        )
        .with_retry(Retry::Later)),
        400..=499 => Ok(Some(refusal.unwrap_or_else(|| "refused".into()))),
        status => Err(error(
            ErrorKind::Unexpected,
            format!("{provider} returned HTTP {status}"),
        )),
    }
}
