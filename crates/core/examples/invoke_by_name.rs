//! Registers one integration and calls an operation by name with JSON.
//! No network: the integration answers from memory.
//!
//! Run with `cargo run -p socketkit-core --example invoke_by_name`.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use socketkit_core::{
    ApiKeySpec, AuthScheme, Connection, ConnectionKey, Effect, Error, ErrorKind, Integration, KeyPlacement,
    MemoryTokenStore, OperationInfo, ProviderId, ProviderSpec, Result, Socket, TokenSet, TokenStore,
};

struct Greeter;

#[async_trait]
impl Integration for Greeter {
    fn provider(&self) -> ProviderSpec {
        ProviderSpec {
            id: ProviderId::new("greeter").expect("valid id"),
            display_name: "Greeter".into(),
            api_base: "https://api.greeter.test/".parse().expect("valid url"),
            allowed_hosts: vec!["api.greeter.test".into()],
            content_hosts: Vec::new(),
            auth: AuthScheme::ApiKey(ApiKeySpec {
                placement: KeyPlacement::Header {
                    name: "Authorization".into(),
                    prefix: Some("Bearer ".into()),
                },
            }),
        }
    }

    fn operations(&self) -> Vec<OperationInfo> {
        vec![OperationInfo {
            name: "greeter.hello".into(),
            description: "Greet someone by name.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "name": { "type": "string" } },
                "required": ["name"]
            }),
            output_schema: json!({ "type": "object", "properties": { "greeting": { "type": "string" } } }),
            effect: Effect::Read,
            required_scopes: Vec::new(),
        }]
    }

    async fn invoke(&self, connection: Connection, _operation: String, input: Value) -> Result<Value> {
        let Some(name) = input.get("name").and_then(Value::as_str) else {
            return Err(Error::new(ErrorKind::InvalidInput, "name is required"));
        };
        Ok(json!({ "greeting": format!("Hello, {name}, from tenant {}", connection.key.tenant) }))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let store = Arc::new(MemoryTokenStore::new());
    let key = ConnectionKey::new(ProviderId::new("greeter")?, "acme");
    store.save(key.clone(), TokenSet::bearer("demo-key")).await?;

    let socket = Socket::builder(store).integration(Arc::new(Greeter)).build()?;

    for operation in socket.operations() {
        println!("{} [{:?}] {}", operation.name, operation.effect, operation.description);
    }

    let output = socket
        .invoke(key.clone(), "greeter.hello".into(), json!({ "name": "Ada" }))
        .await?;
    println!("{output}");

    let refused = socket.invoke(key, "greeter.hello".into(), json!({})).await.unwrap_err();
    println!(
        "{}",
        serde_json::to_string(&refused.to_wire()).expect("wire errors serialize")
    );
    Ok(())
}
