//! Socket integration for Attio.
//!
//! Offers the provider definition, identity, lookup of an object, and typed
//! methods for Attio's REST API grouped by area: the schema a workspace
//! defines (`objects`, `attributes`), its data (`records`, `lists`,
//! `entries`), what people write about it (`notes`, `tasks`, `threads`), who
//! is in the workspace (`workspace_members`), and its meetings with their
//! recordings and transcripts (`meetings`, `call_recordings`). Every typed
//! method is also a named operation. See `docs/integrations/attio.md`.

mod client;
pub mod models;
mod operations;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Access, Account, AuthScheme, Classifier, ClientAuth, Connection, Error, ErrorKind, Integration, OAuth2Spec,
    OAuthClient, OperationInfo, ProviderId, ProviderSpec, RawRequest, RawResponse, Resource, Result, Retry,
    SecretString, StandardClassifier, TokenSet, identity_operation, provider_message, resolve_input, resolve_operation,
    to_output,
};

pub use client::{
    Attributes, CallRecordings, Entries, Lists, Meetings, Notes, Objects, Records, Tasks, Threads, WorkspaceMembers,
};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "attio";

/// Attio's definition: where its API lives and how it authenticates.
///
/// No scopes are asked for at sign-in. Attio takes none there: what an
/// application may do is set on the application itself, in Attio's developer
/// console, and a workspace approves that when it installs the application.
///
/// # Panics
/// Never in practice: the URLs are constants that parse.
pub fn provider() -> ProviderSpec {
    ProviderSpec {
        id: ProviderId::new(PROVIDER_ID).expect("a valid provider id"),
        display_name: "Attio".into(),
        api_base: "https://api.attio.com/v2/".parse().expect("a valid URL"),
        // app.attio.com serves the token endpoint.
        allowed_hosts: vec!["api.attio.com".into(), "app.attio.com".into()],
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: "https://app.attio.com/authorize".parse().expect("a valid URL"),
            token_url: "https://app.attio.com/oauth/token".parse().expect("a valid URL"),
            default_scopes: Vec::new(),
            scope_separator: " ".into(),
            // Attio documents PKCE, and requires it for a token that acts as one person.
            pkce: true,
            client_auth: ClientAuth::Body,
            extra_authorize_params: Vec::new(),
        }),
    }
}

/// Attio follows HTTP conventions, with two statuses of its own to read: a
/// write that met another write to the same record, and content that is too large.
#[derive(Debug, Clone, Copy, Default)]
pub struct AttioClassifier;

impl Classifier for AttioClassifier {
    fn classify(&self, provider: &ProviderId, response: &RawResponse) -> Result<()> {
        let error = |kind, message: String| Error::new(kind, message).with_provider(provider.clone());
        match (response.status, response.body["code"].as_str()) {
            // Attio refused the write before making it, and says to try again.
            // Nothing is wrong with what was sent.
            (409, Some("concurrent_write_conflict")) => Err(error(
                ErrorKind::Unexpected,
                format!("{provider} was changing the same record for another request; nothing was written, try again"),
            )
            .with_retry(Retry::Later)),
            // A note or a comment longer than Attio takes.
            (413, _) => Err(error(
                ErrorKind::InvalidInput,
                format!(
                    "{provider} rejected the request as too large: {}",
                    provider_message(&response.body)
                ),
            )),
            _ => StandardClassifier.classify(provider, response),
        }
    }
}

/// Whose permissions a token acts with (Attio's `token_level` parameter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenLevel {
    /// The workspace's: everything the application's scopes allow.
    Workspace,
    /// The person's who connected: the application's scopes, within what
    /// that person may see and change.
    User,
}

impl TokenLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::User => "user",
        }
    }
}

/// OAuth settings for Attio. A plain [`OAuthClient`] converts into this with
/// the defaults, so `Attio::with_oauth(client)` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct AttioOAuth {
    /// The application's own OAuth app, from Attio's developer console.
    pub client: OAuthClient,
    /// Whose permissions the token acts with. Attio issues a workspace token
    /// when this is not set.
    pub token_level: Option<TokenLevel>,
}

impl From<OAuthClient> for AttioOAuth {
    fn from(client: OAuthClient) -> Self {
        Self {
            client,
            token_level: None,
        }
    }
}

/// An Attio access token: a workspace's API key, or a token an application
/// already holds. A plain string converts into this, so
/// `Attio::with_token("…")` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct AttioToken {
    pub token: SecretString,
}

impl From<String> for AttioToken {
    fn from(token: String) -> Self {
        Self {
            token: SecretString::new(token),
        }
    }
}

impl From<&str> for AttioToken {
    fn from(token: &str) -> Self {
        token.to_owned().into()
    }
}

/// The Attio integration.
#[derive(Debug, Clone)]
pub struct Attio {
    spec: ProviderSpec,
    access: Access,
}

impl Default for Attio {
    fn default() -> Self {
        Self::new()
    }
}

impl Attio {
    /// Attio with no connection details of its own: the OAuth app is set on the
    /// `Socket` builder and tokens come from the application's token store.
    pub fn new() -> Self {
        Self::with_spec(provider())
    }

    /// Attio with the application's OAuth app, for connecting workspaces through OAuth.
    /// Takes an [`OAuthClient`], or an [`AttioOAuth`] for the settings only Attio has.
    pub fn with_oauth(settings: impl Into<AttioOAuth>) -> Self {
        let settings = settings.into();
        let mut this = Self::new();
        if let (AuthScheme::OAuth2(oauth), Some(level)) = (&mut this.spec.auth, settings.token_level) {
            oauth
                .extra_authorize_params
                .push(("token_level".into(), level.as_str().into()));
        }
        this.oauth(settings.client)
    }

    /// Attio with a token the application already holds, such as a
    /// workspace's API key. Every call uses it.
    /// Takes a string, or an [`AttioToken`] for the settings only Attio has.
    pub fn with_token(settings: impl Into<AttioToken>) -> Self {
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

    /// The objects a workspace keeps: people, companies, deals and its own.
    pub fn objects<'a>(&self, connection: &'a Connection) -> Objects<'a> {
        Objects(client::Api { connection })
    }

    /// The attributes of an object or a list, with their options and statuses.
    pub fn attributes<'a>(&self, connection: &'a Connection) -> Attributes<'a> {
        Attributes(client::Api { connection })
    }

    /// The records of an object: one person, one company, one deal.
    pub fn records<'a>(&self, connection: &'a Connection) -> Records<'a> {
        Records(client::Api { connection })
    }

    /// The lists of a workspace, each a process its records move through.
    pub fn lists<'a>(&self, connection: &'a Connection) -> Lists<'a> {
        Lists(client::Api { connection })
    }

    /// The entries of a list: a record's place in it, with the list's own values.
    pub fn entries<'a>(&self, connection: &'a Connection) -> Entries<'a> {
        Entries(client::Api { connection })
    }

    /// The notes written on records.
    pub fn notes<'a>(&self, connection: &'a Connection) -> Notes<'a> {
        Notes(client::Api { connection })
    }

    /// Tasks, with the records they are about and the people they are assigned to.
    pub fn tasks<'a>(&self, connection: &'a Connection) -> Tasks<'a> {
        Tasks(client::Api { connection })
    }

    /// The comment threads on a record or a list entry.
    pub fn threads<'a>(&self, connection: &'a Connection) -> Threads<'a> {
        Threads(client::Api { connection })
    }

    /// The people who have access to the workspace.
    pub fn workspace_members<'a>(&self, connection: &'a Connection) -> WorkspaceMembers<'a> {
        WorkspaceMembers(client::Api { connection })
    }

    /// The meetings Attio knows of, from calendars and integrations.
    pub fn meetings<'a>(&self, connection: &'a Connection) -> Meetings<'a> {
        Meetings(client::Api { connection })
    }

    /// The recordings of a meeting, with what was said.
    pub fn call_recordings<'a>(&self, connection: &'a Connection) -> CallRecordings<'a> {
        CallRecordings(client::Api { connection })
    }

    fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.spec.id.clone())
    }

    /// The workspace the connection is authorised for. Needs no scope.
    ///
    /// A token is issued for a workspace, so the workspace is the account:
    /// its id and its name. That holds for a token that acts as one member
    /// too, so two members of one workspace are the same account here.
    /// Attio answers 200 for a token it no longer accepts and says so in the
    /// body; that is reported as a connection to renew.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let body = connection.request(RawRequest::get("self")).await?.body;
        if body["active"] == false {
            return Err(self.error(
                ErrorKind::ReconnectRequired,
                "attio no longer accepts the stored authorization",
            ));
        }
        let filled = |value: &Value| value.as_str().filter(|s| !s.trim().is_empty()).map(str::to_owned);
        let Some(id) = filled(&body["workspace_id"]) else {
            return Err(self.error(ErrorKind::Decode, "attio answered without a workspace"));
        };
        let name = filled(&body["workspace_name"])
            .or_else(|| filled(&body["workspace_slug"]))
            .unwrap_or_else(|| id.clone());
        Ok(Account { id, name, email: None })
    }

    /// Confirms an object exists and the connection can read it. `input` is
    /// the object's slug, such as `people`, `companies` or `deals`, or its id.
    ///
    /// The resource's id is the slug, which is what every other method takes.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        let object = match self.objects(connection).get(input.trim()).await {
            Ok(object) => object,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Err(self.error(ErrorKind::NotFound, "that Attio object was not found"));
            }
            Err(e) => return Err(e),
        };
        let filled = |text: &Option<String>| text.clone().filter(|s| !s.trim().is_empty());
        let id = filled(&object.api_slug).unwrap_or_else(|| object.id.object_id.clone());
        let label = filled(&object.plural_noun)
            .or_else(|| filled(&object.singular_noun))
            .unwrap_or_else(|| id.clone());
        Ok(Resource::new(id, label, "Attio object"))
    }
}

#[async_trait]
impl Integration for Attio {
    fn provider(&self) -> ProviderSpec {
        self.spec.clone()
    }

    /// Attio's operations are named `attio.…`, so the definition must keep that id.
    fn check(&self) -> Result<()> {
        if self.spec.id.as_str() == PROVIDER_ID {
            return Ok(());
        }
        Err(self.error(
            ErrorKind::Config,
            format!(
                "the Attio integration needs the provider id {PROVIDER_ID:?}, not {:?}",
                self.spec.id.as_str()
            ),
        ))
    }

    fn classifier(&self) -> Arc<dyn Classifier> {
        Arc::new(AttioClassifier)
    }

    fn oauth_client(&self) -> Option<OAuthClient> {
        self.access.oauth.clone()
    }

    fn fixed_token(&self) -> Option<TokenSet> {
        self.access.token.clone()
    }

    fn operations(&self) -> Vec<OperationInfo> {
        // Lookup says which scope it needs, so that what to grant the
        // application can be read from the catalogue. Identity needs none.
        let lookup = OperationInfo {
            required_scopes: vec!["object_configuration:read".to_owned()],
            ..resolve_operation(
                &self.spec.id,
                "an object's slug, such as people or companies, or its id",
            )
        };
        let mut operations = vec![identity_operation(&self.spec.id), lookup];
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
                None => Err(self.error(ErrorKind::Unsupported, format!("attio has no operation {operation:?}"))),
            },
        }
    }
}
