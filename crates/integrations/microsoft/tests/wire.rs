//! Microsoft against a local server that answers as Microsoft Graph and its sign-in service do.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use socketkit_core::{
    AuthScheme, ConnectionKey, ErrorKind, Integration, MemoryTokenStore, OAuthClient, ProviderSpec, Retry,
    SecretString, Socket, TokenStore,
};
use socketkit_microsoft::{Microsoft, MicrosoftOAuth, provider};
use socketkit_testkit::wiremock::matchers::{body_string_contains, header, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{conformance, connect, point_at};

fn build(spec: ProviderSpec) -> Arc<dyn Integration> {
    Arc::new(Microsoft::with_spec(spec))
}

async fn microsoft() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let (socket, key) = connect(build(point_at(provider(), &server)), "eyJ-good").await;
    (server, socket, key)
}

fn ok(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn client() -> OAuthClient {
    OAuthClient {
        client_id: "app-id".into(),
        client_secret: SecretString::new("app-secret"),
        redirect_uri: "https://app.example.test/callback".parse().unwrap(),
    }
}

fn settings() -> MicrosoftOAuth {
    client().into()
}

fn authorize_url(microsoft: Microsoft) -> url::Url {
    let socket = Socket::in_memory().integration(Arc::new(microsoft)).build().unwrap();
    let key = ConnectionKey::new(provider().id, "user-42");
    socket.begin_authorization(key, None).unwrap().url
}

async fn transcripts(socket: &Socket, key: &ConnectionKey) -> socketkit_core::Error {
    socket
        .invoke(
            key.clone(),
            "microsoft.transcripts.list".into(),
            json!({ "meeting": "m-1" }),
        )
        .await
        .unwrap_err()
}

#[tokio::test]
async fn passes_the_conformance_checks_for_a_provider_and_its_identity() {
    // `microsoft.resource.resolve` is not built yet, so the lookup check is not run.
    let real = provider();
    conformance::definition_is_sound(&real, &build);
    conformance::a_rejected_token_requires_reconnect(&real, &build).await;
    conformance::throttling_is_reported_with_the_wait(&real, &build).await;
    conformance::an_empty_success_is_not_an_account(&real, &build).await;
}

#[test]
fn the_definition_is_graph_v1_with_the_common_tenant_and_only_the_scopes_every_app_needs() {
    let spec = provider();
    assert_eq!(spec.api_base.as_str(), "https://graph.microsoft.com/v1.0/");
    assert_eq!(spec.allowed_hosts, ["graph.microsoft.com", "login.microsoftonline.com"]);
    let AuthScheme::OAuth2(oauth) = spec.auth else {
        panic!("Microsoft signs in with OAuth")
    };
    assert_eq!(
        oauth.authorize_url.as_str(),
        "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
    );
    assert_eq!(
        oauth.token_url.as_str(),
        "https://login.microsoftonline.com/common/oauth2/v2.0/token"
    );
    // Without `offline_access` Microsoft issues no refresh token.
    assert_eq!(oauth.default_scopes, ["offline_access", "User.Read"]);
    assert_eq!(oauth.scope_separator, " ");
    assert!(oauth.pkce);
}

#[tokio::test]
async fn identity_returns_the_person_behind_the_token() {
    let (server, socket, key) = microsoft().await;
    Mock::given(method("GET"))
        .and(path("/v1.0/me"))
        .and(header("authorization", "Bearer eyJ-good"))
        .respond_with(ok(json!({
            "id": "87d349ed-44d7-43e1-9a83-5f2406dee5bd",
            "displayName": "Ada Lovelace",
            "mail": "ada@example.test",
            "userPrincipalName": "ada@example.onmicrosoft.com"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let account = socket
        .invoke(key, "microsoft.identity.get".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(
        account,
        json!({ "id": "87d349ed-44d7-43e1-9a83-5f2406dee5bd", "name": "Ada Lovelace", "email": "ada@example.test" })
    );
}

#[tokio::test]
async fn an_account_without_a_mailbox_or_a_name_is_named_by_its_sign_in_name() {
    let (server, socket, key) = microsoft().await;
    Mock::given(path("/v1.0/me"))
        .respond_with(ok(
            json!({ "id": "u-1", "displayName": null, "mail": null, "userPrincipalName": "ada@example.onmicrosoft.com" }),
        ))
        .mount(&server)
        .await;
    let account = socket
        .invoke(key, "microsoft.identity.get".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(
        account,
        json!({ "id": "u-1", "name": "ada@example.onmicrosoft.com", "email": "ada@example.onmicrosoft.com" })
    );
}

#[tokio::test]
async fn an_account_without_an_id_is_refused() {
    let (server, socket, key) = microsoft().await;
    Mock::given(path("/v1.0/me"))
        .respond_with(ok(json!({ "id": "", "displayName": "Ada Lovelace" })))
        .mount(&server)
        .await;
    let err = socket
        .invoke(key, "microsoft.identity.get".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
}

#[tokio::test]
async fn graph_errors_reach_the_caller_as_something_to_act_on() {
    let graph_error = |status: u16, code: &str, message: &str| {
        ResponseTemplate::new(status).set_body_json(json!({ "error": { "code": code, "message": message } }))
    };
    let after = |reply: ResponseTemplate| async move {
        let (server, socket, key) = microsoft().await;
        Mock::given(method("GET")).respond_with(reply).mount(&server).await;
        transcripts(&socket, &key).await
    };

    let expired = after(graph_error(
        401,
        "InvalidAuthenticationToken",
        "Lifetime validation failed",
    ))
    .await;
    assert_eq!(expired.kind(), ErrorKind::ReconnectRequired);

    // A permission the application was not granted: Graph's own words are the only place it is named.
    let forbidden = after(graph_error(
        403,
        "Forbidden",
        "Application is not allowed to perform this operation",
    ))
    .await;
    assert_eq!(forbidden.kind(), ErrorKind::AccessDenied);
    assert!(forbidden.message().contains("not allowed"), "{}", forbidden.message());

    let missing = after(graph_error(404, "NotFound", "Meeting not found")).await;
    assert_eq!(missing.kind(), ErrorKind::NotFound);

    let bad = after(graph_error(400, "BadRequest", "Invalid meeting id")).await;
    assert_eq!(bad.kind(), ErrorKind::InvalidInput);
    assert!(bad.message().contains("Invalid meeting id"), "{}", bad.message());
}

#[tokio::test]
async fn a_tenant_setting_that_blocks_transcripts_is_a_refusal_that_says_who_can_change_it() {
    let inner = |spelling: &str, code: &str| {
        ResponseTemplate::new(403).set_body_json(json!({
            "error": { "code": "Forbidden", "message": "subject to change", spelling: { "code": code } }
        }))
    };
    // Graph writes the inner error's name both ways.
    for spelling in ["innerError", "innererror"] {
        let (server, socket, key) = microsoft().await;
        Mock::given(method("GET"))
            .respond_with(inner(spelling, "GraphAccessToTranscriptsDisabled"))
            .mount(&server)
            .await;
        let off = transcripts(&socket, &key).await;
        assert_eq!(off.kind(), ErrorKind::AccessDenied, "{spelling}");
        assert!(off.message().contains("administrator"), "{}", off.message());
        assert!(off.message().contains("transcripts"), "{}", off.message());
    }

    let (server, socket, key) = microsoft().await;
    Mock::given(method("GET"))
        .respond_with(inner("innerError", "SpeakerAttributionNotAllowed"))
        .mount(&server)
        .await;
    let unattributed = socket
        .invoke(
            key,
            "microsoft.transcripts.content".into(),
            json!({ "meeting": "m-1", "transcript": "t-1" }),
        )
        .await
        .unwrap_err();
    assert_eq!(unattributed.kind(), ErrorKind::AccessDenied);
    assert!(unattributed.message().contains("speaker"), "{}", unattributed.message());
}

#[tokio::test]
async fn an_unavailable_graph_says_how_long_to_wait_and_a_read_is_tried_again() {
    let (server, socket, key) = microsoft().await;
    // Longer than the test's retry policy will sleep, so the wait reaches the caller.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503).insert_header("retry-after", "120"))
        .mount(&server)
        .await;
    let err = transcripts(&socket, &key).await;
    assert_eq!(err.kind(), ErrorKind::Unexpected);
    assert_eq!(err.retry(), Retry::After(Duration::from_secs(120)));

    let (server, socket, key) = microsoft().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    assert_eq!(transcripts(&socket, &key).await.retry(), Retry::Later);
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        2,
        "a read is sent again after a server error"
    );
}

#[tokio::test]
async fn microsoft_built_with_a_token_calls_graph_with_it_and_needs_no_stored_connection() {
    let server = MockServer::start().await;
    Mock::given(path("/v1.0/me"))
        .and(header("authorization", "Bearer eyJ-given"))
        .respond_with(ok(json!({ "id": "u-1", "displayName": "Ada" })))
        .expect(1)
        .mount(&server)
        .await;
    let microsoft = Microsoft::with_spec(point_at(provider(), &server)).token("eyJ-given");
    let socket = Socket::in_memory().integration(Arc::new(microsoft)).build().unwrap();
    let key = ConnectionKey::new(provider().id, "anyone");
    let account = socket
        .invoke(key, "microsoft.identity.get".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(account["id"], "u-1");
    assert!(Microsoft::with_token("eyJ-given").fixed_token().is_some());
}

#[test]
fn signing_in_goes_to_the_tenant_with_the_scopes_and_hints_the_application_set() {
    let plain = authorize_url(Microsoft::with_oauth(client()));
    assert_eq!(plain.host_str(), Some("login.microsoftonline.com"));
    assert_eq!(plain.path(), "/common/oauth2/v2.0/authorize");
    let query: HashMap<String, String> = plain.query_pairs().into_owned().collect();
    assert_eq!(query["client_id"], "app-id");
    assert_eq!(query["scope"], "offline_access User.Read");
    assert_eq!(query["code_challenge_method"], "S256");
    assert!(!query.contains_key("login_hint") && !query.contains_key("prompt"));
    assert!(!plain.as_str().contains("app-secret"));

    let tenant_id = "909c6581-5130-43e9-88f3-fcb3582cde37";
    let tuned = authorize_url(Microsoft::with_oauth(MicrosoftOAuth {
        scopes: Some(vec![
            "offline_access".into(),
            "OnlineMeetings.Read".into(),
            "OnlineMeetingTranscript.Read.All".into(),
        ]),
        tenant: Some(tenant_id.into()),
        login_hint: Some("ada@example.test".into()),
        prompt: Some("select_account".into()),
        ..settings()
    }));
    assert_eq!(tuned.path(), format!("/{tenant_id}/oauth2/v2.0/authorize"));
    let query: HashMap<String, String> = tuned.query_pairs().into_owned().collect();
    assert_eq!(
        query["scope"],
        "offline_access OnlineMeetings.Read OnlineMeetingTranscript.Read.All"
    );
    assert_eq!(query["login_hint"], "ada@example.test");
    assert_eq!(query["prompt"], "select_account");
}

#[test]
fn the_tenant_is_one_of_microsofts_words_an_id_or_a_domain_and_anything_else_is_refused() {
    for tenant in [
        "common",
        "organizations",
        "consumers",
        "contoso.onmicrosoft.com",
        " Contoso.com ",
    ] {
        let microsoft = Microsoft::with_oauth(MicrosoftOAuth {
            tenant: Some(tenant.into()),
            ..settings()
        });
        let AuthScheme::OAuth2(oauth) = microsoft.provider().auth else {
            panic!("Microsoft signs in with OAuth")
        };
        let expected = format!("/{}/oauth2/v2.0/token", tenant.trim());
        assert_eq!(
            oauth.token_url.path(),
            expected,
            "the token endpoint is the tenant's too"
        );
        microsoft.check().unwrap();
    }
    // Anything that could change where the client secret is sent.
    for tenant in [
        "",
        " ",
        ".",
        "..",
        "common/../evil",
        "contoso.com/",
        "contoso com",
        "contoso.com?x=1",
        "contoso.com#x",
        "-contoso.com",
        "contoso..com",
        "contoso.com.",
        "%2e%2e",
        "a@b",
    ] {
        let microsoft = Microsoft::with_oauth(MicrosoftOAuth {
            tenant: Some(tenant.into()),
            ..settings()
        });
        let err = Socket::in_memory()
            .integration(Arc::new(microsoft.clone()))
            .build()
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Config, "{tenant:?}");
        assert!(err.message().contains("tenant"), "{}", err.message());
        // The sign-in address is never built from a tenant that was refused.
        let AuthScheme::OAuth2(oauth) = microsoft.provider().auth else {
            panic!("Microsoft signs in with OAuth")
        };
        assert_eq!(oauth.token_url.path(), "/common/oauth2/v2.0/token", "{tenant:?}");
    }
}

#[tokio::test]
async fn connecting_exchanges_the_code_with_pkce_and_keeps_the_refresh_token() {
    let server = MockServer::start().await;
    let store = Arc::new(MemoryTokenStore::new());
    let microsoft = Microsoft::with_spec(point_at(provider(), &server)).oauth(client());
    let socket = Socket::builder(store.clone())
        .integration(Arc::new(microsoft))
        .state_secret(b"0123456789abcdef0123456789abcdef".to_vec())
        .build()
        .unwrap();
    let key = ConnectionKey::new(provider().id, "user-42");

    let authorization = socket.begin_authorization(key.clone(), None).unwrap();
    let verifier = authorization
        .pending
        .pkce_verifier
        .clone()
        .expect("PKCE is on for Microsoft");
    Mock::given(method("POST"))
        .and(path("/common/oauth2/v2.0/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains(format!("code_verifier={}", verifier.expose())))
        .and(body_string_contains("client_id=app-id"))
        .and(body_string_contains("client_secret=app-secret"))
        .respond_with(ok(json!({
            "token_type": "Bearer",
            "scope": "User.Read OnlineMeetings.Read",
            "expires_in": 3599,
            "access_token": "eyJ-access",
            "refresh_token": "0.AR-refresh"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let state = authorization.pending.state.clone();
    let tokens = socket
        .complete_authorization(authorization.pending, "the-code".into(), state)
        .await
        .unwrap();
    assert_eq!(tokens.access_token.expose(), "eyJ-access");
    assert_eq!(tokens.scopes, ["User.Read", "OnlineMeetings.Read"]);
    let stored = store.load(key).await.unwrap().expect("the tokens were saved");
    assert_eq!(stored.refresh_token.unwrap().expose(), "0.AR-refresh");
}
