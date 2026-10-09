//! Socket integration for Linear.
//!
//! Offers the provider definition, `linear.identity.get` and
//! `linear.resource.resolve` (a team). Linear's API is GraphQL only.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use socketkit_core::{
    Access, Account, ApiKeySpec, AuthScheme, Classifier, ClientAuth, Connection, Error, ErrorKind, Integration,
    KeyPlacement, OAuth2Spec, OAuthClient, OperationInfo, ProviderId, ProviderSpec, RawRequest, RawResponse, Resource,
    Result, Retry, SecretString, StandardClassifier, TokenSet, identity_operation, resolve_input, resolve_operation,
    to_output,
};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "linear";

/// How a Linear personal API key begins.
const API_KEY_PREFIX: &str = "lin_api_";

const VIEWER: &str = "query { viewer { id name email } }";
const TEAM_BY_KEY: &str = "query($key: String!) { teams(filter: { key: { eq: $key } }) { nodes { id key name } } }";

/// Linear's definition: where its API lives and how it authenticates.
///
/// # Panics
/// Never in practice: the URLs are constants that parse.
pub fn provider() -> ProviderSpec {
    ProviderSpec {
        id: ProviderId::new(PROVIDER_ID).expect("a valid provider id"),
        display_name: "Linear".into(),
        api_base: "https://api.linear.app/".parse().expect("a valid URL"),
        allowed_hosts: vec!["api.linear.app".into()],
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: "https://linear.app/oauth/authorize".parse().expect("a valid URL"),
            token_url: "https://api.linear.app/oauth/token".parse().expect("a valid URL"),
            default_scopes: vec!["read".into(), "write".into()],
            scope_separator: ",".into(),
            pkce: false,
            client_auth: ClientAuth::Body,
            extra_authorize_params: Vec::new(),
        }),
    }
}

/// GraphQL reports failures in an `errors` array, often with HTTP 200.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinearClassifier;

impl Classifier for LinearClassifier {
    fn classify(&self, provider: &ProviderId, response: &RawResponse) -> Result<()> {
        // The body is read first: Linear answers a bad token with a 400 whose
        // error code is the only thing that tells it from a bad query.
        let errors = response.body["errors"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let error = |kind, message: String| Error::new(kind, message).with_provider(provider.clone());
        let has = |code: &str| {
            errors
                .iter()
                .any(|e| e["extensions"]["code"] == code || e["extensions"]["type"] == code)
        };
        if has("AUTHENTICATION_ERROR") {
            return Err(error(
                ErrorKind::ReconnectRequired,
                format!("{provider} rejected the stored authorization"),
            ));
        }
        if has("RATELIMITED") {
            return Err(
                error(ErrorKind::RateLimited, format!("{provider} is rate limiting requests")).with_retry(Retry::Later),
            );
        }
        StandardClassifier.classify(provider, response)?;
        let Some(first) = errors.first() else {
            return Ok(());
        };
        // Any other error leaves `data` empty too, and must not read as "nothing matched".
        let message = first["message"].as_str().unwrap_or("unknown error");
        Err(if has("FORBIDDEN") {
            error(
                ErrorKind::AccessDenied,
                format!("{provider} denied the request: {message}"),
            )
        } else if has("GRAPHQL_VALIDATION_FAILED") || has("BAD_USER_INPUT") || has("INVALID_INPUT") {
            error(
                ErrorKind::InvalidInput,
                format!("{provider} rejected the request: {message}"),
            )
        } else {
            error(ErrorKind::Unexpected, format!("{provider} failed: {message}"))
        })
    }
}

/// Reads a Linear team key such as `ENG`; case is normalised.
pub fn parse_team_key(input: &str) -> Result<String> {
    let key = input.trim().to_ascii_uppercase();
    let valid = (1..=10).contains(&key.len()) && key.chars().all(|c| c.is_ascii_alphanumeric());
    if valid {
        Ok(key)
    } else {
        Err(Error::new(
            ErrorKind::InvalidInput,
            format!(
                "\"{}\" is not a Linear team key; use the short key, for example ENG",
                input.trim()
            ),
        ))
    }
}

/// OAuth settings for Linear. A plain [`OAuthClient`] converts into this with
/// the defaults, so `Linear::with_oauth(client)` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct LinearOAuth {
    /// The application's own OAuth app.
    pub client: OAuthClient,
    /// Scopes to ask for in place of the defaults.
    pub scopes: Option<Vec<String>>,
}

impl From<OAuthClient> for LinearOAuth {
    fn from(client: OAuthClient) -> Self {
        Self { client, scopes: None }
    }
}

/// A Linear personal API key or OAuth access token. A plain string converts
/// into this, so `Linear::with_token("…")` works when nothing else is needed.
///
/// Linear takes the two differently: an OAuth token as `Bearer <token>`, a
/// personal API key (`lin_api_…`) as the bare key. Socket tells them apart by
/// that prefix when the token is given to the integration.
///
/// A token loaded from the application's token store is always sent as
/// `Bearer`: the store is for the OAuth tokens of connected users. Give a
/// personal API key to the integration with `with_token`.
#[derive(Debug, Clone)]
pub struct LinearToken {
    pub token: SecretString,
}

impl From<String> for LinearToken {
    fn from(token: String) -> Self {
        Self {
            token: SecretString::new(token),
        }
    }
}

impl From<&str> for LinearToken {
    fn from(token: &str) -> Self {
        token.to_owned().into()
    }
}

/// The Linear integration.
#[derive(Debug, Clone)]
pub struct Linear {
    spec: ProviderSpec,
    access: Access,
    /// The scheme a personal API key replaced, kept so that setting another kind of token restores it.
    replaced_by_key: Option<AuthScheme>,
}

impl Default for Linear {
    fn default() -> Self {
        Self::new()
    }
}

impl Linear {
    /// Linear with no connection details of its own: the OAuth app is set on the
    /// `Socket` builder and tokens come from the application's token store.
    pub fn new() -> Self {
        Self::with_spec(provider())
    }

    /// Linear with the application's OAuth app, for connecting users through OAuth.
    /// Takes an [`OAuthClient`], or a [`LinearOAuth`] for the settings only Linear has.
    pub fn with_oauth(settings: impl Into<LinearOAuth>) -> Self {
        let settings = settings.into();
        let mut this = Self::new();
        if let AuthScheme::OAuth2(oauth) = &mut this.spec.auth {
            if let Some(scopes) = settings.scopes {
                oauth.default_scopes = scopes;
            }
        }
        this.oauth(settings.client)
    }

    /// Linear with a token the application already holds. Every call uses it.
    /// Takes a string, or a [`LinearToken`] for the settings only Linear has.
    pub fn with_token(settings: impl Into<LinearToken>) -> Self {
        Self::new().token(settings.into().token.expose())
    }

    /// Uses another definition, for a test server.
    pub fn with_spec(spec: ProviderSpec) -> Self {
        Self {
            replaced_by_key: None,
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
    ///
    /// A personal API key (`lin_api_…`) is sent bare and any other token as
    /// `Bearer`, whichever was set before.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        let token = token.into();
        if token.starts_with(API_KEY_PREFIX) {
            let bare = AuthScheme::ApiKey(ApiKeySpec {
                placement: KeyPlacement::Header {
                    name: "Authorization".into(),
                    prefix: None,
                },
            });
            // The scheme in place now, scopes and all, is put back if a
            // different kind of token is set later.
            let previous = std::mem::replace(&mut self.spec.auth, bare);
            self.replaced_by_key.get_or_insert(previous);
        } else if let Some(previous) = self.replaced_by_key.take() {
            self.spec.auth = previous;
        }
        // Otherwise the definition's own scheme is left exactly as it is.
        self.access.token = Some(TokenSet::bearer(token));
        self
    }

    fn decode(&self, what: &str) -> Error {
        Error::new(ErrorKind::Decode, format!("linear answered without {what}")).with_provider(self.spec.id.clone())
    }

    /// Runs one GraphQL query and returns its `data`.
    pub async fn graphql(&self, connection: &Connection, query: &str, variables: Value) -> Result<Value> {
        let request = RawRequest::post("graphql", json!({ "query": query, "variables": variables }));
        let mut body = connection.request(request).await?.body;
        match body["data"].take() {
            Value::Null => Err(self.decode("data")),
            data => Ok(data),
        }
    }

    /// The account the connection is authorised as.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let data = self.graphql(connection, VIEWER, json!({})).await?;
        let viewer = &data["viewer"];
        let filled = |value: &Value| value.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
        let (Some(id), Some(name)) = (filled(&viewer["id"]), filled(&viewer["name"])) else {
            return Err(self.decode("an account"));
        };
        Ok(Account {
            id: id.to_owned(),
            name: name.to_owned(),
            email: viewer["email"].as_str().map(str::to_owned),
        })
    }

    /// Confirms a team exists and the account can see it. Accepts the team key.
    ///
    /// The result's id is the team's id, not its key: a key can be renamed,
    /// and Linear's API takes the id.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        let key = parse_team_key(input).map_err(|e| e.with_provider(self.spec.id.clone()))?;
        let data = self.graphql(connection, TEAM_BY_KEY, json!({ "key": key })).await?;
        let nodes = data["teams"]["nodes"]
            .as_array()
            .ok_or_else(|| self.decode("a team list"))?;
        let Some(team) = nodes.first() else {
            return Err(
                Error::new(ErrorKind::NotFound, format!("Linear team {key} was not found"))
                    .with_provider(self.spec.id.clone()),
            );
        };
        if !team["key"]
            .as_str()
            .is_some_and(|returned| returned.eq_ignore_ascii_case(&key))
        {
            return Err(self.decode("the team that was asked for"));
        }
        let id = team["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| self.decode("a team id"))?;
        let name = team["name"].as_str().unwrap_or(&key);
        Ok(Resource::new(id, format!("{name} ({key})"), "Linear team"))
    }
}

#[async_trait]
impl Integration for Linear {
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
            resolve_operation(&self.spec.id, "a team key such as ENG"),
        ]
    }

    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value> {
        let id = &self.spec.id;
        match operation.strip_prefix(&format!("{id}.")) {
            Some("identity.get") => to_output(id, &self.identity(&connection).await?),
            Some("resource.resolve") => to_output(id, &self.resolve(&connection, &resolve_input(id, &input)?).await?),
            _ => Err(
                Error::new(ErrorKind::Unsupported, format!("linear has no operation {operation:?}"))
                    .with_provider(id.clone()),
            ),
        }
    }

    fn classifier(&self) -> Arc<dyn Classifier> {
        Arc::new(LinearClassifier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(status: u16, body: Value) -> Result<()> {
        let response = RawResponse {
            status,
            headers: Vec::new(),
            body,
        };
        LinearClassifier.classify(&ProviderId::new("linear").unwrap(), &response)
    }

    #[test]
    fn parse_team_key_normalises_case_and_refuses_anything_else() {
        assert_eq!(parse_team_key(" eng ").unwrap(), "ENG");
        assert_eq!(parse_team_key("A1").unwrap(), "A1");
        for bad in ["", "ENG-1", "two words", "ABCDEFGHIJK", "é"] {
            assert_eq!(
                parse_team_key(bad).unwrap_err().kind(),
                ErrorKind::InvalidInput,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn graphql_errors_are_classified_by_their_code_whatever_the_status() {
        let with = |code: &str| json!({ "errors": [{ "message": "nope", "extensions": { "code": code } }] });
        classify(200, json!({ "data": { "viewer": {} } })).unwrap();
        assert_eq!(
            classify(200, with("AUTHENTICATION_ERROR")).unwrap_err().kind(),
            ErrorKind::ReconnectRequired
        );
        assert_eq!(
            classify(400, with("AUTHENTICATION_ERROR")).unwrap_err().kind(),
            ErrorKind::ReconnectRequired
        );
        assert_eq!(
            classify(400, with("RATELIMITED")).unwrap_err().kind(),
            ErrorKind::RateLimited
        );
        assert_eq!(
            classify(200, with("FORBIDDEN")).unwrap_err().kind(),
            ErrorKind::AccessDenied
        );
        assert_eq!(
            classify(200, with("GRAPHQL_VALIDATION_FAILED")).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
        let other = classify(200, with("INTERNAL_SERVER_ERROR")).unwrap_err();
        assert_eq!(
            other.kind(),
            ErrorKind::Unexpected,
            "an unknown failure is never read as an empty result"
        );
        assert!(other.message().contains("nope"));
        assert_eq!(
            classify(401, Value::Null).unwrap_err().kind(),
            ErrorKind::ReconnectRequired
        );
    }
}
