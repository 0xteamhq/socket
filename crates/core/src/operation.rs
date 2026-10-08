use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Result;
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

/// One stored authorization, loaded and ready to use.
#[derive(Debug, Clone)]
pub struct Connection {
    pub key: ConnectionKey,
    pub tokens: TokenSet,
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
