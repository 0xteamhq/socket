//! Socket integration for Notion.
//!
//! Offers the provider definition, identity, lookup of a page or a database,
//! and typed methods for Notion's API grouped by area: `search`, `pages`,
//! `blocks`, `databases`, `users` and `comments`. Every typed method is also
//! a named operation. See `docs/integrations/notion.md`.

mod client;
pub mod models;
mod operations;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Access, Account, AuthScheme, Classifier, ClientAuth, Connection, Error, ErrorKind, Integration, OAuth2Spec,
    OAuthClient, OperationInfo, ProviderId, ProviderSpec, RawRequest, RawResponse, Resource, Result, Retry,
    SecretString, StandardClassifier, TokenSet, identity_operation, resolve_input, resolve_operation, to_output,
};

pub use client::{Blocks, Comments, Databases, Pages, Search, Users};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "notion";

/// The Notion API version every request declares, and the one the typed
/// methods are written for: a database's rows belong to its data sources,
/// what is trashed is marked `in_trash`, and new blocks are placed with
/// `position`.
pub const NOTION_VERSION: &str = "2026-03-11";

/// Notion's definition: where its API lives and how it authenticates.
///
/// # Panics
/// Never in practice: the URLs are constants that parse.
pub fn provider() -> ProviderSpec {
    ProviderSpec {
        id: ProviderId::new(PROVIDER_ID).expect("a valid provider id"),
        display_name: "Notion".into(),
        api_base: "https://api.notion.com/v1/".parse().expect("a valid URL"),
        allowed_hosts: vec!["api.notion.com".into()],
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: "https://api.notion.com/v1/oauth/authorize"
                .parse()
                .expect("a valid URL"),
            token_url: "https://api.notion.com/v1/oauth/token".parse().expect("a valid URL"),
            // Notion has no scopes: the person picks pages when they approve.
            default_scopes: Vec::new(),
            scope_separator: " ".into(),
            pkce: false,
            client_auth: ClientAuth::Basic,
            extra_authorize_params: vec![("owner".into(), "user".into())],
        }),
    }
}

/// Notion follows HTTP conventions, with two additions: what is not found
/// may only be unshared, and an overloaded service says when to come back.
#[derive(Debug, Clone, Copy, Default)]
pub struct NotionClassifier;

impl Classifier for NotionClassifier {
    fn classify(&self, provider: &ProviderId, response: &RawResponse) -> Result<()> {
        let error = |kind, message: String| Error::new(kind, message).with_provider(provider.clone());
        match response.status {
            // Notion gives the same answer for what does not exist and for
            // what exists but was not shared, so a caller is told of both.
            404 => Err(error(
                ErrorKind::NotFound,
                format!("{provider} has nothing with that id, or it was not shared with this integration"),
            )),
            // Notion asks for an overloaded service to be treated as a
            // rate limit, and states the wait in whole seconds.
            529 => {
                let wait = response
                    .header("retry-after")
                    .and_then(|value| value.trim().parse().ok());
                let retry = wait.map_or(Retry::Later, |seconds| Retry::After(Duration::from_secs(seconds)));
                Err(error(ErrorKind::RateLimited, format!("{provider} is overloaded")).with_retry(retry))
            }
            _ => StandardClassifier.classify(provider, response),
        }
    }
}

/// Reads an id with or without dashes, or the address of a page or a
/// database, and returns the id with dashes. `None` when there is no id in it.
pub(crate) fn dashed(input: &str) -> Option<String> {
    let path = input.trim().split(['?', '#']).next().unwrap_or_default();
    let segment = path.trim_end_matches('/').rsplit('/').next().unwrap_or_default();
    let compact: String = segment.chars().filter(|c| *c != '-').collect();
    // A URL's last segment is `Title-<32 hex>`; the id is its tail. Slicing by
    // characters keeps a title with non-ASCII letters from splitting a character.
    let tail: String = {
        let chars: Vec<char> = compact.chars().collect();
        chars[chars.len().saturating_sub(32)..].iter().collect()
    };
    if tail.len() != 32 || !tail.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let hex = tail.to_ascii_lowercase();
    Some(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

/// Reads a page or database URL, or a bare id with or without dashes, and
/// returns the dashed UUID the API expects.
pub fn parse_id(input: &str) -> Result<String> {
    dashed(input).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!(
                "\"{}\" is not a Notion page or database; paste its URL or id",
                input.trim()
            ),
        )
    })
}

fn page_title(page: &Value) -> String {
    page["properties"]
        .as_object()
        .and_then(|props| props.values().find(|p| p["type"] == "title"))
        .map_or_else(|| "Untitled".into(), |prop| plain_text(&prop["title"]))
}

fn plain_text(rich_text: &Value) -> String {
    let text: String = rich_text
        .as_array()
        .map(|parts| parts.iter().filter_map(|p| p["plain_text"].as_str()).collect())
        .unwrap_or_default();
    if text.is_empty() { "Untitled".into() } else { text }
}

/// OAuth settings for Notion. A plain [`OAuthClient`] converts into this, so
/// `Notion::with_oauth(client)` works.
///
/// There are no scopes to set: Notion has none. The person chooses which
/// pages to share when they approve.
#[derive(Debug, Clone)]
pub struct NotionOAuth {
    /// The application's own OAuth app.
    pub client: OAuthClient,
}

impl From<OAuthClient> for NotionOAuth {
    fn from(client: OAuthClient) -> Self {
        Self { client }
    }
}

/// A Notion internal integration secret or access token. A plain string converts into this, so
/// `Notion::with_token("…")` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct NotionToken {
    pub token: SecretString,
    /// The Notion API version to declare, in place of [`NOTION_VERSION`].
    pub version: Option<String>,
}

impl From<String> for NotionToken {
    fn from(token: String) -> Self {
        Self {
            token: SecretString::new(token),
            version: None,
        }
    }
}

impl From<&str> for NotionToken {
    fn from(token: &str) -> Self {
        token.to_owned().into()
    }
}

/// The Notion integration.
#[derive(Debug, Clone)]
pub struct Notion {
    spec: ProviderSpec,
    access: Access,
    version: String,
}

impl Default for Notion {
    fn default() -> Self {
        Self::new()
    }
}

impl Notion {
    /// Notion with no connection details of its own: the OAuth app is set on the
    /// `Socket` builder and tokens come from the application's token store.
    pub fn new() -> Self {
        Self::with_spec(provider())
    }

    /// Notion with the application's OAuth app, for connecting users through OAuth.
    /// Takes an [`OAuthClient`], or a [`NotionOAuth`] for the settings only Notion has.
    pub fn with_oauth(settings: impl Into<NotionOAuth>) -> Self {
        Self::new().oauth(settings.into().client)
    }

    /// Notion with a token the application already holds. Every call uses it.
    /// Takes a string, or a [`NotionToken`] for the settings only Notion has.
    pub fn with_token(settings: impl Into<NotionToken>) -> Self {
        let settings = settings.into();
        let mut this = Self::new();
        if let Some(version) = settings.version {
            this.version = version;
        }
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
            version: NOTION_VERSION.to_owned(),
        }
    }

    /// Sets the application's OAuth app.
    pub fn oauth(mut self, client: OAuthClient) -> Self {
        self.access.oauth = Some(client);
        self
    }

    /// Uses another definition while keeping the token, OAuth app and version already set.
    pub fn spec(mut self, spec: ProviderSpec) -> Self {
        self.spec = spec;
        self
    }

    /// Sets the Notion API version to declare, in place of [`NOTION_VERSION`].
    ///
    /// The typed methods are written for [`NOTION_VERSION`]. Under an older
    /// version Notion names some fields differently, and those methods may
    /// be refused or return less.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Sets a token the application already holds.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.access.token = Some(TokenSet::bearer(token));
        self
    }

    fn api<'a>(&'a self, connection: &'a Connection) -> client::Api<'a> {
        client::Api {
            connection,
            version: &self.version,
        }
    }

    /// Search: finding pages and data sources by title.
    pub fn search<'a>(&'a self, connection: &'a Connection) -> Search<'a> {
        Search(self.api(connection))
    }

    /// Pages: their properties and content, and creating, changing and trashing them.
    pub fn pages<'a>(&'a self, connection: &'a Connection) -> Pages<'a> {
        Pages(self.api(connection))
    }

    /// Blocks: the pieces a page's content is made of.
    pub fn blocks<'a>(&'a self, connection: &'a Connection) -> Blocks<'a> {
        Blocks(self.api(connection))
    }

    /// Databases, and the data sources that hold their rows.
    pub fn databases<'a>(&'a self, connection: &'a Connection) -> Databases<'a> {
        Databases(self.api(connection))
    }

    /// Users: the people and integrations of a workspace.
    pub fn users<'a>(&'a self, connection: &'a Connection) -> Users<'a> {
        Users(self.api(connection))
    }

    /// Comments on a page or a block.
    pub fn comments<'a>(&'a self, connection: &'a Connection) -> Comments<'a> {
        Comments(self.api(connection))
    }

    fn request(&self, path: String) -> RawRequest {
        RawRequest::get(path).with_header("Notion-Version", self.version.as_str())
    }

    /// The account the connection is authorised as: the integration's bot
    /// user, shown by the person or workspace that owns it.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let body = connection.request(self.request("users/me".into())).await?.body;
        let Some(id) = body["id"].as_str().filter(|id| !id.is_empty()) else {
            return Err(
                Error::new(ErrorKind::Decode, "notion answered without an account").with_provider(self.spec.id.clone())
            );
        };
        let owner = &body["bot"]["owner"]["user"];
        let name = [&owner["name"], &body["bot"]["workspace_name"], &body["name"]]
            .into_iter()
            .find_map(|v| v.as_str().filter(|s| !s.is_empty()))
            .unwrap_or("Notion integration");
        Ok(Account {
            id: id.to_owned(),
            name: name.to_owned(),
            email: owner["person"]["email"].as_str().map(str::to_owned),
        })
    }

    /// Confirms a page or database exists and was shared with the integration.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        let id = parse_id(input).map_err(|e| e.with_provider(self.spec.id.clone()))?;
        // Notion refuses a database's id asked for as a page with a 400, the
        // status it also gives a request that is wrong in itself. Which of
        // the two a 400 was is known once the database has been asked for.
        let refused = match self.fetch(connection, "pages", &id).await {
            Ok(Some(page)) => return Ok(Resource::new(id, page_title(&page), "Notion page")),
            Ok(None) => None,
            Err(e) if e.kind() == ErrorKind::InvalidInput => Some(e),
            Err(e) => return Err(e),
        };
        if let Some(database) = self.fetch(connection, "databases", &id).await? {
            return Ok(Resource::new(id, plain_text(&database["title"]), "Notion database"));
        }
        // There is no database either, so a 400 on the page was not about
        // the kind of id. It is passed on, and not reported as "not found".
        Err(refused.unwrap_or_else(|| {
            Error::new(
                ErrorKind::NotFound,
                format!("Notion page or database {id} was not found, or it was not shared with this integration"),
            )
            .with_provider(self.spec.id.clone())
        }))
    }

    /// `None` when Notion answers 404: nothing of this kind has the id, or
    /// it was not shared. Any refusal, a 400 included, is an error.
    async fn fetch(&self, connection: &Connection, kind: &str, id: &str) -> Result<Option<Value>> {
        let body = match connection.request(self.request(format!("{kind}/{id}"))).await {
            Ok(response) => response.body,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        // `pages` answers with a page object, `databases` with a database object.
        // Anything else in a 200 is not Notion confirming the id.
        let expected = kind.trim_end_matches('s');
        if body["object"] == expected {
            Ok(Some(body))
        } else {
            Err(
                Error::new(ErrorKind::Decode, format!("notion answered without a {expected}"))
                    .with_provider(self.spec.id.clone()),
            )
        }
    }
}

#[async_trait]
impl Integration for Notion {
    fn provider(&self) -> ProviderSpec {
        self.spec.clone()
    }

    fn oauth_client(&self) -> Option<OAuthClient> {
        self.access.oauth.clone()
    }

    fn fixed_token(&self) -> Option<TokenSet> {
        self.access.token.clone()
    }

    fn operations(&self) -> Vec<OperationInfo> {
        let mut operations = vec![
            identity_operation(&self.spec.id),
            resolve_operation(&self.spec.id, "a page or database URL, or its id"),
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
                None => Err(
                    Error::new(ErrorKind::Unsupported, format!("notion has no operation {operation:?}"))
                        .with_provider(id.clone()),
                ),
            },
        }
    }

    fn classifier(&self) -> Arc<dyn Classifier> {
        Arc::new(NotionClassifier)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const DASHED: &str = "0123abcd-4567-89ab-cdef-0123456789ab";

    #[test]
    fn parse_id_accepts_urls_and_ids_in_either_form() {
        for input in [
            "0123abcd456789abcdef0123456789ab",
            "0123ABCD-4567-89AB-CDEF-0123456789AB",
            "https://www.notion.so/acme/Roadmap-0123abcd456789abcdef0123456789ab",
            "https://www.notion.so/Roadmap-0123abcd456789abcdef0123456789ab?v=abc#block",
            "https://www.notion.so/0123abcd456789abcdef0123456789ab/",
            "https://www.notion.so/Café-plan-0123abcd456789abcdef0123456789ab",
        ] {
            assert_eq!(parse_id(input).unwrap(), DASHED, "{input}");
        }
    }

    #[test]
    fn parse_id_refuses_anything_without_a_full_id() {
        for input in [
            "",
            "roadmap",
            "0123abcd",
            "https://www.notion.so/acme/Roadmap",
            "zzzzabcd456789abcdef0123456789ab",
            "ééééééééééééééééé",
        ] {
            assert_eq!(
                parse_id(input).unwrap_err().kind(),
                ErrorKind::InvalidInput,
                "{input:?}"
            );
        }
    }

    #[test]
    fn titles_are_read_from_rich_text_and_default_to_untitled() {
        let page = json!({ "properties": {
            "Status": { "type": "select" },
            "Name": { "type": "title", "title": [{ "plain_text": "Q3 " }, { "plain_text": "Roadmap" }] }
        }});
        assert_eq!(page_title(&page), "Q3 Roadmap");
        assert_eq!(
            page_title(&json!({ "properties": { "Name": { "type": "title", "title": [] } } })),
            "Untitled"
        );
        assert_eq!(page_title(&json!({})), "Untitled");
        assert_eq!(plain_text(&json!(null)), "Untitled");
    }
}
