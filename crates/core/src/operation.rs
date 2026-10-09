use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::auth::OAuthClient;
use crate::error::{Error, ErrorKind, Result};
use crate::http::{Classifier, RawRequest, RawResponse, StandardClassifier, Transport};
use crate::oauth::{OAuthFlow, StandardOAuth};
use crate::provider::{ProviderId, ProviderSpec};
use crate::secret::TokenSet;
use crate::store::ConnectionKey;

/// What an operation does to the provider's data. A host uses it to decide
/// which calls need approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Read,
    Write,
    Destructive,
}

/// An operation's description of itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationInfo {
    /// `"<provider id>.<rest>"`, for example `"slack.chat.post_message"`.
    pub name: String,
    pub description: String,
    /// JSON Schema of the input object.
    pub input_schema: Value,
    /// JSON Schema of the output.
    pub output_schema: Value,
    pub effect: Effect,
    pub required_scopes: Vec<String>,
}

/// The account behind a connection: whose token this is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Account {
    /// The provider's stable identifier for the account.
    pub id: String,
    /// What to show a person.
    pub name: String,
    pub email: Option<String>,
}

/// Something a person named that the provider confirmed exists and the account can reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Resource {
    /// The provider's stable identifier: a repository's full name, a channel id, a team key.
    pub id: String,
    /// What to show a person.
    pub label: String,
    pub description: String,
}

impl Resource {
    pub fn new(id: impl Into<String>, label: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: description.into(),
        }
    }
}

/// The description of `<provider>.identity.get`, which every integration offers.
pub fn identity_operation(provider: &ProviderId) -> OperationInfo {
    OperationInfo {
        name: format!("{provider}.identity.get"),
        description: "Return the account this connection is authorised as, confirming the token still works.".into(),
        input_schema: serde_json::json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "name": { "type": "string" },
                "email": { "type": ["string", "null"] }
            },
            "required": ["id", "name"]
        }),
        effect: Effect::Read,
        required_scopes: Vec::new(),
    }
}

/// The description of `<provider>.resource.resolve`. `accepts` says what a person may type.
pub fn resolve_operation(provider: &ProviderId, accepts: &str) -> OperationInfo {
    OperationInfo {
        name: format!("{provider}.resource.resolve"),
        description: format!("Confirm that a resource exists and the account can reach it. Accepts {accepts}."),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": { "input": { "type": "string", "description": accepts } },
            "required": ["input"],
            "additionalProperties": false
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "label": { "type": "string" },
                "description": { "type": "string" }
            },
            "required": ["id", "label", "description"]
        }),
        effect: Effect::Read,
        required_scopes: Vec::new(),
    }
}

/// The JSON Schema of `T`, for an operation's input or output.
pub fn schema_of<T: schemars::JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap_or(Value::Null)
}

/// Reads the `input` string of a `resource.resolve` call.
pub fn resolve_input(provider: &ProviderId, input: &Value) -> Result<String> {
    input["input"].as_str().map(str::to_owned).ok_or_else(|| {
        Error::new(ErrorKind::InvalidInput, "`input` is required and must be a string").with_provider(provider.clone())
    })
}

/// Turns a typed result into the JSON an operation returns.
pub fn to_output<T: Serialize>(provider: &ProviderId, value: &T) -> Result<Value> {
    serde_json::to_value(value).map_err(|e| {
        Error::new(ErrorKind::Unexpected, "could not encode the result")
            .with_provider(provider.clone())
            .with_source(e)
    })
}

/// One stored authorization, loaded and ready to use.
#[derive(Clone)]
pub struct Connection {
    pub key: ConnectionKey,
    pub tokens: TokenSet,
    spec: Arc<ProviderSpec>,
    transport: Transport,
    classifier: Arc<dyn Classifier>,
}

impl Connection {
    pub(crate) fn new(
        key: ConnectionKey,
        tokens: TokenSet,
        spec: Arc<ProviderSpec>,
        transport: Transport,
        classifier: Arc<dyn Classifier>,
    ) -> Self {
        Self {
            key,
            tokens,
            spec,
            transport,
            classifier,
        }
    }

    /// The provider this connection belongs to.
    pub fn provider(&self) -> &ProviderSpec {
        &self.spec
    }

    /// Sends `request` with this connection's credentials through the shared transport.
    pub async fn request(&self, request: RawRequest) -> Result<RawResponse> {
        self.transport
            .send(&self.spec, &self.tokens, self.classifier.as_ref(), request)
            .await
    }
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection")
            .field("key", &self.key)
            .field("tokens", &self.tokens)
            .finish_non_exhaustive()
    }
}

/// What an integration's own settings come down to once it has read them.
///
/// Each integration crate defines its own settings types, with whatever
/// fields that service needs (extra scopes, a host name, a workspace). This
/// is only where the two things the core understands end up.
#[derive(Debug, Clone, Default)]
pub struct Access {
    pub oauth: Option<OAuthClient>,
    pub token: Option<TokenSet>,
}

/// One service's operations. Implemented once per integration crate.
#[async_trait]
pub trait Integration: Send + Sync {
    fn provider(&self) -> ProviderSpec;

    /// Every operation this integration offers.
    fn operations(&self) -> Vec<OperationInfo>;

    /// Runs the operation called `operation` with `input`, a JSON object.
    /// `operation` is always one of the names returned by [`Integration::operations`].
    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value>;

    /// Reports a problem with the settings this integration was created with,
    /// such as a host name that is not one. Checked when the `Socket` is built.
    fn check(&self) -> Result<()> {
        Ok(())
    }

    /// The application's OAuth app for this provider, when it was given to the
    /// integration itself, as in `Slack::with_oauth(client)`.
    fn oauth_client(&self) -> Option<OAuthClient> {
        None
    }

    /// A token every call should use, when it was given to the integration
    /// itself, as in `Slack::with_token("xoxb-…")`. With one set, the token
    /// store is not consulted for this provider and every tenant shares it.
    fn fixed_token(&self) -> Option<TokenSet> {
        None
    }

    /// How this provider's responses are told apart. Override when the provider
    /// reports errors inside a successful status.
    fn classifier(&self) -> Arc<dyn Classifier> {
        Arc::new(StandardClassifier)
    }

    /// The OAuth flow for this provider. Override when a step differs from
    /// the standard; see [`OAuthFlow`].
    fn oauth_flow(&self) -> Arc<dyn OAuthFlow> {
        Arc::new(StandardOAuth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_info_serializes_with_snake_case_effect() {
        let info = OperationInfo {
            name: "slack.chat.post_message".into(),
            description: "Post a message to a channel.".into(),
            input_schema: serde_json::json!({ "type": "object", "required": ["channel", "text"] }),
            output_schema: serde_json::json!({ "type": "object" }),
            effect: Effect::Write,
            required_scopes: vec!["chat:write".into()],
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["effect"], "write");
        assert_eq!(json["input_schema"]["required"][0], "channel");
        let back: OperationInfo = serde_json::from_value(json).unwrap();
        assert_eq!(back, info);
        assert_eq!(serde_json::to_value(Effect::Destructive).unwrap(), "destructive");
    }
}
