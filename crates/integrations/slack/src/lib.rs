//! Socket integration for Slack.
//!
//! Offers the provider definition, `slack.identity.get` and
//! `slack.resource.resolve` (a channel). Typed operations arrive in phase 2.

use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Account, AuthScheme, Classifier, ClientAuth, Connection, Error, ErrorKind, Integration, OAuth2Spec, OperationInfo,
    ProviderId, ProviderSpec, RawRequest, RawResponse, Resource, Result, Retry, StandardClassifier, TokenSet,
    identity_operation, resolve_input, resolve_operation, standard_token_response, to_output,
};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "slack";

/// Workspaces rarely exceed a few thousand public channels; stop paging
/// instead of walking an unbounded list.
const MAX_PAGES: usize = 20;

/// Slack's definition: where its API lives and how it authenticates.
///
/// # Panics
/// Never in practice: the URLs are constants that parse.
pub fn provider() -> ProviderSpec {
    ProviderSpec {
        id: ProviderId::new(PROVIDER_ID).expect("a valid provider id"),
        display_name: "Slack".into(),
        api_base: "https://slack.com/api/".parse().expect("a valid URL"),
        allowed_hosts: vec!["slack.com".into()],
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: "https://slack.com/oauth/v2/authorize".parse().expect("a valid URL"),
            token_url: "https://slack.com/api/oauth.v2.access".parse().expect("a valid URL"),
            default_scopes: vec![
                "channels:history".into(),
                "channels:read".into(),
                "users:read".into(),
                "users:read.email".into(),
            ],
            scope_separator: ",".into(),
            pkce: false,
            client_auth: ClientAuth::Body,
            extra_authorize_params: Vec::new(),
        }),
    }
}

/// Slack reports most failures as HTTP 200 with `"ok": false` and an error code.
#[derive(Debug, Clone, Copy, Default)]
pub struct SlackClassifier;

impl Classifier for SlackClassifier {
    fn classify(&self, provider: &ProviderId, response: &RawResponse) -> Result<()> {
        StandardClassifier.classify(provider, response)?;
        if response.body["ok"] == true {
            return Ok(());
        }
        let Some(code) = response.body["error"].as_str() else {
            // A success status that does not say `ok: true` confirms nothing.
            return Err(
                Error::new(ErrorKind::Decode, format!("{provider} answered without a result"))
                    .with_provider(provider.clone()),
            );
        };
        let error = |kind, message: String| Error::new(kind, message).with_provider(provider.clone());
        Err(match code {
            "invalid_auth" | "not_authed" | "token_revoked" | "token_expired" | "account_inactive" => error(
                ErrorKind::ReconnectRequired,
                format!("{provider} rejected the stored authorization"),
            ),
            "ratelimited" | "rate_limited" => {
                error(ErrorKind::RateLimited, format!("{provider} is rate limiting requests")).with_retry(Retry::Later)
            }
            "missing_scope" => {
                let needed = response.body["needed"].as_str().unwrap_or("a scope it was not granted");
                error(
                    ErrorKind::AccessDenied,
                    format!("{provider} denied the request: it needs {needed}"),
                )
            }
            "not_in_channel"
            | "access_denied"
            | "no_permission"
            | "restricted_action"
            | "ekm_access_denied"
            | "not_allowed_token_type"
            | "team_access_not_granted"
            | "org_login_required" => error(
                ErrorKind::AccessDenied,
                format!("{provider} denied the request: {code}"),
            ),
            code if code.ends_with("_not_found") => {
                error(ErrorKind::NotFound, format!("{provider} has no such resource: {code}"))
            }
            "internal_error" | "fatal_error" | "service_unavailable" | "request_timeout" => {
                error(ErrorKind::Unexpected, format!("{provider} failed: {code}")).with_retry(Retry::Later)
            }
            _ => error(
                ErrorKind::InvalidInput,
                format!("{provider} rejected the request: {code}"),
            ),
        })
    }
}

/// A channel as a person may name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelRef {
    Id(String),
    Name(String),
}

/// Reads a channel id (`C0123ABCD`) or a name with or without `#`.
pub fn parse_channel(input: &str) -> Result<ChannelRef> {
    let trimmed = input.trim();
    let is_id = trimmed.len() >= 9
        && trimmed.starts_with(['C', 'G'])
        && trimmed.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if is_id {
        return Ok(ChannelRef::Id(trimmed.to_owned()));
    }
    let name = trimmed.trim_start_matches('#').to_lowercase();
    let valid =
        (1..=80).contains(&name.chars().count()) && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'));
    if valid {
        Ok(ChannelRef::Name(name))
    } else {
        Err(Error::new(
            ErrorKind::InvalidInput,
            format!("\"{trimmed}\" is not a Slack channel; use #channel-name or a channel id"),
        ))
    }
}

/// The Slack integration.
#[derive(Debug, Clone)]
pub struct Slack {
    spec: ProviderSpec,
}

impl Default for Slack {
    fn default() -> Self {
        Self::new()
    }
}

impl Slack {
    pub fn new() -> Self {
        Self { spec: provider() }
    }

    /// Uses another definition, for a test server.
    pub fn with_spec(spec: ProviderSpec) -> Self {
        Self { spec }
    }

    fn not_found(&self, what: &str) -> Error {
        Error::new(ErrorKind::NotFound, format!("{what} was not found")).with_provider(self.spec.id.clone())
    }

    /// The account the connection is authorised as.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let body = connection.request(RawRequest::get("auth.test")).await?.body;
        let (Some(id), Some(name)) = (body["user_id"].as_str(), body["user"].as_str()) else {
            return Err(
                Error::new(ErrorKind::Decode, "slack answered without an account").with_provider(self.spec.id.clone())
            );
        };
        Ok(Account {
            id: id.to_owned(),
            name: name.to_owned(),
            email: None,
        })
    }

    /// Confirms a public channel exists and the account can see it.
    ///
    /// Accepts a channel id or a name with or without `#`.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        let channel = match parse_channel(input).map_err(|e| e.with_provider(self.spec.id.clone()))? {
            ChannelRef::Id(id) => self.channel_by_id(connection, &id).await?,
            ChannelRef::Name(name) => self.channel_by_name(connection, &name).await?,
        };
        let id = channel["id"].as_str().filter(|id| !id.is_empty()).ok_or_else(|| {
            Error::new(ErrorKind::Decode, "slack answered without a channel").with_provider(self.spec.id.clone())
        })?;
        let name = channel["name"].as_str().unwrap_or(id);
        Ok(Resource::new(id, format!("#{name}"), "Slack channel"))
    }

    async fn channel_by_id(&self, connection: &Connection, id: &str) -> Result<Value> {
        let request = RawRequest::get("conversations.info").with_query("channel", id);
        match connection.request(request).await {
            Ok(response) => Ok(response.body["channel"].clone()),
            Err(e) if e.kind() == ErrorKind::NotFound => Err(self.not_found(&format!("Slack channel {id}"))),
            Err(e) => Err(e),
        }
    }

    async fn channel_by_name(&self, connection: &Connection, name: &str) -> Result<Value> {
        let mut cursor = String::new();
        for _ in 0..MAX_PAGES {
            let request = RawRequest::get("conversations.list")
                .with_query("types", "public_channel")
                .with_query("exclude_archived", "true")
                .with_query("limit", "1000")
                .with_query("cursor", cursor.as_str());
            let body = connection.request(request).await?.body;
            let found = body["channels"]
                .as_array()
                .and_then(|channels| channels.iter().find(|c| c["name"] == name));
            if let Some(channel) = found {
                return Ok(channel.clone());
            }
            cursor = body["response_metadata"]["next_cursor"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            if cursor.is_empty() {
                break;
            }
        }
        Err(self.not_found(&format!("Slack channel #{name}")))
    }
}

#[async_trait]
impl Integration for Slack {
    fn provider(&self) -> ProviderSpec {
        self.spec.clone()
    }

    fn operations(&self) -> Vec<OperationInfo> {
        vec![
            identity_operation(&self.spec.id),
            resolve_operation(&self.spec.id, "a public channel as #name or a channel id"),
        ]
    }

    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value> {
        let id = &self.spec.id;
        match operation.strip_prefix(&format!("{id}.")) {
            Some("identity.get") => to_output(id, &self.identity(&connection).await?),
            Some("resource.resolve") => to_output(id, &self.resolve(&connection, &resolve_input(id, &input)?).await?),
            _ => Err(
                Error::new(ErrorKind::Unsupported, format!("slack has no operation {operation:?}"))
                    .with_provider(id.clone()),
            ),
        }
    }

    fn classifier(&self) -> Arc<dyn Classifier> {
        Arc::new(SlackClassifier)
    }

    /// Slack nests a user token under `authed_user`; a bot token sits at the top.
    fn parse_token_response(&self, raw: Value, now: SystemTime) -> Result<TokenSet> {
        let user = &raw["authed_user"];
        let source = if user["access_token"].as_str().is_some_and(|t| !t.is_empty()) {
            user
        } else {
            &raw
        };
        standard_token_response(&self.spec.id, source, now)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;

    fn classify(status: u16, body: Value) -> Result<()> {
        let response = RawResponse {
            status,
            headers: Vec::new(),
            body,
        };
        SlackClassifier.classify(&ProviderId::new("slack").unwrap(), &response)
    }

    #[test]
    fn parse_channel_tells_ids_from_names() {
        assert_eq!(parse_channel("C0123ABCD").unwrap(), ChannelRef::Id("C0123ABCD".into()));
        assert_eq!(
            parse_channel(" G0123ABCDE ").unwrap(),
            ChannelRef::Id("G0123ABCDE".into())
        );
        assert_eq!(
            parse_channel("#Eng-Backend").unwrap(),
            ChannelRef::Name("eng-backend".into())
        );
        assert_eq!(parse_channel("general").unwrap(), ChannelRef::Name("general".into()));
        assert_eq!(
            parse_channel("CHANNEL").unwrap(),
            ChannelRef::Name("channel".into()),
            "too short to be an id"
        );
        for bad in ["", "#", "has space", "a/b", &"x".repeat(81)] {
            assert_eq!(
                parse_channel(bad).unwrap_err().kind(),
                ErrorKind::InvalidInput,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn ok_false_is_an_error_even_with_status_200() {
        classify(200, json!({ "ok": true })).unwrap();
        let kind = |code: &str| classify(200, json!({ "ok": false, "error": code })).unwrap_err().kind();
        for code in [
            "invalid_auth",
            "not_authed",
            "token_revoked",
            "token_expired",
            "account_inactive",
        ] {
            assert_eq!(kind(code), ErrorKind::ReconnectRequired, "{code}");
        }
        assert_eq!(kind("channel_not_found"), ErrorKind::NotFound);
        assert_eq!(kind("user_not_found"), ErrorKind::NotFound);
        assert_eq!(kind("not_in_channel"), ErrorKind::AccessDenied);
        assert_eq!(kind("ratelimited"), ErrorKind::RateLimited);
        assert_eq!(kind("invalid_arguments"), ErrorKind::InvalidInput);
        assert_eq!(kind("internal_error"), ErrorKind::Unexpected);
    }

    #[test]
    fn a_missing_scope_names_the_scope_and_a_bare_200_confirms_nothing() {
        let err = classify(
            200,
            json!({ "ok": false, "error": "missing_scope", "needed": "channels:read" }),
        )
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::AccessDenied);
        assert!(err.message().contains("channels:read"), "{}", err.message());
        assert_eq!(classify(200, json!({})).unwrap_err().kind(), ErrorKind::Decode);
        assert_eq!(classify(200, Value::Null).unwrap_err().kind(), ErrorKind::Decode);
    }

    #[test]
    fn http_level_failures_still_use_the_standard_rules() {
        assert_eq!(classify(429, Value::Null).unwrap_err().kind(), ErrorKind::RateLimited);
        assert_eq!(classify(503, Value::Null).unwrap_err().retry(), Retry::Later);
    }

    #[test]
    fn a_user_token_is_read_from_authed_user_and_a_bot_token_from_the_top() {
        let slack = Slack::new();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let user = json!({ "ok": true, "authed_user": { "id": "U1", "access_token": "xoxp-user", "scope": "channels:read,users:read" }, "team": { "id": "T1" } });
        let tokens = slack.parse_token_response(user, now).unwrap();
        assert_eq!(tokens.access_token.expose(), "xoxp-user");
        assert_eq!(tokens.scopes, ["channels:read", "users:read"]);

        let bot = json!({ "ok": true, "access_token": "xoxb-bot", "scope": "chat:write", "authed_user": { "id": "U1" }, "refresh_token": "xoxe-1", "expires_in": 43200 });
        let tokens = slack.parse_token_response(bot, now).unwrap();
        assert_eq!(tokens.access_token.expose(), "xoxb-bot");
        assert_eq!(tokens.expires_at, Some(now + Duration::from_secs(43_200)));
        assert!(tokens.refresh_token.is_some());

        assert_eq!(
            slack
                .parse_token_response(json!({ "ok": true }), now)
                .unwrap_err()
                .kind(),
            ErrorKind::Decode
        );
    }
}
