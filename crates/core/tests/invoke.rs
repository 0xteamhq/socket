//! The by-name layer, exercised the way an application uses it.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use serde_json::{Value, json};
use socketkit_core::{
    ApiKeySpec, AuthScheme, Connection, ConnectionKey, Effect, Error, ErrorKind, Integration, KeyPlacement,
    MemoryTokenStore, OperationInfo, ProviderId, ProviderSpec, Result, Socket, TokenSet, TokenStore,
};

/// A stand-in integration: `<id>.echo` returns its input and the tenant it ran for.
struct Echo {
    id: &'static str,
    operations: Vec<&'static str>,
    calls: AtomicUsize,
}

impl Echo {
    fn new(id: &'static str, operations: &[&'static str]) -> Arc<Self> {
        Arc::new(Self {
            id,
            operations: operations.to_vec(),
            calls: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl Integration for Echo {
    fn provider(&self) -> ProviderSpec {
        ProviderSpec {
            id: ProviderId::new(self.id).unwrap(),
            display_name: self.id.to_uppercase(),
            api_base: format!("https://api.{}.test/", self.id).parse().unwrap(),
            allowed_hosts: vec![format!("api.{}.test", self.id)],
            auth: AuthScheme::ApiKey(ApiKeySpec {
                placement: KeyPlacement::Basic {},
            }),
        }
    }

    fn operations(&self) -> Vec<OperationInfo> {
        self.operations
            .iter()
            .map(|name| OperationInfo {
                name: (*name).to_owned(),
                description: "Return the input.".into(),
                input_schema: json!({ "type": "object" }),
                output_schema: json!({ "type": "object" }),
                effect: Effect::Read,
                required_scopes: Vec::new(),
            })
            .collect()
    }

    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(json!({
            "operation": operation,
            "tenant": connection.key.tenant,
            "token_len": connection.tokens.access_token.expose().len(),
            "input": input,
        }))
    }
}

/// A store whose every call fails, as a database outage would.
struct BrokenStore;

#[async_trait]
impl TokenStore for BrokenStore {
    async fn load(&self, _key: ConnectionKey) -> Result<Option<TokenSet>> {
        Err(Error::new(ErrorKind::Unexpected, "store is down"))
    }
    async fn save(&self, _key: ConnectionKey, _tokens: TokenSet) -> Result<()> {
        Err(Error::new(ErrorKind::Unexpected, "store is down"))
    }
    async fn delete(&self, _key: ConnectionKey) -> Result<()> {
        Err(Error::new(ErrorKind::Unexpected, "store is down"))
    }
}

fn key(provider: &str, tenant: &str) -> ConnectionKey {
    ConnectionKey::new(ProviderId::new(provider).unwrap(), tenant)
}

async fn socket_with_slack_connected() -> (Socket, Arc<Echo>) {
    let store = Arc::new(MemoryTokenStore::new());
    store
        .save(key("slack", "acme"), TokenSet::bearer("xoxb-123"))
        .await
        .unwrap();
    let slack = Echo::new("slack", &["slack.echo", "slack.auth.test"]);
    let github = Echo::new("github", &["github.echo"]);
    let socket = Socket::builder(store)
        .integration(slack.clone())
        .integration(github)
        .build()
        .unwrap();
    (socket, slack)
}

#[tokio::test]
async fn invoke_runs_the_named_operation_with_the_stored_tokens() {
    let (socket, slack) = socket_with_slack_connected().await;
    let out = socket
        .invoke(key("slack", "acme"), "slack.echo".into(), json!({ "text": "hi" }))
        .await
        .unwrap();
    assert_eq!(out["operation"], "slack.echo");
    assert_eq!(out["tenant"], "acme");
    assert_eq!(out["token_len"], 8);
    assert_eq!(out["input"]["text"], "hi");
    assert_eq!(slack.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn operations_lists_every_integration_sorted_by_name() {
    let (socket, _) = socket_with_slack_connected().await;
    let names: Vec<_> = socket.operations().into_iter().map(|o| o.name).collect();
    assert_eq!(names, ["github.echo", "slack.auth.test", "slack.echo"]);
}

#[tokio::test]
async fn an_unknown_operation_is_unsupported_and_runs_nothing() {
    let (socket, slack) = socket_with_slack_connected().await;
    for name in ["slack.nope", "slack", "", "SLACK.ECHO", "slack.echo "] {
        let err = socket
            .invoke(key("slack", "acme"), name.into(), json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unsupported, "{name:?}");
    }
    assert_eq!(slack.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_operation_cannot_run_on_another_providers_connection() {
    let (socket, slack) = socket_with_slack_connected().await;
    let err = socket
        .invoke(key("slack", "acme"), "github.echo".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(slack.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn input_that_is_not_an_object_is_refused_before_the_integration_runs() {
    let (socket, slack) = socket_with_slack_connected().await;
    for input in [json!(null), json!("text"), json!([1, 2]), json!(3)] {
        let err = socket
            .invoke(key("slack", "acme"), "slack.echo".into(), input)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }
    assert_eq!(slack.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_tenant_with_no_stored_tokens_must_reconnect() {
    let (socket, slack) = socket_with_slack_connected().await;
    let err = socket
        .invoke(key("slack", "globex"), "slack.echo".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ReconnectRequired);
    assert_eq!(err.provider().map(ProviderId::as_str), Some("slack"));
    assert_eq!(slack.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_failing_store_surfaces_its_own_error() {
    let socket = Socket::builder(Arc::new(BrokenStore))
        .integration(Echo::new("slack", &["slack.echo"]))
        .build()
        .unwrap();
    let err = socket
        .invoke(key("slack", "acme"), "slack.echo".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unexpected);
    assert_eq!(err.message(), "store is down");
}

#[tokio::test]
async fn build_refuses_duplicate_providers_duplicate_names_and_foreign_names() {
    let store = || Arc::new(MemoryTokenStore::new());
    let kind = |r: Result<Socket>| r.unwrap_err().kind();

    let twice = Socket::builder(store())
        .integration(Echo::new("slack", &["slack.echo"]))
        .integration(Echo::new("slack", &["slack.other"]));
    assert_eq!(kind(twice.build()), ErrorKind::Config);

    let duplicate_name = Socket::builder(store()).integration(Echo::new("slack", &["slack.echo", "slack.echo"]));
    assert_eq!(kind(duplicate_name.build()), ErrorKind::Config);

    for foreign in ["github.echo", "echo", "slack.", "slackecho", "slack"] {
        let builder = Socket::builder(store()).integration(Echo::new("slack", &[foreign]));
        assert_eq!(kind(builder.build()), ErrorKind::Config, "{foreign:?}");
    }
}

#[tokio::test]
async fn a_socket_with_no_integrations_builds_and_supports_nothing() {
    let socket = Socket::builder(Arc::new(MemoryTokenStore::new())).build().unwrap();
    assert!(socket.operations().is_empty());
    let err = socket
        .invoke(key("slack", "acme"), "slack.echo".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unsupported);
}

#[tokio::test]
async fn a_socket_is_shared_across_tasks() {
    let (socket, slack) = socket_with_slack_connected().await;
    let socket = Arc::new(socket);
    let mut handles = Vec::new();
    for n in 0..8 {
        let socket = Arc::clone(&socket);
        handles.push(tokio::spawn(async move {
            socket
                .invoke(key("slack", "acme"), "slack.echo".into(), json!({ "n": n }))
                .await
                .unwrap()
        }));
    }
    for (n, handle) in handles.into_iter().enumerate() {
        assert_eq!(handle.await.unwrap()["input"]["n"], n);
    }
    assert_eq!(slack.calls.load(Ordering::SeqCst), 8);
}
