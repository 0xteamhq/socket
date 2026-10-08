//! The transport and the OAuth flow, against a local HTTP server that plays the provider.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use serde_json::{Value, json};
use socketkit_core::{
    ApiKeySpec, AuthScheme, ClientAuth, Connection, ConnectionKey, Effect, ErrorKind, Integration, KeyPlacement,
    MemoryTokenStore, OAuth2Spec, OAuthClient, OperationInfo, ProviderId, ProviderSpec, RawRequest, Result, Retry,
    RetryPolicy, SecretString, Socket, SocketBuilder, TokenSet, TokenStore,
};
use wiremock::matchers::{body_string_contains, header, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const STATE_SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";

fn host_entry(server: &MockServer) -> String {
    let url = url::Url::parse(&server.uri()).unwrap();
    format!("{}:{}", url.host_str().unwrap(), url.port().unwrap())
}

fn oauth_spec(server: &MockServer, client_auth: ClientAuth, pkce: bool) -> ProviderSpec {
    ProviderSpec {
        id: ProviderId::new("acme").unwrap(),
        display_name: "Acme".into(),
        api_base: format!("{}/api", server.uri()).parse().unwrap(),
        allowed_hosts: vec![host_entry(server)],
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: format!("{}/authorize", server.uri()).parse().unwrap(),
            token_url: format!("{}/token", server.uri()).parse().unwrap(),
            default_scopes: vec!["read".into(), "write".into()],
            scope_separator: " ".into(),
            pkce,
            client_auth,
            extra_authorize_params: Vec::new(),
        }),
    }
}

fn key_spec(server: &MockServer, placement: KeyPlacement) -> ProviderSpec {
    ProviderSpec {
        auth: AuthScheme::ApiKey(ApiKeySpec { placement }),
        ..oauth_spec(server, ClientAuth::Body, false)
    }
}

fn key() -> ConnectionKey {
    ConnectionKey::new(ProviderId::new("acme").unwrap(), "tenant-1")
}

fn client() -> OAuthClient {
    OAuthClient {
        client_id: "client-id".into(),
        client_secret: SecretString::new("client-secret"),
        redirect_uri: "https://app.example.test/callback".parse().unwrap(),
    }
}

fn fast_retry() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(1500),
    }
}

fn builder(store: Arc<MemoryTokenStore>, spec: ProviderSpec) -> SocketBuilder {
    Socket::builder(store)
        .provider(spec)
        .oauth_client(ProviderId::new("acme").unwrap(), client())
        .state_secret(STATE_SECRET)
        .retry(fast_retry())
}

async fn connected(spec: ProviderSpec, tokens: TokenSet) -> (Socket, Arc<MemoryTokenStore>) {
    let store = Arc::new(MemoryTokenStore::new());
    store.save(key(), tokens).await.unwrap();
    let socket = match spec.auth {
        AuthScheme::OAuth2(_) => builder(store.clone(), spec).build().unwrap(),
        AuthScheme::ApiKey(_) => Socket::builder(store.clone())
            .provider(spec)
            .retry(fast_retry())
            .build()
            .unwrap(),
    };
    (socket, store)
}

fn expired(refresh: Option<&str>) -> TokenSet {
    TokenSet {
        access_token: SecretString::new("old-access"),
        refresh_token: refresh.map(SecretString::new),
        expires_at: Some(SystemTime::now() - Duration::from_secs(10)),
        scopes: vec!["read".into()],
    }
}

async fn hits(server: &MockServer, wanted_path: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == wanted_path)
        .count()
}

// ── The generic request ───────────────────────────────────────────────────────

#[tokio::test]
async fn a_request_joins_the_path_adds_the_query_and_carries_the_bearer_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/users/me"))
        .and(query_param("fields", "id,name"))
        .and(header("authorization", "Bearer tok-1"))
        .and(header_exists("user-agent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "u1" })))
        .expect(2)
        .mount(&server)
        .await;
    let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), TokenSet::bearer("tok-1")).await;

    // With and without a leading slash: neither may drop the `/api` of the base.
    for requested in ["users/me", "/users/me"] {
        let response = socket
            .request(key(), RawRequest::get(requested).with_query("fields", "id,name"))
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body["id"], "u1");
        assert_eq!(response.header("Content-Type"), Some("application/json"));
    }
}

#[tokio::test]
async fn a_json_body_is_sent_and_an_empty_success_reads_as_null() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/items"))
        .and(header("content-type", "application/json"))
        .and(body_string_contains(r#""title":"hello""#))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), TokenSet::bearer("t")).await;
    let response = socket
        .request(key(), RawRequest::post("items", json!({ "title": "hello" })))
        .await
        .unwrap();
    assert_eq!((response.status, response.body), (204, Value::Null));
}

#[tokio::test]
async fn an_api_key_goes_where_the_provider_definition_says() {
    let server = MockServer::start().await;
    Mock::given(path("/api/h"))
        .and(header("x-api-key", "Token k-1"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/q"))
        .and(query_param("api_key", "k-1"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    // base64("k-1:")
    Mock::given(path("/api/b"))
        .and(header("authorization", "Basic ay0xOg=="))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let cases = [
        (
            "h",
            KeyPlacement::Header {
                name: "X-Api-Key".into(),
                prefix: Some("Token ".into()),
            },
        ),
        ("q", KeyPlacement::Query { name: "api_key".into() }),
        ("b", KeyPlacement::Basic {}),
    ];
    for (route, placement) in cases {
        let (socket, _) = connected(key_spec(&server, placement), TokenSet::bearer("k-1")).await;
        socket.request(key(), RawRequest::get(route)).await.unwrap();
    }
}

#[tokio::test]
async fn credentials_are_never_sent_to_a_host_outside_the_allowlist() {
    let server = MockServer::start().await;
    let elsewhere = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&elsewhere)
        .await;
    let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), TokenSet::bearer("tok-1")).await;

    for target in [
        format!("{}/steal", elsewhere.uri()),
        "https://evil.test/steal".to_owned(),
    ] {
        let err = socket.request(key(), RawRequest::get(target)).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(err.message().contains("allowed hosts"), "{}", err.message());
        assert!(!err.message().contains("tok-1"));
    }
    assert!(
        elsewhere.received_requests().await.unwrap().is_empty(),
        "nothing may reach the other server"
    );
}

#[tokio::test]
async fn a_caller_cannot_set_the_headers_that_carry_credentials_or_choose_the_host() {
    let server = MockServer::start().await;
    Mock::given(path("/api/x"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let (oauth, _) = connected(oauth_spec(&server, ClientAuth::Body, false), TokenSet::bearer("real")).await;
    let placement = KeyPlacement::Header {
        name: "X-Api-Key".into(),
        prefix: None,
    };
    let (keyed, _) = connected(key_spec(&server, placement), TokenSet::bearer("real")).await;

    let refused = [
        (&oauth, "Authorization", "Bearer forged"),
        (&oauth, "authorization", "Bearer forged"),
        (&oauth, "Host", "evil.test"),
        (&oauth, "Proxy-Authorization", "Basic eA=="),
        (&oauth, "Content-Length", "0"),
        (&oauth, "Transfer-Encoding", "chunked"),
        (&keyed, "x-api-key", "forged"),
        (&oauth, "Bad Name", "v"),
        (&oauth, "X-Ok", "line\r\nbreak"),
    ];
    for (socket, name, value) in refused {
        let err = socket
            .request(key(), RawRequest::get("x").with_header(name, value))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}");
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "a refused request is never sent"
    );
}

#[tokio::test]
async fn a_caller_header_replaces_a_default_and_exactly_one_credential_is_sent() {
    let server = MockServer::start().await;
    Mock::given(path("/api/x"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), TokenSet::bearer("real")).await;
    socket
        .request(
            key(),
            RawRequest::get("x").with_header("Accept", "application/vnd.acme+json"),
        )
        .await
        .unwrap();
    let received = server.received_requests().await.unwrap();
    let all = |name: &str| -> Vec<String> {
        received[0]
            .headers
            .get_all(name)
            .iter()
            .map(|v| v.to_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(all("accept"), ["application/vnd.acme+json"]);
    assert_eq!(all("authorization"), ["Bearer real"]);
}

#[tokio::test]
async fn a_caller_cannot_add_a_second_api_key_to_the_query() {
    let server = MockServer::start().await;
    let spec = key_spec(&server, KeyPlacement::Query { name: "api_key".into() });
    let (socket, _) = connected(spec, TokenSet::bearer("real")).await;
    let err = socket
        .request(key(), RawRequest::get("x").with_query("api_key", "forged"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    let in_path = socket
        .request(key(), RawRequest::get("x?api_key=forged"))
        .await
        .unwrap_err();
    assert_eq!(in_path.kind(), ErrorKind::InvalidInput);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_redirect_is_not_followed_so_credentials_never_leave_the_allowed_host() {
    let server = MockServer::start().await;
    let elsewhere = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&elsewhere)
        .await;
    for status in [301, 302, 307, 308] {
        Mock::given(path(format!("/api/moved-{status}")))
            .respond_with(ResponseTemplate::new(status).insert_header("location", format!("{}/steal", elsewhere.uri())))
            .mount(&server)
            .await;
    }
    // A custom header and a query key are the credentials an HTTP client does not strip on a redirect.
    let placements = [
        KeyPlacement::Header {
            name: "X-Api-Key".into(),
            prefix: None,
        },
        KeyPlacement::Query { name: "api_key".into() },
    ];
    for placement in placements {
        let (socket, _) = connected(key_spec(&server, placement), TokenSet::bearer("k-1")).await;
        for status in [301, 302, 307, 308] {
            let err = socket
                .request(key(), RawRequest::get(format!("moved-{status}")))
                .await
                .unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Unexpected, "HTTP {status}");
        }
    }
    assert!(
        elsewhere.received_requests().await.unwrap().is_empty(),
        "nothing may follow the redirect"
    );
}

#[tokio::test]
async fn an_application_supplied_http_client_still_does_not_follow_redirects() {
    let server = MockServer::start().await;
    let elsewhere = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&elsewhere)
        .await;
    Mock::given(path("/api/moved"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", format!("{}/steal", elsewhere.uri())))
        .mount(&server)
        .await;
    let store = Arc::new(MemoryTokenStore::new());
    store.save(key(), TokenSet::bearer("k-1")).await.unwrap();
    let spec = key_spec(
        &server,
        KeyPlacement::Header {
            name: "X-Api-Key".into(),
            prefix: None,
        },
    );
    let lenient = reqwest::Client::builder().redirect(reqwest::redirect::Policy::limited(5));
    let socket = Socket::builder(store)
        .provider(spec)
        .http_client(lenient)
        .build()
        .unwrap();
    assert_eq!(
        socket
            .request(key(), RawRequest::get("moved"))
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::Unexpected
    );
    assert!(elsewhere.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_throttled_request_is_retried_after_the_delay_the_provider_asks_for_even_a_post() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/send"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/send"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount(&server)
        .await;
    let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), TokenSet::bearer("t")).await;

    let started = std::time::Instant::now();
    let response = socket
        .request(key(), RawRequest::post("send", json!({})))
        .await
        .unwrap();
    assert_eq!(response.body["ok"], true);
    assert!(
        started.elapsed() >= Duration::from_millis(950),
        "it waited the second it was asked to"
    );
    assert_eq!(hits(&server, "/api/send").await, 2);
}

#[tokio::test]
async fn a_wait_longer_than_the_policy_allows_is_returned_to_the_caller_not_slept() {
    let server = MockServer::start().await;
    Mock::given(path("/api/slow"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "3600"))
        .mount(&server)
        .await;
    let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), TokenSet::bearer("t")).await;
    let err = socket.request(key(), RawRequest::get("slow")).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimited);
    assert_eq!(err.retry(), Retry::After(Duration::from_secs(3600)));
    assert_eq!(hits(&server, "/api/slow").await, 1);
}

#[tokio::test]
async fn a_server_error_is_retried_for_a_read_and_never_for_a_write() {
    let server = MockServer::start().await;
    Mock::given(path("/api/flaky"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), TokenSet::bearer("t")).await;

    let read = socket.request(key(), RawRequest::get("flaky")).await.unwrap_err();
    assert_eq!((read.kind(), read.retry()), (ErrorKind::Unexpected, Retry::Later));
    assert_eq!(hits(&server, "/api/flaky").await, 3, "three attempts is the policy");

    let write = socket
        .request(key(), RawRequest::post("flaky", json!({})))
        .await
        .unwrap_err();
    assert_eq!(write.kind(), ErrorKind::Unexpected);
    assert_eq!(
        hits(&server, "/api/flaky").await,
        4,
        "a write that may have happened is not repeated"
    );
}

#[tokio::test]
async fn provider_answers_map_to_the_error_a_caller_can_act_on() {
    let server = MockServer::start().await;
    Mock::given(path("/api/401"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({ "message": "bad token tok-1" })))
        .mount(&server)
        .await;
    Mock::given(path("/api/403"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({ "message": "org policy" })))
        .mount(&server)
        .await;
    Mock::given(path("/api/404"))
        .respond_with(ResponseTemplate::new(404).set_body_string("<html>nope</html>"))
        .mount(&server)
        .await;
    Mock::given(path("/api/html"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>login</html>"))
        .mount(&server)
        .await;
    let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), TokenSet::bearer("tok-1")).await;

    let get = |route: &'static str| socket.request(key(), RawRequest::get(route));
    let rejected = get("401").await.unwrap_err();
    assert_eq!(rejected.kind(), ErrorKind::ReconnectRequired);
    assert!(
        !rejected.message().contains("tok-1"),
        "a rejected-token message never repeats the body"
    );
    let denied = get("403").await.unwrap_err();
    assert_eq!(
        (denied.kind(), denied.message()),
        (ErrorKind::AccessDenied, "acme denied the request: org policy")
    );
    assert_eq!(get("404").await.unwrap_err().kind(), ErrorKind::NotFound);
    assert_eq!(
        get("html").await.unwrap_err().kind(),
        ErrorKind::Decode,
        "a 200 that is not JSON is not a success"
    );
}

#[tokio::test]
async fn an_unreachable_provider_is_a_transport_error_that_hides_the_api_key() {
    // A port nothing listens on: bind one, note it, and close it again.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let spec = ProviderSpec {
        id: ProviderId::new("acme").unwrap(),
        display_name: "Acme".into(),
        api_base: format!("http://127.0.0.1:{port}/api").parse().unwrap(),
        allowed_hosts: vec![format!("127.0.0.1:{port}")],
        auth: AuthScheme::ApiKey(ApiKeySpec {
            placement: KeyPlacement::Query { name: "api_key".into() },
        }),
    };
    let (socket, _) = connected(spec, TokenSet::bearer("k-secret")).await;
    let err = socket.request(key(), RawRequest::get("x")).await.unwrap_err();
    assert_eq!((err.kind(), err.retry()), (ErrorKind::Transport, Retry::Later));
    let shown = format!("{err:?} {err}");
    assert!(!shown.contains("k-secret"), "{shown}");
}

#[tokio::test]
async fn an_unregistered_provider_and_a_bad_method_are_refused() {
    let server = MockServer::start().await;
    let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), TokenSet::bearer("t")).await;
    let other = ConnectionKey::new(ProviderId::new("nowhere").unwrap(), "tenant-1");
    assert_eq!(
        socket.request(other, RawRequest::get("x")).await.unwrap_err().kind(),
        ErrorKind::Unsupported
    );
    let bad = socket.request(key(), RawRequest::new("GE T", "x")).await.unwrap_err();
    assert_eq!(bad.kind(), ErrorKind::InvalidInput);
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// An integration whose one operation calls the provider through its connection.
struct Profile(ProviderSpec);

#[async_trait]
impl Integration for Profile {
    fn provider(&self) -> ProviderSpec {
        self.0.clone()
    }
    fn operations(&self) -> Vec<OperationInfo> {
        vec![OperationInfo {
            name: "acme.profile.get".into(),
            description: "Fetch the profile.".into(),
            input_schema: json!({ "type": "object" }),
            output_schema: json!({ "type": "object" }),
            effect: Effect::Read,
            required_scopes: Vec::new(),
        }]
    }
    async fn invoke(&self, connection: Connection, _operation: String, _input: Value) -> Result<Value> {
        Ok(connection.request(RawRequest::get("profile")).await?.body)
    }
}

#[tokio::test]
async fn an_integration_reaches_its_provider_through_the_connection_it_is_given() {
    let server = MockServer::start().await;
    Mock::given(path("/api/profile"))
        .and(header("authorization", "Bearer t-9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "name": "Ada" })))
        .mount(&server)
        .await;
    let store = Arc::new(MemoryTokenStore::new());
    store.save(key(), TokenSet::bearer("t-9")).await.unwrap();
    let spec = oauth_spec(&server, ClientAuth::Body, false);
    let socket = Socket::builder(store)
        .integration(Arc::new(Profile(spec)))
        .build()
        .unwrap();
    let out = socket
        .invoke(key(), "acme.profile.get".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(out["name"], "Ada");
}

// ── Connecting ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn connecting_exchanges_the_code_with_pkce_and_saves_the_tokens() {
    let server = MockServer::start().await;
    let store = Arc::new(MemoryTokenStore::new());
    let socket = builder(store.clone(), oauth_spec(&server, ClientAuth::Body, true))
        .build()
        .unwrap();

    let authorization = socket.begin_authorization(key(), None).unwrap();
    let query: std::collections::HashMap<String, String> = authorization.url.query_pairs().into_owned().collect();
    assert!(
        authorization
            .url
            .as_str()
            .starts_with(&format!("{}/authorize?", server.uri()))
    );
    assert_eq!(query["client_id"], "client-id");
    assert_eq!(query["scope"], "read write");
    assert_eq!(query["state"], authorization.pending.state);
    assert_eq!(query["code_challenge_method"], "S256");
    let verifier = authorization
        .pending
        .pkce_verifier
        .clone()
        .expect("PKCE is on for this provider");
    assert!(
        !authorization.url.as_str().contains(verifier.expose()),
        "the verifier never goes in the URL"
    );

    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains(format!("code_verifier={}", verifier.expose())))
        .and(body_string_contains("redirect_uri=https%3A%2F%2Fapp.example.test%2Fcallback"))
        .and(body_string_contains("client_id=client-id"))
        .and(body_string_contains("client_secret=client-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "access_token": "new-access", "refresh_token": "new-refresh", "expires_in": 3600, "scope": "read write" }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let state = authorization.pending.state.clone();
    let tokens = socket
        .complete_authorization(authorization.pending, "the-code".into(), state)
        .await
        .unwrap();
    assert_eq!(tokens.access_token.expose(), "new-access");
    assert_eq!(tokens.scopes, ["read", "write"]);
    assert_eq!(
        store.load(key()).await.unwrap(),
        Some(tokens),
        "the tokens are in the application's store"
    );
}

#[tokio::test]
async fn requested_scopes_replace_the_defaults() {
    let server = MockServer::start().await;
    let socket = builder(
        Arc::new(MemoryTokenStore::new()),
        oauth_spec(&server, ClientAuth::Body, false),
    )
    .build()
    .unwrap();
    let authorization = socket.begin_authorization(key(), Some(vec!["admin".into()])).unwrap();
    assert!(
        authorization
            .url
            .query_pairs()
            .any(|(n, v)| n == "scope" && v == "admin")
    );
    assert!(authorization.pending.pkce_verifier.is_none());
    assert!(!authorization.url.query_pairs().any(|(n, _)| n == "code_challenge"));
}

#[tokio::test]
async fn basic_client_auth_keeps_the_secret_out_of_the_form() {
    let server = MockServer::start().await;
    // base64("client-id:client-secret")
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(header("authorization", "Basic Y2xpZW50LWlkOmNsaWVudC1zZWNyZXQ="))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token": "a" })))
        .expect(1)
        .mount(&server)
        .await;
    let socket = builder(
        Arc::new(MemoryTokenStore::new()),
        oauth_spec(&server, ClientAuth::Basic, false),
    )
    .build()
    .unwrap();
    let authorization = socket.begin_authorization(key(), None).unwrap();
    let state = authorization.pending.state.clone();
    socket
        .complete_authorization(authorization.pending, "c".into(), state)
        .await
        .unwrap();
    let body = String::from_utf8(server.received_requests().await.unwrap()[0].body.clone()).unwrap();
    assert!(
        !body.contains("client_secret") && !body.contains("client-secret"),
        "{body}"
    );
}

#[tokio::test]
async fn a_callback_with_the_wrong_state_never_reaches_the_token_endpoint() {
    let server = MockServer::start().await;
    let store = Arc::new(MemoryTokenStore::new());
    let socket = builder(store.clone(), oauth_spec(&server, ClientAuth::Body, false))
        .build()
        .unwrap();
    let ours = socket.begin_authorization(key(), None).unwrap();
    let someone_elses = socket.begin_authorization(key(), None).unwrap();

    for state in [
        someone_elses.pending.state.clone(),
        String::new(),
        format!("{}x", ours.pending.state),
    ] {
        let err = socket
            .complete_authorization(ours.pending.clone(), "code".into(), state)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }
    let no_code = socket
        .complete_authorization(ours.pending.clone(), String::new(), ours.pending.state.clone())
        .await;
    assert_eq!(no_code.unwrap_err().kind(), ErrorKind::InvalidInput);
    assert!(server.received_requests().await.unwrap().is_empty());
    assert_eq!(store.load(key()).await.unwrap(), None);
}

#[tokio::test]
async fn a_refused_code_is_reported_and_nothing_is_saved() {
    for (status, body) in [
        (200, json!({ "error": "bad_verification_code" })),
        (200, json!({ "ok": false, "error": "invalid_code" })),
        (400, json!({ "error": "invalid_grant" })),
    ] {
        let server = MockServer::start().await;
        Mock::given(path("/token"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body.clone()))
            .mount(&server)
            .await;
        let store = Arc::new(MemoryTokenStore::new());
        let socket = builder(store.clone(), oauth_spec(&server, ClientAuth::Body, false))
            .build()
            .unwrap();
        let authorization = socket.begin_authorization(key(), None).unwrap();
        let state = authorization.pending.state.clone();
        let err = socket
            .complete_authorization(authorization.pending, "used-code".into(), state)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{body}");
        assert!(
            err.message().contains(body["error"].as_str().unwrap()),
            "{}",
            err.message()
        );
        assert_eq!(store.load(key()).await.unwrap(), None);
    }
}

#[tokio::test]
async fn connecting_without_a_client_or_a_state_secret_is_a_configuration_error() {
    let server = MockServer::start().await;
    let spec = oauth_spec(&server, ClientAuth::Body, false);
    let store = || Arc::new(MemoryTokenStore::new());
    let acme = || ProviderId::new("acme").unwrap();

    let no_client = Socket::builder(store())
        .provider(spec.clone())
        .state_secret(STATE_SECRET)
        .build()
        .unwrap();
    assert_eq!(
        no_client.begin_authorization(key(), None).unwrap_err().kind(),
        ErrorKind::Config
    );
    let no_secret = Socket::builder(store())
        .provider(spec.clone())
        .oauth_client(acme(), client())
        .build()
        .unwrap();
    assert_eq!(
        no_secret.begin_authorization(key(), None).unwrap_err().kind(),
        ErrorKind::Config
    );

    let short = Socket::builder(store())
        .provider(spec.clone())
        .state_secret(b"too-short".to_vec())
        .build();
    assert_eq!(short.unwrap_err().kind(), ErrorKind::Config);
    let blank = OAuthClient {
        client_secret: SecretString::new("  "),
        ..client()
    };
    assert_eq!(
        Socket::builder(store())
            .provider(spec.clone())
            .oauth_client(acme(), blank)
            .build()
            .unwrap_err()
            .kind(),
        ErrorKind::Config
    );
    let stray = Socket::builder(store()).oauth_client(acme(), client()).build();
    assert_eq!(
        stray.unwrap_err().kind(),
        ErrorKind::Config,
        "a client for a provider that is not registered"
    );

    let api_key = Socket::builder(store())
        .provider(key_spec(&server, KeyPlacement::Basic {}))
        .state_secret(STATE_SECRET)
        .build()
        .unwrap();
    assert_eq!(
        api_key.begin_authorization(key(), None).unwrap_err().kind(),
        ErrorKind::Config
    );
}

// ── Refreshing ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_expired_token_is_refreshed_saved_and_used_for_the_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=old-refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token": "fresh", "expires_in": 3600 })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/me"))
        .and(header("authorization", "Bearer fresh"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&server)
        .await;
    let (socket, store) = connected(
        oauth_spec(&server, ClientAuth::Body, false),
        expired(Some("old-refresh")),
    )
    .await;

    socket.request(key(), RawRequest::get("me")).await.unwrap();
    let saved = store.load(key()).await.unwrap().unwrap();
    assert_eq!(saved.access_token.expose(), "fresh");
    assert_eq!(
        saved.refresh_token.as_ref().map(SecretString::expose),
        Some("old-refresh"),
        "kept when not rotated"
    );
    assert_eq!(saved.scopes, ["read"], "kept when the provider does not repeat them");
    assert!(saved.expires_at.unwrap() > SystemTime::now() + Duration::from_secs(3000));

    // The saved token is still valid, so the second call does not refresh again.
    socket.request(key(), RawRequest::get("me")).await.unwrap();
}

#[tokio::test]
async fn a_rotated_refresh_token_replaces_the_old_one() {
    let server = MockServer::start().await;
    Mock::given(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "access_token": "fresh", "refresh_token": "rotated", "expires_in": 60, "scope": "read admin" }),
        ))
        .mount(&server)
        .await;
    Mock::given(path("/api/me"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let (socket, store) = connected(
        oauth_spec(&server, ClientAuth::Body, false),
        expired(Some("old-refresh")),
    )
    .await;
    socket.request(key(), RawRequest::get("me")).await.unwrap();
    let saved = store.load(key()).await.unwrap().unwrap();
    assert_eq!(saved.refresh_token.as_ref().map(SecretString::expose), Some("rotated"));
    assert_eq!(saved.scopes, ["read", "admin"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_calls_with_an_expired_token_cause_exactly_one_refresh() {
    let server = MockServer::start().await;
    Mock::given(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(150))
                .set_body_json(json!({ "access_token": "fresh", "refresh_token": "rotated", "expires_in": 3600 })),
        )
        .mount(&server)
        .await;
    Mock::given(path("/api/me"))
        .and(header("authorization", "Bearer fresh"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let (socket, _) = connected(
        oauth_spec(&server, ClientAuth::Body, false),
        expired(Some("old-refresh")),
    )
    .await;
    let socket = Arc::new(socket);

    let calls: Vec<_> = (0..12)
        .map(|_| {
            let socket = Arc::clone(&socket);
            tokio::spawn(async move { socket.request(key(), RawRequest::get("me")).await })
        })
        .collect();
    for call in calls {
        call.await.unwrap().unwrap();
    }
    assert_eq!(
        hits(&server, "/token").await,
        1,
        "a second refresh would spend a token that was already rotated"
    );
    assert_eq!(hits(&server, "/api/me").await, 12);
}

#[tokio::test]
async fn a_refused_refresh_means_reconnect_and_a_failing_token_endpoint_means_try_later() {
    for (status, body, kind, retry) in [
        (
            400,
            json!({ "error": "invalid_grant" }),
            ErrorKind::ReconnectRequired,
            Retry::Never,
        ),
        (
            200,
            json!({ "error": "bad_refresh_token" }),
            ErrorKind::ReconnectRequired,
            Retry::Never,
        ),
        (503, Value::Null, ErrorKind::Unexpected, Retry::Later),
    ] {
        let server = MockServer::start().await;
        Mock::given(path("/token"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(&server)
            .await;
        let (socket, store) = connected(
            oauth_spec(&server, ClientAuth::Body, false),
            expired(Some("old-refresh")),
        )
        .await;
        let err = socket.request(key(), RawRequest::get("me")).await.unwrap_err();
        assert_eq!((err.kind(), err.retry()), (kind, retry), "HTTP {status}");
        assert_eq!(
            hits(&server, "/api/me").await,
            0,
            "the API is not called with a token known to be expired"
        );
        assert_eq!(
            hits(&server, "/token").await,
            1,
            "a refresh is never retried: the token may be single-use"
        );
        assert_eq!(
            store.load(key()).await.unwrap().unwrap().access_token.expose(),
            "old-access"
        );
    }
}

#[tokio::test]
async fn an_expired_token_without_a_refresh_token_or_an_empty_token_requires_reconnecting() {
    let server = MockServer::start().await;
    for tokens in [expired(None), TokenSet::bearer("")] {
        let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), tokens).await;
        let err = socket.request(key(), RawRequest::get("me")).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ReconnectRequired);
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn refreshing_without_an_oauth_client_is_a_configuration_error() {
    let server = MockServer::start().await;
    let store = Arc::new(MemoryTokenStore::new());
    store.save(key(), expired(Some("r"))).await.unwrap();
    let socket = Socket::builder(store)
        .provider(oauth_spec(&server, ClientAuth::Body, false))
        .build()
        .unwrap();
    assert_eq!(
        socket.request(key(), RawRequest::get("me")).await.unwrap_err().kind(),
        ErrorKind::Config
    );
}

#[tokio::test]
async fn credentials_embedded_in_a_url_are_refused() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let host = host_entry(&server);
    let specs = [
        oauth_spec(&server, ClientAuth::Body, false),
        key_spec(
            &server,
            KeyPlacement::Header {
                name: "X-Api-Key".into(),
                prefix: None,
            },
        ),
        key_spec(&server, KeyPlacement::Query { name: "api_key".into() }),
        key_spec(&server, KeyPlacement::Basic {}),
    ];
    for spec in specs {
        let (socket, _) = connected(spec, TokenSet::bearer("real")).await;
        for target in [
            format!("http://forged:pw@{host}/api/x"),
            format!("http://forged@{host}/api/x"),
        ] {
            let err = socket.request(key(), RawRequest::get(target)).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput);
            assert!(
                !err.message().contains("forged"),
                "the refused credentials are not echoed: {}",
                err.message()
            );
        }
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "a URL carrying its own credentials is never sent"
    );
}

#[tokio::test]
async fn the_api_key_parameter_cannot_be_shadowed_by_a_differently_written_name() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let spec = key_spec(&server, KeyPlacement::Query { name: "api_key".into() });
    let (socket, _) = connected(spec, TokenSet::bearer("real")).await;

    let shadowing = [
        RawRequest::get("x").with_query("API_KEY", "forged"),
        RawRequest::get("x").with_query("Api_Key", "forged"),
        RawRequest::get("x").with_query(" api_key ", "forged"),
        RawRequest::get("x?API_KEY=forged"),
        RawRequest::get("x?%61pi_key=forged"),
        RawRequest::get("x?api%5Fkey=forged"),
    ];
    for request in shadowing {
        let shown = format!("{request:?}");
        let err = socket.request(key(), request).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{shown}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // An unrelated parameter is still fine, and the real key is the only one sent.
    socket
        .request(key(), RawRequest::get("x").with_query("page", "2"))
        .await
        .unwrap();
    let received = server.received_requests().await.unwrap();
    let keys: Vec<String> = received[0]
        .url
        .query_pairs()
        .filter(|(n, _)| n.eq_ignore_ascii_case("api_key"))
        .map(|(_, v)| v.into_owned())
        .collect();
    assert_eq!(keys, ["real"]);
}

// ── Findings from the branch review ───────────────────────────────────────────

/// A store whose first `save` fails, as a database blip would.
struct FailsFirstSave {
    inner: MemoryTokenStore,
    failed: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl TokenStore for FailsFirstSave {
    async fn load(&self, key: ConnectionKey) -> Result<Option<TokenSet>> {
        self.inner.load(key).await
    }
    async fn save(&self, key: ConnectionKey, tokens: TokenSet) -> Result<()> {
        if !self.failed.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err(socketkit_core::Error::new(ErrorKind::Unexpected, "store is down"));
        }
        self.inner.save(key, tokens).await
    }
    async fn delete(&self, key: ConnectionKey) -> Result<()> {
        self.inner.delete(key).await
    }
}

fn refresh_tokens_sent(requests: &[wiremock::Request]) -> Vec<String> {
    requests
        .iter()
        .filter(|r| r.url.path() == "/token")
        .map(|r| {
            url::form_urlencoded::parse(&r.body)
                .find(|(n, _)| n == "refresh_token")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default()
        })
        .collect()
}

#[tokio::test]
async fn a_rotated_refresh_token_survives_a_failed_save() {
    let server = MockServer::start().await;
    Mock::given(path("/token"))
        .and(body_string_contains("refresh_token=R1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "access_token": "A2", "refresh_token": "R2", "expires_in": 3600 })),
        )
        .mount(&server)
        .await;
    Mock::given(path("/api/me"))
        .and(header("authorization", "Bearer A2"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let store = Arc::new(FailsFirstSave {
        inner: MemoryTokenStore::new(),
        failed: true.into(),
    });
    store.save(key(), expired(Some("R1"))).await.unwrap();
    store.failed.store(false, std::sync::atomic::Ordering::SeqCst);
    let socket = Socket::builder(store.clone())
        .provider(oauth_spec(&server, ClientAuth::Body, false))
        .oauth_client(ProviderId::new("acme").unwrap(), client())
        .retry(fast_retry())
        .build()
        .unwrap();

    let first = socket.request(key(), RawRequest::get("me")).await.unwrap_err();
    assert_eq!(first.message(), "store is down", "the failure is reported, not hidden");

    socket.request(key(), RawRequest::get("me")).await.unwrap();
    let saved = store.load(key()).await.unwrap().unwrap();
    assert_eq!(
        saved.refresh_token.as_ref().map(SecretString::expose),
        Some("R2"),
        "the rotated token reached the store"
    );
    assert_eq!(
        refresh_tokens_sent(&server.received_requests().await.unwrap()),
        ["R1"],
        "R1 was spent exactly once"
    );
}

#[tokio::test]
async fn a_connection_replaced_in_the_store_is_never_served_the_old_accounts_token() {
    let server = MockServer::start().await;
    Mock::given(path("/token"))
        .and(body_string_contains("refresh_token=RA"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token": "A-fresh" })))
        .mount(&server)
        .await;
    Mock::given(path("/token"))
        .and(body_string_contains("refresh_token=RB"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "access_token": "B-fresh", "expires_in": 3600 })),
        )
        .mount(&server)
        .await;
    Mock::given(path("/api/me"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let (socket, store) = connected(oauth_spec(&server, ClientAuth::Body, false), expired(Some("RA"))).await;
    socket.request(key(), RawRequest::get("me")).await.unwrap();

    // The person reconnects as a different account; the application stores the new tokens.
    store.delete(key()).await.unwrap();
    store.save(key(), expired(Some("RB"))).await.unwrap();
    socket.request(key(), RawRequest::get("me")).await.unwrap();

    let received = server.received_requests().await.unwrap();
    let last_api_call = received.iter().rev().find(|r| r.url.path() == "/api/me").unwrap();
    assert_eq!(last_api_call.headers.get("authorization").unwrap(), "Bearer B-fresh");
    assert_eq!(
        store.load(key()).await.unwrap().unwrap().access_token.expose(),
        "B-fresh"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn short_lived_tokens_never_cause_a_refresh_token_to_be_spent_twice() {
    let server = MockServer::start().await;
    let issued = Arc::new(std::sync::atomic::AtomicUsize::new(1));
    let counter = issued.clone();
    // Each refresh rotates the token and returns one that is already inside the expiry skew.
    Mock::given(path("/token"))
        .respond_with(move |_: &wiremock::Request| {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(30))
                .set_body_json(
                    json!({ "access_token": format!("A{n}"), "refresh_token": format!("R{n}"), "expires_in": 30 }),
                )
        })
        .mount(&server)
        .await;
    Mock::given(path("/api/me"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), expired(Some("R1"))).await;
    let socket = Arc::new(socket);

    let calls: Vec<_> = (0..6)
        .map(|_| {
            let socket = Arc::clone(&socket);
            tokio::spawn(async move { socket.request(key(), RawRequest::get("me")).await })
        })
        .collect();
    for call in calls {
        call.await.unwrap().expect("every caller gets a working token");
    }
    let mut spent = refresh_tokens_sent(&server.received_requests().await.unwrap());
    let total = spent.len();
    spent.sort();
    spent.dedup();
    assert_eq!(
        spent.len(),
        total,
        "a provider with reuse detection would revoke the grant"
    );
}

#[tokio::test]
async fn a_provider_that_never_answers_is_a_transport_error_not_a_hang() {
    let server = MockServer::start().await;
    Mock::given(path("/api/slow"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(20)))
        .mount(&server)
        .await;
    let store = Arc::new(MemoryTokenStore::new());
    store.save(key(), TokenSet::bearer("t")).await.unwrap();
    let quick = reqwest::Client::builder().timeout(Duration::from_millis(200));
    let no_retry = RetryPolicy {
        max_attempts: 1,
        ..fast_retry()
    };
    let socket = Socket::builder(store)
        .provider(oauth_spec(&server, ClientAuth::Body, false))
        .http_client(quick)
        .retry(no_retry)
        .build()
        .unwrap();
    let started = std::time::Instant::now();
    let err = socket.request(key(), RawRequest::get("slow")).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Transport);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn an_oversized_response_is_refused_instead_of_filling_memory() {
    let server = MockServer::start().await;
    let huge = format!("\"{}\"", "x".repeat(11 * 1024 * 1024));
    Mock::given(path("/api/huge"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(huge, "application/json"))
        .mount(&server)
        .await;
    let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), TokenSet::bearer("t")).await;
    let err = socket.request(key(), RawRequest::get("huge")).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
    assert!(err.message().contains("too large"), "{}", err.message());
}

#[tokio::test]
async fn a_provider_message_that_echoes_the_credential_is_redacted_and_kept_short() {
    let server = MockServer::start().await;
    let echo = format!("bad request to /x?api_key=k-SECRET {}", "y".repeat(5000));
    Mock::given(path("/api/x"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({ "message": echo })))
        .mount(&server)
        .await;
    Mock::given(path("/api/y"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({ "message": "token k-SECRET lacks scope" })))
        .mount(&server)
        .await;
    let spec = key_spec(&server, KeyPlacement::Query { name: "api_key".into() });
    let (socket, _) = connected(spec, TokenSet::bearer("k-SECRET")).await;
    for route in ["x", "y"] {
        let err = socket.request(key(), RawRequest::get(route)).await.unwrap_err();
        let everything = format!("{err} {err:?} {}", serde_json::to_string(&err.to_wire()).unwrap());
        assert!(!everything.contains("k-SECRET"), "{everything}");
        assert!(err.message().chars().count() < 600, "{}", err.message().len());
    }
}

#[tokio::test]
async fn header_names_that_servers_treat_alike_are_reserved_too_and_content_type_is_sent_once() {
    let server = MockServer::start().await;
    Mock::given(path("/api/x"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let spec = key_spec(
        &server,
        KeyPlacement::Header {
            name: "X-Api-Key".into(),
            prefix: None,
        },
    );
    let (keyed, _) = connected(spec, TokenSet::bearer("real")).await;
    for name in ["X_Api_Key", "x_api-key", "Cookie", "Connection", "Upgrade"] {
        let err = keyed
            .request(key(), RawRequest::get("x").with_header(name, "v"))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    let request = RawRequest::post("x", json!({ "a": 1 })).with_header("Content-Type", "application/vnd.acme+json");
    keyed.request(key(), request).await.unwrap();
    let received = server.received_requests().await.unwrap();
    let types: Vec<_> = received[0]
        .headers
        .get_all("content-type")
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect();
    assert_eq!(types, ["application/vnd.acme+json"]);
}

#[tokio::test]
async fn idempotent_writes_are_retried_on_a_server_error() {
    let server = MockServer::start().await;
    Mock::given(path("/api/item"))
        .respond_with(ResponseTemplate::new(502))
        .mount(&server)
        .await;
    let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), TokenSet::bearer("t")).await;
    for (n, verb) in ["PUT", "DELETE"].into_iter().enumerate() {
        socket.request(key(), RawRequest::new(verb, "item")).await.unwrap_err();
        assert_eq!(
            hits(&server, "/api/item").await,
            3 * (n + 1),
            "{verb} is safe to repeat"
        );
    }
}

#[tokio::test]
async fn a_throttled_or_misconfigured_token_endpoint_does_not_tell_the_person_to_reconnect() {
    for (status, body, kind) in [
        (429, json!({}), ErrorKind::RateLimited),
        (401, json!({ "error": "invalid_client" }), ErrorKind::Config),
        (400, json!({ "error": "invalid_client" }), ErrorKind::Config),
        (400, json!({ "error": "invalid_grant" }), ErrorKind::ReconnectRequired),
    ] {
        let server = MockServer::start().await;
        Mock::given(path("/token"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body.clone()))
            .mount(&server)
            .await;
        let (socket, _) = connected(oauth_spec(&server, ClientAuth::Body, false), expired(Some("R1"))).await;
        let err = socket.request(key(), RawRequest::get("me")).await.unwrap_err();
        assert_eq!(err.kind(), kind, "refresh: HTTP {status} {body}");

        if kind != ErrorKind::ReconnectRequired {
            let authorization = socket.begin_authorization(key(), None).unwrap();
            let state = authorization.pending.state.clone();
            let err = socket
                .complete_authorization(authorization.pending, "code".into(), state)
                .await
                .unwrap_err();
            assert_eq!(err.kind(), kind, "exchange: HTTP {status} {body}");
        }
    }
}

#[tokio::test]
async fn a_pending_record_signed_by_another_application_is_refused_at_the_callback() {
    let server = MockServer::start().await;
    let spec = oauth_spec(&server, ClientAuth::Body, false);
    let acme = || ProviderId::new("acme").unwrap();
    let ours = builder(Arc::new(MemoryTokenStore::new()), spec.clone())
        .build()
        .unwrap();
    let theirs = Socket::builder(Arc::new(MemoryTokenStore::new()))
        .provider(spec)
        .oauth_client(acme(), client())
        .state_secret(b"another-application-another-secret".to_vec())
        .build()
        .unwrap();
    // The state matches the record it came with, so only the signature check can refuse it.
    let forged = theirs.begin_authorization(key(), None).unwrap().pending;
    let state = forged.state.clone();
    let err = ours
        .complete_authorization(forged, "code".into(), state)
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(server.received_requests().await.unwrap().is_empty());
}
