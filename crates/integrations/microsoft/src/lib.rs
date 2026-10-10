//! Socket integration for Microsoft Graph.
//!
//! One provider for everything behind Microsoft's one sign-in. Offers the
//! provider definition, identity, and typed methods for Teams meetings:
//! online meetings, their transcripts, recordings and attendance. Every typed
//! method is also a named operation. See `docs/integrations/microsoft.md`.

mod client;
pub mod models;
mod operations;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Access, Account, AuthScheme, Classifier, ClientAuth, Connection, Error, ErrorKind, Integration, OAuth2Spec,
    OAuthClient, OperationInfo, ProviderId, ProviderSpec, RawRequest, RawResponse, Result, Retry, SecretString,
    StandardClassifier, TokenSet, identity_operation, to_output,
};

pub use client::{Attendance, OnlineMeetings, Recordings, Transcripts};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "microsoft";

/// The tenant that lets any work, school or personal account sign in.
const ANY_TENANT: &str = "common";

/// Microsoft's definition: where Graph lives and how it authenticates.
///
/// # Panics
/// Never in practice: the URLs are constants that parse.
pub fn provider() -> ProviderSpec {
    let sign_in = |tenant: &str, step: &str| {
        format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/{step}")
            .parse()
            .expect("a valid URL")
    };
    ProviderSpec {
        id: ProviderId::new(PROVIDER_ID).expect("a valid provider id"),
        display_name: "Microsoft".into(),
        api_base: "https://graph.microsoft.com/v1.0/".parse().expect("a valid URL"),
        // login.microsoftonline.com serves the OAuth token endpoint.
        allowed_hosts: vec!["graph.microsoft.com".into(), "login.microsoftonline.com".into()],
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: sign_in(ANY_TENANT, "authorize"),
            token_url: sign_in(ANY_TENANT, "token"),
            // Without `offline_access` Microsoft issues no refresh token.
            // Everything else is the application's to ask for.
            default_scopes: vec!["offline_access".into(), "User.Read".into()],
            scope_separator: " ".into(),
            pkce: true,
            client_auth: ClientAuth::Body,
            extra_authorize_params: Vec::new(),
        }),
    }
}

/// Graph follows HTTP conventions, with two additions: it says how long to
/// wait when it is unavailable, and it names a tenant's own restriction in an
/// inner error code that is steadier than its message.
#[derive(Debug, Clone, Copy, Default)]
pub struct MicrosoftClassifier;

impl Classifier for MicrosoftClassifier {
    fn classify(&self, provider: &ProviderId, response: &RawResponse) -> Result<()> {
        let error = |kind, message: String| Error::new(kind, message).with_provider(provider.clone());
        if response.status == 403 {
            // Graph writes the name both as `innerError` and as `innererror`.
            let graph = &response.body["error"];
            let inner = [&graph["innerError"]["code"], &graph["innererror"]["code"]]
                .into_iter()
                .find_map(Value::as_str);
            let reason = match inner {
                Some("GraphAccessToTranscriptsDisabled") => {
                    Some("an administrator has switched off reading transcripts through the API for this organisation")
                }
                Some("SpeakerAttributionNotAllowed") => Some(
                    "an administrator has switched off speaker names in transcripts for this organisation, \
                     and the transcript is only available without them",
                ),
                _ => None,
            };
            if let Some(reason) = reason {
                return Err(error(
                    ErrorKind::AccessDenied,
                    format!("{provider} denied the request: {reason}"),
                ));
            }
        }
        let wait = response
            .header("retry-after")
            .and_then(|seconds| seconds.trim().parse::<u64>().ok());
        if let (503 | 504, Some(seconds)) = (response.status, wait) {
            return Err(error(
                ErrorKind::Unexpected,
                format!("{provider} returned HTTP {}", response.status),
            )
            .with_retry(Retry::After(Duration::from_secs(seconds))));
        }
        StandardClassifier.classify(provider, response)
    }
}

/// OAuth settings for Microsoft. A plain [`OAuthClient`] converts into this with
/// the defaults, so `Microsoft::with_oauth(client)` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct MicrosoftOAuth {
    /// The application's own app registration.
    pub client: OAuthClient,
    /// Graph permissions to ask for in place of the defaults. Keep
    /// `offline_access` among them, or Microsoft issues no refresh token.
    pub scopes: Option<Vec<String>>,
    /// Who may sign in: `common` (the default) for any account,
    /// `organizations` for work and school accounts, `consumers` for personal
    /// ones, or one organisation's tenant id or domain.
    pub tenant: Option<String>,
    /// Prefills the sign-in page with this account (Microsoft's `login_hint` parameter).
    pub login_hint: Option<String>,
    /// Microsoft's `prompt` parameter: `login`, `none`, `consent` or `select_account`.
    pub prompt: Option<String>,
}

impl From<OAuthClient> for MicrosoftOAuth {
    fn from(client: OAuthClient) -> Self {
        Self {
            client,
            scopes: None,
            tenant: None,
            login_hint: None,
            prompt: None,
        }
    }
}

/// A Microsoft Graph access token. A plain string converts into this, so
/// `Microsoft::with_token("…")` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct MicrosoftToken {
    pub token: SecretString,
}

impl From<String> for MicrosoftToken {
    fn from(token: String) -> Self {
        Self {
            token: SecretString::new(token),
        }
    }
}

impl From<&str> for MicrosoftToken {
    fn from(token: &str) -> Self {
        token.to_owned().into()
    }
}

/// True for what may stand in the sign-in address as the tenant: one of
/// Microsoft's words, a tenant id, or a domain. Labels of letters, digits and
/// `-`, joined by dots.
fn is_tenant(tenant: &str) -> bool {
    let is_label = |label: &str| {
        (1..=63).contains(&label.len())
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    tenant.len() <= 253 && tenant.split('.').all(is_label)
}

/// The Microsoft integration.
#[derive(Debug, Clone)]
pub struct Microsoft {
    spec: ProviderSpec,
    access: Access,
    /// What is wrong with the settings it was created with, if anything.
    problem: Option<String>,
}

impl Default for Microsoft {
    fn default() -> Self {
        Self::new()
    }
}

impl Microsoft {
    /// Microsoft with no connection details of its own: the OAuth app is set on the
    /// `Socket` builder and tokens come from the application's token store.
    pub fn new() -> Self {
        Self::with_spec(provider())
    }

    /// Microsoft with the application's app registration, for connecting users through OAuth.
    /// Takes an [`OAuthClient`], or a [`MicrosoftOAuth`] for the settings only Microsoft has.
    pub fn with_oauth(settings: impl Into<MicrosoftOAuth>) -> Self {
        let settings = settings.into();
        let mut this = Self::new();
        if let AuthScheme::OAuth2(oauth) = &mut this.spec.auth {
            if let Some(scopes) = settings.scopes {
                oauth.default_scopes = scopes;
            }
            if let Some(hint) = settings.login_hint {
                oauth.extra_authorize_params.push(("login_hint".into(), hint));
            }
            if let Some(prompt) = settings.prompt {
                oauth.extra_authorize_params.push(("prompt".into(), prompt));
            }
        }
        let this = match settings.tenant {
            Some(tenant) => this.tenant(&tenant),
            None => this,
        };
        this.oauth(settings.client)
    }

    /// Microsoft with a token the application already holds. Every call uses it.
    /// Takes a string, or a [`MicrosoftToken`] for the settings only Microsoft has.
    pub fn with_token(settings: impl Into<MicrosoftToken>) -> Self {
        let settings = settings.into();
        let mut this = Self::new();
        this.access.token = Some(TokenSet {
            access_token: settings.token,
            refresh_token: None,
            expires_at: None,
            scopes: Vec::new(),
        });
        this
    }

    /// Uses another definition, for a test server.
    pub fn with_spec(spec: ProviderSpec) -> Self {
        Self {
            spec,
            access: Access::default(),
            problem: None,
        }
    }

    /// Sets the application's OAuth app.
    pub fn oauth(mut self, client: OAuthClient) -> Self {
        self.access.oauth = Some(client);
        self
    }

    /// Sets a token the application already holds.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.access.token = Some(TokenSet::bearer(token));
        self
    }

    /// Signs people in at one tenant. The tenant is part of the address the
    /// client secret is posted to, so anything that is not a tenant is
    /// reported when the `Socket` is built and never put in that address.
    fn tenant(mut self, tenant: &str) -> Self {
        let tenant = tenant.trim();
        if !is_tenant(tenant) {
            self.problem = Some(format!(
                "{tenant:?} is not a Microsoft tenant; use common, organizations, consumers, a tenant id or a domain"
            ));
            return self;
        }
        if let AuthScheme::OAuth2(oauth) = &mut self.spec.auth {
            for url in [&mut oauth.authorize_url, &mut oauth.token_url] {
                url.set_path(&url.path().replacen(ANY_TENANT, tenant, 1));
            }
        }
        self
    }

    /// Teams online meetings: the meeting behind an id or a join link.
    pub fn online_meetings<'a>(&self, connection: &'a Connection) -> OnlineMeetings<'a> {
        OnlineMeetings(client::Api { connection })
    }

    /// Transcripts of Teams online meetings, and what was said in them.
    pub fn transcripts<'a>(&self, connection: &'a Connection) -> Transcripts<'a> {
        Transcripts(client::Api { connection })
    }

    /// Recordings of Teams online meetings.
    pub fn recordings<'a>(&self, connection: &'a Connection) -> Recordings<'a> {
        Recordings(client::Api { connection })
    }

    /// Who joined a Teams online meeting, when, and for how long.
    pub fn attendance<'a>(&self, connection: &'a Connection) -> Attendance<'a> {
        Attendance(client::Api { connection })
    }

    /// The account the connection is authorised as. Needs the `User.Read` permission.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let body = connection.request(RawRequest::get("me")).await?.body;
        let filled = |value: &Value| value.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
        let Some(id) = filled(&body["id"]) else {
            return Err(Error::new(ErrorKind::Decode, "microsoft answered without an account")
                .with_provider(self.spec.id.clone()));
        };
        // An account without a mailbox has no `mail`; its sign-in name is what a person knows it by.
        let email = filled(&body["mail"]).or_else(|| filled(&body["userPrincipalName"]));
        let name = filled(&body["displayName"])
            .or_else(|| email.clone())
            .unwrap_or_else(|| id.clone());
        Ok(Account { id, name, email })
    }
}

#[async_trait]
impl Integration for Microsoft {
    fn provider(&self) -> ProviderSpec {
        self.spec.clone()
    }

    /// Microsoft's operations are named `microsoft.…`, so the definition must keep that id.
    fn check(&self) -> Result<()> {
        let config = |message: String| Error::new(ErrorKind::Config, message).with_provider(self.spec.id.clone());
        if let Some(problem) = &self.problem {
            return Err(config(problem.clone()));
        }
        if self.spec.id.as_str() != PROVIDER_ID {
            return Err(config(format!(
                "the Microsoft integration needs the provider id {PROVIDER_ID:?}, not {:?}",
                self.spec.id.as_str()
            )));
        }
        Ok(())
    }

    fn oauth_client(&self) -> Option<OAuthClient> {
        self.access.oauth.clone()
    }

    fn fixed_token(&self) -> Option<TokenSet> {
        self.access.token.clone()
    }

    fn operations(&self) -> Vec<OperationInfo> {
        let mut operations = vec![identity_operation(&self.spec.id)];
        operations.extend(operations::all().iter().map(|operation| operation.info.clone()));
        operations
    }

    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value> {
        let id = &self.spec.id;
        match operation.strip_prefix(&format!("{id}.")) {
            Some("identity.get") => to_output(id, &self.identity(&connection).await?),
            _ => match operations::all().iter().find(|known| known.info.name == operation) {
                Some(known) => known.run(self.clone(), connection, input).await,
                None => Err(Error::new(
                    ErrorKind::Unsupported,
                    format!("microsoft has no operation {operation:?}"),
                )
                .with_provider(id.clone())),
            },
        }
    }

    fn classifier(&self) -> Arc<dyn Classifier> {
        Arc::new(MicrosoftClassifier)
    }
}
