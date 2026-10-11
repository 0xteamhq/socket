//! Pipedrive against local servers that answer as its sign-in host and a company's API host do.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use socketkit_core::{
    ApiKeySpec, AuthScheme, ClientAuth, ConnectionKey, ErrorKind, Integration, KeyPlacement, MemoryTokenStore,
    OAuthClient, ProviderSpec, Retry, RetryPolicy, SecretString, Socket, TokenSet, TokenStore,
};
use socketkit_pipedrive::{Pipedrive, PipedriveToken, api_token_provider, provider};
use socketkit_testkit::wiremock::matchers::{any, body_string_contains, header, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{TENANT, conformance, connect, point_at};
use url::Url;

mod support;
use support::{
    LEAD, TOKEN, answering, data, deal, failure, invoke, lead, me, note, offset_page, ok, only_request, page,
    pipedrive, query_of, requests,
};

/// `client-id:client-secret`, as HTTP Basic writes it.
const BASIC: &str = "Basic Y2xpZW50LWlkOmNsaWVudC1zZWNyZXQ=";

fn build(spec: ProviderSpec) -> Arc<dyn Integration> {
    Arc::new(Pipedrive::with_spec(spec))
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

/// The OAuth definition with sign-in on `sign_in` and the company's host,
/// which a real definition admits by its `*.pipedrive.com` rule, on `company`.
fn signed_in_at(sign_in: &MockServer, company: &MockServer) -> ProviderSpec {
    let mut spec = point_at(provider(), sign_in);
    let company = Url::parse(&company.uri()).unwrap();
    spec.allowed_hosts
        .push(format!("{}:{}", company.host_str().unwrap(), company.port().unwrap()));
    spec
}

async fn with_oauth_app(spec: ProviderSpec, stored: Option<TokenSet>) -> (Socket, Arc<MemoryTokenStore>) {
    let store = Arc::new(MemoryTokenStore::new());
    if let Some(tokens) = stored {
        store.save(key(), tokens).await.unwrap();
    }
    let retry = RetryPolicy {
        max_attempts: 2,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(50),
    };
    let socket = Socket::builder(store.clone())
        .integration(Arc::new(Pipedrive::with_spec(spec).oauth(client())))
        .retry(retry)
        .build()
        .unwrap();
    (socket, store)
}

/// Pipedrive's answer at the token endpoint.
fn granted(access: &str, api_domain: Option<&str>) -> Value {
    let mut body = json!({
        "access_token": access, "token_type": "bearer", "refresh_token": "1:2:refresh", "expires_in": 3599,
        "scope": "base,deals:read,contacts:read"
    });
    if let Some(domain) = api_domain {
        body["api_domain"] = json!(domain);
    }
    body
}

async fn connect_with_code(socket: &Socket) -> socketkit_core::Result<TokenSet> {
    let authorization = socket.begin_authorization(key(), None).unwrap();
    let state = authorization.pending.state.clone();
    socket
        .complete_authorization(authorization.pending, "the-code".into(), state)
        .await
}

async fn identity(socket: &Socket, key: &ConnectionKey) -> socketkit_core::Result<Value> {
    socket
        .invoke(key.clone(), "pipedrive.identity.get".into(), json!({}))
        .await
}

async fn resolve(socket: &Socket, key: &ConnectionKey, input: &str) -> socketkit_core::Result<Value> {
    socket
        .invoke(
            key.clone(),
            "pipedrive.resource.resolve".into(),
            json!({ "input": input }),
        )
        .await
}

// ── The two definitions ───────────────────────────────────────────────────────

#[tokio::test]
async fn both_definitions_pass_the_conformance_suite() {
    conformance::all(provider(), build, "deal/42").await;
    conformance::all(api_token_provider(), build, "deal/42").await;
}

#[tokio::test]
async fn a_connection_calls_the_company_host_its_authorization_named_and_no_other() {
    conformance::a_connection_calls_the_host_its_authorization_named(&provider(), &build, |address| {
        granted("v1u:first", Some(address))
    })
    .await;
}

#[tokio::test]
async fn the_oauth_definition_signs_in_at_one_host_with_basic_and_calls_each_companys_own() {
    let spec = provider();
    assert_eq!(spec.id.as_str(), "pipedrive");
    assert_eq!(spec.api_base.as_str(), "https://api.pipedrive.com/api/");
    assert_eq!(
        spec.allowed_hosts,
        ["api.pipedrive.com", "oauth.pipedrive.com", "*.pipedrive.com"]
    );
    let AuthScheme::OAuth2(oauth) = &spec.auth else {
        panic!("the default definition is the OAuth one");
    };
    assert_eq!(
        oauth.authorize_url.as_str(),
        "https://oauth.pipedrive.com/oauth/authorize"
    );
    assert_eq!(oauth.token_url.as_str(), "https://oauth.pipedrive.com/oauth/token");
    assert_eq!(oauth.client_auth, ClientAuth::Basic);
    assert!(!oauth.pkce, "Pipedrive documents no PKCE");
    assert!(oauth.default_scopes.is_empty(), "scopes are set in Developer Hub");

    // A company's host may be a connection's own address; a look-alike may not.
    let allows = |base: &str| spec.allows_api_base(&base.parse().unwrap());
    assert!(allows("https://acme.pipedrive.com/api/"));
    assert!(!allows("https://acme.pipedrive.com.evil.test/api/"));
    assert!(!allows("https://pipedrive.com.evil.test/api/"));
    assert!(!allows("http://acme.pipedrive.com/api/"));
    // And only as that connection's own: it is not a host any token may go to.
    assert!(!spec.allows_host(&"https://acme.pipedrive.com/api/".parse().unwrap()));
    assert_eq!(Pipedrive::new().provider(), spec);
}

#[tokio::test]
async fn the_sign_in_address_carries_the_client_and_the_state_and_never_scopes() {
    let socket = Socket::in_memory()
        .integration(Arc::new(Pipedrive::with_oauth(client())))
        .build()
        .unwrap();
    for scopes in [None, Some(vec!["deals:full".to_owned(), "admin".to_owned()])] {
        let authorization = socket.begin_authorization(key(), scopes).unwrap();
        let url = authorization.url;
        let param = |name: &str| url.query_pairs().find(|(n, _)| n == name).map(|(_, v)| v.into_owned());
        assert_eq!(url.host_str(), Some("oauth.pipedrive.com"));
        assert_eq!(url.path(), "/oauth/authorize");
        assert_eq!(param("client_id").as_deref(), Some("client-id"));
        assert_eq!(
            param("redirect_uri").as_deref(),
            Some("https://app.example.test/callback")
        );
        assert_eq!(param("state"), Some(authorization.pending.state.clone()));
        assert_eq!(param("scope"), None, "Pipedrive takes scopes from the app's settings");
        assert_eq!(param("code_challenge"), None);
        assert!(authorization.pending.pkce_verifier.is_none());
    }
}

// ── OAuth ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn connecting_exchanges_the_code_with_basic_and_later_calls_go_to_the_companys_host() {
    let sign_in = MockServer::start().await;
    let company = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(header("authorization", BASIC))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains(
            "redirect_uri=https%3A%2F%2Fapp.example.test%2Fcallback",
        ))
        .respond_with(ok(granted("v1u:first", Some(&company.uri()))))
        .expect(1)
        .mount(&sign_in)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/users/me"))
        .and(header("authorization", "Bearer v1u:first"))
        .respond_with(ok(data(me())))
        .expect(1)
        .mount(&company)
        .await;
    let (socket, store) = with_oauth_app(signed_in_at(&sign_in, &company), None).await;

    let tokens = connect_with_code(&socket).await.unwrap();
    assert_eq!(tokens.access_token.expose(), "v1u:first");
    assert_eq!(
        tokens.refresh_token.as_ref().map(SecretString::expose),
        Some("1:2:refresh")
    );
    assert_eq!(tokens.scopes, ["base", "deals:read", "contacts:read"]);
    // Both versions of the API are below `/api/` on the company's host.
    assert_eq!(
        tokens.api_base.as_ref().map(Url::as_str),
        Some(format!("{}/api/", company.uri()).as_str())
    );
    assert_eq!(store.load(key()).await.unwrap(), Some(tokens));

    // The secret travelled in the header only.
    let exchange = only_request(&sign_in).await;
    let form = String::from_utf8(exchange.body.clone()).unwrap();
    assert!(
        !form.contains("client_secret") && !form.contains("client-secret"),
        "{form}"
    );
    assert!(!form.contains("code_verifier"), "{form}");

    assert_eq!(identity(&socket, &key()).await.unwrap()["id"], "7");
    assert_eq!(
        requests(&sign_in).await.len(),
        1,
        "nothing but the exchange went to the sign-in host"
    );
}

#[tokio::test]
async fn an_expired_token_is_refreshed_with_basic_and_the_connection_keeps_its_companys_host() {
    let sign_in = MockServer::start().await;
    let company = MockServer::start().await;
    // A refresh answers with the same refresh token, and here without the host.
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(header("authorization", BASIC))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=1%3A2%3Arefresh"))
        .respond_with(ok(granted("v1u:second", None)))
        .expect(1)
        .mount(&sign_in)
        .await;
    Mock::given(path("/api/v1/users/me"))
        .and(header("authorization", "Bearer v1u:second"))
        .respond_with(ok(data(me())))
        .expect(1)
        .mount(&company)
        .await;
    let own: Url = format!("{}/api/", company.uri()).parse().unwrap();
    let expired = TokenSet {
        access_token: SecretString::new("v1u:first"),
        refresh_token: Some(SecretString::new("1:2:refresh")),
        expires_at: Some(SystemTime::now() - Duration::from_secs(10)),
        scopes: vec!["base".into()],
        api_base: Some(own.clone()),
    };
    let (socket, store) = with_oauth_app(signed_in_at(&sign_in, &company), Some(expired)).await;

    assert_eq!(identity(&socket, &key()).await.unwrap()["id"], "7");
    let saved = store.load(key()).await.unwrap().unwrap();
    assert_eq!(saved.access_token.expose(), "v1u:second");
    assert_eq!(saved.api_base, Some(own), "the company's host outlives the refresh");
    assert_eq!(
        requests(&sign_in).await.len(),
        1,
        "the API call did not go to the sign-in host"
    );
}

#[tokio::test]
async fn a_token_response_that_names_something_other_than_a_pipedrive_host_is_refused_and_nothing_stored() {
    for named in [
        json!("not a url"),
        json!("acme.pipedrive.com"),
        json!("https://acme.pipedrive.com/api/v1"),
        json!("https://user:pw@acme.pipedrive.com"),
        json!("https://acme.pipedrive.com/?next=https://evil.test"),
        json!(42),
        json!({ "host": "acme.pipedrive.com" }),
        // A real address, but not one of Pipedrive's: the core refuses it.
        json!("https://pipedrive.com.evil.test"),
        json!("https://evil.test"),
    ] {
        let sign_in = MockServer::start().await;
        let mut body = granted("v1u:first", None);
        body["api_domain"] = named.clone();
        Mock::given(path("/oauth/token"))
            .respond_with(ok(body))
            .mount(&sign_in)
            .await;
        let (socket, store) = with_oauth_app(point_at(provider(), &sign_in), None).await;
        let error = connect_with_code(&socket).await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Decode, "{named}");
        assert!(!error.message().contains("evil"), "{}", error.message());
        assert_eq!(store.load(key()).await.unwrap(), None, "{named}");
    }
}

#[tokio::test]
async fn a_token_response_that_names_no_host_leaves_the_connection_on_the_shared_one() {
    let server = MockServer::start().await;
    Mock::given(path("/oauth/token"))
        .respond_with(ok(granted("v1u:first", None)))
        .mount(&server)
        .await;
    Mock::given(path("/api/v1/users/me"))
        .respond_with(ok(data(me())))
        .expect(1)
        .mount(&server)
        .await;
    let (socket, _store) = with_oauth_app(point_at(provider(), &server), None).await;
    let tokens = connect_with_code(&socket).await.unwrap();
    assert_eq!(tokens.api_base, None);
    identity(&socket, &key()).await.unwrap();
}

#[tokio::test]
async fn a_refused_code_and_a_refused_refresh_token_are_told_apart() {
    let server = MockServer::start().await;
    Mock::given(path("/oauth/token"))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant", "error_description": "bad" })),
        )
        .mount(&server)
        .await;
    let (socket, _) = with_oauth_app(point_at(provider(), &server), None).await;
    assert_eq!(
        connect_with_code(&socket).await.unwrap_err().kind(),
        ErrorKind::InvalidInput
    );

    // A refresh token unused for 60 days is no longer accepted: the person connects again.
    let expired = TokenSet {
        access_token: SecretString::new("v1u:first"),
        refresh_token: Some(SecretString::new("1:2:refresh")),
        expires_at: Some(SystemTime::now() - Duration::from_secs(10)),
        scopes: Vec::new(),
        api_base: None,
    };
    let (socket, _) = with_oauth_app(point_at(provider(), &server), Some(expired)).await;
    assert_eq!(
        identity(&socket, &key()).await.unwrap_err().kind(),
        ErrorKind::ReconnectRequired
    );
}

// ── The personal API token ────────────────────────────────────────────────────

#[tokio::test]
async fn an_api_token_goes_in_its_header_and_never_in_an_address() {
    let server = MockServer::start().await;
    Mock::given(any()).respond_with(ok(data(me()))).mount(&server).await;
    let pipedrive = Pipedrive::with_spec(point_at(api_token_provider(), &server)).token("pd-secret-token");
    let socket = Socket::in_memory().integration(Arc::new(pipedrive)).build().unwrap();
    // Given to the integration, it serves every tenant and needs no stored connection.
    let anyone = ConnectionKey::new(provider().id, "anyone");
    assert_eq!(identity(&socket, &anyone).await.unwrap()["id"], "7");

    let request = only_request(&server).await;
    assert_eq!(request.headers.get("x-api-token").unwrap(), "pd-secret-token");
    assert!(
        request.headers.get("authorization").is_none(),
        "it is not a bearer token"
    );
    assert_eq!(request.url.path(), "/api/v1/users/me");
    assert!(!request.url.as_str().contains("pd-secret-token"), "{}", request.url);
    assert_eq!(request.url.query(), None);

    // A caller cannot set the header beside it, or in its place.
    let stolen = socket
        .request(
            anyone,
            socketkit_core::RawRequest::get("v1/users/me").with_header("X-Api-Token", "another"),
        )
        .await
        .unwrap_err();
    assert_eq!(stolen.kind(), ErrorKind::InvalidInput);
}

#[tokio::test]
async fn what_with_token_builds_sends_the_token_in_its_header_on_every_kind_of_typed_operation() {
    // `with_token` aims at Pipedrive's own hosts. What it built, its
    // definition and its token, is aimed at a local server here unchanged.
    let given = Pipedrive::with_token("pd-secret-token");
    let token = given.fixed_token().unwrap().access_token;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v2/deals"))
        .respond_with(ok(page(json!([deal()]), None)))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v2/deals"))
        .respond_with(ok(data(deal())))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v2/deals/42"))
        .respond_with(ok(data(deal())))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/notes/3"))
        .respond_with(ok(data(json!(true))))
        .mount(&server)
        .await;
    let pipedrive = Pipedrive::with_spec(point_at(given.provider(), &server)).token(token.expose());
    let socket = Socket::in_memory().integration(Arc::new(pipedrive)).build().unwrap();
    let anyone = ConnectionKey::new(provider().id, "anyone");

    for (name, input) in [
        ("deals.list", json!({ "owner_id": 7 })),
        ("deals.create", json!({ "title": "Acme renewal" })),
        ("deals.update", json!({ "deal": 42, "stage_id": 4 })),
        ("notes.delete", json!({ "note": 3 })),
    ] {
        invoke(&socket, &anyone, name, input)
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    let sent = requests(&server).await;
    assert_eq!(sent.len(), 4);
    for request in sent {
        let what = format!("{} {}", request.method, request.url.path());
        assert_eq!(request.headers.get("x-api-token").unwrap(), "pd-secret-token", "{what}");
        assert!(
            request.headers.get("authorization").is_none(),
            "{what}: it is not a bearer token"
        );
        assert!(
            !request.url.as_str().contains("pd-secret-token"),
            "{what}: {}",
            request.url
        );
        assert!(!request.url.as_str().contains("api_token"), "{what}: {}", request.url);
        assert!(
            !String::from_utf8_lossy(&request.body).contains("pd-secret-token"),
            "{what}"
        );
    }
}

#[tokio::test]
async fn with_token_uses_the_api_token_definition_which_has_no_sign_in() {
    let pipedrive = Pipedrive::with_token("pd-secret-token");
    assert_eq!(pipedrive.provider(), api_token_provider());
    assert_eq!(
        pipedrive.provider().auth,
        AuthScheme::ApiKey(ApiKeySpec {
            placement: KeyPlacement::Header {
                name: "x-api-token".into(),
                prefix: None
            }
        })
    );
    let fixed = pipedrive.fixed_token().unwrap();
    assert_eq!(fixed.access_token.expose(), "pd-secret-token");
    assert_eq!(fixed.api_base, None, "without a company, calls go to api.pipedrive.com");
    assert!(pipedrive.oauth_client().is_none());
    assert!(!format!("{pipedrive:?}").contains("pd-secret-token"));

    let socket = Socket::in_memory().integration(Arc::new(pipedrive)).build().unwrap();
    let error = socket.begin_authorization(key(), None).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Config, "an API token is not signed in for");
}

#[tokio::test]
async fn a_company_named_with_the_token_is_where_its_calls_go_and_a_bad_name_is_refused_at_build() {
    let named = |company: &str| {
        Pipedrive::with_token(PipedriveToken {
            token: SecretString::new("pd-secret-token"),
            company_domain: Some(company.to_owned()),
        })
    };
    let acme = named("Acme");
    assert_eq!(
        acme.fixed_token().unwrap().api_base.as_ref().map(Url::as_str),
        Some("https://acme.pipedrive.com/api/")
    );
    let socket = Socket::in_memory().integration(Arc::new(acme)).build().unwrap();
    let connection = socket.connection(key()).await.unwrap();
    assert_eq!(connection.api_base().as_str(), "https://acme.pipedrive.com/api/");

    for bad in [
        "evil.test/acme",
        "acme.evil.test",
        "acme@evil.test",
        "",
        "a b",
        "acme:8443",
    ] {
        let error = Socket::in_memory()
            .integration(Arc::new(named(bad)))
            .build()
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Config, "{bad:?}");
        assert!(!error.message().contains("evil"), "{}", error.message());
    }
}

#[tokio::test]
async fn a_definition_with_another_id_or_the_token_in_the_address_is_refused_at_build() {
    let mut renamed = provider();
    renamed.id = socketkit_core::ProviderId::new("crm").unwrap();
    let error = Socket::in_memory().integration(build(renamed)).build().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Config);

    for placement in [
        KeyPlacement::Query {
            name: "api_token".into(),
        },
        KeyPlacement::Basic {},
    ] {
        let mut in_address = api_token_provider();
        in_address.auth = AuthScheme::ApiKey(ApiKeySpec { placement });
        let error = Socket::in_memory().integration(build(in_address)).build().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Config);
        assert!(error.message().contains("x-api-token"), "{}", error.message());
    }
}

// ── Identity and lookup ───────────────────────────────────────────────────────

#[tokio::test]
async fn identity_is_the_signed_in_user_in_the_company_the_connection_is_to() {
    let (server, socket, key) = answering(ok(data(me()))).await;
    assert_eq!(
        identity(&socket, &key).await.unwrap(),
        json!({ "id": "7", "name": "Ada Lovelace (Acme Ltd)", "email": "ada@example.test" })
    );
    let request = only_request(&server).await;
    assert_eq!(
        (request.method.as_str(), request.url.path()),
        ("GET", "/api/v1/users/me")
    );

    // A user with no name is known by their email, and one with neither by their id.
    let (_server, socket, key) =
        answering(ok(data(json!({ "id": 9, "name": "", "email": "alan@example.test" })))).await;
    assert_eq!(identity(&socket, &key).await.unwrap()["name"], "alan@example.test");
    let (_server, socket, key) = answering(ok(data(json!({ "id": 9 })))).await;
    assert_eq!(
        identity(&socket, &key).await.unwrap(),
        json!({ "id": "9", "name": "9", "email": null })
    );
}

#[tokio::test]
async fn a_record_is_looked_up_by_its_link_or_its_short_form() {
    let (server, socket, key) = pipedrive().await;
    // A link names a company, which is checked against the connection's own.
    Mock::given(path("/api/v1/users/me"))
        .respond_with(ok(data(me())))
        .mount(&server)
        .await;
    Mock::given(path("/api/v2/deals/42"))
        .respond_with(ok(data(deal())))
        .mount(&server)
        .await;
    Mock::given(path(format!("/api/v1/leads/{LEAD}")))
        .respond_with(ok(data(lead())))
        .mount(&server)
        .await;
    let found = json!({ "id": "deal/42", "label": "Acme renewal", "description": "Pipedrive deal" });
    assert_eq!(resolve(&socket, &key, "deal/42").await.unwrap(), found);
    assert_eq!(
        resolve(&socket, &key, "https://acme.pipedrive.com/deal/42")
            .await
            .unwrap(),
        found
    );
    assert_eq!(
        resolve(&socket, &key, &format!("https://acme.pipedrive.com/leads/inbox/{LEAD}"))
            .await
            .unwrap(),
        json!({ "id": format!("lead/{LEAD}"), "label": "Jane Doe lead", "description": "Pipedrive lead" })
    );

    // A record the account cannot see is not found, and what is not a record is not asked for.
    let (server, socket, key) = answering(failure(404, "Deal not found")).await;
    assert_eq!(
        resolve(&socket, &key, "deal/42").await.unwrap_err().kind(),
        ErrorKind::NotFound
    );
    for bad in ["42", "pipeline/1", "https://evil.test/deal/42", "deal/42/../../users"] {
        assert_eq!(
            resolve(&socket, &key, bad).await.unwrap_err().kind(),
            ErrorKind::InvalidInput,
            "{bad}"
        );
    }
    assert_eq!(requests(&server).await.len(), 1);
}

#[tokio::test]
async fn a_link_to_another_companys_record_is_refused_on_the_shared_host_too_and_the_record_never_fetched() {
    // This connection calls the shared host, as an API token without a
    // company does, so its address does not say which company it is to.
    // Deal 42 exists in its own company, acme; the link is to globex's.
    let (server, socket, key) = pipedrive().await;
    Mock::given(path("/api/v1/users/me"))
        .respond_with(ok(data(me())))
        .mount(&server)
        .await;
    Mock::given(path("/api/v2/deals/42"))
        .respond_with(ok(data(deal())))
        .mount(&server)
        .await;
    let paths = |sent: Vec<socketkit_testkit::wiremock::Request>| -> Vec<String> {
        sent.iter().map(|request| request.url.path().to_owned()).collect()
    };

    for other in [
        "https://globex.pipedrive.com/deal/42",
        "https://acme-eu.pipedrive.com/deal/42",
        "https://acme.eu.pipedrive.com/deal/42",
    ] {
        let error = resolve(&socket, &key, other).await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{other}");
        assert!(error.message().contains("another company"), "{}", error.message());
    }
    assert_eq!(
        paths(requests(&server).await),
        ["/api/v1/users/me"; 3],
        "the company is asked for, and another company's deal is never looked up"
    );

    // The connection's own company is found, however the link writes it.
    let found = resolve(&socket, &key, "https://ACME.pipedrive.com/deal/42")
        .await
        .unwrap();
    assert_eq!(found["id"], "deal/42");
    // The short form names no company, and costs no second request.
    resolve(&socket, &key, "deal/42").await.unwrap();
    assert_eq!(
        paths(requests(&server).await)[3..],
        ["/api/v1/users/me", "/api/v2/deals/42", "/api/v2/deals/42"]
    );

    // When Pipedrive does not say which company the connection is to, a
    // link cannot be checked, and is not taken on trust.
    for nameless in [json!({ "id": 7 }), json!({ "id": 7, "company_domain": "  " })] {
        let (server, socket, key) = pipedrive().await;
        Mock::given(path("/api/v1/users/me"))
            .respond_with(ok(data(nameless)))
            .mount(&server)
            .await;
        Mock::given(path("/api/v2/deals/42"))
            .respond_with(ok(data(deal())))
            .mount(&server)
            .await;
        let error = resolve(&socket, &key, "https://acme.pipedrive.com/deal/42")
            .await
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Decode);
        assert_eq!(paths(requests(&server).await), ["/api/v1/users/me"]);
    }
}

#[tokio::test]
async fn a_link_to_another_companys_record_is_refused_by_a_connection_to_one_company() {
    // The connection calls acme's own host, so it knows which company it is to.
    let acme = Pipedrive::with_token(PipedriveToken {
        token: SecretString::new("pd-secret-token"),
        company_domain: Some("acme".into()),
    });
    let socket = Socket::in_memory().integration(Arc::new(acme)).build().unwrap();
    let error = resolve(&socket, &key(), "https://globex.pipedrive.com/deal/42")
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert!(error.message().contains("another company"), "{}", error.message());
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pipedrives_answers_map_to_the_error_a_caller_can_act_on() {
    let get = |socket: Socket, key: ConnectionKey| async move {
        invoke(&socket, &key, "deals.get", json!({ "deal": 42 }))
            .await
            .unwrap_err()
    };

    // The token is not echoed, whatever Pipedrive writes about it.
    let (_s, socket, key) = answering(failure(401, &format!("unauthorized access: {TOKEN}"))).await;
    let error = get(socket, key).await;
    assert_eq!(error.kind(), ErrorKind::ReconnectRequired);
    assert!(!error.message().contains(TOKEN), "{}", error.message());

    let (_s, socket, key) = answering(failure(403, "Scope and URL mismatch")).await;
    let error = get(socket, key).await;
    assert_eq!(error.kind(), ErrorKind::AccessDenied);
    assert!(error.message().contains("lacks a scope"), "{}", error.message());

    // Pipedrive sends its rate-limit headers on every answer. An exhausted
    // window beside a 403 does not make the refusal a throttle to wait out.
    let refused = failure(403, "You do not have permission to see this deal")
        .insert_header("x-ratelimit-remaining", "0")
        .insert_header("x-ratelimit-reset", "2");
    let (_s, socket, key) = answering(refused).await;
    let error = get(socket, key).await;
    assert_eq!(error.kind(), ErrorKind::AccessDenied);
    assert_eq!(error.retry(), Retry::Never);
    assert!(
        error.message().contains("You do not have permission"),
        "{}",
        error.message()
    );

    let capped = ResponseTemplate::new(403).set_body_json(
        json!({ "success": false, "error": "Open deals limit reached", "code": "feature_capping_deals_limit" }),
    );
    let (_s, socket, key) = answering(capped).await;
    assert!(get(socket, key).await.message().contains("plan has reached a limit"));

    // After repeated throttling the answer is a page, not JSON.
    let blocked = ResponseTemplate::new(403).set_body_raw("<html>Access denied</html>", "text/html");
    let (_s, socket, key) = answering(blocked).await;
    let error = get(socket, key).await;
    assert_eq!(error.kind(), ErrorKind::AccessDenied);
    assert!(
        error.message().contains("repeated rate limiting"),
        "{}",
        error.message()
    );

    let (_s, socket, key) = answering(failure(402, "Company account is not open")).await;
    let error = get(socket, key).await;
    assert_eq!(error.kind(), ErrorKind::AccessDenied);
    assert!(error.message().contains("not open"), "{}", error.message());

    let (_s, socket, key) = answering(failure(404, "Deal not found")).await;
    assert_eq!(get(socket, key).await.kind(), ErrorKind::NotFound);

    // A retired address is not a record that was not found.
    let (_s, socket, key) = answering(failure(410, "Gone")).await;
    let error = get(socket, key).await;
    assert_eq!(error.kind(), ErrorKind::Unexpected);
    assert!(error.message().contains("retired"), "{}", error.message());

    let (_s, socket, key) = answering(failure(400, "Validation failed: stage_id: must be a number")).await;
    let error = invoke(&socket, &key, "deals.update", json!({ "deal": 42, "stage_id": 4 }))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert!(
        error.message().contains("stage_id: must be a number"),
        "{}",
        error.message()
    );

    let (server, socket, key) = answering(failure(500, "Internal error")).await;
    let error = get(socket, key).await;
    assert_eq!(error.kind(), ErrorKind::Unexpected);
    assert_eq!(error.retry(), Retry::Later);
    assert_eq!(requests(&server).await.len(), 2, "a read is tried once more");

    // A write that failed at the server may have happened, and is not sent twice.
    let (server, socket, key) = answering(failure(500, "Internal error")).await;
    invoke(&socket, &key, "deals.create", json!({ "title": "Acme renewal" }))
        .await
        .unwrap_err();
    assert_eq!(requests(&server).await.len(), 1);
}

#[tokio::test]
async fn a_throttle_is_reported_with_the_wait_pipedrive_states_in_either_header() {
    let throttled = |headers: &[(&str, &str)]| {
        headers
            .iter()
            .fold(failure(429, "Rate limit exceeded"), |response, (name, value)| {
                response.insert_header(*name, *value)
            })
    };
    let wait = |response: ResponseTemplate| async move {
        let (server, socket, key) = answering(response).await;
        let error = invoke(&socket, &key, "deals.list", json!({})).await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::RateLimited);
        (error.retry(), requests(&server).await.len())
    };

    // The standard header wins when Pipedrive sends it.
    let both = throttled(&[("retry-after", "30"), ("x-ratelimit-reset", "2")]);
    assert_eq!(wait(both).await.0, Retry::After(Duration::from_secs(30)));
    // Pipedrive documents only its own: the time left in the window.
    let own = throttled(&[("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", "2")]);
    assert_eq!(wait(own).await.0, Retry::After(Duration::from_secs(2)));
    // A window that is already over is waited out at once, and the call sent again.
    let over = throttled(&[("x-ratelimit-reset", "0")]);
    assert_eq!(wait(over).await, (Retry::After(Duration::ZERO), 2));
    // With no wait stated the caller is told to come back later.
    assert_eq!(wait(throttled(&[])).await, (Retry::Later, 2));
    // A value that cannot be a wait within a day is not taken for one.
    for odd in ["1791000000", "-5", "soon", "2.5"] {
        assert_eq!(
            wait(throttled(&[("x-ratelimit-reset", odd)])).await.0,
            Retry::Later,
            "{odd}"
        );
    }
}

#[tokio::test]
async fn a_failure_written_in_a_success_is_a_failure() {
    let said = |body: Value| async move {
        let (_server, socket, key) = answering(ok(body)).await;
        invoke(&socket, &key, "deals.get", json!({ "deal": 42 }))
            .await
            .unwrap_err()
    };
    let error = said(json!({ "success": false, "error": "Deal not found", "errorCode": 404 })).await;
    assert_eq!(error.kind(), ErrorKind::NotFound);
    let error = said(json!({ "success": false, "error": "unauthorized access", "errorCode": 401 })).await;
    assert_eq!(error.kind(), ErrorKind::ReconnectRequired);
    // Without a status of its own it is still not a deal.
    let error = said(json!({ "success": false, "error": "Something went wrong", "data": deal() })).await;
    assert_eq!(error.kind(), ErrorKind::Unexpected);
    assert!(error.message().contains("Something went wrong"), "{}", error.message());
    let error = said(json!({ "success": false, "errorCode": 200 })).await;
    assert_eq!(error.kind(), ErrorKind::Unexpected);

    // A write that Pipedrive says failed is not reported as done, whatever
    // record the answer carries, and is not sent a second time.
    for (name, input) in [
        ("deals.create", json!({ "title": "Acme renewal" })),
        ("deals.update", json!({ "deal": 42, "stage_id": 4 })),
        ("notes.create", json!({ "content": "<p>Call back.</p>", "deal_id": 42 })),
        ("notes.update", json!({ "note": 3, "content": "<p>Call back.</p>" })),
        ("leads.delete", json!({ "lead": LEAD })),
    ] {
        let failed = json!({ "success": false, "error": "The record could not be saved", "data": deal() });
        let (server, socket, key) = answering(ok(failed)).await;
        let error = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Unexpected, "{name}");
        assert!(
            error.message().contains("could not be saved"),
            "{name}: {}",
            error.message()
        );
        assert_eq!(requests(&server).await.len(), 1, "{name}");
    }
    // The status the body names says why, on a write as on a read.
    let refused = json!({ "success": false, "error": "Scope and URL mismatch", "errorCode": 403 });
    let (server, socket, key) = answering(ok(refused)).await;
    let error = invoke(&socket, &key, "deals.update", json!({ "deal": 42, "stage_id": 4 }))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::AccessDenied);
    assert!(error.message().contains("lacks a scope"), "{}", error.message());
    assert_eq!(requests(&server).await.len(), 1);
}

// ── Paging ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_version_2_list_is_paged_by_pipedrives_cursor_passed_back_whole() {
    let (server, socket, key) = pipedrive().await;
    Mock::given(path("/api/v2/deals"))
        .and(socketkit_testkit::wiremock::matchers::query_param(
            "cursor",
            "eyJpZCI6NDJ9",
        ))
        .respond_with(ok(page(json!([]), None)))
        .mount(&server)
        .await;
    Mock::given(path("/api/v2/deals"))
        .respond_with(ok(page(json!([deal()]), Some("eyJpZCI6NDJ9"))))
        .mount(&server)
        .await;

    let first = invoke(&socket, &key, "deals.list", json!({ "status": ["open"], "limit": 1 }))
        .await
        .unwrap();
    assert_eq!(first["next_cursor"], "eyJpZCI6NDJ9");
    let last = invoke(
        &socket,
        &key,
        "deals.list",
        json!({ "status": ["open"], "limit": 1, "cursor": first["next_cursor"] }),
    )
    .await
    .unwrap();
    assert_eq!(last, json!({ "items": [], "next_cursor": null }));
    let sent = requests(&server).await;
    assert_eq!(query_of(&sent[0]), json!({ "status": "open", "limit": "1" }));
    assert_eq!(
        query_of(&sent[1]),
        json!({ "status": "open", "limit": "1", "cursor": "eyJpZCI6NDJ9" }),
        "the filters travel with every page"
    );
}

#[tokio::test]
async fn a_forged_cursor_stays_one_parameter_and_cannot_change_where_the_request_goes() {
    let (server, socket, key) = answering(ok(page(json!([]), None))).await;
    let forged = "x&limit=500&owner_id=1#/../../v1/users?api_token=y";
    invoke(&socket, &key, "deals.list", json!({ "cursor": forged, "owner_id": 7 }))
        .await
        .unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), "/api/v2/deals");
    assert_eq!(request.url.fragment(), None);
    assert_eq!(query_of(&request), json!({ "owner_id": "7", "cursor": forged }));

    // A cursor that is an address is no more than text either.
    let (server, socket, key) = answering(ok(page(json!([]), None))).await;
    invoke(
        &socket,
        &key,
        "persons.list",
        json!({ "cursor": "https://evil.test/api/v2/persons" }),
    )
    .await
    .unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), "/api/v2/persons");
    assert_eq!(
        query_of(&request),
        json!({ "cursor": "https://evil.test/api/v2/persons" })
    );
}

#[tokio::test]
async fn a_version_1_list_is_paged_by_offset_behind_the_same_cursor() {
    let (server, socket, key) = pipedrive().await;
    Mock::given(path("/api/v1/notes"))
        .and(socketkit_testkit::wiremock::matchers::query_param("start", "2"))
        // Version 1 writes `null` for a page with nothing on it.
        .respond_with(ok(offset_page(json!(null), 2, 2, None)))
        .mount(&server)
        .await;
    Mock::given(path("/api/v1/notes"))
        .respond_with(ok(offset_page(
            json!([note("<p>One</p>"), note("<p>Two</p>")]),
            0,
            2,
            Some(2),
        )))
        .mount(&server)
        .await;

    let first = invoke(&socket, &key, "notes.list", json!({ "deal_id": 42, "limit": 2 }))
        .await
        .unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 2);
    assert_eq!(first["next_cursor"], "offset:2", "an offset says that it is one");
    let last = invoke(
        &socket,
        &key,
        "notes.list",
        json!({ "deal_id": 42, "limit": 2, "cursor": first["next_cursor"] }),
    )
    .await
    .unwrap();
    assert_eq!(last, json!({ "items": [], "next_cursor": null }));
    let sent = requests(&server).await;
    assert_eq!(query_of(&sent[0]), json!({ "deal_id": "42", "limit": "2" }));
    assert_eq!(
        query_of(&sent[1]),
        json!({ "deal_id": "42", "limit": "2", "start": "2" })
    );
}

#[tokio::test]
async fn a_cursor_of_the_wrong_kind_or_shape_and_a_limit_out_of_range_are_refused_before_any_request() {
    let (server, socket, key) = answering(ok(page(json!([]), None))).await;
    let refused = async |name: &str, input: Value| {
        let error = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{name} {input}");
        error.message().to_owned()
    };
    // A version 1 list takes only what it gave: `offset:` and digits.
    for forged in [
        "2",
        "eyJpZCI6NDJ9",
        "offset:",
        "offset:-1",
        "offset:2&start=0",
        "offset:2/../../users",
        "offset:99999999999999999999999",
        "https://evil.test/api/v1/notes?start=2",
    ] {
        for list in ["notes.list", "leads.list"] {
            let message = refused(list, json!({ "cursor": forged })).await;
            assert!(message.contains("`cursor`") && !message.contains("evil"), "{message}");
        }
    }
    // A version 2 list does not take an offset for its cursor, nor one with a space in it.
    for list in [
        "deals.list",
        "activities.list",
        "pipelines.list",
        "fields.deal_fields",
        "leads.search",
    ] {
        let mut input = json!({ "cursor": "offset:2" });
        if list == "leads.search" {
            input["term"] = json!("jane");
        }
        refused(list, input).await;
    }
    refused("deals.list", json!({ "cursor": "eyJp ZCI6NDJ9" })).await;

    for (list, limit, range) in [
        ("deals.list", 0, "from 1 to 500"),
        ("deals.list", 501, "from 1 to 500"),
        ("notes.list", 501, "from 1 to 500"),
        ("leads.list", 0, "from 1 to 500"),
        ("pipelines.stages", 501, "from 1 to 500"),
    ] {
        assert!(refused(list, json!({ "limit": limit })).await.contains(range));
    }
    for search in [
        "deals.search",
        "persons.search",
        "organizations.search",
        "leads.search",
        "search.items",
    ] {
        let message = refused(search, json!({ "term": "acme", "limit": 101 })).await;
        assert!(message.contains("from 1 to 100"), "{message}");
    }
    assert!(requests(&server).await.is_empty(), "nothing reached Pipedrive");
}

#[tokio::test]
async fn a_version_1_list_that_says_it_goes_on_without_saying_where_is_not_cut_short_in_silence() {
    let more = json!({ "success": true, "data": [note("<p>One</p>")], "additional_data": { "pagination": { "start": 0, "limit": 1, "more_items_in_collection": true } } });
    let (_server, socket, key) = answering(ok(more)).await;
    let error = invoke(&socket, &key, "notes.list", json!({ "limit": 1 }))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
}

// ── What goes into a path ─────────────────────────────────────────────────────

#[tokio::test]
async fn an_id_that_tries_to_add_a_segment_is_refused_before_any_request() {
    let (server, socket, key) = answering(ok(data(deal()))).await;
    // A lead's id is a UUID and nothing else.
    for bad in [
        "../deals/42",
        "adf21080-0e10-11eb-879b-05d71fb426ec/../../users/me",
        "adf21080-0e10-11eb-879b-05d71fb426ec?x=1",
        "adf21080-0e10-11eb-879b-05d71fb426ec#x",
        "adf21080%2D0e10%2D11eb%2D879b%2D05d71fb426ec",
        "..",
        "",
        "42",
    ] {
        for (name, input) in [
            ("leads.get", json!({ "lead": bad })),
            ("leads.update", json!({ "lead": bad, "title": "x" })),
            ("leads.delete", json!({ "lead": bad })),
        ] {
            let error = invoke(&socket, &key, name, input).await.unwrap_err();
            assert_eq!(error.kind(), ErrorKind::InvalidInput, "{name} {bad:?}");
            assert!(error.message().contains("UUID"), "{}", error.message());
        }
    }
    // Every other id is a number, and only a whole one that is not negative is taken.
    for bad in [
        json!("42/../7"),
        json!("42"),
        json!(-1),
        json!(4.5),
        json!(null),
        json!([42]),
    ] {
        for (name, id) in [
            ("deals.get", "deal"),
            ("deals.delete", "deal"),
            ("persons.get", "person"),
            ("organizations.delete", "organization"),
            ("activities.get", "activity"),
            ("notes.delete", "note"),
        ] {
            let error = invoke(&socket, &key, name, json!({ id: bad })).await.unwrap_err();
            assert_eq!(error.kind(), ErrorKind::InvalidInput, "{name} {bad}");
            assert!(!error.message().contains("42/../7"), "{}", error.message());
        }
    }
    assert!(requests(&server).await.is_empty(), "nothing reached Pipedrive");

    // A UUID is written into the path as it is, in lower case.
    let (server, socket, key) = answering(ok(data(lead()))).await;
    invoke(&socket, &key, "leads.get", json!({ "lead": LEAD.to_uppercase() }))
        .await
        .unwrap();
    assert_eq!(only_request(&server).await.url.path(), format!("/api/v1/leads/{LEAD}"));
}

#[tokio::test]
async fn text_in_a_query_is_percent_encoded_so_it_stays_one_value() {
    let (server, socket, key) = answering(ok(support::found(json!([]), None))).await;
    let term = "R&D + Söhne/2026? #1";
    invoke(&socket, &key, "search.items", json!({ "term": term }))
        .await
        .unwrap();
    let request = only_request(&server).await;
    assert_eq!(query_of(&request), json!({ "term": term }));
    assert_eq!(
        request.url.query(),
        Some("term=R%26D%20%2B%20S%C3%B6hne%2F2026%3F%20%231"),
        "a space is written %20, and a plus stays a plus"
    );
}

#[tokio::test]
async fn a_stored_api_token_serves_each_tenant_with_its_own() {
    // The API-token definition with no token of its own: each tenant's is in the store.
    let server = MockServer::start().await;
    Mock::given(any()).respond_with(ok(data(me()))).mount(&server).await;
    let integration = build(point_at(api_token_provider(), &server));
    let (socket, key) = connect(integration, "pd-tenant-token").await;
    identity(&socket, &key).await.unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.headers.get("x-api-token").unwrap(), "pd-tenant-token");
    assert!(request.headers.get("authorization").is_none());
}
