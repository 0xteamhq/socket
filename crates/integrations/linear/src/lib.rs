//! Socket integration for Linear.
//!
//! Offers the provider definition, `linear.identity.get` and
//! `linear.resource.resolve` (a team). Linear's API is GraphQL only.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use socketkit_core::{
    Account, AuthScheme, Classifier, ClientAuth, Connection, Error, ErrorKind, Integration, OAuth2Spec, OperationInfo,
    ProviderId, ProviderSpec, RawRequest, RawResponse, Resource, Result, Retry, StandardClassifier, identity_operation,
    resolve_input, resolve_operation, to_output,
};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "linear";

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

/// The Linear integration.
#[derive(Debug, Clone)]
pub struct Linear {
    spec: ProviderSpec,
}

impl Default for Linear {
    fn default() -> Self {
        Self::new()
    }
}

impl Linear {
    pub fn new() -> Self {
        Self { spec: provider() }
    }

    /// Uses another definition, for a test server.
    pub fn with_spec(spec: ProviderSpec) -> Self {
        Self { spec }
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
        let (Some(id), Some(name)) = (viewer["id"].as_str(), viewer["name"].as_str()) else {
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
        let id = team["id"].as_str().ok_or_else(|| self.decode("a team id"))?;
        let name = team["name"].as_str().unwrap_or(&key);
        Ok(Resource::new(id, format!("{name} ({key})"), "Linear team"))
    }
}

#[async_trait]
impl Integration for Linear {
    fn provider(&self) -> ProviderSpec {
        self.spec.clone()
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
