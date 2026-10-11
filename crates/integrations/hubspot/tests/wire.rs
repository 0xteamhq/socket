//! HubSpot against a local server that answers as HubSpot's API and its token endpoint do.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use socketkit_core::{
    AuthScheme, ConnectionKey, ErrorKind, Integration, MemoryTokenStore, OAuthClient, ProviderSpec, Retry, RetryPolicy,
    SecretString, Socket, TokenSet, TokenStore,
};
use socketkit_hubspot::{HubSpot, HubSpotOAuth, provider};
use socketkit_testkit::wiremock::matchers::{any, body_string_contains, header, method, path, query_param};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{TENANT, conformance, connect, point_at};

mod support;
use support::{hubspot_error, rate_limited};

/// Where the account's own details are read.
const DETAILS: &str = "/account-info/2026-09/details";
const TOKEN: &str = "/oauth/2026-09/token";

fn build(spec: ProviderSpec) -> Arc<dyn Integration> {
    Arc::new(HubSpot::with_spec(spec))
}

async fn hubspot() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let (socket, key) = connect(build(point_at(provider(), &server)), "pat-na1-good").await;
    (server, socket, key)
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

/// An account's details as HubSpot returns them.
fn account() -> Value {
    json!({
        "portalId": 1_234_567,
        "portalName": "Analytical Engines",
        "accountType": "STANDARD",
        "timeZone": "Europe/London",
        "companyCurrency": "GBP",
        "additionalCurrencies": [],
        "utcOffset": "+01:00",
        "utcOffsetMilliseconds": 3_600_000,
        "uiDomain": "app.hubspot.com",
        "dataHostingLocation": "na1",
        "createdAt": 1_572_406_217_883_i64
    })
}

fn client() -> OAuthClient {
    OAuthClient {
        client_id: "client-id".into(),
        client_secret: SecretString::new("client-secret"),
        redirect_uri: "https://app.example.test/callback".parse().unwrap(),
    }
}

fn key() -> ConnectionKey {
    ConnectionKey::new(provider().id, TENANT)
}

/// A `Socket` whose HubSpot integration holds the application's own app and
/// exchanges tokens on `server`.
async fn with_oauth_app(server: &MockServer, stored: Option<TokenSet>) -> (Socket, Arc<MemoryTokenStore>) {
    let store = Arc::new(MemoryTokenStore::new());
    if let Some(tokens) = stored {
        store.save(key(), tokens).await.unwrap();
    }
    let hubspot = HubSpot::with_spec(point_at(provider(), server)).oauth(client());
    let retry = RetryPolicy {
        max_attempts: 2,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(50),
    };
    let socket = Socket::builder(store.clone())
        .integration(Arc::new(hubspot))
        .retry(retry)
        .build()
        .unwrap();
    (socket, store)
}

fn tokens(access: &str, refresh: &str, expires_in: i64) -> TokenSet {
    let now = SystemTime::now();
    let expires_at = if expires_in >= 0 {
        now + Duration::from_secs(expires_in.unsigned_abs())
    } else {
        now - Duration::from_secs(expires_in.unsigned_abs())
    };
    TokenSet {
        access_token: SecretString::new(access),
        refresh_token: Some(SecretString::new(refresh)),
        expires_at: Some(expires_at),
        scopes: vec!["oauth".into(), "crm.objects.contacts.read".into()],
    }
}

fn authorize_url(hubspot: HubSpot, scopes: Option<Vec<String>>) -> url::Url {
    Socket::in_memory()
        .integration(Arc::new(hubspot))
        .build()
        .unwrap()
        .begin_authorization(key(), scopes)
        .unwrap()
        .url
}

fn param(url: &url::Url, name: &str) -> Option<String> {
    url.query_pairs().find(|(n, _)| n == name).map(|(_, v)| v.into_owned())
}

async fn identity(socket: &Socket, key: &ConnectionKey) -> socketkit_core::Result<Value> {
    socket
        .invoke(key.clone(), "hubspot.identity.get".into(), json!({}))
        .await
}

async fn resolve(socket: &Socket, key: &ConnectionKey, input: &str) -> socketkit_core::Result<Value> {
    socket
        .invoke(
            key.clone(),
            "hubspot.resource.resolve".into(),
            json!({ "input": input }),
        )
        .await
}

// ── The definition and its settings ───────────────────────────────────────────

#[tokio::test]
async fn passes_the_conformance_suite() {
    conformance::all(provider(), build, "contacts").await;
}

#[tokio::test]
async fn the_definition_is_hubspots_api_with_its_token_endpoint_on_the_same_host() {
    let spec = provider();
    assert_eq!(spec.api_base.as_str(), "https://api.hubapi.com/");
    let AuthScheme::OAuth2(oauth) = &spec.auth else {
        panic!("hubspot uses OAuth")
    };
    assert_eq!(oauth.authorize_url.as_str(), "https://app.hubspot.com/oauth/authorize");
    // The dated endpoint. HubSpot retires `/oauth/v1/token` in February 2027.
    assert_eq!(oauth.token_url.as_str(), "https://api.hubapi.com/oauth/2026-09/token");
    assert_eq!(oauth.default_scopes, ["oauth"], "the least that identifies the account");
    assert_eq!(oauth.scope_separator, " ");
    assert!(
        !oauth.pkce,
        "HubSpot documents no code challenge for its authorise page"
    );
    assert!(oauth.extra_authorize_params.is_empty());

    let allows = |u: &str| spec.allows_host(&u.parse().unwrap());
    assert!(allows("https://api.hubapi.com/crm/objects/2026-09/contacts"));
    assert!(allows("https://api.hubapi.com/oauth/2026-09/token"));
    // The page a person approves on is never sent a credential, and neither
    // are HubSpot's other hosts.
    for elsewhere in [
        "https://app.hubspot.com/oauth/authorize",
        "https://api.hubspot.com/crm/objects/2026-09/contacts",
        "https://api-eu1.hubapi.com/crm/objects/2026-09/contacts",
        "https://evil.hubapi.com/",
    ] {
        assert!(!allows(elsewhere), "{elsewhere}");
    }
}

#[tokio::test]
async fn the_authorise_page_is_asked_for_the_scopes_and_the_optional_scopes() {
    let settings = HubSpotOAuth {
        client: client(),
        scopes: Some(vec!["oauth".into(), "crm.objects.contacts.read".into()]),
        optional_scopes: Some(vec!["crm.objects.custom.read".into(), "crm.schemas.custom.read".into()]),
    };
    let url = authorize_url(HubSpot::with_oauth(settings), None);
    assert_eq!(url.host_str(), Some("app.hubspot.com"));
    assert_eq!(url.path(), "/oauth/authorize");
    assert_eq!(param(&url, "client_id").as_deref(), Some("client-id"));
    assert_eq!(
        param(&url, "redirect_uri").as_deref(),
        Some("https://app.example.test/callback")
    );
    assert_eq!(
        param(&url, "scope").as_deref(),
        Some("oauth crm.objects.contacts.read"),
        "what the account has to grant"
    );
    assert_eq!(
        param(&url, "optional_scope").as_deref(),
        Some("crm.objects.custom.read crm.schemas.custom.read"),
        "what it grants if its plan has it"
    );
    assert!(param(&url, "state").is_some());
    assert_eq!(param(&url, "code_challenge"), None);
    // Each parameter is written once.
    for name in ["scope", "optional_scope", "client_id", "state"] {
        assert_eq!(url.query_pairs().filter(|(n, _)| n == name).count(), 1, "{name}");
    }

    let plain = authorize_url(HubSpot::with_oauth(client()), None);
    assert_eq!(param(&plain, "scope").as_deref(), Some("oauth"));
    assert_eq!(param(&plain, "optional_scope"), None);

    // No optional scopes, or none that is a scope, means no parameter.
    for none in [Some(Vec::new()), Some(vec![" ".to_owned(), String::new()]), None] {
        let settings = HubSpotOAuth {
            optional_scopes: none,
            ..client().into()
        };
        let url = authorize_url(HubSpot::with_oauth(settings), None);
        assert_eq!(param(&url, "optional_scope"), None);
    }
}

#[tokio::test]
async fn the_scope_hubspot_requires_is_always_asked_for_whichever_scopes_are_given() {
    // HubSpot requires `oauth` of every app, and it is what reads the account's details.
    let settings = HubSpotOAuth {
        scopes: Some(vec!["crm.objects.deals.read".into()]),
        ..client().into()
    };
    let from_settings = authorize_url(HubSpot::with_oauth(settings), None);
    assert_eq!(
        param(&from_settings, "scope").as_deref(),
        Some("crm.objects.deals.read oauth")
    );

    let at_the_call = authorize_url(
        HubSpot::with_oauth(client()),
        Some(vec!["crm.objects.contacts.write".into()]),
    );
    assert_eq!(
        param(&at_the_call, "scope").as_deref(),
        Some("crm.objects.contacts.write oauth")
    );

    let already = authorize_url(
        HubSpot::with_oauth(client()),
        Some(vec!["oauth".into(), "tickets".into()]),
    );
    assert_eq!(
        param(&already, "scope").as_deref(),
        Some("oauth tickets"),
        "it is not asked for twice"
    );
}

// ── Connecting and refreshing ────────────────────────────────────────────────

#[tokio::test]
async fn connecting_exchanges_the_code_with_the_client_credentials_in_the_body() {
    let server = MockServer::start().await;
    let (socket, store) = with_oauth_app(&server, None).await;

    let authorization = socket.begin_authorization(key(), None).unwrap();
    assert_eq!(authorization.url.path(), "/oauth/authorize");
    assert!(authorization.pending.pkce_verifier.is_none());

    Mock::given(method("POST"))
        .and(path(TOKEN))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains(
            "redirect_uri=https%3A%2F%2Fapp.example.test%2Fcallback",
        ))
        .and(body_string_contains("client_id=client-id"))
        .and(body_string_contains("client_secret=client-secret"))
        .respond_with(ok(json!({
            "token_type": "bearer",
            "refresh_token": "na1-aaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
            "access_token": "CJSP5qf1KhICAQEYs-gDIIGOBii1hQIyGQAf3xBKmlwHjX7OIpuIFEavB2-qYAGQsF4",
            "hub_id": 1_234_567,
            "scopes": ["oauth", "crm.objects.contacts.read"],
            "expires_in": 1800
        })))
        .expect(1)
        .mount(&server)
        .await;

    let state = authorization.pending.state.clone();
    let granted = socket
        .complete_authorization(authorization.pending, "the-code".into(), state)
        .await
        .unwrap();
    assert!(granted.access_token.expose().starts_with("CJSP5qf1"));
    assert_eq!(
        granted.refresh_token.as_ref().map(SecretString::expose),
        Some("na1-aaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
    );
    // HubSpot lists what was granted under `scopes`. With optional scopes
    // that list is the only way to know which of them the account has.
    assert_eq!(granted.scopes, ["oauth", "crm.objects.contacts.read"]);
    let lifetime = granted.expires_at.unwrap().duration_since(SystemTime::now()).unwrap();
    assert!(
        (Duration::from_secs(1790)..=Duration::from_secs(1800)).contains(&lifetime),
        "half an hour: {lifetime:?}"
    );
    assert_eq!(store.load(key()).await.unwrap(), Some(granted));

    // The credentials travel in the body and nowhere else.
    let exchange = &server.received_requests().await.unwrap()[0];
    assert_eq!(exchange.url.query(), None);
    assert!(exchange.headers.get("authorization").is_none());
}

#[tokio::test]
async fn an_expired_token_is_refreshed_and_the_refresh_token_is_kept() {
    // An access token lasts half an hour. HubSpot's refresh token does not
    // change: it comes back as it was, or does not come back at all, and
    // either way it is still the connection's only way to renew.
    for answered in [
        json!({ "token_type": "bearer", "access_token": "second", "refresh_token": "na1-refresh", "expires_in": 1800, "scopes": ["oauth", "crm.objects.contacts.read", "crm.objects.deals.read"] }),
        json!({ "token_type": "bearer", "access_token": "second", "expires_in": 1800 }),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(TOKEN))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("refresh_token=na1-refresh"))
            .and(body_string_contains("client_id=client-id"))
            .and(body_string_contains("client_secret=client-secret"))
            .respond_with(ok(answered.clone()))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(path(DETAILS))
            .and(header("authorization", "Bearer second"))
            .respond_with(ok(account()))
            .expect(1)
            .mount(&server)
            .await;
        let (socket, store) = with_oauth_app(&server, Some(tokens("first", "na1-refresh", -10))).await;

        assert_eq!(identity(&socket, &key()).await.unwrap()["id"], "1234567");
        let saved = store.load(key()).await.unwrap().unwrap();
        assert_eq!(saved.access_token.expose(), "second");
        assert_eq!(
            saved.refresh_token.as_ref().map(SecretString::expose),
            Some("na1-refresh"),
            "{answered}"
        );
        let scopes = if answered.get("scopes").is_some() {
            vec!["oauth", "crm.objects.contacts.read", "crm.objects.deals.read"]
        } else {
            // What HubSpot does not say again is kept as it was.
            vec!["oauth", "crm.objects.contacts.read"]
        };
        assert_eq!(saved.scopes, scopes, "{answered}");
    }
}

#[tokio::test]
async fn a_token_hubspot_rejects_before_its_time_is_renewed_once_and_the_call_sent_again() {
    let server = MockServer::start().await;
    Mock::given(path(DETAILS))
        .and(header("authorization", "Bearer first"))
        .respond_with(hubspot_error(
            401,
            "EXPIRED_AUTHENTICATION",
            "The OAuth token used to make this call expired 2 minute(s) ago.",
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path(DETAILS))
        .and(header("authorization", "Bearer second"))
        .respond_with(ok(account()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path(TOKEN))
        .respond_with(ok(
            json!({ "access_token": "second", "refresh_token": "na1-refresh", "expires_in": 1800 }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let (socket, _store) = with_oauth_app(&server, Some(tokens("first", "na1-refresh", 1000))).await;
    assert_eq!(identity(&socket, &key()).await.unwrap()["name"], "Analytical Engines");
}

#[tokio::test]
async fn a_refresh_token_hubspot_no_longer_accepts_means_reconnect() {
    // The app was uninstalled from the account, or the token was revoked.
    let server = MockServer::start().await;
    Mock::given(path(TOKEN))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "status": "BAD_REFRESH_TOKEN",
            "message": "missing or unknown refresh token",
            "correlationId": "c033cdaa-2c40-4a64-ae48-b4cec88dad24"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let (socket, _store) = with_oauth_app(&server, Some(tokens("first", "na1-refresh", -10))).await;
    let err = identity(&socket, &key()).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ReconnectRequired, "{err}");
    assert!(!err.message().contains("na1-refresh"), "{}", err.message());
}

// ── Identity ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn identity_reads_the_account_with_the_token_in_a_header_and_not_in_the_address() {
    let (server, socket, key) = hubspot().await;
    Mock::given(method("GET"))
        .and(path(DETAILS))
        .and(header("authorization", "Bearer pat-na1-good"))
        .respond_with(ok(account()))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        identity(&socket, &key).await.unwrap(),
        json!({ "id": "1234567", "name": "Analytical Engines", "email": null })
    );
    let request = &server.received_requests().await.unwrap()[0];
    assert!(!request.url.as_str().contains("pat-na1-good"), "{}", request.url);
    assert_eq!(request.url.query(), None);
}

#[tokio::test]
async fn an_account_hubspot_does_not_name_is_known_by_its_number() {
    for (details, id, name) in [
        (
            json!({ "portalId": 1_234_567, "uiDomain": "app.hubspot.com" }),
            "1234567",
            "1234567",
        ),
        (json!({ "portalId": 1_234_567, "portalName": "" }), "1234567", "1234567"),
        (
            json!({ "portalId": 1_234_567, "portalName": null }),
            "1234567",
            "1234567",
        ),
        (
            json!({ "portalId": "1234567", "portalName": " Analytical Engines " }),
            "1234567",
            " Analytical Engines ",
        ),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(path(DETAILS))
            .respond_with(ok(details.clone()))
            .mount(&server)
            .await;
        assert_eq!(
            identity(&socket, &key).await.unwrap(),
            json!({ "id": id, "name": name, "email": null }),
            "{details}"
        );
    }
}

#[tokio::test]
async fn a_success_without_an_account_is_an_error_and_never_an_account_with_blank_fields() {
    for body in [
        json!({}),
        json!({ "portalName": "Analytical Engines", "uiDomain": "app.hubspot.com" }),
        json!({ "portalId": null }),
        json!({ "portalId": "" }),
        json!({ "portalId": -1 }),
        json!({ "portalId": 12.5 }),
        json!({ "portalId": { "id": 1 } }),
        json!({ "status": "error", "message": "Something went wrong", "category": "INTERNAL" }),
        json!([]),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(any()).respond_with(ok(body.clone())).mount(&server).await;
        let err = identity(&socket, &key).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{body}: {err}");
    }
}

// ── Resource lookup ──────────────────────────────────────────────────────────

#[tokio::test]
async fn an_object_type_resolves_with_one_cheap_read_of_its_records() {
    for (input, kind) in [
        ("contacts", "contacts"),
        (" deals ", "deals"),
        ("0-5", "0-5"),
        ("2-12345", "2-12345"),
        ("p1234567_cars", "p1234567_cars"),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(method("GET"))
            .and(path(format!("/crm/objects/2026-09/{kind}")))
            .and(query_param("limit", "1"))
            // An object type with no records in it is still one the account can read.
            .respond_with(ok(json!({ "results": [] })))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(
            resolve(&socket, &key, input).await.unwrap(),
            json!({ "id": kind, "label": kind, "description": "HubSpot object type" }),
            "{input:?}"
        );
        let request = &server.received_requests().await.unwrap()[0];
        assert_eq!(
            request.url.query(),
            Some("limit=1"),
            "nothing more than one record is asked for"
        );
    }
}

#[tokio::test]
async fn what_is_not_an_object_type_is_refused_without_calling_hubspot() {
    let (server, socket, key) = hubspot().await;
    for bad in [
        "",
        "  ",
        "contacts/512",
        "contacts?archived=true",
        "../oauth/2026-09/token",
        "my contacts",
        "https://app.hubspot.com/contacts/1234567/objects/0-1",
        "contacts#",
        "..",
        "a%2Fb",
    ] {
        let err = resolve(&socket, &key, bad).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{bad:?}");
        assert!(
            bad.trim().is_empty() || !err.message().contains(bad.trim()),
            "what was typed is not repeated: {}",
            err.message()
        );
    }
    let too_long = "a".repeat(101);
    assert_eq!(
        resolve(&socket, &key, &too_long).await.unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    let not_text = socket
        .invoke(key.clone(), "hubspot.resource.resolve".into(), json!({ "input": 7 }))
        .await
        .unwrap_err();
    assert_eq!(not_text.kind(), ErrorKind::InvalidInput);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_object_type_the_account_lacks_is_not_found_and_a_missing_scope_is_named() {
    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(hubspot_error(
            404,
            "OBJECT_NOT_FOUND",
            "Unable to infer object type from: carz",
        ))
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, "carz").await.unwrap_err();
    assert_eq!(
        (err.kind(), err.message()),
        (ErrorKind::NotFound, "this HubSpot account has no such object type")
    );

    // HubSpot's own reason is kept, and the scope a read needs is named beside it.
    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "status": "error",
            "message": "This app hasn't been granted all required scopes to make this call.",
            "errors": [{ "message": "One or more of the following scopes are required.", "context": { "requiredScopes": ["crm.objects.deals.read"] } }],
            "category": "MISSING_SCOPES"
        })))
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, "deals").await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert!(
        err.message()
            .contains("any one of these grants it: crm.objects.deals.read;"),
        "{}",
        err.message()
    );
    assert!(err.message().contains("needs its read scope"), "{}", err.message());

    // A success that does not carry the records is not an object type.
    for body in [
        json!({}),
        json!({ "results": null }),
        json!({ "total": 0 }),
        json!({ "id": "contacts" }),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(any()).respond_with(ok(body.clone())).mount(&server).await;
        let err = resolve(&socket, &key, "contacts").await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{body}");
    }
}

// ── HubSpot's errors ─────────────────────────────────────────────────────────

#[tokio::test]
async fn hubspots_answers_map_to_the_error_a_caller_can_act_on() {
    let limited = |policy: &str| rate_limited(policy, "You have reached your limit.");
    for (response, kind, retry) in [
        (
            hubspot_error(401, "INVALID_AUTHENTICATION", "Authentication credentials not found."),
            ErrorKind::ReconnectRequired,
            Retry::Never,
        ),
        (
            hubspot_error(
                403,
                "MISSING_SCOPES",
                "This app hasn't been granted all required scopes to make this call.",
            ),
            ErrorKind::AccessDenied,
            Retry::Never,
        ),
        (
            hubspot_error(404, "OBJECT_NOT_FOUND", "resource not found"),
            ErrorKind::NotFound,
            Retry::Never,
        ),
        (
            hubspot_error(400, "VALIDATION_ERROR", "Invalid input JSON on line 1, column 2"),
            ErrorKind::InvalidInput,
            Retry::Never,
        ),
        // Each limit of an account is a rate limit, with the wait when HubSpot gives one.
        (limited("TEN_SECONDLY_ROLLING"), ErrorKind::RateLimited, Retry::Later),
        (limited("DAILY"), ErrorKind::RateLimited, Retry::Later),
        (
            limited("DAILY").insert_header("retry-after", "43200"),
            ErrorKind::RateLimited,
            Retry::After(Duration::from_secs(43_200)),
        ),
        // A limit this crate has no name for is still a rate limit.
        (limited("SOMETHING_NEW"), ErrorKind::RateLimited, Retry::Later),
        (
            ResponseTemplate::new(429).insert_header("retry-after", "3"),
            ErrorKind::RateLimited,
            Retry::After(Duration::from_secs(3)),
        ),
        // Too much sent in too short a time. HubSpot holds the lock for two seconds.
        (
            hubspot_error(423, "LOCKED", "The resource is locked."),
            ErrorKind::Unexpected,
            Retry::After(Duration::from_secs(2)),
        ),
        // HubSpot's own status for an account that is being moved, with the wait in seconds.
        (
            ResponseTemplate::new(477).insert_header("retry-after", "86400"),
            ErrorKind::Unexpected,
            Retry::After(Duration::from_secs(86_400)),
        ),
        (ResponseTemplate::new(477), ErrorKind::Unexpected, Retry::Later),
        (
            ResponseTemplate::new(477).insert_header("retry-after", "tomorrow"),
            ErrorKind::Unexpected,
            Retry::Later,
        ),
        (
            hubspot_error(500, "INTERNAL_ERROR", "internal error"),
            ErrorKind::Unexpected,
            Retry::Later,
        ),
        (ResponseTemplate::new(502), ErrorKind::Unexpected, Retry::Later),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(path(DETAILS)).respond_with(response).mount(&server).await;
        let err = identity(&socket, &key).await.unwrap_err();
        assert_eq!((err.kind(), err.retry()), (kind, retry), "{err}");
    }
}

#[tokio::test]
async fn a_refusal_for_a_missing_scope_names_the_scopes_and_nothing_else_from_hubspots_answer() {
    let refusal = |errors: Value, context: Value| {
        ResponseTemplate::new(403).set_body_json(json!({
            "status": "error",
            "message": "This app hasn't been granted all required scopes to make this call.",
            "correlationId": "c033cdaa-2c40-4a64-ae48-b4cec88dad24",
            "errors": errors,
            "context": context,
            "category": "MISSING_SCOPES"
        }))
    };
    for (errors, context, said) in [
        (
            json!([{ "message": "One or more of the following scopes are required.", "context": { "requiredScopes": ["crm.objects.owners.read"] } }]),
            json!(null),
            "any one of these grants it: crm.objects.owners.read",
        ),
        // Wherever HubSpot lists them, and each of them once.
        (
            json!([{ "context": { "requiredGranularScopes": ["crm.objects.deals.read", "crm.schemas.deals.read"] } }, { "context": { "requiredScopes": ["crm.objects.deals.read", "e-commerce"] } }]),
            json!(null),
            "any one of these grants it: crm.objects.deals.read, crm.schemas.deals.read, e-commerce",
        ),
        (
            json!(null),
            json!({ "requiredScopes": ["tickets"] }),
            "any one of these grants it: tickets",
        ),
        // What does not look like a scope is not repeated.
        (
            json!([{ "context": { "requiredScopes": ["crm.objects.deals.read", "see https://example.test/?token=hunter2", 7, ""] } }]),
            json!(null),
            "any one of these grants it: crm.objects.deals.read",
        ),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(any())
            .respond_with(refusal(errors, context))
            .mount(&server)
            .await;
        let err = identity(&socket, &key).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::AccessDenied);
        assert!(err.message().ends_with(said), "{}", err.message());
        assert!(!err.message().contains("hunter2"), "{}", err.message());
    }

    // With no scope named, the refusal still says what is wrong.
    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(refusal(json!([]), json!({})))
        .mount(&server)
        .await;
    let err = identity(&socket, &key).await.unwrap_err();
    assert!(
        err.message().ends_with("has not been granted a scope this call needs"),
        "{}",
        err.message()
    );
}

#[tokio::test]
async fn hubspots_own_reason_reaches_the_caller() {
    let (server, socket, key) = hubspot().await;
    Mock::given(path(DETAILS))
        .respond_with(hubspot_error(
            403,
            "FORBIDDEN",
            "The scope needed for this API call isn't available for public use.",
        ))
        .mount(&server)
        .await;
    let err = identity(&socket, &key).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert!(
        err.message()
            .ends_with("The scope needed for this API call isn't available for public use."),
        "{}",
        err.message()
    );
}

#[tokio::test]
async fn hubspot_built_with_a_private_apps_token_calls_with_it_and_needs_no_stored_connection() {
    let server = MockServer::start().await;
    Mock::given(path(DETAILS))
        .and(header("authorization", "Bearer pat-na1-given"))
        .respond_with(ok(account()))
        .expect(1)
        .mount(&server)
        .await;
    let hubspot = HubSpot::with_spec(point_at(provider(), &server)).token("pat-na1-given");
    let socket = Socket::in_memory().integration(Arc::new(hubspot)).build().unwrap();
    let anyone = ConnectionKey::new(provider().id, "anyone");
    assert_eq!(identity(&socket, &anyone).await.unwrap()["id"], "1234567");

    let given = HubSpot::with_token("pat-na1-given");
    let fixed = given.fixed_token().unwrap();
    assert_eq!(fixed.access_token.expose(), "pat-na1-given");
    assert!(fixed.refresh_token.is_none(), "a private app's token is not renewed");
    assert!(given.oauth_client().is_none());
    assert_eq!(given.provider(), provider());
}

#[tokio::test]
async fn the_integration_keeps_the_provider_id_its_operations_are_named_after() {
    let mut spec = provider();
    spec.id = socketkit_core::ProviderId::new("crm").unwrap();
    let err = Socket::in_memory().integration(build(spec)).build().unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Config);
}
