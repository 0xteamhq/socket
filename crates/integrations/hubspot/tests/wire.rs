//! HubSpot against a local server that answers as HubSpot's API and its token endpoint do.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use socketkit_core::{
    AuthScheme, ConnectionKey, ErrorKind, Integration, MemoryTokenStore, OAuthClient, ProviderSpec, Retry, RetryPolicy,
    SecretString, Socket, TokenSet, TokenStore,
};
use socketkit_hubspot::{HubSpot, HubSpotOAuth, provider};
use socketkit_testkit::wiremock::matchers::{any, body_string_contains, header, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{TENANT, conformance, point_at};

mod support;
use support::{ACCOUNT, OBJECTS, TOKEN, contact, hubspot, hubspot_error, rate_limited};

const TOKEN_PATH: &str = "/oauth/2026-09/token";

fn build(spec: ProviderSpec) -> Arc<dyn Integration> {
    Arc::new(HubSpot::with_spec(spec))
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
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

/// The account as HubSpot describes it.
fn account() -> Value {
    json!({
        "portalId": 8_675_309, "portalName": "Analytical Engines", "accountType": "STANDARD", "timeZone": "Europe/London",
        "companyCurrency": "GBP", "additionalCurrencies": [], "utcOffset": "+01:00", "utcOffsetMilliseconds": 3_600_000,
        "uiDomain": "app.hubspot.com", "dataHostingLocation": "na1"
    })
}

/// A `Socket` whose HubSpot integration holds the application's OAuth app.
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
        api_base: None,
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

fn scopes(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
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
    conformance::all(provider(), build, "contacts/12345").await;
}

#[tokio::test]
async fn the_definition_is_hubspots_api_signed_in_through_its_own_page() {
    let spec = provider();
    assert_eq!(spec.id.as_str(), "hubspot");
    assert_eq!(spec.api_base.as_str(), "https://api.hubapi.com/");
    let AuthScheme::OAuth2(oauth) = &spec.auth else {
        panic!("hubspot uses OAuth")
    };
    assert_eq!(oauth.authorize_url.as_str(), "https://app.hubspot.com/oauth/authorize");
    assert_eq!(oauth.token_url.as_str(), "https://api.hubapi.com/oauth/2026-09/token");
    // The smallest set that identifies the account.
    assert_eq!(oauth.default_scopes, ["oauth"]);
    assert_eq!(oauth.scope_separator, " ");
    assert!(!oauth.pkce, "HubSpot's sign-in page documents no code challenge");
    assert_eq!(oauth.client_auth, socketkit_core::ClientAuth::Body);

    let allows = |u: &str| spec.allows_host(&u.parse().unwrap());
    assert!(allows("https://api.hubapi.com/crm/objects/2026-09/contacts"));
    assert!(allows("https://api.hubapi.com/oauth/2026-09/token"));
    // The page a person approves on is never sent a credential, and no
    // other host of HubSpot's is either.
    for elsewhere in [
        "https://app.hubspot.com/oauth/authorize",
        "https://api.hubspot.com/crm/objects/2026-09/contacts",
        "https://api-eu1.hubapi.com/crm/objects/2026-09/contacts",
        "https://forms.hubspot.com/",
        "https://api.hubapi.com.evil.test/",
    ] {
        assert!(!allows(elsewhere), "{elsewhere}");
    }
}

#[tokio::test]
async fn the_sign_in_page_is_asked_for_the_scopes_and_the_optional_ones_apart() {
    let settings = HubSpotOAuth {
        client: client(),
        scopes: Some(scopes(&[
            "oauth",
            "crm.objects.contacts.read",
            "crm.objects.deals.read",
        ])),
        optional_scopes: scopes(&["crm.objects.custom.read", "sales-email-read"]),
    };
    let url = authorize_url(HubSpot::with_oauth(settings), None);
    assert_eq!(url.host_str(), Some("app.hubspot.com"));
    assert_eq!(url.path(), "/oauth/authorize");
    assert_eq!(
        param(&url, "scope").as_deref(),
        Some("oauth crm.objects.contacts.read crm.objects.deals.read")
    );
    // An account whose plan lacks one of these still connects, without it.
    assert_eq!(
        param(&url, "optional_scope").as_deref(),
        Some("crm.objects.custom.read sales-email-read")
    );
    assert_eq!(param(&url, "client_id").as_deref(), Some("client-id"));
    assert_eq!(
        param(&url, "redirect_uri").as_deref(),
        Some("https://app.example.test/callback")
    );
    assert_eq!(param(&url, "response_type").as_deref(), Some("code"));
    assert!(param(&url, "state").is_some_and(|state| !state.is_empty()));
    assert_eq!(param(&url, "code_challenge"), None);
    assert!(!url.as_str().contains("client-secret"));

    // With nothing set, only what identifies the account is asked for.
    let plain = authorize_url(HubSpot::with_oauth(client()), None);
    assert_eq!(param(&plain, "scope").as_deref(), Some("oauth"));
    assert_eq!(param(&plain, "optional_scope"), None);
}

#[tokio::test]
async fn oauth_is_always_asked_for_and_no_scope_is_asked_for_twice() {
    // Every HubSpot application requires `oauth`, and HubSpot refuses a
    // link that leaves out a scope the application requires.
    let settings = HubSpotOAuth {
        scopes: Some(scopes(&["crm.objects.contacts.read"])),
        ..client().into()
    };
    let from_settings = authorize_url(HubSpot::with_oauth(settings), None);
    assert_eq!(
        param(&from_settings, "scope").as_deref(),
        Some("crm.objects.contacts.read oauth")
    );

    // Scopes given for one connection replace the settings' own. The
    // optional ones stay, less any that is now asked for outright.
    let settings = HubSpotOAuth {
        optional_scopes: scopes(&[
            "crm.objects.deals.read",
            "crm.objects.custom.read",
            " ",
            "crm.objects.custom.read",
            "oauth",
        ]),
        ..client().into()
    };
    let at_the_call = authorize_url(
        HubSpot::with_oauth(settings),
        Some(scopes(&["oauth", "crm.objects.deals.read"])),
    );
    assert_eq!(
        param(&at_the_call, "scope").as_deref(),
        Some("oauth crm.objects.deals.read")
    );
    assert_eq!(
        param(&at_the_call, "optional_scope").as_deref(),
        Some("crm.objects.custom.read")
    );

    let builder = HubSpot::new()
        .optional_scopes(["tickets"])
        .optional_scopes(["crm.objects.custom.write"])
        .oauth(client());
    assert_eq!(
        param(&authorize_url(builder, None), "optional_scope").as_deref(),
        Some("crm.objects.custom.write"),
        "the last list given is the list"
    );
}

// ── Connecting and refreshing ────────────────────────────────────────────────

#[tokio::test]
async fn connecting_exchanges_the_code_and_keeps_the_scopes_that_were_granted() {
    let server = MockServer::start().await;
    let (socket, store) = with_oauth_app(&server, None).await;
    let authorization = socket.begin_authorization(key(), None).unwrap();
    assert!(authorization.pending.pkce_verifier.is_none());

    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
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
            "refresh_token": "na1-refresh-1",
            "access_token": "access-1",
            "hub_id": 8_675_309,
            // Of the optional scopes asked for, these are the ones the account has.
            "scopes": ["oauth", "crm.objects.contacts.read"],
            "expires_in": 1800
        })))
        .expect(1)
        .mount(&server)
        .await;

    let state = authorization.pending.state.clone();
    let before = SystemTime::now();
    let granted = socket
        .complete_authorization(authorization.pending, "the-code".into(), state)
        .await
        .unwrap();
    assert_eq!(granted.access_token.expose(), "access-1");
    assert_eq!(
        granted.refresh_token.as_ref().map(SecretString::expose),
        Some("na1-refresh-1")
    );
    assert_eq!(granted.scopes, ["oauth", "crm.objects.contacts.read"]);
    let lifetime = granted.expires_at.unwrap().duration_since(before).unwrap();
    assert!(
        (1795..=1805).contains(&lifetime.as_secs()),
        "thirty minutes: {lifetime:?}"
    );
    assert_eq!(granted.api_base, None, "every account is served from one host");
    assert_eq!(store.load(key()).await.unwrap(), Some(granted));

    // The client secret went in the form and nowhere else.
    let request = &server.received_requests().await.unwrap()[0];
    assert_eq!(request.url.query(), None);
    assert!(request.headers.get("authorization").is_none());
}

#[tokio::test]
async fn an_expired_token_is_refreshed_and_the_same_refresh_token_goes_on_working() {
    // HubSpot's refresh token does not change. Whether its answer repeats
    // it or leaves it out, the stored one is what the next refresh uses.
    for answer in [
        json!({ "token_type": "bearer", "access_token": "access-2", "refresh_token": "na1-refresh-1", "expires_in": 1800,
                "scopes": ["oauth", "crm.objects.contacts.read"] }),
        json!({ "token_type": "bearer", "access_token": "access-2", "expires_in": 1800 }),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(TOKEN_PATH))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("refresh_token=na1-refresh-1"))
            .and(body_string_contains("client_id=client-id"))
            .and(body_string_contains("client_secret=client-secret"))
            .respond_with(ok(answer.clone()))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(ACCOUNT))
            .and(header("authorization", "Bearer access-2"))
            .respond_with(ok(account()))
            .expect(1)
            .mount(&server)
            .await;
        let (socket, store) = with_oauth_app(&server, Some(tokens("access-1", "na1-refresh-1", -10))).await;

        assert_eq!(identity(&socket, &key()).await.unwrap()["id"], "8675309");
        let saved = store.load(key()).await.unwrap().unwrap();
        assert_eq!(saved.access_token.expose(), "access-2");
        assert_eq!(
            saved.refresh_token.as_ref().map(SecretString::expose),
            Some("na1-refresh-1"),
            "{answer}"
        );
        assert_eq!(saved.scopes, ["oauth", "crm.objects.contacts.read"], "{answer}");
        assert!(saved.expires_at.is_some_and(|at| at > SystemTime::now()));
    }
}

#[tokio::test]
async fn a_token_hubspot_rejects_before_its_time_is_renewed_once_and_the_call_sent_again() {
    let server = MockServer::start().await;
    Mock::given(path(ACCOUNT))
        .and(header("authorization", "Bearer access-1"))
        .respond_with(hubspot_error(
            401,
            "EXPIRED_AUTHENTICATION",
            "The OAuth token used to make this call expired 3 minute(s) ago.",
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path(ACCOUNT))
        .and(header("authorization", "Bearer access-2"))
        .respond_with(ok(account()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path(TOKEN_PATH))
        .respond_with(ok(
            json!({ "access_token": "access-2", "refresh_token": "na1-refresh-1", "expires_in": 1800 }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let (socket, store) = with_oauth_app(&server, Some(tokens("access-1", "na1-refresh-1", 1200))).await;
    assert_eq!(identity(&socket, &key()).await.unwrap()["name"], "Analytical Engines");
    assert_eq!(
        store.load(key()).await.unwrap().unwrap().access_token.expose(),
        "access-2"
    );
}

#[tokio::test]
async fn a_refresh_token_hubspot_no_longer_accepts_means_reconnect_and_a_bad_code_means_start_again() {
    // HubSpot answers with the standard's fields beside its own older ones.
    let refused = |status: &str, description: &str| {
        ResponseTemplate::new(400).set_body_json(json!({
            "error": "invalid_grant", "error_description": description, "status": status, "message": description
        }))
    };
    let server = MockServer::start().await;
    Mock::given(path(TOKEN_PATH))
        .respond_with(refused(
            "BAD_REFRESH_TOKEN",
            "refresh token is invalid, expired or revoked",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let (socket, store) = with_oauth_app(&server, Some(tokens("access-1", "na1-refresh-1", -10))).await;
    let err = identity(&socket, &key()).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ReconnectRequired, "{err}");
    assert!(!err.message().contains("na1-refresh-1"), "{}", err.message());
    assert!(store.load(key()).await.unwrap().is_some());

    let server = MockServer::start().await;
    Mock::given(path(TOKEN_PATH))
        .respond_with(refused("BAD_AUTH_CODE", "missing or unknown auth code"))
        .mount(&server)
        .await;
    let (socket, store) = with_oauth_app(&server, None).await;
    let authorization = socket.begin_authorization(key(), None).unwrap();
    let state = authorization.pending.state.clone();
    let err = socket
        .complete_authorization(authorization.pending, "the-code".into(), state)
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput, "{err}");
    assert!(!err.message().contains("the-code"), "{}", err.message());
    assert_eq!(store.load(key()).await.unwrap(), None);
}

// ── A token the application already holds ────────────────────────────────────

#[tokio::test]
async fn hubspot_built_with_a_private_apps_token_calls_with_it_and_needs_no_stored_connection() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ACCOUNT))
        .and(header("authorization", "Bearer pat-na1-given"))
        .respond_with(ok(account()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{OBJECTS}/contacts/12345")))
        .and(header("authorization", "Bearer pat-na1-given"))
        .respond_with(ok(contact()))
        .expect(1)
        .mount(&server)
        .await;
    let hubspot = HubSpot::with_spec(point_at(provider(), &server)).token("pat-na1-given");
    let socket = Socket::in_memory().integration(Arc::new(hubspot)).build().unwrap();
    let anyone = ConnectionKey::new(provider().id, "anyone");
    assert_eq!(identity(&socket, &anyone).await.unwrap()["id"], "8675309");
    let record = socket
        .invoke(
            anyone,
            "hubspot.objects.get".into(),
            json!({ "object_type": "contacts", "id": "12345" }),
        )
        .await
        .unwrap();
    assert_eq!(record["properties"]["email"], "ada@example.com");

    let given = HubSpot::with_token("pat-na1-given");
    let fixed = given.fixed_token().unwrap();
    assert_eq!(fixed.access_token.expose(), "pat-na1-given");
    assert_eq!((fixed.refresh_token, fixed.expires_at), (None, None));
    assert!(given.oauth_client().is_none());
    assert!(
        !format!("{given:?}").contains("pat-na1-given"),
        "a token is never printed"
    );
}

#[tokio::test]
async fn a_private_apps_token_hubspot_rejects_is_reported_and_nothing_is_renewed() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(hubspot_error(
            401,
            "INVALID_AUTHENTICATION",
            "Authentication credentials not found. pat-na1-given",
        ))
        .mount(&server)
        .await;
    let hubspot = HubSpot::with_spec(point_at(provider(), &server)).token("pat-na1-given");
    let socket = Socket::in_memory().integration(Arc::new(hubspot)).build().unwrap();
    let err = identity(&socket, &ConnectionKey::new(provider().id, "anyone"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ReconnectRequired);
    assert!(!err.message().contains("pat-na1-given"), "{}", err.message());
    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1, "no token endpoint is asked: {received:?}");
}

// ── Identity ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn identity_is_the_account_and_the_token_travels_only_in_the_header() {
    let (server, socket, key) = hubspot().await;
    Mock::given(method("GET"))
        .and(path(ACCOUNT))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ok(account()))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        identity(&socket, &key).await.unwrap(),
        json!({ "id": "8675309", "name": "Analytical Engines", "email": null })
    );
    // HubSpot's older way to ask about a token writes it into the address.
    let request = &server.received_requests().await.unwrap()[0];
    assert!(!request.url.as_str().contains(TOKEN), "{}", request.url);
    assert_eq!(request.url.query(), None);
    assert!(request.body.is_empty());
}

#[tokio::test]
async fn an_account_hubspot_does_not_name_is_known_by_its_id() {
    for (answer, id, name) in [
        (
            json!({ "portalId": 123_456, "uiDomain": "app.hubspot.com" }),
            "123456",
            "HubSpot account 123456",
        ),
        (
            json!({ "portalId": 123_456, "portalName": "  " }),
            "123456",
            "HubSpot account 123456",
        ),
        (
            json!({ "portalId": "123456", "portalName": null }),
            "123456",
            "HubSpot account 123456",
        ),
        (
            json!({ "portalId": 123_456, "portalName": " Engines Ltd " }),
            "123456",
            "Engines Ltd",
        ),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(path(ACCOUNT))
            .respond_with(ok(answer.clone()))
            .mount(&server)
            .await;
        assert_eq!(
            identity(&socket, &key).await.unwrap(),
            json!({ "id": id, "name": name, "email": null }),
            "{answer}"
        );
    }
    // What is not an account id is not taken for one.
    for answer in [
        json!({ "portalId": 0 }),
        json!({ "portalId": -4 }),
        json!({ "portalId": "" }),
        json!({ "portalId": "12 34" }),
        json!({ "portalId": null, "portalName": "Engines" }),
        json!({ "status": "error", "message": "not an account" }),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(path(ACCOUNT))
            .respond_with(ok(answer.clone()))
            .mount(&server)
            .await;
        let err = identity(&socket, &key).await.unwrap_err();
        assert_eq!(
            (err.kind(), err.message()),
            (ErrorKind::Decode, "hubspot answered without an account"),
            "{answer}"
        );
    }
}

// ── Resource lookup ──────────────────────────────────────────────────────────

#[tokio::test]
async fn a_record_resolves_from_its_link_or_from_its_type_and_id() {
    for (input, at, record, resolved) in [
        (
            "https://app.hubspot.com/contacts/8675309/record/0-1/12345",
            "0-1/12345",
            contact(),
            json!({ "id": "0-1/12345", "label": "Ada Lovelace", "description": "HubSpot contact" }),
        ),
        (
            " deals/777 ",
            "deals/777",
            json!({ "id": "777", "properties": { "dealname": "Renewal", "hs_object_id": "777" } }),
            json!({ "id": "deals/777", "label": "Renewal", "description": "HubSpot deal" }),
        ),
        (
            "companies/2",
            "companies/2",
            json!({ "id": "2", "properties": { "name": null, "domain": "engines.example" } }),
            json!({ "id": "companies/2", "label": "engines.example", "description": "HubSpot company" }),
        ),
        // A record of a custom object has no property this crate knows the name by.
        (
            "https://app-eu1.hubspot.com/contacts/8675309/record/2-3465404/4388553737",
            "2-3465404/4388553737",
            json!({ "id": "4388553737", "properties": { "hs_object_id": "4388553737" } }),
            json!({ "id": "2-3465404/4388553737", "label": "record 4388553737", "description": "HubSpot record" }),
        ),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(method("GET"))
            .and(path(ACCOUNT))
            .respond_with(ok(account()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{OBJECTS}/{at}")))
            .respond_with(ok(record))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(resolve(&socket, &key, input).await.unwrap(), resolved, "{input}");
        // A link names an account, which is checked first. An object type
        // and an id name none, and take one call.
        let received = server.received_requests().await.unwrap();
        let paths: Vec<&str> = received.iter().map(|request| request.url.path()).collect();
        if input.contains("://") {
            assert_eq!(paths, [ACCOUNT.to_owned(), format!("{OBJECTS}/{at}")], "{input}");
        } else {
            assert_eq!(paths, [format!("{OBJECTS}/{at}")], "{input}");
        }
        // Only the properties that name a record are asked for, never its content.
        let asked = param(&received.last().unwrap().url, "properties").unwrap();
        assert!(asked.contains("firstname") && asked.contains("dealname"), "{asked}");
        assert!(!asked.contains("hs_note_body"), "{asked}");
    }
}

#[tokio::test]
async fn a_link_to_a_record_in_another_account_is_not_resolved_in_this_one() {
    // Record ids are only unique within an account. Contact 12345 exists
    // here too, and is not the record the link is to.
    for other in [
        "https://app.hubspot.com/contacts/1111111/record/0-1/12345",
        "https://app-eu1.hubspot.com/contacts/86753090/record/0-1/12345",
        "https://app.hubspot.com/contacts/867530/record/0-1/12345",
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(path(ACCOUNT))
            .respond_with(ok(account()))
            .mount(&server)
            .await;
        Mock::given(path(format!("{OBJECTS}/0-1/12345")))
            .respond_with(ok(contact()))
            .mount(&server)
            .await;
        let err = resolve(&socket, &key, other).await.unwrap_err();
        assert_eq!(
            (err.kind(), err.message()),
            (
                ErrorKind::NotFound,
                "that link is to a record in another HubSpot account than the one this connection is for"
            ),
            "{other}"
        );
        let received = server.received_requests().await.unwrap();
        let paths: Vec<&str> = received.iter().map(|request| request.url.path()).collect();
        assert_eq!(paths, [ACCOUNT], "{other}: the record is never read");
    }

    // The same account, however its id is written, is the same account.
    let (server, socket, key) = hubspot().await;
    Mock::given(path(ACCOUNT))
        .respond_with(ok(account()))
        .mount(&server)
        .await;
    Mock::given(path(format!("{OBJECTS}/0-1/12345")))
        .respond_with(ok(contact()))
        .mount(&server)
        .await;
    let own = resolve(
        &socket,
        &key,
        "https://app.hubspot.com/contacts/008675309/record/0-1/12345",
    )
    .await;
    assert_eq!(own.unwrap()["id"], "0-1/12345");

    // When the account cannot be read, the link is not resolved on trust.
    let (server, socket, key) = hubspot().await;
    Mock::given(path(ACCOUNT))
        .respond_with(hubspot_error(403, "FORBIDDEN", "not allowed"))
        .mount(&server)
        .await;
    Mock::given(path(format!("{OBJECTS}/0-1/12345")))
        .respond_with(ok(contact()))
        .mount(&server)
        .await;
    let err = resolve(
        &socket,
        &key,
        "https://app.hubspot.com/contacts/8675309/record/0-1/12345",
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_record_that_is_not_there_is_not_found_and_a_missing_scope_is_named() {
    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(hubspot_error(404, "OBJECT_NOT_FOUND", "resource not found"))
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, "contacts/12345").await.unwrap_err();
    assert_eq!(
        (err.kind(), err.message()),
        (ErrorKind::NotFound, "that HubSpot record was not found")
    );

    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "status": "error", "category": "MISSING_SCOPES",
            "message": "This app hasn't been granted all required scopes to make this call.",
            "errors": [{ "message": "One or more of the following scopes are required.",
                         "context": { "requiredGranularScopes": ["crm.objects.contacts.read"] } }]
        })))
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, "contacts/12345").await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert!(err.message().contains("crm.objects.contacts.read"), "{}", err.message());
}

#[tokio::test]
async fn what_is_not_a_record_is_refused_without_calling_hubspot() {
    let (server, socket, key) = hubspot().await;
    for bad in [
        "",
        "Ada Lovelace",
        "98765",
        "contacts/ada@example.com",
        "contacts/../owners/1",
        "https://app.hubspot.com/contacts/8675309/objects/0-1/views/all/list",
        "https://evil.test/contacts/8675309/record/0-1/12345",
        "https://user:pw@app.hubspot.com/contacts/8675309/record/0-1/12345",
    ] {
        let err = resolve(&socket, &key, bad).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{bad:?}");
        let shown = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!shown.contains("pw@") && !shown.contains("evil"), "{shown}");
        assert!(bad.is_empty() || !shown.contains(bad), "{shown}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ── HubSpot's errors ─────────────────────────────────────────────────────────

#[tokio::test]
async fn hubspots_answers_map_to_the_error_a_caller_can_act_on() {
    let missing_scope = || {
        ResponseTemplate::new(403).set_body_json(json!({
            "status": "error", "category": "MISSING_SCOPES",
            "message": "This app hasn't been granted all required scopes to make this call. Read more about required scopes here: https://developers.hubspot.com/scopes.",
            "correlationId": "a43683b0-5717-4ceb-80b4-104d02915d8c",
            "errors": [{ "message": "One or more of the following scopes are required.", "context": { "requiredGranularScopes": ["oauth"] } }],
            "links": { "scopes": "https://developers.hubspot.com/scopes" }
        }))
    };
    for (response, kind, retry, says) in [
        (
            hubspot_error(401, "INVALID_AUTHENTICATION", "Authentication credentials not found."),
            ErrorKind::ReconnectRequired,
            Retry::Never,
            "rejected the stored authorization",
        ),
        (
            missing_scope(),
            ErrorKind::AccessDenied,
            Retry::Never,
            "HubSpot names oauth. Connect again asking for it or, for a private app's token, add it to the app in HubSpot",
        ),
        (
            hubspot_error(403, "FORBIDDEN", "This account has been deactivated."),
            ErrorKind::AccessDenied,
            Retry::Never,
            "denied the request: This account has been deactivated.",
        ),
        (
            hubspot_error(404, "OBJECT_NOT_FOUND", "resource not found"),
            ErrorKind::NotFound,
            Retry::Never,
            "no such resource",
        ),
        (
            hubspot_error(400, "VALIDATION_ERROR", "Invalid input JSON on line 1, column 2"),
            ErrorKind::InvalidInput,
            Retry::Never,
            "rejected the request: Invalid input JSON",
        ),
        // The ten-second limit, with the wait HubSpot states and without one.
        (
            rate_limited("TEN_SECONDLY_ROLLING").insert_header("retry-after", "9"),
            ErrorKind::RateLimited,
            Retry::After(Duration::from_secs(9)),
            "its limit for any ten seconds was reached",
        ),
        (
            rate_limited("TEN_SECONDLY_ROLLING"),
            ErrorKind::RateLimited,
            Retry::Later,
            "its limit for any ten seconds was reached",
        ),
        // The daily limit: trying again before midnight cannot succeed.
        (
            rate_limited("DAILY").insert_header("retry-after", "7200"),
            ErrorKind::RateLimited,
            Retry::After(Duration::from_secs(7200)),
            "daily request limit for this account is used up",
        ),
        (
            rate_limited("DAILY"),
            ErrorKind::RateLimited,
            Retry::Never,
            "starts again at midnight in the account's time zone",
        ),
        // A 429 that says nothing more is still a limit.
        (
            ResponseTemplate::new(429),
            ErrorKind::RateLimited,
            Retry::Later,
            "is rate limiting requests",
        ),
        (
            ResponseTemplate::new(423).set_body_json(json!({ "status": "error", "message": "locked" })),
            ErrorKind::Unexpected,
            Retry::After(Duration::from_secs(2)),
            "locked the records",
        ),
        (
            ResponseTemplate::new(477).insert_header("retry-after", "86400"),
            ErrorKind::Unexpected,
            Retry::After(Duration::from_secs(86400)),
            "moving this account between its data centres",
        ),
        (
            hubspot_error(500, "INTERNAL_ERROR", "internal error"),
            ErrorKind::Unexpected,
            Retry::Later,
            "returned HTTP 500",
        ),
        (
            ResponseTemplate::new(503),
            ErrorKind::Unexpected,
            Retry::Later,
            "returned HTTP 503",
        ),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(path(ACCOUNT)).respond_with(response).mount(&server).await;
        let err = identity(&socket, &key).await.unwrap_err();
        assert_eq!((err.kind(), err.retry()), (kind, retry), "{err}");
        assert!(err.message().contains(says), "{}", err.message());
        assert_eq!(err.provider().map(|p| p.as_str()), Some("hubspot"));
    }
}

#[tokio::test]
async fn a_limit_that_passes_is_waited_out_and_the_days_is_not_knocked_on_again() {
    // The ten-second limit frees up as requests grow old, so the call is
    // tried again, a write included: a request that was limited was not done.
    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(rate_limited("TEN_SECONDLY_ROLLING"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(201).set_body_json(contact()))
        .mount(&server)
        .await;
    let created = socket
        .invoke(
            key,
            "hubspot.objects.create".into(),
            json!({ "object_type": "contacts", "properties": { "email": "ada@example.com" } }),
        )
        .await
        .unwrap();
    assert_eq!(created["id"], "12345");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);

    // Every refusal counts against the account, and until midnight every
    // call is refused. So the day's limit is reported at once.
    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(rate_limited("DAILY"))
        .mount(&server)
        .await;
    let err = identity(&socket, &key).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimited);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_token_hubspot_echoes_in_its_error_is_not_passed_on() {
    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(hubspot_error(
            400,
            "VALIDATION_ERROR",
            &format!("could not parse the request made with {TOKEN}"),
        ))
        .mount(&server)
        .await;
    let err = identity(&socket, &key).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    let shown = format!("{err} {err:?} {:?}", err.to_wire());
    assert!(!shown.contains(TOKEN), "{shown}");
}

#[tokio::test]
async fn the_integration_keeps_the_provider_id_its_operations_are_named_after() {
    let mut spec = provider();
    spec.id = socketkit_core::ProviderId::new("crm").unwrap();
    let err = Socket::in_memory().integration(build(spec)).build().unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Config);

    // And an operation it does not have is said to be unsupported.
    let (_server, socket, key) = hubspot().await;
    let err = socket
        .invoke(key, "hubspot.objects.purge".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unsupported);
}
