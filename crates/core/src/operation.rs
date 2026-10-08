use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::auth::standard_token_response;
use crate::error::Result;
use crate::http::{Classifier, RawRequest, RawResponse, StandardClassifier, Transport};
use crate::provider::ProviderSpec;
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// The provider's stable identifier for the account.
    pub id: String,
    /// What to show a person.
    pub name: String,
    pub email: Option<String>,
}

/// Something a person named that the provider confirmed exists and the account can reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    /// The provider's stable identifier: a repository's full name, a channel id, a team key.
    pub id: String,
    /// What to show a person.
    pub label: String,
    pub description: String,
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

/// One service's operations. Implemented once per integration crate.
#[async_trait]
pub trait Integration: Send + Sync {
    fn provider(&self) -> ProviderSpec;

    /// Every operation this integration offers.
    fn operations(&self) -> Vec<OperationInfo>;

    /// Runs the operation called `operation` with `input`, a JSON object.
    /// `operation` is always one of the names returned by [`Integration::operations`].
    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value>;

    /// How this provider's responses are told apart. Override when the provider
    /// reports errors inside a successful status.
    fn classifier(&self) -> Arc<dyn Classifier> {
        Arc::new(StandardClassifier)
    }

    /// Reads the provider's token response. Override when the token is not at
    /// the standard place. `now` is the moment the response arrived.
    fn parse_token_response(&self, raw: Value, now: SystemTime) -> Result<TokenSet> {
        standard_token_response(&self.provider().id, &raw, now)
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
