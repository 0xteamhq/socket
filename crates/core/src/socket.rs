use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::auth::{self, Authorization, OAuthClient, PendingAuthorization};
use crate::error::{Error, ErrorKind, Result, Retry};
use crate::http::{Classifier, RawRequest, RawResponse, RetryPolicy, StandardClassifier, Transport};
use crate::operation::{Connection, Integration, OperationInfo};
use crate::provider::{AuthScheme, ClientAuth, OAuth2Spec, ProviderId, ProviderSpec};
use crate::secret::TokenSet;
use crate::store::{ConnectionKey, TokenStore};

/// A token this close to its expiry is refreshed before it is used.
const EXPIRY_SKEW: Duration = Duration::from_secs(60);

struct Registered {
    spec: Arc<ProviderSpec>,
    /// `None` for a provider registered as a definition only.
    integration: Option<Arc<dyn Integration>>,
}

impl Registered {
    fn classifier(&self) -> Arc<dyn Classifier> {
        match &self.integration {
            Some(integration) => integration.classifier(),
            None => Arc::new(StandardClassifier),
        }
    }

    fn parse_token_response(&self, raw: Value, now: SystemTime) -> Result<TokenSet> {
        match &self.integration {
            Some(integration) => integration.parse_token_response(raw, now),
            None => auth::standard_token_response(&self.spec.id, &raw, now),
        }
    }
}

/// The most recent refresh for one connection. Waiters read it instead of refreshing again.
type RefreshSlot = Arc<tokio::sync::Mutex<Option<TokenSet>>>;

/// The handle an application builds once and shares.
pub struct Socket {
    store: Arc<dyn TokenStore>,
    providers: HashMap<ProviderId, Registered>,
    /// Operation name to the provider that owns it.
    owners: HashMap<String, ProviderId>,
    /// Every operation, sorted by name.
    operations: Vec<OperationInfo>,
    oauth_clients: HashMap<ProviderId, OAuthClient>,
    state_secret: Option<Vec<u8>>,
    transport: Transport,
    refreshes: Mutex<HashMap<ConnectionKey, RefreshSlot>>,
}

impl fmt::Debug for Socket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut providers: Vec<_> = self.providers.keys().collect();
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
            providers: Vec::new(),
            oauth_clients: Vec::new(),
            state_secret: None,
            http_client: None,
            retry: RetryPolicy::default(),
        }
    }

    /// Every operation of every registered integration, sorted by name.
    pub fn operations(&self) -> Vec<OperationInfo> {
        self.operations.clone()
    }

    /// The definitions of every registered provider, sorted by id.
    pub fn providers(&self) -> Vec<ProviderSpec> {
        let mut specs: Vec<_> = self.providers.values().map(|r| (*r.spec).clone()).collect();
        specs.sort_by(|a, b| a.id.cmp(&b.id));
        specs
    }

    /// Runs the operation called `operation` on the connection `key`.
    ///
    /// `input` must be a JSON object. An expired token is refreshed first.
    pub async fn invoke(&self, key: ConnectionKey, operation: String, input: Value) -> Result<Value> {
        let Some(owner) = self.owners.get(&operation) else {
            return Err(Error::new(
                ErrorKind::Unsupported,
                format!("no operation named {}", shown(&operation)),
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
        let registered = &self.providers[owner];
        let tokens = self.tokens_for(&key, registered).await?;
        let connection = Connection::new(
            key,
            tokens,
            registered.spec.clone(),
            self.transport.clone(),
            registered.classifier(),
        );
        match &registered.integration {
            Some(integration) => integration.invoke(connection, operation, input).await,
            // `owners` only holds operations that came from an integration.
            None => Err(Error::new(
                ErrorKind::Unsupported,
                format!("no operation named {}", shown(&operation)),
            )),
        }
    }

    /// Calls any endpoint of a registered provider with the connection's
    /// credentials, retry and error classification applied.
    pub async fn request(&self, key: ConnectionKey, request: RawRequest) -> Result<RawResponse> {
        let registered = self.registered(&key.provider)?;
        let tokens = self.tokens_for(&key, registered).await?;
        self.transport
            .send(&registered.spec, &tokens, registered.classifier().as_ref(), request)
            .await
    }

    /// Starts connecting `key`: returns the URL to send the person to and the
    /// record to keep, server-side, until the provider calls back.
    ///
    /// `scopes` replaces the provider's default scopes when given.
    pub fn begin_authorization(&self, key: ConnectionKey, scopes: Option<Vec<String>>) -> Result<Authorization> {
        let registered = self.registered(&key.provider)?;
        let (oauth, client, secret) = self.oauth_parts(registered)?;
        let state = auth::sign_state(&key, secret, SystemTime::now())?;
        let (pkce_verifier, challenge) = if oauth.pkce {
            let (verifier, challenge) = auth::pkce_pair()?;
            (Some(verifier), Some(challenge))
        } else {
            (None, None)
        };
        let scopes = scopes.unwrap_or_else(|| oauth.default_scopes.clone());
        let url = auth::authorization_url(oauth, client, &state, &scopes, challenge.as_deref());
        Ok(Authorization {
            url,
            pending: PendingAuthorization {
                key,
                state,
                pkce_verifier,
            },
        })
    }

    /// Finishes connecting: checks `state`, exchanges `code` for tokens, saves
    /// them in the store, and returns them.
    ///
    /// `code` and `state` are the query parameters the provider sent to the callback.
    pub async fn complete_authorization(
        &self,
        pending: PendingAuthorization,
        code: String,
        state: String,
    ) -> Result<TokenSet> {
        let registered = self.registered(&pending.key.provider)?;
        let provider = &registered.spec.id;
        let (oauth, client, secret) = self.oauth_parts(registered)?;
        let invalid = |message: String| Error::new(ErrorKind::InvalidInput, message).with_provider(provider.clone());
        if state != pending.state {
            return Err(invalid(
                "the authorization state does not match the one that was issued".into(),
            ));
        }
        auth::verify_state(&state, secret, SystemTime::now(), &pending.key)?;
        if code.is_empty() {
            return Err(invalid(format!("{provider} sent no authorization code")));
        }

        let mut form = vec![
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", client.redirect_uri.as_str()),
        ];
        if let Some(verifier) = &pending.pkce_verifier {
            form.push(("code_verifier", verifier.expose()));
        }
        let now = SystemTime::now();
        let response = self.token_request(registered, oauth, client, form).await?;
        let refusal = match response.status {
            200..=299 => auth::grant_refusal(&response.body),
            400..=499 => Some(auth::grant_refusal(&response.body).unwrap_or_else(|| "refused".into())),
            status => {
                return Err(
                    Error::new(ErrorKind::Unexpected, format!("{provider} returned HTTP {status}"))
                        .with_provider(provider.clone())
                        .with_retry(Retry::Later),
                );
            }
        };
        if let Some(code) = refusal {
            return Err(invalid(format!(
                "{provider} refused the authorization code ({code}); start the connection again"
            )));
        }
        let tokens = registered.parse_token_response(response.body, now)?;
        self.store.save(pending.key, tokens.clone()).await?;
        Ok(tokens)
    }

    fn registered(&self, provider: &ProviderId) -> Result<&Registered> {
        self.providers.get(provider).ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                format!("no provider named {provider} is registered"),
            )
        })
    }

    fn oauth_parts<'a>(&'a self, registered: &'a Registered) -> Result<(&'a OAuth2Spec, &'a OAuthClient, &'a [u8])> {
        let provider = &registered.spec.id;
        let config = |message: String| Error::new(ErrorKind::Config, message).with_provider(provider.clone());
        let AuthScheme::OAuth2(oauth) = &registered.spec.auth else {
            return Err(config(format!(
                "{provider} does not use OAuth; save its API key in the token store"
            )));
        };
        let client = self
            .oauth_clients
            .get(provider)
            .ok_or_else(|| config(format!("no OAuth client is set for {provider}")))?;
        let secret = self
            .state_secret
            .as_deref()
            .ok_or_else(|| config("no state secret is set".into()))?;
        Ok((oauth, client, secret))
    }

    async fn token_request(
        &self,
        registered: &Registered,
        oauth: &OAuth2Spec,
        client: &OAuthClient,
        mut form: Vec<(&str, &str)>,
    ) -> Result<RawResponse> {
        let basic = match oauth.client_auth {
            ClientAuth::Basic => Some((client.client_id.as_str(), client.client_secret.expose())),
            ClientAuth::Body => {
                form.push(("client_id", client.client_id.as_str()));
                form.push(("client_secret", client.client_secret.expose()));
                None
            }
        };
        self.transport
            .post_form(&registered.spec, &oauth.token_url, &form, basic)
            .await
    }

    /// The connection's tokens, refreshed first when they have expired.
    async fn tokens_for(&self, key: &ConnectionKey, registered: &Registered) -> Result<TokenSet> {
        let provider = &registered.spec.id;
        let reconnect =
            |message: String| Error::new(ErrorKind::ReconnectRequired, message).with_provider(provider.clone());
        let Some(tokens) = self.store.load(key.clone()).await? else {
            return Err(reconnect(format!("no stored connection for {provider}")));
        };
        if tokens.access_token.expose().is_empty() {
            return Err(reconnect(format!(
                "the stored connection for {provider} has no access token"
            )));
        }
        if !tokens.is_expired(SystemTime::now(), EXPIRY_SKEW) {
            return Ok(tokens);
        }
        let AuthScheme::OAuth2(oauth) = &registered.spec.auth else {
            return Ok(tokens);
        };
        let Some(refresh_token) = tokens.refresh_token.clone() else {
            return Err(reconnect(format!("the authorization for {provider} has expired")));
        };
        let client = self.oauth_clients.get(provider).ok_or_else(|| {
            Error::new(
                ErrorKind::Config,
                format!("no OAuth client is set for {provider}, so its token cannot be refreshed"),
            )
            .with_provider(provider.clone())
        })?;

        // One refresh at a time per connection: a provider that rotates refresh
        // tokens invalidates the old one on use, so a second refresh would
        // break the connection. The lock covers only the call to the provider;
        // the store is never called while it is held.
        let slot = self.refresh_slot(key);
        let (fresh, refreshed_here) = {
            let mut latest = slot.lock().await;
            match latest
                .as_ref()
                .filter(|t| !t.is_expired(SystemTime::now(), EXPIRY_SKEW))
            {
                Some(already) => (already.clone(), false),
                None => {
                    let fresh = self
                        .refresh(registered, oauth, client, &tokens, refresh_token.expose())
                        .await?;
                    *latest = Some(fresh.clone());
                    (fresh, true)
                }
            }
        };
        if refreshed_here {
            self.store.save(key.clone(), fresh.clone()).await?;
        }
        Ok(fresh)
    }

    fn refresh_slot(&self, key: &ConnectionKey) -> RefreshSlot {
        let mut slots = self.refreshes.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        slots.entry(key.clone()).or_default().clone()
    }

    async fn refresh(
        &self,
        registered: &Registered,
        oauth: &OAuth2Spec,
        client: &OAuthClient,
        old: &TokenSet,
        refresh_token: &str,
    ) -> Result<TokenSet> {
        let provider = &registered.spec.id;
        let form = vec![("grant_type", "refresh_token"), ("refresh_token", refresh_token)];
        let now = SystemTime::now();
        let response = self.token_request(registered, oauth, client, form).await?;
        let refused = match response.status {
            200..=299 => auth::grant_refusal(&response.body).is_some(),
            400..=499 => true,
            status => {
                return Err(
                    Error::new(ErrorKind::Unexpected, format!("{provider} returned HTTP {status}"))
                        .with_provider(provider.clone())
                        .with_retry(Retry::Later),
                );
            }
        };
        if refused {
            // Distinct from a transient failure: only the person can fix this.
            return Err(Error::new(
                ErrorKind::ReconnectRequired,
                format!("{provider} no longer accepts the stored authorization"),
            )
            .with_provider(provider.clone()));
        }
        let mut fresh = registered.parse_token_response(response.body, now)?;
        // Providers that do not rotate omit the refresh token and often the scopes.
        if fresh.refresh_token.is_none() {
            fresh.refresh_token = old.refresh_token.clone();
        }
        if fresh.scopes.is_empty() {
            fresh.scopes = old.scopes.clone();
        }
        Ok(fresh)
    }
}

/// A caller-supplied name, shortened so an error message stays a message.
fn shown(name: &str) -> String {
    const LIMIT: usize = 80;
    if name.chars().count() <= LIMIT {
        format!("{name:?}")
    } else {
        format!("{:?}…", name.chars().take(LIMIT).collect::<String>())
    }
}

/// Collects integrations and settings, and checks them before a [`Socket`] exists.
pub struct SocketBuilder {
    store: Arc<dyn TokenStore>,
    integrations: Vec<Arc<dyn Integration>>,
    providers: Vec<ProviderSpec>,
    oauth_clients: Vec<(ProviderId, OAuthClient)>,
    state_secret: Option<Vec<u8>>,
    http_client: Option<reqwest::Client>,
    retry: RetryPolicy,
}

impl fmt::Debug for SocketBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SocketBuilder")
            .field("integrations", &self.integrations.len())
            .field("providers", &self.providers.len())
            .finish_non_exhaustive()
    }
}

impl SocketBuilder {
    /// Registers an integration: its provider and its operations.
    pub fn integration(mut self, integration: Arc<dyn Integration>) -> Self {
        self.integrations.push(integration);
        self
    }

    /// Registers a provider by definition alone. It can be authorised and
    /// called through [`Socket::request`]; it has no operations.
    pub fn provider(mut self, spec: ProviderSpec) -> Self {
        self.providers.push(spec);
        self
    }

    /// Sets the application's own OAuth app for `provider`.
    pub fn oauth_client(mut self, provider: ProviderId, client: OAuthClient) -> Self {
        self.oauth_clients.push((provider, client));
        self
    }

    /// Sets the secret that signs the OAuth `state` value. At least 32 bytes
    /// of high-entropy data; the same value on every instance of the application.
    pub fn state_secret(mut self, secret: impl Into<Vec<u8>>) -> Self {
        self.state_secret = Some(secret.into());
        self
    }

    /// Uses the application's own HTTP client, with its proxies and timeouts.
    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.http_client = Some(client);
        self
    }

    pub fn retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Fails with [`ErrorKind::Config`] when a provider spec is invalid, a
    /// provider is registered twice, an operation name is duplicated or does
    /// not start with its provider's id and a dot, an OAuth client names an
    /// unregistered provider, or the state secret is too short.
    pub fn build(self) -> Result<Socket> {
        let mut providers: HashMap<ProviderId, Registered> = HashMap::new();
        let mut owners = HashMap::new();
        let mut operations = Vec::new();

        let entries = self
            .integrations
            .into_iter()
            .map(|integration| (integration.provider(), Some(integration)))
            .chain(self.providers.into_iter().map(|spec| (spec, None)));
        for (spec, integration) in entries {
            spec.validate()?;
            let id = spec.id.clone();
            let config = |message: String| Error::new(ErrorKind::Config, message).with_provider(id.clone());
            if providers.contains_key(&id) {
                return Err(config(format!("provider {id} is registered twice")));
            }
            if let Some(integration) = &integration {
                let prefix = format!("{id}.");
                for info in integration.operations() {
                    let rest = info.name.strip_prefix(&prefix).unwrap_or("");
                    let well_formed = !rest.is_empty()
                        && rest.split('.').all(|part| {
                            !part.is_empty()
                                && part
                                    .chars()
                                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                        });
                    if !well_formed {
                        return Err(config(format!(
                            "operation {:?} must be named {prefix}<name>, in lowercase letters, digits, `_` and `.`",
                            info.name
                        )));
                    }
                    if owners.insert(info.name.clone(), id.clone()).is_some() {
                        return Err(config(format!("operation {:?} is registered twice", info.name)));
                    }
                    operations.push(info);
                }
            }
            providers.insert(
                id,
                Registered {
                    spec: Arc::new(spec),
                    integration,
                },
            );
        }

        let mut oauth_clients = HashMap::new();
        for (provider, client) in self.oauth_clients {
            let config = |message: String| Error::new(ErrorKind::Config, message).with_provider(provider.clone());
            if !providers.contains_key(&provider) {
                return Err(config(format!(
                    "an OAuth client is set for {provider}, which is not registered"
                )));
            }
            if client.client_id.trim().is_empty() || client.client_secret.expose().trim().is_empty() {
                return Err(config(format!(
                    "the OAuth client for {provider} has a blank id or secret"
                )));
            }
            oauth_clients.insert(provider, client);
        }
        if let Some(secret) = &self.state_secret {
            if secret.len() < auth::MIN_STATE_SECRET {
                return Err(Error::new(
                    ErrorKind::Config,
                    format!("the state secret must be at least {} bytes", auth::MIN_STATE_SECRET),
                ));
            }
        }
        let client = match self.http_client {
            Some(client) => client,
            None => reqwest::Client::builder().build().map_err(|e| {
                Error::new(ErrorKind::Config, "could not build the HTTP client").with_source(e.without_url())
            })?,
        };
        if self.retry.max_attempts == 0 {
            return Err(Error::new(
                ErrorKind::Config,
                "the retry policy must allow at least one attempt",
            ));
        }

        operations.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Socket {
            store: self.store,
            providers,
            owners,
            operations,
            oauth_clients,
            state_secret: self.state_secret,
            transport: Transport::new(client, self.retry),
            refreshes: Mutex::new(HashMap::new()),
        })
    }
}
