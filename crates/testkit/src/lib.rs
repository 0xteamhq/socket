//! Test support for Socket integrations.
//!
//! [`point_at`] aims a real provider definition at a local [`wiremock`]
//! server, [`connect`] builds a `Socket` with one stored connection, and
//! [`conformance`] holds the checks every integration must pass.

use std::sync::Arc;
use std::time::Duration;

use socketkit_core::{
    AuthScheme, ConnectionKey, Integration, MemoryTokenStore, ProviderSpec, RetryPolicy, Socket, TokenSet, TokenStore,
};
use url::Url;
pub use wiremock;
use wiremock::MockServer;

/// The tenant every testkit connection is stored under.
pub const TENANT: &str = "test-tenant";

/// Rewrites `spec` so its API and OAuth endpoints are on `server`, keeping every path.
///
/// # Panics
/// Panics when `server`'s address is not a URL with a host and port, which a
/// running mock server always has.
pub fn point_at(mut spec: ProviderSpec, server: &MockServer) -> ProviderSpec {
    let base = Url::parse(&server.uri()).expect("a mock server has a valid address");
    let host = base.host_str().expect("a mock server has a host").to_owned();
    let port = base.port().expect("a mock server has a port");
    let moved = |url: &Url| {
        let mut moved = url.clone();
        moved.set_scheme("http").expect("http is a valid scheme");
        moved.set_host(Some(&host)).expect("the mock host is valid");
        moved.set_port(Some(port)).expect("the mock port is valid");
        moved
    };
    spec.api_base = moved(&spec.api_base);
    if let AuthScheme::OAuth2(oauth) = &mut spec.auth {
        oauth.authorize_url = moved(&oauth.authorize_url);
        oauth.token_url = moved(&oauth.token_url);
    }
    spec.allowed_hosts = vec![format!("{host}:{port}")];
    spec
}

/// A `Socket` holding `integration`, with `token` stored for [`TENANT`] and
/// retries that do not slow a test down.
///
/// # Panics
/// Panics when the integration does not register cleanly; a test cannot continue.
pub async fn connect(integration: Arc<dyn Integration>, token: &str) -> (Socket, ConnectionKey) {
    let key = ConnectionKey::new(integration.provider().id, TENANT);
    let store = Arc::new(MemoryTokenStore::new());
    store
        .save(key.clone(), TokenSet::bearer(token))
        .await
        .expect("the memory store cannot fail");
    let retry = RetryPolicy {
        max_attempts: 2,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(50),
    };
    let socket = Socket::builder(store)
        .integration(integration)
        .retry(retry)
        .build()
        .expect("the integration registers");
    (socket, key)
}

pub mod conformance {
    //! Checks every integration must pass. Each takes a function that builds
    //! the integration from a provider definition, so the check can aim it at
    //! a local server.

    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;
    use socketkit_core::{AuthScheme, ErrorKind, Integration, ProviderSpec, Retry};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{connect, point_at};

    /// Runs every check below. `resolvable` is an input `resource.resolve`
    /// accepts for this provider, such as `"acme/api"` for GitHub.
    pub async fn all<F>(real: ProviderSpec, build: F, resolvable: &str)
    where
        F: Fn(ProviderSpec) -> Arc<dyn Integration>,
    {
        definition_is_sound(&real, &build);
        a_rejected_token_requires_reconnect(&real, &build).await;
        throttling_is_reported_with_the_wait(&real, &build).await;
        an_empty_success_is_not_an_account(&real, &build).await;
        a_success_without_the_resource_is_not_a_resource(&real, &build, resolvable).await;
    }

    /// The real definition is valid, uses https throughout, and the
    /// integration describes the operations every integration must offer.
    pub fn definition_is_sound<F>(real: &ProviderSpec, build: &F)
    where
        F: Fn(ProviderSpec) -> Arc<dyn Integration>,
    {
        real.validate().expect("the provider definition is valid");
        assert_eq!(real.api_base.scheme(), "https", "{}: the real API is https", real.id);
        assert!(
            real.allowed_hosts.iter().all(|h| !h.contains(':')),
            "{}: no loopback entries in the real definition",
            real.id
        );
        if let AuthScheme::OAuth2(oauth) = &real.auth {
            assert!(
                real.allows_host(&oauth.token_url),
                "{}: the token endpoint is an allowed host",
                real.id
            );
            assert_eq!(oauth.authorize_url.scheme(), "https");
        }

        let integration = build(real.clone());
        assert_eq!(
            integration.provider(),
            *real,
            "the integration reports the definition it was built with"
        );
        let names: Vec<String> = integration.operations().into_iter().map(|o| o.name).collect();
        assert!(names.contains(&format!("{}.identity.get", real.id)), "{names:?}");
        for operation in integration.operations() {
            assert_eq!(
                operation.input_schema["type"], "object",
                "{}: input is an object",
                operation.name
            );
            assert!(
                !operation.description.trim().is_empty(),
                "{}: has a description",
                operation.name
            );
        }
    }

    async fn identity_error<F>(real: &ProviderSpec, build: &F, response: ResponseTemplate) -> socketkit_core::Error
    where
        F: Fn(ProviderSpec) -> Arc<dyn Integration>,
    {
        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::any())
            .respond_with(response)
            .mount(&server)
            .await;
        let (socket, key) = connect(build(point_at(real.clone(), &server)), "a-token").await;
        let operation = format!("{}.identity.get", real.id);
        socket
            .invoke(key, operation, json!({}))
            .await
            .expect_err("the provider did not confirm an account")
    }

    /// A provider answering 401 means the person must reconnect, and the
    /// message does not repeat the provider's body.
    pub async fn a_rejected_token_requires_reconnect<F>(real: &ProviderSpec, build: &F)
    where
        F: Fn(ProviderSpec) -> Arc<dyn Integration>,
    {
        let response = ResponseTemplate::new(401).set_body_json(json!({ "message": "bad token a-token" }));
        let err = identity_error(real, build, response).await;
        assert_eq!(err.kind(), ErrorKind::ReconnectRequired, "{}: {err}", real.id);
        assert!(!err.message().contains("a-token"), "{}: {}", real.id, err.message());
    }

    /// A provider answering 429 is rate limiting, and the wait it asks for reaches the caller.
    pub async fn throttling_is_reported_with_the_wait<F>(real: &ProviderSpec, build: &F)
    where
        F: Fn(ProviderSpec) -> Arc<dyn Integration>,
    {
        let response = ResponseTemplate::new(429).insert_header("retry-after", "3600");
        let err = identity_error(real, build, response).await;
        assert_eq!(err.kind(), ErrorKind::RateLimited, "{}: {err}", real.id);
        assert_eq!(err.retry(), Retry::After(Duration::from_secs(3600)), "{}", real.id);
    }

    /// A 200 that carries no account is an error, never an account with blank fields.
    pub async fn an_empty_success_is_not_an_account<F>(real: &ProviderSpec, build: &F)
    where
        F: Fn(ProviderSpec) -> Arc<dyn Integration>,
    {
        let err = identity_error(real, build, ResponseTemplate::new(200).set_body_json(json!({}))).await;
        assert!(
            matches!(
                err.kind(),
                ErrorKind::Decode | ErrorKind::ReconnectRequired | ErrorKind::InvalidInput
            ),
            "{}: {err}",
            real.id
        );
    }

    /// A 200 that does not carry the resource asked for is an error, never a
    /// resource: an empty object, or the provider's error written in a 200.
    pub async fn a_success_without_the_resource_is_not_a_resource<F>(real: &ProviderSpec, build: &F, resolvable: &str)
    where
        F: Fn(ProviderSpec) -> Arc<dyn Integration>,
    {
        let operation = format!("{}.resource.resolve", real.id);
        for body in [
            json!({}),
            json!({ "code": 124, "message": "Invalid access token." }),
            json!({ "object": "error" }),
        ] {
            let server = MockServer::start().await;
            Mock::given(wiremock::matchers::any())
                .respond_with(ResponseTemplate::new(200).set_body_json(body.clone()))
                .mount(&server)
                .await;
            let (socket, key) = connect(build(point_at(real.clone(), &server)), "a-token").await;
            let outcome = socket
                .invoke(key, operation.clone(), json!({ "input": resolvable }))
                .await;
            assert!(
                outcome.is_err(),
                "{}: {body} was accepted as {:?}",
                real.id,
                outcome.ok()
            );
        }
    }
}
