//! Socket integration for Attio.
//!
//! Offers the provider definition, identity, lookup of an object or a list,
//! and typed methods for Attio's API grouped as Attio groups them. A
//! workspace defines its own objects, attributes and lists, so the methods
//! are generic: `objects`, `attributes` and `lists` say what a workspace
//! holds, and `records` and `entries` read and write it. Beside them are
//! `notes`, `tasks`, `threads`, `workspace_members`, `meetings`,
//! `call_recordings` and `meta`. Every typed method is also a named
//! operation. See `docs/integrations/attio.md`.

mod client;
pub mod models;
mod operations;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Access, Account, AuthScheme, AuthorizationRequest, Classifier, ClientAuth, Connection, Error, ErrorKind,
    Integration, OAuth2Spec, OAuthClient, OAuthContext, OAuthFlow, OperationInfo, ProviderId, ProviderSpec,
    RawResponse, Resource, Result, Retry, SecretString, StandardClassifier, StandardOAuth, TokenSet,
    identity_operation, provider_message, resolve_input, resolve_operation, to_output,
};
use url::Url;

pub use client::{
    Attributes, CallRecordings, Entries, Lists, Meetings, Meta, Notes, Objects, Records, Tasks, Threads,
    WorkspaceMembers,
};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "attio";

/// Attio's definition: where its API lives and how it authenticates.
///
/// There are no default scopes. What an Attio token may do is set on the app
/// in Attio's developer console, or on the workspace's own token when it is
/// made, and is not asked for at sign-in.
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
        content_hosts: Vec::new(),
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: "https://app.attio.com/authorize".parse().expect("a valid URL"),
            token_url: "https://app.attio.com/oauth/token".parse().expect("a valid URL"),
            default_scopes: Vec::new(),
            scope_separator: " ".into(),
            // Attio requires it for a token that acts as one member, and takes it for any.
            pkce: true,
            client_auth: ClientAuth::Body,
            extra_authorize_params: Vec::new(),
        }),
    }
}

/// Attio follows HTTP conventions and says why in a `message`. Three
/// answers are read more closely than the standard does: what was not
/// found, a write that met another write, and content that was too large.
#[derive(Debug, Clone, Copy, Default)]
pub struct AttioClassifier;

impl Classifier for AttioClassifier {
    fn classify(&self, provider: &ProviderId, response: &RawResponse) -> Result<()> {
        let error = |kind, message: String| Error::new(kind, message).with_provider(provider.clone());
        let said = response.body["message"].is_string();
        match (response.status, response.body["code"].as_str()) {
            // Attio names what was missing: the object, the list or the
            // record. The name is one the caller passed.
            (404, _) if said => Err(error(
                ErrorKind::NotFound,
                format!("{provider} has no such resource: {}", provider_message(&response.body)),
            )),
            // Another request changed the same record while this one was
            // being checked. Attio did not carry this one out, and asks for
            // it to be sent again.
            (409, Some("concurrent_write_conflict")) => Err(error(
                ErrorKind::Unexpected,
                format!("{provider} was changing the same record for another request; try again"),
            )
            .with_retry(Retry::Later)),
            (413, _) => Err(error(
                ErrorKind::InvalidInput,
                format!("{provider} rejected the request as too large"),
            )),
            // A rejected token, a refusal, a request Attio could not accept
            // and throttling, whose wait Attio states as a date.
            _ => StandardClassifier.classify(provider, response),
        }
    }
}

/// Whose permissions a token has (Attio's `token_level` parameter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenLevel {
    /// The token acts as the workspace as a whole. Only an administrator of
    /// the workspace can grant one. Attio's default.
    Workspace,
    /// The token acts as the member who granted it, and reaches only what
    /// that member can.
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
///
/// There is no setting for scopes: an Attio app's scopes are chosen in
/// Attio's developer console, and every token the app is granted carries them.
#[derive(Debug, Clone)]
pub struct AttioOAuth {
    /// The application's own Attio app.
    pub client: OAuthClient,
    /// Whether tokens act as the workspace or as the member who signs in.
    /// As the workspace when not given.
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

/// An Attio access token: a workspace's own API key, or a token an app was
/// granted. A plain string converts into this, so `Attio::with_token("…")`
/// works when nothing else is needed.
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

    /// Attio with the application's Attio app, for connecting workspaces through OAuth.
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
            api_base: None,
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

    /// Sets the application's Attio app.
    pub fn oauth(mut self, client: OAuthClient) -> Self {
        self.access.oauth = Some(client);
        self
    }

    /// Sets a token the application already holds.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.access.token = Some(TokenSet::bearer(token));
        self
    }

    /// What Attio says about the token itself.
    pub fn meta<'a>(&self, connection: &'a Connection) -> Meta<'a> {
        Meta(client::Api { connection })
    }

    /// The kinds of record the workspace keeps.
    pub fn objects<'a>(&self, connection: &'a Connection) -> Objects<'a> {
        Objects(client::Api { connection })
    }

    /// The fields of an object or of a list.
    pub fn attributes<'a>(&self, connection: &'a Connection) -> Attributes<'a> {
        Attributes(client::Api { connection })
    }

    /// The records of an object.
    pub fn records<'a>(&self, connection: &'a Connection) -> Records<'a> {
        Records(client::Api { connection })
    }

    /// The workspace's lists.
    pub fn lists<'a>(&self, connection: &'a Connection) -> Lists<'a> {
        Lists(client::Api { connection })
    }

    /// The entries of a list.
    pub fn entries<'a>(&self, connection: &'a Connection) -> Entries<'a> {
        Entries(client::Api { connection })
    }

    /// Notes on records.
    pub fn notes<'a>(&self, connection: &'a Connection) -> Notes<'a> {
        Notes(client::Api { connection })
    }

    /// Tasks.
    pub fn tasks<'a>(&self, connection: &'a Connection) -> Tasks<'a> {
        Tasks(client::Api { connection })
    }

    /// Threads of comments on a record or on a list entry.
    pub fn threads<'a>(&self, connection: &'a Connection) -> Threads<'a> {
        Threads(client::Api { connection })
    }

    /// The people who work in the workspace.
    pub fn workspace_members<'a>(&self, connection: &'a Connection) -> WorkspaceMembers<'a> {
        WorkspaceMembers(client::Api { connection })
    }

    /// Meetings Attio knows of.
    pub fn meetings<'a>(&self, connection: &'a Connection) -> Meetings<'a> {
        Meetings(client::Api { connection })
    }

    /// Recordings of the calls held in a meeting, and what was said in them.
    pub fn call_recordings<'a>(&self, connection: &'a Connection) -> CallRecordings<'a> {
        CallRecordings(client::Api { connection })
    }

    fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.spec.id.clone())
    }

    /// The workspace the connection is authorised for. Needs no scope.
    ///
    /// An Attio token belongs to a workspace, so the account is the
    /// workspace: its id, and its name. There is no email. What the token
    /// may do is in `meta(&connection).identify()`.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let token = self.meta(connection).identify().await?;
        let filled = |text: Option<String>| text.filter(|text| !text.trim().is_empty());
        let name = filled(token.workspace_name)
            .or_else(|| filled(token.workspace_slug))
            .unwrap_or_else(|| token.workspace_id.clone());
        Ok(Account {
            id: token.workspace_id,
            name,
            email: None,
        })
    }

    /// Confirms an object or a list exists and the token can see it.
    ///
    /// The input is an object's slug or id (`people`), which may be written
    /// `objects/people`, or a list's written `lists/enterprise_sales`. The
    /// resource's id is Attio's own id for it, which every method takes
    /// where it takes a slug and which does not change when the slug does.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        let input = input.trim();
        let (is_list, name) = match input.split_once('/') {
            Some(("lists", name)) => (true, name),
            Some(("objects", name)) => (false, name),
            Some(_) => {
                // What was typed is not repeated: it is refused for what it holds.
                return Err(self.error(
                    ErrorKind::InvalidInput,
                    "that is not an Attio object or list; use an object's slug such as `people`, or `lists/` and a list's slug",
                ));
            }
            None => (false, input),
        };
        let filled = |text: Option<String>| text.filter(|text| !text.trim().is_empty());
        if is_list {
            let list = self.lists(connection).get(name).await?;
            let label = filled(list.name).or_else(|| filled(list.api_slug));
            let label = label.unwrap_or_else(|| list.id.list_id.clone());
            return Ok(Resource::new(list.id.list_id, label, "Attio list"));
        }
        let object = self.objects(connection).get(name).await?;
        let label = filled(object.plural_noun)
            .or_else(|| filled(object.singular_noun))
            .or_else(|| filled(object.api_slug));
        let label = label.unwrap_or_else(|| object.id.object_id.clone());
        Ok(Resource::new(object.id.object_id, label, "Attio object"))
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

    fn oauth_client(&self) -> Option<OAuthClient> {
        self.access.oauth.clone()
    }

    fn fixed_token(&self) -> Option<TokenSet> {
        self.access.token.clone()
    }

    fn operations(&self) -> Vec<OperationInfo> {
        // Identity needs no scope: Attio describes any token to itself. A
        // lookup reads an object or a list, and says which scopes that takes.
        let resolve = OperationInfo {
            required_scopes: vec!["object_configuration:read".into(), "list_configuration:read".into()],
            ..resolve_operation(
                &self.spec.id,
                "an object's slug or id such as `people`, or a list's written `lists/<slug or id>`",
            )
        };
        let mut operations = vec![identity_operation(&self.spec.id), resolve];
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

    fn classifier(&self) -> Arc<dyn Classifier> {
        Arc::new(AttioClassifier)
    }

    fn oauth_flow(&self) -> Arc<dyn OAuthFlow> {
        Arc::new(AttioFlow)
    }
}

/// Attio follows the standard flow, with one difference: scopes are not part
/// of it. An app's scopes are chosen in Attio's developer console, and the
/// sign-in page has no parameter for them, so none is sent, whatever the
/// application passes when it begins an authorisation.
///
/// Attio's tokens do not expire and it issues no refresh token, so the
/// standard refresh, which reports that the person must connect again, is
/// the right answer if a token is ever rejected.
#[derive(Debug, Clone, Copy, Default)]
struct AttioFlow;

#[async_trait]
impl OAuthFlow for AttioFlow {
    fn authorization_url(&self, context: OAuthContext, mut request: AuthorizationRequest) -> Result<Url> {
        request.scopes.clear();
        StandardOAuth.authorization_url(context, request)
    }
}
