//! Salesforce against a local server that answers as its sign-in service and an organisation's own host do.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use socketkit_core::{
    AuthScheme, ConnectionKey, ErrorKind, Integration, MemoryTokenStore, OAuthClient, ProviderSpec, Retry, RetryPolicy,
    SecretString, Socket, TokenSet, TokenStore,
};
use socketkit_salesforce::models::QueryOptions;
use socketkit_salesforce::{DEFAULT_API_VERSION, LoginHost, Salesforce, SalesforceOAuth, provider};
use socketkit_testkit::wiremock::matchers::{any, body_string_contains, header, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{TENANT, conformance, connect, point_at};

mod support;
use support::{
    ACCOUNT, API, CASE, TOKEN, USER, account, answering, case, invoke, only_request, query_of, refusal, salesforce,
};

const USERINFO: &str = "/services/oauth2/userinfo";
const TOKEN_ENDPOINT: &str = "/services/oauth2/token";
const ORGANISATION: &str = "00Dxx0000001gPLEAY";
const RECORD: &str = "Account/001xx000003DGb2AAG";
const NEXT: &str = "/services/data/v67.0/query/01gxx0000004RpzAAE-2000";

fn build(spec: ProviderSpec) -> Arc<dyn Integration> {
    Arc::new(Salesforce::with_spec(spec))
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

fn oauth_of(salesforce: &Salesforce) -> socketkit_core::OAuth2Spec {
    match salesforce.provider().auth {
        AuthScheme::OAuth2(oauth) => oauth,
        AuthScheme::ApiKey(_) => panic!("salesforce uses OAuth"),
    }
}

fn authorize_url(salesforce: Salesforce, scopes: Option<Vec<String>>) -> url::Url {
    Socket::in_memory()
        .integration(Arc::new(salesforce))
        .build()
        .unwrap()
        .begin_authorization(key(), scopes)
        .unwrap()
        .url
}

fn param(url: &url::Url, name: &str) -> Option<String> {
    url.query_pairs().find(|(n, _)| n == name).map(|(_, v)| v.into_owned())
}

/// The API address of the organisation that lives on `server`.
fn organisation_at(server: &MockServer) -> url::Url {
    format!("{}{API}/", server.uri()).parse().unwrap()
}

/// Tokens as Salesforce issues them: no stated lifetime, and the address of
/// the organisation they are for.
fn tokens(access: &str, refresh: Option<&str>, organisation: &MockServer) -> TokenSet {
    TokenSet {
        access_token: SecretString::new(access),
        refresh_token: refresh.map(SecretString::new),
        expires_at: None,
        scopes: vec!["api".into(), "refresh_token".into(), "id".into()],
        api_base: Some(organisation_at(organisation)),
    }
}

/// A `Socket` whose Salesforce signs people in at `sign_in` and may call the
/// organisation on `organisation`, with the application's OAuth app.
async fn with_oauth_app(
    sign_in: &MockServer,
    organisation: &MockServer,
    stored: Option<TokenSet>,
) -> (Socket, Arc<MemoryTokenStore>) {
    let store = Arc::new(MemoryTokenStore::new());
    if let Some(tokens) = stored {
        store.save(key(), tokens).await.unwrap();
    }
    let mut spec = point_at(provider(), sign_in);
    // The test's stand-in for the rule that admits an organisation's own host.
    let host = organisation_at(organisation);
    spec.allowed_hosts
        .push(format!("{}:{}", host.host_str().unwrap(), host.port().unwrap()));
    let retry = RetryPolicy {
        max_attempts: 2,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(50),
    };
    let socket = Socket::builder(store.clone())
        .integration(Arc::new(Salesforce::with_spec(spec).oauth(client())))
        .retry(retry)
        .build()
        .unwrap();
    (socket, store)
}

async fn identity(socket: &Socket, key: &ConnectionKey) -> socketkit_core::Result<Value> {
    invoke(socket, key, "identity.get", json!({})).await
}

fn userinfo() -> Value {
    json!({
        "sub": format!("https://login.salesforce.com/id/{ORGANISATION}/{USER}"),
        "user_id": USER,
        "organization_id": ORGANISATION,
        "preferred_username": "ada@acme.example.sandbox",
        "nickname": "ada",
        "name": "Ada Lovelace",
        "email": "ada@acme.example",
        "email_verified": true,
        "zoneinfo": "Europe/London",
        "active": true,
        "user_type": "STANDARD"
    })
}

// ── The definition and its settings ───────────────────────────────────────────

#[tokio::test]
async fn passes_the_conformance_suite() {
    conformance::all(provider(), build, RECORD).await;
}

#[tokio::test]
async fn a_connection_calls_the_organisation_its_authorisation_named_and_no_other() {
    conformance::a_connection_calls_the_host_its_authorization_named(&provider(), &build, |address| {
        json!({
            "access_token": "00D.first",
            "refresh_token": "5Aep.refresh",
            "signature": "c2lnbmF0dXJl",
            "scope": "refresh_token api id",
            "instance_url": address,
            "id": format!("https://login.salesforce.com/id/{ORGANISATION}/{USER}"),
            "token_type": "Bearer",
            "issued_at": "1760000000000"
        })
    })
    .await;
}

#[tokio::test]
async fn the_definition_signs_in_at_login_with_pkce_and_admits_only_an_organisations_own_host() {
    let spec = provider();
    assert_eq!(spec.id.as_str(), "salesforce");
    let AuthScheme::OAuth2(oauth) = &spec.auth else {
        panic!("salesforce uses OAuth")
    };
    assert_eq!(
        oauth.authorize_url.as_str(),
        "https://login.salesforce.com/services/oauth2/authorize"
    );
    assert_eq!(
        oauth.token_url.as_str(),
        "https://login.salesforce.com/services/oauth2/token"
    );
    assert_eq!(oauth.default_scopes, ["api", "refresh_token", "id"]);
    assert_eq!(oauth.scope_separator, " ");
    assert!(oauth.pkce);
    // The version is part of the definition's address, and nothing is read from that host.
    assert_eq!(
        spec.api_base.as_str(),
        format!("https://login.salesforce.com/services/data/v{DEFAULT_API_VERSION}/")
    );

    let an_organisation = |address: &str| spec.allows_api_base(&address.parse().unwrap());
    for own in [
        "https://acme.my.salesforce.com/services/data/v67.0/",
        "https://acme--uat.sandbox.my.salesforce.com/services/data/v67.0/",
        "https://acme-dev-ed.develop.my.salesforce.com/services/data/v67.0/",
        "https://acme.scratch.my.salesforce.com/services/data/v67.0/",
    ] {
        assert!(an_organisation(own), "{own}");
        // Admitted only as the address of the connection that was given it:
        // no other connection's token may be sent there.
        assert!(!spec.allows_host(&own.parse().unwrap()), "{own}");
    }
    for elsewhere in [
        // The older instance hosts, and other products and clouds.
        "https://na1.salesforce.com/services/data/v67.0/",
        "https://acme.lightning.force.com/",
        "https://acme.my.site.com/",
        "https://acme.my.salesforce.mil/services/data/v67.0/",
        "https://my.salesforce.com/",
        "https://acme.my.salesforce.com.evil.example/",
        "http://acme.my.salesforce.com/",
        "https://acme.my.salesforce.com:8443/",
        "https://test.salesforce.com/services/data/v67.0/",
    ] {
        assert!(!an_organisation(elsewhere), "{elsewhere}");
    }
}

#[tokio::test]
async fn the_login_setting_chooses_where_people_sign_in() {
    for (login, host) in [
        (None, "login.salesforce.com"),
        (Some(LoginHost::Production), "login.salesforce.com"),
        (Some(LoginHost::Sandbox), "test.salesforce.com"),
        (
            Some(LoginHost::MyDomain("acme.my.salesforce.com".into())),
            "acme.my.salesforce.com",
        ),
        (
            Some(LoginHost::MyDomain(
                " https://Acme--UAT.sandbox.my.salesforce.com/ ".into(),
            )),
            "acme--uat.sandbox.my.salesforce.com",
        ),
    ] {
        let salesforce = Salesforce::with_oauth(SalesforceOAuth {
            login: login.clone(),
            ..client().into()
        });
        let oauth = oauth_of(&salesforce);
        assert_eq!(
            oauth.token_url.as_str(),
            format!("https://{host}/services/oauth2/token"),
            "{login:?}"
        );
        let spec = salesforce.provider();
        assert!(
            spec.allows_host(&oauth.token_url),
            "{login:?}: the client secret may go there"
        );
        // Only the sign-in addresses move. No organisation's API is assumed.
        assert_eq!(spec.api_base.host_str(), Some("login.salesforce.com"), "{login:?}");
        let url = authorize_url(salesforce, None);
        assert_eq!(url.host_str(), Some(host), "{login:?}");
        assert_eq!(url.path(), "/services/oauth2/authorize");
        assert_eq!(url.scheme(), "https");
    }
}

#[tokio::test]
async fn a_login_host_that_is_not_an_organisations_own_is_refused_when_the_socket_is_built() {
    for bad in [
        "",
        " ",
        "acme",
        "evil.example",
        "acme.my.salesforce.com.evil.example",
        "acme.my.salesforce.com@evil.example",
        "evil.example/acme.my.salesforce.com",
        "evil.example#acme.my.salesforce.com",
        "evil.example?x=.my.salesforce.com",
        "acme.my.salesforce.com/services/oauth2/token",
        "acme.my.salesforce.com:8443",
        "https://user:hunter2@acme.my.salesforce.com",
        "http://acme.my.salesforce.com",
        "my.salesforce.com",
        ".my.salesforce.com",
        "acme..my.salesforce.com",
        "na1.salesforce.com",
        "acme.lightning.force.com",
        "acme.my.salesforce.mil",
    ] {
        let salesforce = Salesforce::new().login(LoginHost::MyDomain(bad.into()));
        let oauth = oauth_of(&salesforce);
        assert_eq!(
            (oauth.authorize_url.host_str(), oauth.token_url.host_str()),
            (Some("login.salesforce.com"), Some("login.salesforce.com")),
            "{bad:?} never reaches an address"
        );
        assert_eq!(salesforce.provider().allowed_hosts, provider().allowed_hosts, "{bad:?}");
        let err = Socket::in_memory()
            .integration(Arc::new(salesforce))
            .build()
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Config, "{bad:?}");
        assert!(!err.message().contains("hunter2"), "{}", err.message());
    }

    // Correcting it clears the problem, and a host that was allowed for an
    // earlier choice does not stay allowed.
    let corrected = Salesforce::new()
        .login(LoginHost::MyDomain("not a host".into()))
        .login(LoginHost::MyDomain("acme.my.salesforce.com".into()))
        .login(LoginHost::Sandbox);
    assert_eq!(oauth_of(&corrected).token_url.host_str(), Some("test.salesforce.com"));
    let spec = corrected.provider();
    assert!(!spec.allows_host(&"https://acme.my.salesforce.com/".parse().unwrap()));
    assert!(spec.allows_host(&"https://test.salesforce.com/".parse().unwrap()));
    Socket::in_memory().integration(Arc::new(corrected)).build().unwrap();

    let back = Salesforce::new().login(LoginHost::Sandbox).login(LoginHost::Production);
    assert_eq!(back.provider(), provider());
}

#[tokio::test]
async fn the_api_version_is_a_setting_and_a_bad_one_is_refused_when_the_socket_is_built() {
    let salesforce = Salesforce::with_oauth(SalesforceOAuth {
        api_version: Some("v66.0".into()),
        ..client().into()
    });
    assert_eq!(salesforce.provider().api_base.path(), "/services/data/v66.0/");
    Socket::in_memory().integration(Arc::new(salesforce)).build().unwrap();

    // A token given directly is aimed at the version too, whichever is set first.
    let fixed = Salesforce::with_token("00D.fixed", "https://acme.my.salesforce.com").api_version("62.0");
    assert_eq!(
        fixed
            .fixed_token()
            .and_then(|token| token.api_base)
            .map(String::from)
            .as_deref(),
        Some("https://acme.my.salesforce.com/services/data/v62.0/")
    );
    let fixed = Salesforce::new()
        .api_version("62.0")
        .token("00D.fixed", "acme.my.salesforce.com");
    assert_eq!(
        fixed
            .fixed_token()
            .and_then(|token| token.api_base)
            .map(String::from)
            .as_deref(),
        Some("https://acme.my.salesforce.com/services/data/v62.0/")
    );

    for bad in [
        "",
        "latest",
        "67",
        "45.0",
        "67.0/../../oauth2/revoke",
        "67.0?x=1",
        "../v67.0",
    ] {
        let salesforce = Salesforce::new().api_version(bad);
        assert_eq!(
            salesforce.provider().api_base,
            provider().api_base,
            "{bad:?} never reaches the address"
        );
        let err = Socket::in_memory()
            .integration(Arc::new(salesforce))
            .build()
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Config, "{bad:?}");
    }
    let corrected = Salesforce::new().api_version("latest").api_version("66.0");
    Socket::in_memory().integration(Arc::new(corrected)).build().unwrap();
}

#[tokio::test]
async fn the_sign_in_page_is_asked_with_pkce_and_always_for_a_refresh_token() {
    let url = authorize_url(Salesforce::with_oauth(client()), None);
    assert_eq!(param(&url, "response_type").as_deref(), Some("code"));
    assert_eq!(param(&url, "client_id").as_deref(), Some("client-id"));
    assert_eq!(
        param(&url, "redirect_uri").as_deref(),
        Some("https://app.example.test/callback")
    );
    assert_eq!(param(&url, "scope").as_deref(), Some("api refresh_token id"));
    assert_eq!(param(&url, "code_challenge_method").as_deref(), Some("S256"));
    // A SHA-256 hash in base64url without padding is 43 characters.
    assert_eq!(param(&url, "code_challenge").map(|challenge| challenge.len()), Some(43));

    // Without a refresh token the connection stops when the organisation's
    // session policy ends the access token, which Salesforce does not announce.
    let settings = SalesforceOAuth {
        scopes: Some(vec!["api".into()]),
        ..client().into()
    };
    let from_settings = authorize_url(Salesforce::with_oauth(settings), None);
    assert_eq!(param(&from_settings, "scope").as_deref(), Some("api refresh_token"));
    let at_the_call = authorize_url(Salesforce::with_oauth(client()), Some(vec!["api".into(), "id".into()]));
    assert_eq!(param(&at_the_call, "scope").as_deref(), Some("api id refresh_token"));
    // `offline_access` is Salesforce's other name for it; it is not asked for twice.
    let named = authorize_url(
        Salesforce::with_oauth(client()),
        Some(vec!["api".into(), "Offline_Access".into()]),
    );
    assert_eq!(param(&named, "scope").as_deref(), Some("api Offline_Access"));
}

// ── Connecting and renewing ──────────────────────────────────────────────────

#[tokio::test]
async fn connecting_exchanges_the_code_with_pkce_and_keeps_the_organisations_address() {
    let sign_in = MockServer::start().await;
    let organisation = MockServer::start().await;
    let (socket, store) = with_oauth_app(&sign_in, &organisation, None).await;

    let authorization = socket.begin_authorization(key(), None).unwrap();
    let verifier = authorization.pending.pkce_verifier.clone().expect("PKCE is on");
    Mock::given(method("POST"))
        .and(path(TOKEN_ENDPOINT))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains(format!("code_verifier={}", verifier.expose())))
        .and(body_string_contains("client_id=client-id"))
        .and(body_string_contains("client_secret=client-secret"))
        .respond_with(ok(json!({
            "access_token": "00D.first",
            "refresh_token": "5Aep.refresh-1",
            "signature": "c2lnbmF0dXJl",
            "scope": "refresh_token api id",
            // Whatever follows the host in what Salesforce names is not kept.
            "instance_url": format!("{}/", organisation.uri()),
            "id": format!("https://login.salesforce.com/id/{ORGANISATION}/{USER}"),
            "token_type": "Bearer",
            "issued_at": "1760000000000"
        })))
        .expect(1)
        .mount(&sign_in)
        .await;
    Mock::given(method("GET"))
        .and(path(USERINFO))
        .and(header("authorization", "Bearer 00D.first"))
        .respond_with(ok(userinfo()))
        .expect(1)
        .mount(&organisation)
        .await;

    let state = authorization.pending.state.clone();
    let granted = socket
        .complete_authorization(authorization.pending, "the-code".into(), state)
        .await
        .unwrap();
    assert_eq!(granted.access_token.expose(), "00D.first");
    assert_eq!(
        granted.refresh_token.as_ref().map(SecretString::expose),
        Some("5Aep.refresh-1")
    );
    assert_eq!(granted.scopes, ["refresh_token", "api", "id"]);
    assert_eq!(granted.expires_at, None, "Salesforce states no lifetime");
    assert_eq!(granted.api_base, Some(organisation_at(&organisation)));
    assert_eq!(store.load(key()).await.unwrap(), Some(granted));

    // The account is read from the organisation's own host, not from where it signed in.
    let account = identity(&socket, &key()).await.unwrap();
    assert_eq!(
        account,
        json!({ "id": format!("{ORGANISATION}/{USER}"), "name": "Ada Lovelace", "email": "ada@acme.example" })
    );
    let at_sign_in = sign_in.received_requests().await.unwrap();
    assert_eq!(at_sign_in.len(), 1, "only the token request went to the sign-in host");
}

#[tokio::test]
async fn a_connection_made_with_a_chosen_api_version_calls_that_version() {
    for (version, at) in [("66.0", "/services/data/v66.0"), ("v58.0", "/services/data/v58.0")] {
        let sign_in = MockServer::start().await;
        let organisation = MockServer::start().await;
        let mut spec = point_at(provider(), &sign_in);
        let host = organisation_at(&organisation);
        spec.allowed_hosts
            .push(format!("{}:{}", host.host_str().unwrap(), host.port().unwrap()));
        let salesforce = Salesforce::with_spec(spec).api_version(version).oauth(client());
        let store = Arc::new(MemoryTokenStore::new());
        let socket = Socket::builder(store.clone())
            .integration(Arc::new(salesforce))
            .build()
            .unwrap();
        Mock::given(method("POST"))
            .and(path(TOKEN_ENDPOINT))
            .respond_with(ok(
                json!({ "access_token": "00D.first", "instance_url": organisation.uri() }),
            ))
            .expect(1)
            .mount(&sign_in)
            .await;
        // Only the chosen version answers: a call to any other is not found.
        Mock::given(method("GET"))
            .and(path(format!("{at}/limits")))
            .respond_with(ok(json!({ "DailyApiRequests": { "Max": 15000, "Remaining": 14000 } })))
            .expect(1)
            .mount(&organisation)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{at}/sobjects/Account/{ACCOUNT}")))
            .respond_with(ok(account()))
            .expect(1)
            .mount(&organisation)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{at}/query/01gxx0000004RpzAAE-2000")))
            .respond_with(ok(json!({ "totalSize": 2001, "done": true, "records": [] })))
            .expect(1)
            .mount(&organisation)
            .await;
        Mock::given(method("GET"))
            .and(path(USERINFO))
            .respond_with(ok(userinfo()))
            .expect(1)
            .mount(&organisation)
            .await;

        let authorization = socket.begin_authorization(key(), None).unwrap();
        let state = authorization.pending.state.clone();
        let granted = socket
            .complete_authorization(authorization.pending, "the-code".into(), state)
            .await
            .unwrap();
        let expected: url::Url = format!("{}{at}/", organisation.uri()).parse().unwrap();
        assert_eq!(granted.api_base, Some(expected.clone()), "{version}");
        assert_eq!(
            store.load(key()).await.unwrap().and_then(|tokens| tokens.api_base),
            Some(expected)
        );

        invoke(&socket, &key(), "limits.get", json!({})).await.unwrap();
        let record = invoke(&socket, &key(), "resource.resolve", json!({ "input": RECORD }))
            .await
            .unwrap();
        assert_eq!(record["id"], RECORD);
        // A cursor from the default version is asked for at this one.
        invoke(&socket, &key(), "query.run", json!({ "cursor": NEXT }))
            .await
            .unwrap();
        // The identity service is not under any version.
        identity(&socket, &key()).await.unwrap();
        let paths: Vec<String> = organisation
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|request| request.url.path().to_owned())
            .collect();
        assert_eq!(
            paths,
            [
                format!("{at}/limits"),
                format!("{at}/sobjects/Account/{ACCOUNT}"),
                format!("{at}/query/01gxx0000004RpzAAE-2000"),
                USERINFO.to_owned()
            ],
            "{version}"
        );
    }
}

#[tokio::test]
async fn an_authorisation_that_names_no_organisation_or_one_that_is_not_an_address_is_refused() {
    for named in [
        json!(null),
        json!(""),
        json!("not a url"),
        json!("https://elsewhere.example"),
        json!(7),
    ] {
        let sign_in = MockServer::start().await;
        let organisation = MockServer::start().await;
        let (socket, store) = with_oauth_app(&sign_in, &organisation, None).await;
        Mock::given(path(TOKEN_ENDPOINT))
            .respond_with(ok(json!({ "access_token": "00D.first", "instance_url": named })))
            .mount(&sign_in)
            .await;
        let authorization = socket.begin_authorization(key(), None).unwrap();
        let state = authorization.pending.state.clone();
        let err = socket
            .complete_authorization(authorization.pending, "the-code".into(), state)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{named}: {err}");
        assert!(!err.message().contains("elsewhere.example"), "{}", err.message());
        assert_eq!(store.load(key()).await.unwrap(), None, "{named}: nothing is stored");
    }
}

#[tokio::test]
async fn a_token_salesforce_ends_without_notice_is_renewed_and_the_call_sent_again_to_the_same_organisation() {
    // The renewal names the organisation again, or it does not. Either way
    // the connection goes on calling the same host.
    for renewed in [
        json!({ "access_token": "00D.second", "token_type": "Bearer", "scope": "refresh_token api id", "issued_at": "1760003600000" }),
        json!({ "access_token": "00D.second", "instance_url": "the organisation", "id": "https://login.salesforce.com/id/00D/005" }),
    ] {
        let sign_in = MockServer::start().await;
        let organisation = MockServer::start().await;
        let mut renewed = renewed;
        if renewed["instance_url"].is_string() {
            renewed["instance_url"] = json!(organisation.uri());
        }
        // The token had no expiry to go by: the organisation's session
        // policy ended it, and the first sign of that is the refusal.
        Mock::given(path(format!("{API}/limits")))
            .and(header("authorization", "Bearer 00D.first"))
            .respond_with(refusal(401, "INVALID_SESSION_ID", "Session expired or invalid"))
            .expect(1)
            .mount(&organisation)
            .await;
        Mock::given(path(format!("{API}/limits")))
            .and(header("authorization", "Bearer 00D.second"))
            .respond_with(ok(json!({ "DailyApiRequests": { "Max": 15000, "Remaining": 14000 } })))
            .expect(1)
            .mount(&organisation)
            .await;
        Mock::given(method("POST"))
            .and(path(TOKEN_ENDPOINT))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("refresh_token=5Aep.refresh-1"))
            .and(body_string_contains("client_secret=client-secret"))
            .respond_with(ok(renewed))
            .expect(1)
            .mount(&sign_in)
            .await;
        let stored = tokens("00D.first", Some("5Aep.refresh-1"), &organisation);
        let (socket, store) = with_oauth_app(&sign_in, &organisation, Some(stored)).await;

        let limits = invoke(&socket, &key(), "limits.get", json!({})).await.unwrap();
        assert_eq!(limits["DailyApiRequests"]["Remaining"], 14000);
        let saved = store.load(key()).await.unwrap().unwrap();
        assert_eq!(saved.access_token.expose(), "00D.second");
        assert_eq!(
            saved.api_base,
            Some(organisation_at(&organisation)),
            "the address is kept"
        );
        // Salesforce sends no new refresh token unless it rotates them.
        assert_eq!(
            saved.refresh_token.as_ref().map(SecretString::expose),
            Some("5Aep.refresh-1")
        );
        assert_eq!(saved.expires_at, None);
    }
}

#[tokio::test]
async fn a_renewal_that_names_a_host_outside_the_definition_is_refused_and_the_tokens_are_kept() {
    let sign_in = MockServer::start().await;
    let organisation = MockServer::start().await;
    Mock::given(any())
        .respond_with(refusal(401, "INVALID_SESSION_ID", "Session expired or invalid"))
        .mount(&organisation)
        .await;
    Mock::given(path(TOKEN_ENDPOINT))
        .respond_with(ok(
            json!({ "access_token": "00D.second", "instance_url": "https://elsewhere.example" }),
        ))
        .mount(&sign_in)
        .await;
    let stored = tokens("00D.first", Some("5Aep.refresh-1"), &organisation);
    let (socket, store) = with_oauth_app(&sign_in, &organisation, Some(stored.clone())).await;
    let err = invoke(&socket, &key(), "limits.get", json!({})).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode, "{err}");
    assert_eq!(store.load(key()).await.unwrap(), Some(stored));
}

#[tokio::test]
async fn a_token_without_a_refresh_token_that_salesforce_ends_asks_the_person_to_reconnect() {
    let (server, socket, key) = salesforce().await;
    Mock::given(any())
        .respond_with(refusal(401, "INVALID_SESSION_ID", "Session expired or invalid"))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "limits.get", json!({})).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ReconnectRequired);
    assert_eq!(err.retry(), Retry::Never);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// ── A connection and its organisation ────────────────────────────────────────

#[tokio::test]
async fn a_connection_that_does_not_know_its_organisation_calls_nothing_and_says_why() {
    // The real definition, and a token stored without the address Salesforce
    // gave with it. The only address left is the sign-in host, and a
    // customer's token is not sent there to be turned away.
    let salesforce = Salesforce::new();
    let (socket, key) = connect(Arc::new(salesforce.clone()), "00D.bare").await;
    for (name, input) in [
        ("identity.get", json!({})),
        ("resource.resolve", json!({ "input": RECORD })),
        ("query.run", json!({ "soql": "SELECT Id FROM Account" })),
        ("search.find", json!({ "text": "Acme" })),
        ("sobjects.list", json!({})),
        (
            "records.create",
            json!({ "object": "Lead", "fields": { "LastName": "Turing" } }),
        ),
        ("records.delete", json!({ "object": "Lead", "id": ACCOUNT })),
        ("limits.get", json!({})),
    ] {
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        // Not a failure to reach a host: nothing was sent.
        assert_eq!(err.kind(), ErrorKind::ReconnectRequired, "{name}: {err}");
        assert!(
            err.message().contains("does not say which organisation"),
            "{name}: {}",
            err.message()
        );
        assert!(!err.message().contains("00D.bare"), "{}", err.message());
    }
    let connection = socket.connection(key).await.unwrap();
    let err = salesforce
        .query(&connection)
        .run("SELECT Id FROM Account", QueryOptions::default())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ReconnectRequired);
}

#[tokio::test]
async fn a_token_given_directly_comes_with_its_organisations_address() {
    let key = key();
    let salesforce = Salesforce::with_token("00D.fixed", "https://acme.my.salesforce.com");
    let socket = Socket::in_memory().integration(Arc::new(salesforce)).build().unwrap();
    let connection = socket.connection(key.clone()).await.unwrap();
    assert_eq!(
        connection.api_base().as_str(),
        format!("https://acme.my.salesforce.com/services/data/v{DEFAULT_API_VERSION}/")
    );

    // An address that is not an organisation's own is refused when the
    // Socket is built, and the token is not kept to be sent anywhere.
    for bad in [
        "",
        "https://login.salesforce.com",
        "https://test.salesforce.com",
        "https://evil.example",
        "https://acme.my.salesforce.com.evil.example",
        "http://acme.my.salesforce.com",
        "https://user:hunter2@acme.my.salesforce.com",
        "https://na1.salesforce.com",
    ] {
        let salesforce = Salesforce::with_token("00D.fixed", bad);
        assert!(salesforce.fixed_token().is_none(), "{bad:?}");
        let err = Socket::in_memory()
            .integration(Arc::new(salesforce))
            .build()
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Config, "{bad:?}");
        assert!(
            !err.message().contains("hunter2") && !err.message().contains("00D.fixed"),
            "{}",
            err.message()
        );
    }

    // Against a local server, the token and the address reach the calls.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{API}/limits")))
        .and(header("authorization", "Bearer 00D.fixed"))
        .respond_with(ok(json!({ "DailyApiRequests": { "Max": 15000, "Remaining": 1 } })))
        .expect(1)
        .mount(&server)
        .await;
    let salesforce = Salesforce::with_spec(point_at(provider(), &server)).token("00D.fixed", &server.uri());
    let socket = Socket::in_memory().integration(Arc::new(salesforce)).build().unwrap();
    let limits = invoke(&socket, &key, "limits.get", json!({})).await.unwrap();
    assert_eq!(limits["DailyApiRequests"]["Remaining"], 1);
}

#[tokio::test]
async fn the_account_is_the_user_within_the_organisation() {
    let (server, socket, key) = salesforce().await;
    Mock::given(method("GET"))
        .and(path(USERINFO))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ok(userinfo()))
        .mount(&server)
        .await;
    let account = identity(&socket, &key).await.unwrap();
    assert_eq!(
        account,
        json!({ "id": format!("{ORGANISATION}/{USER}"), "name": "Ada Lovelace", "email": "ada@acme.example" })
    );
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), USERINFO, "outside the versioned data API");
    assert_eq!(request.url.query(), None, "the token is not put in the address");

    // A user without a name or an email is still an account; one without an
    // organisation is not.
    for (body, expected) in [
        (
            json!({ "user_id": USER, "organization_id": ORGANISATION, "preferred_username": "ada@acme.example" }),
            Some(json!({ "id": format!("{ORGANISATION}/{USER}"), "name": "ada@acme.example", "email": null })),
        ),
        (
            json!({ "user_id": USER, "organization_id": ORGANISATION, "name": " ", "email": "" }),
            Some(json!({ "id": format!("{ORGANISATION}/{USER}"), "name": USER, "email": null })),
        ),
        (json!({ "user_id": USER, "name": "Ada Lovelace" }), None),
        (json!({ "organization_id": ORGANISATION, "name": "Ada Lovelace" }), None),
        (json!({ "user_id": "", "organization_id": ORGANISATION }), None),
    ] {
        let (_server, socket, key) = answering(200, body.clone()).await;
        match (identity(&socket, &key).await, expected) {
            (Ok(account), Some(expected)) => assert_eq!(account, expected, "{body}"),
            (Err(err), None) => assert_eq!(err.kind(), ErrorKind::Decode, "{body}"),
            (outcome, expected) => panic!("{body}: {outcome:?}, expected {expected:?}"),
        }
    }
}

#[tokio::test]
async fn a_record_named_by_type_and_id_or_by_its_link_is_confirmed() {
    for (input, at, record, resource) in [
        (
            RECORD,
            format!("{API}/sobjects/Account/{ACCOUNT}"),
            account(),
            json!({ "id": RECORD, "label": "Acme", "description": "Account record" }),
        ),
        (
            "https://acme.lightning.force.com/lightning/r/Case/500xx000000bcdeAAA/view",
            format!("{API}/sobjects/Case/{CASE}"),
            case(),
            // A case has no name. It has a subject, and a number.
            json!({ "id": format!("Case/{CASE}"), "label": "Pump will not start", "description": "Case record" }),
        ),
        (
            // A 15-character id is answered with the record's 18.
            "Account/001xx000003DGb2",
            format!("{API}/sobjects/Account/001xx000003DGb2"),
            json!({ "attributes": { "type": "Account", "url": format!("{API}/sobjects/Account/{ACCOUNT}") }, "Id": ACCOUNT, "Name": null }),
            json!({ "id": RECORD, "label": ACCOUNT, "description": "Account record" }),
        ),
    ] {
        let (server, socket, key) = salesforce().await;
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(ok(record))
            .mount(&server)
            .await;
        let resolved = invoke(&socket, &key, "resource.resolve", json!({ "input": input }))
            .await
            .unwrap_or_else(|e| panic!("{input}: {e}"));
        assert_eq!(resolved, resource, "{input}");
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    let (server, socket, key) = salesforce().await;
    Mock::given(any())
        .respond_with(refusal(404, "NOT_FOUND", "The requested resource does not exist"))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "resource.resolve", json!({ "input": RECORD }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(err.message(), "that Salesforce record was not found");

    // What does not name a record is refused without a call, and not repeated.
    let (server, socket, key) = answering(200, account()).await;
    for bad in [
        "",
        "Acme",
        "Account/../User/005xx000001SvogAAC",
        "https://evil.example/hunter2",
    ] {
        let err = invoke(&socket, &key, "resource.resolve", json!({ "input": bad }))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{bad:?}");
        assert!(!err.message().contains("hunter2"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ── Salesforce's refusals ────────────────────────────────────────────────────

#[tokio::test]
async fn salesforces_refusals_reach_the_caller_as_errors_they_can_act_on() {
    let update = (
        "records.update",
        json!({ "object": "Account", "id": ACCOUNT, "fields": { "Name": "Acme" } }),
    );
    let query = ("query.run", json!({ "soql": "SELECT Id FORM Account" }));
    for (response, (name, input), kind, says) in [
        // The organisation's allowance is used up: nothing is wrong with the
        // account's rights, and the same call works later.
        (
            refusal(403, "REQUEST_LIMIT_EXCEEDED", "TotalRequests Limit exceeded."),
            query.clone(),
            ErrorKind::RateLimited,
            "the organisation has used up an API allowance (REQUEST_LIMIT_EXCEEDED): TotalRequests Limit exceeded.",
        ),
        (
            refusal(403, "INSUFFICIENT_ACCESS", "insufficient access rights on object id"),
            update.clone(),
            ErrorKind::AccessDenied,
            "denied the request (INSUFFICIENT_ACCESS): insufficient access rights on object id",
        ),
        // The same refusal, as Salesforce sends it for a record the account may not write.
        (
            refusal(
                400,
                "INSUFFICIENT_ACCESS_OR_READONLY",
                "insufficient access rights on object id",
            ),
            update.clone(),
            ErrorKind::AccessDenied,
            "(INSUFFICIENT_ACCESS_OR_READONLY): insufficient access rights on object id",
        ),
        (
            refusal(
                400,
                "INSUFFICIENT_ACCESS_ON_CROSS_REFERENCE_ENTITY",
                "insufficient access rights on cross-reference id",
            ),
            update.clone(),
            ErrorKind::AccessDenied,
            "(INSUFFICIENT_ACCESS_ON_CROSS_REFERENCE_ENTITY): insufficient access rights on cross-reference id",
        ),
        // A field that cannot be written. Salesforce says the same for one
        // field-level security withholds as for one nobody may write, such
        // as an id or a formula, so it is the request that is refused, and
        // Salesforce's own words say what to look at.
        (
            refusal(
                400,
                "INVALID_FIELD_FOR_INSERT_UPDATE",
                "Unable to create/update fields: Name. Please check the security settings of this field and verify that it is read/write for your profile or permission set.",
            ),
            update.clone(),
            ErrorKind::InvalidInput,
            "rejected the request (INVALID_FIELD_FOR_INSERT_UPDATE): Unable to create/update fields: Name. Please check the security settings of this field and verify that it is read/write for your profile or permission set.",
        ),
        // An edition without API access.
        (
            refusal(
                403,
                "API_DISABLED_FOR_ORG",
                "The REST API is not enabled for this Organization.",
            ),
            query.clone(),
            ErrorKind::AccessDenied,
            "(API_DISABLED_FOR_ORG): The REST API is not enabled for this Organization.",
        ),
        (
            refusal(400, "MALFORMED_QUERY", "unexpected token: 'FORM'"),
            query.clone(),
            ErrorKind::InvalidInput,
            "rejected the request (MALFORMED_QUERY): unexpected token: 'FORM'",
        ),
        (
            refusal(400, "REQUIRED_FIELD_MISSING", "Required fields are missing: [LastName]"),
            update.clone(),
            ErrorKind::InvalidInput,
            "(REQUIRED_FIELD_MISSING): Required fields are missing: [LastName]",
        ),
        (
            refusal(404, "NOT_FOUND", "The requested resource does not exist"),
            update.clone(),
            ErrorKind::NotFound,
            "has no such resource",
        ),
        (
            refusal(401, "INVALID_SESSION_ID", "Session expired or invalid"),
            query.clone(),
            ErrorKind::ReconnectRequired,
            "rejected the stored authorization",
        ),
        // More than one record holds the external id. Salesforce lists them; they are not repeated.
        (
            ResponseTemplate::new(300).set_body_json(json!([
                format!("{API}/sobjects/Account/{ACCOUNT}"),
                format!("{API}/sobjects/Account/001xx000003DGb3AAG")
            ])),
            (
                "records.upsert",
                json!({ "object": "Account", "field": "ERP_Id__c", "value": "A-17", "fields": {} }),
            ),
            ErrorKind::InvalidInput,
            "found more than one record with that external id, so none was read or written",
        ),
        (
            ResponseTemplate::new(431),
            query.clone(),
            ErrorKind::InvalidInput,
            "it is too long; a query and its headers have about 16,000 bytes between them",
        ),
    ] {
        let (server, socket, key) = salesforce().await;
        Mock::given(any()).respond_with(response).mount(&server).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), kind, "{err}");
        assert!(err.message().ends_with(says), "{}", err.message());
        assert!(!err.message().contains("001xx000003DGb3AAG"), "{}", err.message());
    }
}

#[tokio::test]
async fn a_used_up_allowance_is_something_to_wait_for_and_a_missing_right_is_not() {
    let (server, socket, key) = salesforce().await;
    Mock::given(any())
        .respond_with(refusal(403, "REQUEST_LIMIT_EXCEEDED", "TotalRequests Limit exceeded."))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "limits.get", json!({})).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimited);
    // Salesforce does not say when the allowance comes back.
    assert_eq!(err.retry(), Retry::Later);

    let (server, socket, key) = salesforce().await;
    Mock::given(any())
        .respond_with(refusal(
            403,
            "INSUFFICIENT_ACCESS",
            "insufficient access rights on object id",
        ))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "limits.get", json!({})).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert_eq!(err.retry(), Retry::Never);
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "it is not tried again"
    );
}

#[tokio::test]
async fn a_token_the_identity_service_refuses_is_renewed_and_not_taken_for_a_missing_permission() {
    // The identity service answers a token it does not accept with a 403 and
    // one word of plain text, where the data API answers 401.
    let bad_token = || ResponseTemplate::new(403).set_body_string("Bad_OAuth_Token");
    let (server, socket, key) = salesforce().await;
    Mock::given(path(USERINFO))
        .respond_with(bad_token())
        .mount(&server)
        .await;
    let err = identity(&socket, &key).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ReconnectRequired, "{err}");

    // With a refresh token, the token is renewed and the account read with the new one.
    let sign_in = MockServer::start().await;
    let organisation = MockServer::start().await;
    Mock::given(path(USERINFO))
        .and(header("authorization", "Bearer 00D.first"))
        .respond_with(bad_token())
        .expect(1)
        .mount(&organisation)
        .await;
    Mock::given(path(USERINFO))
        .and(header("authorization", "Bearer 00D.second"))
        .respond_with(ok(userinfo()))
        .expect(1)
        .mount(&organisation)
        .await;
    Mock::given(path(TOKEN_ENDPOINT))
        .respond_with(ok(json!({ "access_token": "00D.second" })))
        .expect(1)
        .mount(&sign_in)
        .await;
    let stored = tokens("00D.first", Some("5Aep.refresh-1"), &organisation);
    let (socket, _store) = with_oauth_app(&sign_in, &organisation, Some(stored)).await;
    assert_eq!(identity(&socket, &self::key()).await.unwrap()["name"], "Ada Lovelace");
}

#[tokio::test]
async fn a_403_that_is_not_salesforces_own_is_a_refusal_and_the_token_is_left_alone() {
    // A page from a proxy or a network rule, or nothing at all. None of
    // them says the token is at fault, so it is not renewed, the call is not
    // sent again, and nobody is told to reconnect.
    let proxy_page = "<html><body><h1>403 Forbidden</h1>Your IP address is not allowed.</body></html>";
    for (what, refused) in [
        (
            "an HTML page",
            ResponseTemplate::new(403).set_body_raw(proxy_page, "text/html; charset=UTF-8"),
        ),
        ("no body", ResponseTemplate::new(403)),
        (
            "an empty JSON body",
            ResponseTemplate::new(403).set_body_raw("", "application/json"),
        ),
    ] {
        for (name, input) in [
            (
                "records.update",
                json!({ "object": "Account", "id": ACCOUNT, "fields": { "Name": "Acme" } }),
            ),
            ("query.run", json!({ "soql": "SELECT Id FROM Account" })),
            ("identity.get", json!({})),
        ] {
            let sign_in = MockServer::start().await;
            let organisation = MockServer::start().await;
            Mock::given(any())
                .respond_with(refused.clone())
                .mount(&organisation)
                .await;
            Mock::given(any())
                .respond_with(ok(json!({ "access_token": "00D.second" })))
                .mount(&sign_in)
                .await;
            let stored = tokens("00D.first", Some("5Aep.refresh-1"), &organisation);
            let (socket, store) = with_oauth_app(&sign_in, &organisation, Some(stored.clone())).await;

            let err = invoke(&socket, &key(), name, input).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::AccessDenied, "{name}, {what}: {err}");
            assert_eq!(err.retry(), Retry::Never, "{name}, {what}");
            assert!(!err.message().contains("IP address"), "{}", err.message());
            assert!(
                sign_in.received_requests().await.unwrap().is_empty(),
                "{name}, {what}: the token endpoint was called"
            );
            assert_eq!(
                organisation.received_requests().await.unwrap().len(),
                1,
                "{name}, {what}: the call was sent again"
            );
            assert_eq!(store.load(key()).await.unwrap(), Some(stored), "{name}, {what}");
        }
    }
}

#[tokio::test]
async fn a_403_that_says_when_to_come_back_is_a_throttle_and_not_a_token_to_renew() {
    // Whatever it carries, the plain text of the identity service included.
    for refused in [
        ResponseTemplate::new(403).insert_header("retry-after", "3600"),
        ResponseTemplate::new(403)
            .insert_header("retry-after", "3600")
            .set_body_string("Bad_OAuth_Token"),
        refusal(403, "REQUEST_LIMIT_EXCEEDED", "TotalRequests Limit exceeded.").insert_header("retry-after", "3600"),
    ] {
        let sign_in = MockServer::start().await;
        let organisation = MockServer::start().await;
        Mock::given(any()).respond_with(refused).mount(&organisation).await;
        let stored = tokens("00D.first", Some("5Aep.refresh-1"), &organisation);
        let (socket, _store) = with_oauth_app(&sign_in, &organisation, Some(stored)).await;
        let err = identity(&socket, &key()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::RateLimited, "{err}");
        assert_eq!(err.retry(), Retry::After(Duration::from_secs(3600)));
        assert!(sign_in.received_requests().await.unwrap().is_empty());
        // The wait is longer than the transport waits, so it is not tried again.
        assert_eq!(organisation.received_requests().await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn a_token_salesforce_repeats_in_a_refusal_is_not_passed_on() {
    let (server, socket, key) = salesforce().await;
    Mock::given(any())
        .respond_with(refusal(400, "MALFORMED_QUERY", &format!("unexpected token: {TOKEN}")))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "query.run", json!({ "soql": "SELECT" }))
        .await
        .unwrap_err();
    assert!(!format!("{err:?}").contains(TOKEN), "{err:?}");
    assert!(err.message().contains("[redacted]"), "{}", err.message());
}

// ── Paging ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_address_of_the_next_batch_is_the_cursor_and_only_its_locator_is_asked_for() {
    let (server, socket, key) = salesforce().await;
    let soql = "SELECT Id, Name FROM Account";
    Mock::given(method("GET"))
        .and(path(format!("{API}/query")))
        .respond_with(ok(
            json!({ "totalSize": 2001, "done": false, "nextRecordsUrl": NEXT, "records": [account()] }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{API}/query/01gxx0000004RpzAAE-2000")))
        .respond_with(ok(json!({ "totalSize": 2001, "done": true, "records": [account()] })))
        .expect(1)
        .mount(&server)
        .await;

    let first = invoke(&socket, &key, "query.run", json!({ "soql": soql }))
        .await
        .unwrap();
    assert_eq!(
        (first["done"].clone(), first["next_cursor"].clone()),
        (json!(false), json!(NEXT))
    );
    // The query is not sent again, nor is a batch size: the cursor is Salesforce's place in the results.
    let input = json!({ "cursor": first["next_cursor"], "batch_size": 500 });
    let last = invoke(&socket, &key, "query.run", input).await.unwrap();
    assert_eq!(
        (last["done"].clone(), last["next_cursor"].clone()),
        (json!(true), json!(null))
    );
    assert_eq!(last["totalSize"], 2001);

    let received = server.received_requests().await.unwrap();
    assert_eq!(query_of(&received[0]), json!({ "q": soql }));
    assert_eq!(received[1].url.query(), None);
    assert!(received[1].headers.get("sforce-query-options").is_none());

    // The results of `run_all` go on at the same place, as Salesforce writes
    // it, and a blank cursor is the first batch.
    let (server, socket, key) = answering(200, json!({ "totalSize": 0, "done": true, "records": [] })).await;
    invoke(&socket, &key, "query.run_all", json!({ "soql": soql, "cursor": NEXT }))
        .await
        .unwrap();
    invoke(&socket, &key, "query.run_all", json!({ "soql": soql, "cursor": "  " }))
        .await
        .unwrap();
    let received = server.received_requests().await.unwrap();
    assert_eq!(received[0].url.path(), format!("{API}/query/01gxx0000004RpzAAE-2000"));
    assert_eq!(
        received[0].url.query(),
        None,
        "a query given beside a cursor is not sent"
    );
    assert_eq!(received[1].url.path(), format!("{API}/queryAll"));
}

#[tokio::test]
async fn a_cursor_that_was_not_salesforces_is_refused_and_nothing_is_called() {
    let (server, socket, key) = answering(200, json!({ "totalSize": 0, "done": true, "records": [] })).await;
    for forged in [
        // Another host, the sign-in host and this very server among them.
        "https://evil.example/services/data/v67.0/query/01gxx0000004RpzAAE-2000".to_owned(),
        "https://login.salesforce.com/services/oauth2/revoke".to_owned(),
        format!("{}{NEXT}", server.uri()),
        "//evil.example/services/data/v67.0/query/01gxx0000004RpzAAE-2000".to_owned(),
        // Another address on the organisation's own host.
        "/services/data/v67.0/sobjects/User/005xx000001SvogAAC".to_owned(),
        "/services/oauth2/userinfo".to_owned(),
        "/services/data/v67.0/query/../sobjects/User/005xx000001SvogAAC".to_owned(),
        "/services/data/v67.0/query/01gxx0000004RpzAAE-2000/../../sobjects".to_owned(),
        "/services/data/v67.0/query/01gxx0000004RpzAAE-2000?q=SELECT+Id+FROM+User".to_owned(),
        "/services/data/v67.0/query/01gxx%2F..%2F..%2Fsobjects".to_owned(),
        "/services/data/v67.0/tooling/query/01gxx0000004RpzAAE-2000".to_owned(),
        "01gxx0000004RpzAAE-2000".to_owned(),
        "SELECT Id FROM User".to_owned(),
    ] {
        for name in ["query.run", "query.run_all"] {
            // With the query beside it or without: a cursor that is not
            // one is refused, never set aside for the query.
            for input in [
                json!({ "soql": "SELECT Id FROM Account", "cursor": forged }),
                json!({ "cursor": forged }),
            ] {
                let err = invoke(&socket, &key, name, input).await.unwrap_err();
                assert_eq!(err.kind(), ErrorKind::InvalidInput, "{forged}");
                assert!(
                    err.message().contains("not the address of a next batch"),
                    "{}",
                    err.message()
                );
                assert!(!err.message().contains("evil.example"), "{}", err.message());
            }
        }
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // Whatever version a cursor names, the batch is asked for at the
    // connection's own: the cursor gives a locator and nothing else.
    let other_version = "/services/data/v20.0/query/01gxx0000004RpzAAE-2000";
    invoke(&socket, &key, "query.run", json!({ "cursor": other_version }))
        .await
        .unwrap();
    assert_eq!(
        only_request(&server).await.url.path(),
        format!("{API}/query/01gxx0000004RpzAAE-2000")
    );
}

// ── Names, ids and fields ────────────────────────────────────────────────────

#[tokio::test]
async fn a_type_a_field_or_an_id_that_would_add_to_the_address_is_refused() {
    let (server, socket, key) = answering(200, account()).await;
    let fields = json!({ "Name": "Acme" });
    for (name, input) in [
        ("records.get", json!({ "object": "Account/../User", "id": ACCOUNT })),
        ("records.get", json!({ "object": "Account/describe", "id": ACCOUNT })),
        ("records.get", json!({ "object": "..", "id": ACCOUNT })),
        (
            "records.get",
            json!({ "object": "Account?fields=Password", "id": ACCOUNT }),
        ),
        ("records.get", json!({ "object": "Account#", "id": ACCOUNT })),
        ("records.get", json!({ "object": "Account%2F..", "id": ACCOUNT })),
        ("records.get", json!({ "object": "Acc ount", "id": ACCOUNT })),
        ("records.get", json!({ "object": "Account", "id": "../User/005xx0000" })),
        (
            "records.get",
            json!({ "object": "Account", "id": "001xx000003DGb2AAG/describe" }),
        ),
        (
            "records.get",
            json!({ "object": "Account", "id": "001xx000003DGb2AA?" }),
        ),
        ("records.get", json!({ "object": "Account", "id": "describe" })),
        ("records.get", json!({ "object": "Account", "id": ".." })),
        (
            "records.get",
            json!({ "object": "Account", "id": ACCOUNT, "fields": ["Name", "Id&x=1"] }),
        ),
        (
            "records.get",
            json!({ "object": "Account", "id": ACCOUNT, "fields": ["Name,Password"] }),
        ),
        (
            "records.get",
            json!({ "object": "Account", "id": ACCOUNT, "fields": ["Owner..Name"] }),
        ),
        (
            "records.get_by_external_id",
            json!({ "object": "Account", "field": "ERP_Id__c/../Id", "value": "A-17" }),
        ),
        (
            "records.get_by_external_id",
            json!({ "object": "Account", "field": "..", "value": "A-17" }),
        ),
        (
            "records.get_by_external_id",
            json!({ "object": "Account", "field": "ERP_Id__c", "value": ".." }),
        ),
        (
            "records.get_by_external_id",
            json!({ "object": "Account", "field": "ERP_Id__c", "value": "." }),
        ),
        (
            "records.create",
            json!({ "object": "Account/001xx000003DGb2AAG", "fields": fields }),
        ),
        (
            "records.update",
            json!({ "object": "Account", "id": "001xx000003DGb2AAG/../001xx000003DGb3AAG", "fields": fields }),
        ),
        (
            "records.upsert",
            json!({ "object": "Account", "field": "Id/001xx000003DGb2AAG", "value": "x", "fields": fields }),
        ),
        ("records.delete", json!({ "object": "Account", "id": "" })),
        (
            "records.delete",
            json!({ "object": "Account/001xx000003DGb2AAG", "id": ACCOUNT }),
        ),
        (
            "records.delete",
            json!({ "object": "Account", "id": "001xx000003DGb2AAG/../.." }),
        ),
        ("sobjects.describe", json!({ "object": "Account/describe/layouts" })),
        ("sobjects.describe", json!({ "object": "../limits" })),
        (
            "search.find",
            json!({ "text": "Acme", "objects": [{ "name": "Account)(" }] }),
        ),
        (
            "search.find",
            json!({ "text": "Acme", "objects": [{ "name": "Account", "fields": ["Name; DROP"] }] }),
        ),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
        // What was refused is not repeated.
        assert!(
            !err.message().contains("Password") && !err.message().contains("DROP"),
            "{}",
            err.message()
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // The value of an external id is another system's text. It is not
    // refused for what it holds: it is written as one segment.
    for (value, written) in [
        ("A-17/../../limits", "A-17%2F..%2F..%2Flimits"),
        ("a?b#c", "a%3Fb%23c"),
        ("zoë@acme.example", "zo%C3%AB%40acme.example"),
        ("100% sure", "100%25%20sure"),
    ] {
        let (server, socket, key) = answering(200, account()).await;
        let input = json!({ "object": "Account", "field": "ERP_Id__c", "value": value });
        invoke(&socket, &key, "records.get_by_external_id", input)
            .await
            .unwrap();
        let request = only_request(&server).await;
        assert_eq!(
            request.url.path(),
            format!("{API}/sobjects/Account/ERP_Id__c/{written}"),
            "{value}"
        );
        assert_eq!(request.url.query(), None, "{value}");
    }
}

#[tokio::test]
async fn a_field_the_operation_does_not_know_is_refused_and_named() {
    // A key that is not known would be dropped without a word, and what it
    // said with it: the fields to return, a limit, the size of a batch.
    let (server, socket, key) = answering(200, account()).await;
    for (name, input, field) in [
        (
            "query.run",
            json!({ "soql": "SELECT Id FROM Account", "limit": 5 }),
            "limit",
        ),
        (
            "query.run",
            json!({ "soql": "SELECT Id FROM Account", "batchSize": 200 }),
            "batchSize",
        ),
        (
            "query.run",
            json!({ "q": "SELECT Id FROM Account", "soql": "SELECT Id FROM Account" }),
            "q",
        ),
        (
            "query.run_all",
            json!({ "soql": "SELECT Id FROM Account", "nextRecordsUrl": NEXT }),
            "nextRecordsUrl",
        ),
        ("search.run", json!({ "sosl": "FIND {Acme}", "limit": 5 }), "limit"),
        (
            "search.find",
            json!({ "text": "Acme", "sobjects": [{ "name": "Account" }] }),
            "sobjects",
        ),
        (
            "search.find",
            json!({ "text": "Acme", "overallLimit": 5 }),
            "overallLimit",
        ),
        // The same below the top, where it is said in which place.
        (
            "search.find",
            json!({ "text": "Acme", "objects": [{ "name": "Account" }, { "name": "Lead", "where": "IsConverted = false" }] }),
            "objects[1].where",
        ),
        ("sobjects.list", json!({ "queryable": true }), "queryable"),
        (
            "sobjects.describe",
            json!({ "object": "Account", "fields": ["Name"] }),
            "fields",
        ),
        (
            "records.get",
            json!({ "object": "Account", "id": ACCOUNT, "select": ["Name"] }),
            "select",
        ),
        // A record's values go in `fields`, not beside the arguments.
        (
            "records.create",
            json!({ "object": "Account", "fields": {}, "Name": "Acme" }),
            "Name",
        ),
        (
            "records.update",
            json!({ "object": "Account", "id": ACCOUNT, "fields": { "Name": "x" }, "allOrNone": true }),
            "allOrNone",
        ),
        (
            "records.upsert",
            json!({ "object": "Account", "field": "ERP_Id__c", "value": "A-17", "fields": {}, "updateOnly": true }),
            "updateOnly",
        ),
        (
            "records.delete",
            json!({ "object": "Account", "id": ACCOUNT, "permanent": true }),
            "permanent",
        ),
        ("limits.get", json!({ "verbose": true }), "verbose"),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
        assert!(
            err.message().contains(&format!("`{field}`")),
            "{name}: the field is named: {}",
            err.message()
        );
    }

    // The name of a field is the caller's own text. One that does not look
    // like a name is not repeated, and neither is any value.
    for input in [
        json!({ "object": "Account", "id": ACCOUNT, "my password is hunter2": true }),
        json!({ "object": "Account", "id": ACCOUNT, "x": "hunter2", "a-very-long-key-that-goes-on-and-on-and-on-well-past-forty-characters": 1 }),
    ] {
        let err = invoke(&socket, &key, "records.get", input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(!err.message().contains("hunter2"), "{}", err.message());
        assert!(!err.message().contains("past-forty"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // What a record holds is the organisation's to define, so its own
    // fields are passed on whatever they are called.
    let (server, socket, key) = answering(201, json!({ "id": ACCOUNT, "success": true, "errors": [] })).await;
    let odd = json!({ "object": "Account", "fields": { "Anything__c": 1, "attributes": { "type": "Account" }, "Parent": { "ERP_Id__c": "A-1" } } });
    invoke(&socket, &key, "records.create", odd).await.unwrap();
    assert_eq!(
        support::body_of(&only_request(&server).await),
        json!({ "Anything__c": 1, "attributes": { "type": "Account" }, "Parent": { "ERP_Id__c": "A-1" } })
    );
}
