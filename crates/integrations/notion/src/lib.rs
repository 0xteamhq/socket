//! Socket integration for Notion.
//!
//! Offers the provider definition, `notion.identity.get` and
//! `notion.resource.resolve` (a page or database).

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Access, Account, AuthScheme, ClientAuth, Connection, Error, ErrorKind, Integration, OAuth2Spec, OAuthClient,
    OperationInfo, ProviderId, ProviderSpec, RawRequest, Resource, Result, SecretString, TokenSet, identity_operation,
    resolve_input, resolve_operation, to_output,
};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "notion";

/// The Notion API version every request declares.
pub const NOTION_VERSION: &str = "2022-06-28";

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

/// Reads a page or database URL, or a bare id with or without dashes, and
/// returns the dashed UUID the API expects.
pub fn parse_id(input: &str) -> Result<String> {
    let trimmed = input.trim();
    let path = trimmed.split(['?', '#']).next().unwrap_or_default();
    let segment = path.trim_end_matches('/').rsplit('/').next().unwrap_or_default();
    let compact: String = segment.chars().filter(|c| *c != '-').collect();
    // A URL's last segment is `Title-<32 hex>`; the id is its tail. Slicing by
    // characters keeps a title with non-ASCII letters from splitting a character.
    let tail: String = {
        let chars: Vec<char> = compact.chars().collect();
        chars[chars.len().saturating_sub(32)..].iter().collect()
    };
    if tail.len() == 32 && tail.chars().all(|c| c.is_ascii_hexdigit()) {
        let hex = tail.to_ascii_lowercase();
        Ok(format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        ))
    } else {
        Err(Error::new(
            ErrorKind::InvalidInput,
            format!("\"{trimmed}\" is not a Notion page or database; paste its URL or id"),
        ))
    }
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

/// OAuth settings for Notion. A plain [`OAuthClient`] converts into this with
/// the defaults, so `Notion::with_oauth(client)` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct NotionOAuth {
    /// The application's own OAuth app.
    pub client: OAuthClient,
    /// Scopes to ask for in place of the defaults.
    pub scopes: Option<Vec<String>>,
}

impl From<OAuthClient> for NotionOAuth {
    fn from(client: OAuthClient) -> Self {
        Self { client, scopes: None }
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
        let settings = settings.into();
        let mut this = Self::new();
        if let AuthScheme::OAuth2(oauth) = &mut this.spec.auth {
            if let Some(scopes) = settings.scopes {
                oauth.default_scopes = scopes;
            }
        }
        this.oauth(settings.client)
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

    /// Sets the Notion API version to declare.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Sets a token the application already holds.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.access.token = Some(TokenSet::bearer(token));
        self
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
        if let Some(page) = self.fetch(connection, "pages", &id).await? {
            return Ok(Resource::new(id, page_title(&page), "Notion page"));
        }
        if let Some(database) = self.fetch(connection, "databases", &id).await? {
            return Ok(Resource::new(id, plain_text(&database["title"]), "Notion database"));
        }
        Err(Error::new(
            ErrorKind::NotFound,
            format!("Notion page or database {id} was not found"),
        )
        .with_provider(self.spec.id.clone()))
    }

    /// `None` when Notion says the id is not an object of this kind: it
    /// answers 404, or 400 when the id belongs to the other kind.
    async fn fetch(&self, connection: &Connection, kind: &str, id: &str) -> Result<Option<Value>> {
        let body = match connection.request(self.request(format!("{kind}/{id}"))).await {
            Ok(response) => response.body,
            Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::InvalidInput) => return Ok(None),
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
        vec![
            identity_operation(&self.spec.id),
            resolve_operation(&self.spec.id, "a page or database URL, or its id"),
        ]
    }

    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value> {
        let id = &self.spec.id;
        match operation.strip_prefix(&format!("{id}.")) {
            Some("identity.get") => to_output(id, &self.identity(&connection).await?),
            Some("resource.resolve") => to_output(id, &self.resolve(&connection, &resolve_input(id, &input)?).await?),
            _ => Err(
                Error::new(ErrorKind::Unsupported, format!("notion has no operation {operation:?}"))
                    .with_provider(id.clone()),
            ),
        }
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
