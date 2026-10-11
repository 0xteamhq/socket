//! Socket integration for the HubSpot CRM.
//!
//! Offers the provider definition, identity, lookup of an object type, and
//! typed methods for the CRM grouped by area: records of any object type
//! (`objects`), the links between them (`associations`), the fields of an
//! object type (`properties`), the stages of deals and tickets (`pipelines`)
//! and who records are assigned to (`owners`). Every typed method is also a
//! named operation. See `docs/integrations/hubspot.md`.

mod client;
pub mod models;
mod operations;

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Access, Account, AuthScheme, AuthorizationRequest, Classifier, ClientAuth, Connection, Error, ErrorKind,
    Integration, OAuth2Spec, OAuthClient, OAuthContext, OAuthFlow, OperationInfo, ProviderId, ProviderSpec, RawRequest,
    RawResponse, Resource, Result, Retry, SecretString, StandardClassifier, StandardOAuth, TokenSet,
    identity_operation, resolve_input, resolve_operation, standard_token_response, to_output,
};
use url::Url;

pub use client::{Associations, Objects, Owners, Pipelines, Properties};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "hubspot";

/// The version of HubSpot's API this crate is written against.
///
/// HubSpot names a version by the month it was released and keeps it for
/// eighteen months. Every address this crate calls carries this one, so
/// moving to the next is a change here and a reading of what HubSpot changed.
pub(crate) const API_VERSION: &str = "2026-09";

/// The scope HubSpot requires of every OAuth app. It is also the one that
/// reads the account's details.
const OAUTH: &str = "oauth";

/// HubSpot's definition: where its API lives and how it authenticates.
///
/// # Panics
/// Never in practice: the URLs are constants that parse.
pub fn provider() -> ProviderSpec {
    ProviderSpec {
        id: ProviderId::new(PROVIDER_ID).expect("a valid provider id"),
        display_name: "HubSpot".into(),
        api_base: "https://api.hubapi.com/".parse().expect("a valid URL"),
        // The token endpoint is on the API's own host. The page a person
        // approves on, app.hubspot.com, is never sent a credential.
        allowed_hosts: vec!["api.hubapi.com".into()],
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: "https://app.hubspot.com/oauth/authorize".parse().expect("a valid URL"),
            token_url: format!("https://api.hubapi.com/oauth/{API_VERSION}/token")
                .parse()
                .expect("a valid URL"),
            default_scopes: vec![OAUTH.into()],
            scope_separator: " ".into(),
            // HubSpot documents no code challenge for its authorise page.
            pkce: false,
            client_auth: ClientAuth::Body,
            extra_authorize_params: Vec::new(),
        }),
    }
}

/// What a 429 says when HubSpot names neither of an account's general limits.
/// A search that is answered so has met the limit of search itself.
pub(crate) fn unnamed_limit(provider: &ProviderId) -> String {
    format!("{provider} is rate limiting requests")
}

/// The scopes HubSpot says a refused call needed, as far as they look like scopes.
fn required_scopes(body: &Value) -> Vec<&str> {
    let is_scope = |scope: &&str| {
        (1..=80).contains(&scope.len())
            && scope
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    };
    let details = body["errors"].as_array().into_iter().flatten();
    let mut scopes: Vec<&str> = Vec::new();
    for context in details.map(|detail| &detail["context"]).chain([&body["context"]]) {
        for list in ["requiredScopes", "requiredGranularScopes"] {
            let named = context[list].as_array().into_iter().flatten().filter_map(Value::as_str);
            for scope in named.filter(is_scope) {
                if !scopes.contains(&scope) && scopes.len() < 12 {
                    scopes.push(scope);
                }
            }
        }
    }
    scopes
}

/// HubSpot follows HTTP conventions, with a few additions: a 429 names which
/// of the account's limits was met, a refusal for a missing scope names the
/// scopes that would do, and two statuses of its own say how long to wait.
#[derive(Debug, Clone, Copy, Default)]
pub struct HubSpotClassifier;

impl Classifier for HubSpotClassifier {
    fn classify(&self, provider: &ProviderId, response: &RawResponse) -> Result<()> {
        let error = |kind, message: String| Error::new(kind, message).with_provider(provider.clone());
        match response.status {
            429 => {
                // The wait, when HubSpot states one, is read as for any provider.
                let retry = StandardClassifier
                    .classify(provider, response)
                    .err()
                    .map_or(Retry::Later, |limited| limited.retry());
                let message = match response.body["policyName"].as_str() {
                    Some("DAILY") => format!(
                        "the account's daily limit of requests to {provider} is used up; it starts again at midnight in the account's time zone"
                    ),
                    Some("TEN_SECONDLY_ROLLING") => format!(
                        "{provider} allows this app only so many requests in ten seconds, and they are used up; wait a few seconds and try again"
                    ),
                    _ => unnamed_limit(provider),
                };
                Err(error(ErrorKind::RateLimited, message).with_retry(retry))
            }
            403 if response.body["category"] == "MISSING_SCOPES" => {
                // Only the scopes are repeated, and only when they look like scopes.
                let scopes = required_scopes(&response.body);
                let which = if scopes.is_empty() {
                    String::new()
                } else {
                    format!("; any one of these grants it: {}", scopes.join(", "))
                };
                Err(error(
                    ErrorKind::AccessDenied,
                    format!(
                        "{provider} denied the request: the connection has not been granted a scope this call needs{which}"
                    ),
                ))
            }
            // HubSpot locks what it is sent a great deal of in a short time,
            // such as thousands of records to change. The lock lasts two seconds.
            423 => Err(error(
                ErrorKind::Unexpected,
                format!(
                    "{provider} has locked this for two seconds because a large amount was sent in a short time; try again then"
                ),
            )
            .with_retry(Retry::After(Duration::from_secs(2)))),
            // HubSpot's own status for an account that is being moved between
            // its data centres. It states the wait in seconds.
            477 => {
                let wait = response
                    .header("retry-after")
                    .and_then(|value| value.trim().parse().ok());
                Err(error(
                    ErrorKind::Unexpected,
                    format!(
                        "{provider} is moving this account to another data centre and cannot be called until it is done"
                    ),
                )
                .with_retry(wait.map_or(Retry::Later, |secs| Retry::After(Duration::from_secs(secs)))))
            }
            _ => StandardClassifier.classify(provider, response),
        }
    }
}

/// OAuth settings for HubSpot. A plain [`OAuthClient`] converts into this with
/// the defaults, so `HubSpot::with_oauth(client)` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct HubSpotOAuth {
    /// The application's own HubSpot app.
    pub client: OAuthClient,
    /// The scopes the account has to grant, in place of the default, such as
    /// `crm.objects.contacts.read`. They have to be the app's own required
    /// scopes. `oauth` is always asked for as well: HubSpot requires it.
    pub scopes: Option<Vec<String>>,
    /// Scopes to ask for where the account's plan has them, such as
    /// `crm.objects.custom.read`. HubSpot refuses the whole authorisation
    /// when a scope in `scopes` is one the account lacks, and quietly leaves
    /// out one that is listed here.
    pub optional_scopes: Option<Vec<String>>,
}

impl From<OAuthClient> for HubSpotOAuth {
    fn from(client: OAuthClient) -> Self {
        Self {
            client,
            scopes: None,
            optional_scopes: None,
        }
    }
}

/// A HubSpot access token: a private app's, or one the application obtained
/// itself. A plain string converts into this, so `HubSpot::with_token("…")`
/// works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct HubSpotToken {
    pub token: SecretString,
}

impl From<String> for HubSpotToken {
    fn from(token: String) -> Self {
        Self {
            token: SecretString::new(token),
        }
    }
}

impl From<&str> for HubSpotToken {
    fn from(token: &str) -> Self {
        token.to_owned().into()
    }
}

/// The HubSpot integration.
#[derive(Debug, Clone)]
pub struct HubSpot {
    spec: ProviderSpec,
    access: Access,
}

impl Default for HubSpot {
    fn default() -> Self {
        Self::new()
    }
}

impl HubSpot {
    /// HubSpot with no connection details of its own: the OAuth app is set on the
    /// `Socket` builder and tokens come from the application's token store.
    pub fn new() -> Self {
        Self::with_spec(provider())
    }

    /// HubSpot with the application's own app, for connecting accounts through OAuth.
    /// Takes an [`OAuthClient`], or a [`HubSpotOAuth`] for the settings only HubSpot has.
    pub fn with_oauth(settings: impl Into<HubSpotOAuth>) -> Self {
        let settings = settings.into();
        let mut this = Self::new();
        if let AuthScheme::OAuth2(oauth) = &mut this.spec.auth {
            if let Some(scopes) = settings.scopes {
                oauth.default_scopes = scopes;
            }
            let optional: Vec<String> = settings
                .optional_scopes
                .unwrap_or_default()
                .into_iter()
                .filter(|scope| !scope.trim().is_empty())
                .collect();
            if !optional.is_empty() {
                oauth
                    .extra_authorize_params
                    .push(("optional_scope".into(), optional.join(" ")));
            }
        }
        this.oauth(settings.client)
    }

    /// HubSpot with a token the application already holds, such as a private
    /// app's access token. Every call uses it.
    /// Takes a string, or a [`HubSpotToken`] for the settings only HubSpot has.
    pub fn with_token(settings: impl Into<HubSpotToken>) -> Self {
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
        }
    }

    /// Sets the application's own HubSpot app.
    pub fn oauth(mut self, client: OAuthClient) -> Self {
        self.access.oauth = Some(client);
        self
    }

    /// Sets a token the application already holds.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.access.token = Some(TokenSet::bearer(token));
        self
    }

    /// Records of any object type: contacts, companies, deals, tickets, the
    /// activities logged on them, and custom objects.
    pub fn objects<'a>(&self, connection: &'a Connection) -> Objects<'a> {
        Objects(client::Api { connection })
    }

    /// The links between records.
    pub fn associations<'a>(&self, connection: &'a Connection) -> Associations<'a> {
        Associations(client::Api { connection })
    }

    /// The fields of an object type, their types and their options.
    pub fn properties<'a>(&self, connection: &'a Connection) -> Properties<'a> {
        Properties(client::Api { connection })
    }

    /// The pipelines of deals and tickets, with their stages.
    pub fn pipelines<'a>(&self, connection: &'a Connection) -> Pipelines<'a> {
        Pipelines(client::Api { connection })
    }

    /// The people and queues a record can be assigned to.
    pub fn owners<'a>(&self, connection: &'a Connection) -> Owners<'a> {
        Owners(client::Api { connection })
    }

    fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.spec.id.clone())
    }

    /// The HubSpot account the connection is authorised for. Needs the `oauth` scope.
    ///
    /// The id is the account's number, which HubSpot also calls the portal
    /// id or the hub id. A connection is to an account and not to a person,
    /// so there is no email.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let request = RawRequest::get(format!("account-info/{API_VERSION}/details"));
        let body = connection.request(request).await?.body;
        let id = match &body["portalId"] {
            Value::Number(id) if id.is_u64() => id.to_string(),
            Value::String(id) if !id.trim().is_empty() => id.trim().to_owned(),
            _ => return Err(self.error(ErrorKind::Decode, "hubspot answered without an account")),
        };
        let name = body["portalName"]
            .as_str()
            .filter(|name| !name.trim().is_empty())
            .map_or_else(|| id.clone(), str::to_owned);
        Ok(Account { id, name, email: None })
    }

    /// Confirms an object type exists and the connection can read its records.
    ///
    /// Accepts the name of an object type (`contacts`, `deals`) or its type
    /// id (`0-1`, or `2-12345` for a custom object), and asks HubSpot for one
    /// record of it. The resource's id is the type as it was given, which is
    /// what every `objects` method takes.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        let object_type = input.trim();
        let is_type = (1..=100).contains(&object_type.len())
            && object_type
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
        if !is_type {
            // The input is not repeated: it is the caller's own text.
            return Err(self.error(
                ErrorKind::InvalidInput,
                "that is not a HubSpot object type; give its name, such as contacts or deals, or its type id, such as 2-12345",
            ));
        }
        let request = RawRequest::get(format!("crm/objects/{API_VERSION}/{object_type}")).with_query("limit", "1");
        let body = match connection.request(request).await {
            Ok(response) => response.body,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Err(self.error(ErrorKind::NotFound, "this HubSpot account has no such object type"));
            }
            // HubSpot's reason is kept, and the scope a read needs is named beside it.
            Err(e) if e.kind() == ErrorKind::AccessDenied => {
                return Err(self.error(
                    ErrorKind::AccessDenied,
                    format!(
                        "{}; reading an object type needs its read scope, such as crm.objects.contacts.read",
                        e.message().trim_end_matches('.')
                    ),
                ));
            }
            Err(e) => return Err(e),
        };
        if !body["results"].is_array() {
            return Err(self.error(ErrorKind::Decode, "hubspot answered without the object type's records"));
        }
        Ok(Resource::new(object_type, object_type, "HubSpot object type"))
    }
}

#[async_trait]
impl Integration for HubSpot {
    fn provider(&self) -> ProviderSpec {
        self.spec.clone()
    }

    /// HubSpot's operations are named `hubspot.…`, so the definition must keep that id.
    fn check(&self) -> Result<()> {
        if self.spec.id.as_str() == PROVIDER_ID {
            return Ok(());
        }
        Err(self.error(
            ErrorKind::Config,
            format!(
                "the HubSpot integration needs the provider id {PROVIDER_ID:?}, not {:?}",
                self.spec.id.as_str()
            ),
        ))
    }

    fn oauth_client(&self) -> Option<OAuthClient> {
        self.access.oauth.clone()
    }

    fn fixed_token(&self) -> Option<TokenSet> {
        self.access.token.clone()
    }

    fn operations(&self) -> Vec<OperationInfo> {
        // Identity says which scope it needs, so that what to ask for at
        // sign-in can be read from the catalogue. The scope a lookup needs
        // depends on the object type that is looked up.
        let mut operations = vec![
            OperationInfo {
                required_scopes: vec![OAUTH.to_owned()],
                ..identity_operation(&self.spec.id)
            },
            resolve_operation(
                &self.spec.id,
                "an object type: contacts, companies, deals, tickets, notes, calls, meetings, emails or tasks, or a custom object's type id such as 2-12345. Needs the read scope of that type, such as crm.objects.contacts.read",
            ),
        ];
        operations.extend(operations::all().iter().map(|operation| operation.info.clone()));
        operations
    }

    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value> {
        let id = &self.spec.id;
        match operation.strip_prefix(&format!("{id}.")) {
            Some("identity.get") => to_output(id, &self.identity(&connection).await?),
            Some("resource.resolve") => to_output(id, &self.resolve(&connection, &resolve_input(id, &input)?).await?),
            _ => match operations::all().iter().find(|known| known.info.name == operation) {
                Some(known) => known.run(self.clone(), connection, input).await,
                None => Err(self.error(
                    ErrorKind::Unsupported,
                    format!("hubspot has no operation {operation:?}"),
                )),
            },
        }
    }

    fn classifier(&self) -> Arc<dyn Classifier> {
        Arc::new(HubSpotClassifier)
    }

    fn oauth_flow(&self) -> Arc<dyn OAuthFlow> {
        Arc::new(self.clone())
    }
}

/// HubSpot follows the standard flow. Two things are added: the `oauth`
/// scope is always asked for, and the scopes that were granted are read from
/// where HubSpot writes them.
///
/// A refresh needs nothing of its own. HubSpot's refresh token does not
/// change, and the standard flow keeps the stored one when none comes back.
impl OAuthFlow for HubSpot {
    fn authorization_url(&self, context: OAuthContext, mut request: AuthorizationRequest) -> Result<Url> {
        if !request.scopes.iter().any(|scope| scope == OAUTH) {
            request.scopes.push(OAUTH.into());
        }
        StandardOAuth.authorization_url(context, request)
    }

    /// HubSpot lists the granted scopes under `scopes`, where the standard
    /// has one string under `scope`. With optional scopes, that list is the
    /// only way to know which of them the account granted.
    fn parse_token_response(&self, provider: ProviderId, raw: Value, now: SystemTime) -> Result<TokenSet> {
        let mut tokens = standard_token_response(&provider, &raw, now)?;
        if tokens.scopes.is_empty() {
            let granted = raw["scopes"].as_array().into_iter().flatten();
            tokens.scopes = granted.filter_map(Value::as_str).map(str::to_owned).collect();
        }
        Ok(tokens)
    }
}
