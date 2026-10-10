//! Microsoft against a local server that answers as Microsoft Graph and the identity platform do.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use socketkit_core::{
    AuthScheme, ConnectionKey, ErrorKind, Integration, MemoryTokenStore, OAuthClient, ProviderSpec, Retry, RetryPolicy,
    SecretString, Socket, TokenSet, TokenStore,
};
use socketkit_microsoft::{Microsoft, MicrosoftOAuth, Prompt, provider};
use socketkit_testkit::wiremock::matchers::{body_string_contains, header, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{TENANT, conformance, connect, point_at};

const LINK: &str = "https://contoso.sharepoint.com/:w:/s/team/EabcDEF?e=x1";
/// `LINK` as Graph wants it: `u!`, then the link in base64url without padding.
const SHARE: &str = "u!aHR0cHM6Ly9jb250b3NvLnNoYXJlcG9pbnQuY29tLzp3Oi9zL3RlYW0vRWFiY0RFRj9lPXgx";

fn build(spec: ProviderSpec) -> Arc<dyn Integration> {
    Arc::new(Microsoft::with_spec(spec))
}

async fn microsoft() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let (socket, key) = connect(build(point_at(provider(), &server)), "eyJ.good").await;
    (server, socket, key)
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn graph_error(status: u16, code: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({ "error": { "code": code, "message": message } }))
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

/// A `Socket` whose Microsoft integration holds the application's OAuth app
/// and signs people in to the `contoso.example` tenant on `server`.
async fn with_oauth_app(server: &MockServer, stored: Option<TokenSet>) -> (Socket, Arc<MemoryTokenStore>) {
    let store = Arc::new(MemoryTokenStore::new());
    if let Some(tokens) = stored {
        store.save(key(), tokens).await.unwrap();
    }
    let microsoft = Microsoft::with_spec(point_at(provider(), server))
        .tenant("contoso.example")
        .oauth(client());
    let retry = RetryPolicy {
        max_attempts: 2,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(50),
    };
    let socket = Socket::builder(store.clone())
        .integration(Arc::new(microsoft))
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
        scopes: vec!["User.Read".into()],
    }
}

fn authorize_url(microsoft: Microsoft, scopes: Option<Vec<String>>) -> url::Url {
    Socket::in_memory()
        .integration(Arc::new(microsoft))
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
        .invoke(key.clone(), "microsoft.identity.get".into(), json!({}))
        .await
}

async fn resolve(socket: &Socket, key: &ConnectionKey, input: &str) -> socketkit_core::Result<Value> {
    socket
        .invoke(
            key.clone(),
            "microsoft.resource.resolve".into(),
            json!({ "input": input }),
        )
        .await
}

// ── The definition and its settings ───────────────────────────────────────────

#[tokio::test]
async fn passes_the_conformance_suite() {
    conformance::all(provider(), build, LINK).await;
}

#[tokio::test]
async fn the_definition_is_graph_v1_signed_in_through_the_identity_platform_with_pkce() {
    let spec = provider();
    assert_eq!(spec.api_base.as_str(), "https://graph.microsoft.com/v1.0/");
    let AuthScheme::OAuth2(oauth) = &spec.auth else {
        panic!("microsoft uses OAuth")
    };
    assert_eq!(
        oauth.authorize_url.as_str(),
        "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
    );
    assert_eq!(
        oauth.token_url.as_str(),
        "https://login.microsoftonline.com/common/oauth2/v2.0/token"
    );
    assert_eq!(oauth.default_scopes, ["offline_access", "User.Read"]);
    assert_eq!(oauth.scope_separator, " ");
    assert!(oauth.pkce);

    let allows = |u: &str| spec.allows_host(&u.parse().unwrap());
    assert!(allows("https://graph.microsoft.com/v1.0/me"));
    assert!(allows("https://login.microsoftonline.com/common/oauth2/v2.0/token"));
    // The national clouds and Outlook's own hosts are other services.
    for elsewhere in [
        "https://graph.microsoft.us/v1.0/me",
        "https://microsoftgraph.chinacloudapi.cn/v1.0/me",
        "https://outlook.office.com/api/v2.0/me",
        "https://contoso.sharepoint.com/",
    ] {
        assert!(!allows(elsewhere), "{elsewhere}");
    }
}

#[tokio::test]
async fn the_tenant_setting_chooses_who_may_sign_in() {
    for tenant in [
        "organizations",
        "consumers",
        "contoso.onmicrosoft.com",
        "8eaef023-2b34-4da1-9baa-8bc8c9d6a490",
    ] {
        let settings = MicrosoftOAuth {
            tenant: Some(tenant.into()),
            ..client().into()
        };
        let microsoft = Microsoft::with_oauth(settings);
        let AuthScheme::OAuth2(oauth) = microsoft.provider().auth else {
            panic!("microsoft uses OAuth")
        };
        assert_eq!(
            oauth.token_url.as_str(),
            format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token")
        );
        let url = authorize_url(microsoft, None);
        assert_eq!(url.host_str(), Some("login.microsoftonline.com"));
        assert_eq!(url.path(), format!("/{tenant}/oauth2/v2.0/authorize"));
    }
    // A tenant id is not case sensitive, and is written one way.
    let shouted = Microsoft::new().tenant(" Contoso.OnMicrosoft.com ");
    assert_eq!(
        authorize_url(shouted.oauth(client()), None).path(),
        "/contoso.onmicrosoft.com/oauth2/v2.0/authorize"
    );
}

#[tokio::test]
async fn a_tenant_that_could_change_the_sign_in_address_is_refused_when_the_socket_is_built() {
    for bad in [
        "",
        " ",
        "contoso/../common",
        "..",
        "contoso.example/oauth2",
        "https://login.microsoftonline.com/common",
        "contoso example",
        "common?domain_hint=evil.test",
        "common#",
        "ada@contoso.example",
        ".contoso.example",
        "contoso..example",
        "-contoso",
        // One word that is not one of the three: neither an id nor a domain.
        "contoso",
        "commons",
        "8eaef023-2b34-4da1-9baa",
        "8eaef0232b344da19baa8bc8c9d6a490",
        "8eaef023-2b34-4da1-9baa-8bc8c9d6a49g",
        // A label longer than a domain name allows.
        "a123456789012345678901234567890123456789012345678901234567890123.example",
    ] {
        let microsoft = Microsoft::new().tenant(bad);
        let AuthScheme::OAuth2(oauth) = microsoft.provider().auth else {
            panic!("microsoft uses OAuth")
        };
        assert_eq!(
            oauth.token_url.path(),
            "/common/oauth2/v2.0/token",
            "{bad:?} never reaches the address"
        );
        let err = Socket::in_memory()
            .integration(Arc::new(microsoft))
            .build()
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Config, "{bad:?}");
    }
    // Correcting it clears the problem.
    let corrected = Microsoft::new().tenant("not a tenant").tenant("organizations");
    Socket::in_memory().integration(Arc::new(corrected)).build().unwrap();
}

#[tokio::test]
async fn the_sign_in_page_takes_a_login_hint_a_prompt_and_the_applications_own_scopes() {
    let settings = MicrosoftOAuth {
        client: client(),
        scopes: Some(vec![
            "offline_access".into(),
            "User.Read".into(),
            "Calendars.Read".into(),
        ]),
        tenant: None,
        login_hint: Some("ada@contoso.example".into()),
        prompt: Some(Prompt::SelectAccount),
    };
    let url = authorize_url(Microsoft::with_oauth(settings), None);
    assert_eq!(url.path(), "/common/oauth2/v2.0/authorize", "the default tenant");
    assert_eq!(
        param(&url, "scope").as_deref(),
        Some("offline_access User.Read Calendars.Read")
    );
    assert_eq!(param(&url, "login_hint").as_deref(), Some("ada@contoso.example"));
    assert_eq!(param(&url, "prompt").as_deref(), Some("select_account"));
    assert_eq!(param(&url, "code_challenge_method").as_deref(), Some("S256"));

    let plain = authorize_url(Microsoft::with_oauth(client()), None);
    assert_eq!(param(&plain, "scope").as_deref(), Some("offline_access User.Read"));
    assert_eq!(param(&plain, "login_hint"), None);
    assert_eq!(param(&plain, "prompt"), None);

    for (prompt, sent) in [
        (Prompt::Login, "login"),
        (Prompt::None, "none"),
        (Prompt::Consent, "consent"),
    ] {
        let settings = MicrosoftOAuth {
            prompt: Some(prompt),
            ..client().into()
        };
        let url = authorize_url(Microsoft::with_oauth(settings), None);
        assert_eq!(param(&url, "prompt").as_deref(), Some(sent));
    }
}

#[tokio::test]
async fn a_refresh_token_is_always_asked_for_whichever_scopes_are_given() {
    // Without `offline_access` Microsoft issues no refresh token, and the
    // connection would stop working within the hour.
    let settings = MicrosoftOAuth {
        scopes: Some(vec!["Calendars.Read".into()]),
        ..client().into()
    };
    let from_settings = authorize_url(Microsoft::with_oauth(settings), None);
    assert_eq!(
        param(&from_settings, "scope").as_deref(),
        Some("Calendars.Read offline_access")
    );

    let at_the_call = authorize_url(Microsoft::with_oauth(client()), Some(vec!["Mail.Read".into()]));
    assert_eq!(
        param(&at_the_call, "scope").as_deref(),
        Some("Mail.Read offline_access")
    );

    let already = authorize_url(
        Microsoft::with_oauth(client()),
        Some(vec!["Offline_Access".into(), "Mail.Read".into()]),
    );
    assert_eq!(
        param(&already, "scope").as_deref(),
        Some("Offline_Access Mail.Read"),
        "it is not asked for twice"
    );
}

// ── Connecting and refreshing ────────────────────────────────────────────────

#[tokio::test]
async fn connecting_exchanges_the_code_at_the_tenants_token_endpoint_with_pkce() {
    let server = MockServer::start().await;
    let (socket, store) = with_oauth_app(&server, None).await;

    let authorization = socket.begin_authorization(key(), None).unwrap();
    assert_eq!(authorization.url.path(), "/contoso.example/oauth2/v2.0/authorize");
    let verifier = authorization.pending.pkce_verifier.clone().expect("PKCE is on");

    Mock::given(method("POST"))
        .and(path("/contoso.example/oauth2/v2.0/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains(format!("code_verifier={}", verifier.expose())))
        .and(body_string_contains("client_id=client-id"))
        .and(body_string_contains("client_secret=client-secret"))
        .respond_with(ok(json!({
            "token_type": "Bearer",
            "scope": "User.Read profile openid email",
            "expires_in": 3599,
            "access_token": "eyJ.first",
            "refresh_token": "0.refresh-1"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let state = authorization.pending.state.clone();
    let granted = socket
        .complete_authorization(authorization.pending, "the-code".into(), state)
        .await
        .unwrap();
    assert_eq!(granted.access_token.expose(), "eyJ.first");
    assert_eq!(
        granted.refresh_token.as_ref().map(SecretString::expose),
        Some("0.refresh-1")
    );
    assert_eq!(granted.scopes, ["User.Read", "profile", "openid", "email"]);
    assert_eq!(store.load(key()).await.unwrap(), Some(granted));
}

#[tokio::test]
async fn an_expired_token_is_refreshed_and_the_rotated_refresh_token_is_saved() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/contoso.example/oauth2/v2.0/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=0.refresh-1"))
        .respond_with(ok(json!({
            "token_type": "Bearer",
            "scope": "User.Read",
            "expires_in": 3599,
            "access_token": "eyJ.second",
            "refresh_token": "0.refresh-2"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/v1.0/me"))
        .and(header("authorization", "Bearer eyJ.second"))
        .respond_with(ok(
            json!({ "id": "u-1", "displayName": "Ada Lovelace", "mail": "ada@contoso.example" }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let (socket, store) = with_oauth_app(&server, Some(tokens("eyJ.first", "0.refresh-1", -10))).await;

    assert_eq!(identity(&socket, &key()).await.unwrap()["id"], "u-1");
    let saved = store.load(key()).await.unwrap().unwrap();
    assert_eq!(saved.access_token.expose(), "eyJ.second");
    assert_eq!(
        saved.refresh_token.as_ref().map(SecretString::expose),
        Some("0.refresh-2"),
        "the old refresh token is spent; only the new one works from now on"
    );
}

#[tokio::test]
async fn a_token_graph_calls_invalid_is_renewed_once_and_the_call_sent_again() {
    let server = MockServer::start().await;
    // Graph has revoked the access token although it has not expired.
    Mock::given(path("/v1.0/me"))
        .and(header("authorization", "Bearer eyJ.first"))
        .respond_with(graph_error(
            401,
            "InvalidAuthenticationToken",
            "Lifetime validation failed, the token is expired.",
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/v1.0/me"))
        .and(header("authorization", "Bearer eyJ.second"))
        .respond_with(ok(json!({ "id": "u-1", "displayName": "Ada Lovelace" })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/contoso.example/oauth2/v2.0/token"))
        .respond_with(ok(
            json!({ "access_token": "eyJ.second", "refresh_token": "0.refresh-2", "expires_in": 3599 }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let (socket, store) = with_oauth_app(&server, Some(tokens("eyJ.first", "0.refresh-1", 3000))).await;

    assert_eq!(identity(&socket, &key()).await.unwrap()["id"], "u-1");
    let saved = store.load(key()).await.unwrap().unwrap();
    assert_eq!(
        saved.refresh_token.as_ref().map(SecretString::expose),
        Some("0.refresh-2")
    );
}

/// What the token endpoint answers when a permission still needs approval,
/// in the shape it takes when Microsoft gives the error no number.
fn consent_required_unnumbered() -> ResponseTemplate {
    ResponseTemplate::new(400).set_body_json(json!({
        "error": "consent_required",
        "error_description": "The request requires user consent. Trace ID: 0000"
    }))
}

/// What the token endpoint answers when a permission still needs approval.
fn consent_needed(error: &str, code: u32, description: &str) -> ResponseTemplate {
    ResponseTemplate::new(400).set_body_json(json!({
        "error": error,
        "error_description": format!("AADSTS{code}: {description} Trace ID: 0000 Correlation ID: 1111"),
        "error_codes": [code],
        "timestamp": "2026-10-10 10:00:00Z",
        "trace_id": "0000",
        "correlation_id": "1111"
    }))
}

#[tokio::test]
async fn a_permission_nobody_has_approved_is_a_refusal_that_says_an_administrator_must_approve_it() {
    for (error, code) in [
        ("invalid_grant", 65001),
        ("interaction_required", 65001),
        ("invalid_grant", 90094),
        ("consent_required", 65001),
    ] {
        // While refreshing: the stored tokens are kept, because approving the
        // permission makes them work again.
        let server = MockServer::start().await;
        Mock::given(path("/contoso.example/oauth2/v2.0/token"))
            .respond_with(consent_needed(
                error,
                code,
                "The user or administrator has not consented.",
            ))
            .expect(1)
            .mount(&server)
            .await;
        let (socket, store) = with_oauth_app(&server, Some(tokens("eyJ.first", "0.refresh-1", -10))).await;
        let err = identity(&socket, &key()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::AccessDenied, "{error} {code}: {err}");
        assert!(err.message().contains("administrator"), "{}", err.message());
        assert!(err.message().contains(&format!("AADSTS{code}")), "{}", err.message());
        // Only 90094 and 90095 say that the person cannot approve it themselves.
        assert_eq!(
            err.message().contains("the person"),
            code == 65001,
            "{code}: {}",
            err.message()
        );
        assert!(
            !err.message().contains("Trace ID"),
            "Microsoft's own text is not repeated: {}",
            err.message()
        );
        assert_eq!(
            store.load(key()).await.unwrap().unwrap().access_token.expose(),
            "eyJ.first"
        );

        // While connecting: nothing is saved.
        let server = MockServer::start().await;
        Mock::given(path("/contoso.example/oauth2/v2.0/token"))
            .respond_with(consent_needed(error, code, "Administrator consent is required."))
            .mount(&server)
            .await;
        let (socket, store) = with_oauth_app(&server, None).await;
        let authorization = socket.begin_authorization(key(), None).unwrap();
        let state = authorization.pending.state.clone();
        let err = socket
            .complete_authorization(authorization.pending, "the-code".into(), state)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::AccessDenied, "{error} {code}: {err}");
        assert!(err.message().contains("administrator"), "{}", err.message());
        assert_eq!(store.load(key()).await.unwrap(), None);
    }
}

#[tokio::test]
async fn consent_is_recognised_by_its_name_when_microsoft_gives_no_number() {
    let server = MockServer::start().await;
    Mock::given(path("/contoso.example/oauth2/v2.0/token"))
        .respond_with(consent_required_unnumbered())
        .mount(&server)
        .await;
    let (socket, store) = with_oauth_app(&server, Some(tokens("eyJ.first", "0.refresh-1", -10))).await;
    let err = identity(&socket, &key()).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied, "{err}");
    assert!(err.message().contains("administrator"), "{}", err.message());
    assert!(!err.message().contains("AADSTS"), "{}", err.message());
    assert!(!err.message().contains("Trace ID"), "{}", err.message());
    assert!(store.load(key()).await.unwrap().is_some());
}

#[tokio::test]
async fn a_refresh_that_sends_no_new_refresh_token_keeps_the_stored_one_and_its_scopes() {
    // Microsoft sends a new refresh token each time. If it ever does not, the
    // stored one is still the connection's only way to renew, and must not be lost.
    let server = MockServer::start().await;
    Mock::given(path("/contoso.example/oauth2/v2.0/token"))
        .respond_with(ok(json!({ "access_token": "eyJ.second", "expires_in": 3599 })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/v1.0/me"))
        .respond_with(ok(json!({ "id": "u-1", "displayName": "Ada" })))
        .mount(&server)
        .await;
    let (socket, store) = with_oauth_app(&server, Some(tokens("eyJ.first", "0.refresh-1", -10))).await;
    identity(&socket, &key()).await.unwrap();
    let saved = store.load(key()).await.unwrap().unwrap();
    assert_eq!(saved.access_token.expose(), "eyJ.second");
    assert_eq!(
        saved.refresh_token.as_ref().map(SecretString::expose),
        Some("0.refresh-1")
    );
    assert_eq!(saved.scopes, ["User.Read"]);
}

#[tokio::test]
async fn a_refresh_token_microsoft_no_longer_accepts_means_reconnect_not_a_missing_approval() {
    for (code, description) in [
        (70008, "The refresh token has expired due to inactivity."),
        (
            50173,
            "The provided grant has expired due to it being revoked, a fresh auth token is needed.",
        ),
        (700082, "The refresh token has expired due to inactivity."),
    ] {
        let server = MockServer::start().await;
        Mock::given(path("/contoso.example/oauth2/v2.0/token"))
            .respond_with(consent_needed("invalid_grant", code, description))
            .expect(1)
            .mount(&server)
            .await;
        let (socket, _store) = with_oauth_app(&server, Some(tokens("eyJ.first", "0.refresh-1", -10))).await;
        let err = identity(&socket, &key()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ReconnectRequired, "AADSTS{code}: {err}");
    }

    // A refused authorization code is the same mistake it is for every provider.
    let server = MockServer::start().await;
    Mock::given(path("/contoso.example/oauth2/v2.0/token"))
        .respond_with(consent_needed(
            "invalid_grant",
            70000,
            "The provided value for the 'code' parameter is not valid.",
        ))
        .mount(&server)
        .await;
    let (socket, _store) = with_oauth_app(&server, None).await;
    let authorization = socket.begin_authorization(key(), None).unwrap();
    let state = authorization.pending.state.clone();
    let err = socket
        .complete_authorization(authorization.pending, "the-code".into(), state)
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
}

// ── Identity ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn identity_reads_the_signed_in_user() {
    let (server, socket, key) = microsoft().await;
    Mock::given(method("GET"))
        .and(path("/v1.0/me"))
        .and(header("authorization", "Bearer eyJ.good"))
        .respond_with(ok(json!({
            "id": "48d31887-5fad-4d73-a9f5-3c356e68a038",
            "displayName": "Ada Lovelace",
            "mail": "ada@contoso.example",
            "userPrincipalName": "ada.lovelace@contoso.onmicrosoft.com"
        })))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        identity(&socket, &key).await.unwrap(),
        json!({
            "id": "48d31887-5fad-4d73-a9f5-3c356e68a038",
            "name": "Ada Lovelace",
            "email": "ada@contoso.example"
        })
    );
}

#[tokio::test]
async fn an_account_without_a_mailbox_is_known_by_its_sign_in_name() {
    for (user, name, email) in [
        (
            json!({ "id": "u-1", "displayName": "Ada", "mail": null, "userPrincipalName": "ada@contoso.onmicrosoft.com" }),
            "Ada",
            json!("ada@contoso.onmicrosoft.com"),
        ),
        (
            json!({ "id": "u-1", "displayName": "", "mail": "", "userPrincipalName": "ada@contoso.onmicrosoft.com" }),
            "ada@contoso.onmicrosoft.com",
            json!("ada@contoso.onmicrosoft.com"),
        ),
        (json!({ "id": "u-1" }), "u-1", json!(null)),
    ] {
        let (server, socket, key) = microsoft().await;
        Mock::given(path("/v1.0/me"))
            .respond_with(ok(user.clone()))
            .mount(&server)
            .await;
        assert_eq!(
            identity(&socket, &key).await.unwrap(),
            json!({ "id": "u-1", "name": name, "email": email }),
            "{user}"
        );
    }
}

// ── Resource lookup ──────────────────────────────────────────────────────────

#[tokio::test]
async fn a_sharing_link_resolves_to_its_drive_item() {
    let (server, socket, key) = microsoft().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1.0/shares/{SHARE}/driveItem")))
        .respond_with(ok(json!({
            "id": "01ABCDEF",
            "name": "Design notes.docx",
            "webUrl": "https://contoso.sharepoint.com/sites/team/Shared%20Documents/Design%20notes.docx",
            "file": { "mimeType": "application/vnd.openxmlformats-officedocument.wordprocessingml.document" },
            "parentReference": { "driveId": "b!drive-1", "driveType": "documentLibrary" }
        })))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        resolve(&socket, &key, &format!("  {LINK} ")).await.unwrap(),
        json!({
            "id": "drives/b!drive-1/items/01ABCDEF",
            "label": "Design notes.docx",
            "description": "SharePoint file"
        })
    );
}

#[tokio::test]
async fn an_item_is_described_by_where_it_lives_and_what_it_is() {
    for (item, id, description) in [
        (
            json!({ "id": "I1", "name": "Plans", "folder": { "childCount": 3 }, "parentReference": { "driveId": "D1", "driveType": "personal" } }),
            "drives/D1/items/I1",
            "OneDrive folder",
        ),
        (
            json!({ "id": "I1", "name": "Plans", "folder": {}, "parentReference": { "driveId": "D1", "driveType": "business" } }),
            "drives/D1/items/I1",
            "OneDrive folder",
        ),
        (
            json!({ "id": "I1", "name": "Plans", "folder": {}, "parentReference": { "driveId": "D1", "driveType": "documentLibrary" } }),
            "drives/D1/items/I1",
            "SharePoint folder",
        ),
        // Without its drive, the item is still addressed through the link.
        (
            json!({ "id": "I1", "name": "budget.xlsx", "file": {} }),
            "shares/u!aHR0cHM6Ly9jb250b3NvLnNoYXJlcG9pbnQuY29tLzp3Oi9zL3RlYW0vRWFiY0RFRj9lPXgx/driveItem",
            "OneDrive or SharePoint file",
        ),
    ] {
        let (server, socket, key) = microsoft().await;
        Mock::given(path(format!("/v1.0/shares/{SHARE}/driveItem")))
            .respond_with(ok(item.clone()))
            .mount(&server)
            .await;
        let resolved = resolve(&socket, &key, LINK).await.unwrap();
        assert_eq!(
            resolved,
            json!({ "id": id, "label": item["name"], "description": description }),
            "{item}"
        );
    }
}

#[tokio::test]
async fn a_link_is_encoded_whole_so_nothing_in_it_can_change_the_request() {
    let (server, socket, key) = microsoft().await;
    Mock::given(wiremock_any())
        .respond_with(ok(json!({ "id": "I1", "name": "x" })))
        .mount(&server)
        .await;
    // `>` and `?` are the bytes whose base64 uses the characters that differ
    // between base64 and base64url.
    resolve(&socket, &key, "https://1drv.ms/f/s!AtuAM4FfY_Kb>>??")
        .await
        .unwrap();
    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].url.path(),
        "/v1.0/shares/u!aHR0cHM6Ly8xZHJ2Lm1zL2YvcyFBdHVBTTRGZllfS2I-Pj8_/driveItem"
    );
    assert_eq!(received[0].url.query(), None);
}

fn wiremock_any() -> socketkit_testkit::wiremock::matchers::AnyMatcher {
    socketkit_testkit::wiremock::matchers::any()
}

#[tokio::test]
async fn a_link_that_leads_nowhere_is_not_found_and_a_missing_permission_is_named() {
    let (server, socket, key) = microsoft().await;
    Mock::given(path(format!("/v1.0/shares/{SHARE}/driveItem")))
        .respond_with(graph_error(404, "itemNotFound", "The sharing link no longer exists."))
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, LINK).await.unwrap_err();
    assert_eq!(
        (err.kind(), err.message()),
        (ErrorKind::NotFound, "that OneDrive or SharePoint link was not found")
    );

    let (server, socket, key) = microsoft().await;
    Mock::given(path(format!("/v1.0/shares/{SHARE}/driveItem")))
        .respond_with(graph_error(
            403,
            "accessDenied",
            "Access denied. You do not have permission to perform this action.",
        ))
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, LINK).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert!(
        err.message().contains("Access denied. You do not have permission"),
        "Graph's reason is kept: {}",
        err.message()
    );
    assert!(
        err.message().contains("Files.ReadWrite"),
        "the permission is named: {}",
        err.message()
    );
}

#[tokio::test]
async fn what_is_not_a_sharing_link_is_refused_without_calling_microsoft() {
    let (server, socket, key) = microsoft().await;
    for bad in [
        "",
        "  ",
        "Design notes.docx",
        "01ABCDEF",
        "http://contoso.sharepoint.com/:w:/s/team/Eabc",
        "ftp://contoso.sharepoint.com/x",
        "https://",
        "https://user:pw@contoso.sharepoint.com/:w:/s/team/Eabc",
    ] {
        let err = resolve(&socket, &key, bad).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{bad:?}");
        // What was refused may be a link with a password in it, so it is not repeated.
        let shown = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!shown.contains("pw@") && !shown.contains("sharepoint"), "{shown}");
        assert!(bad.trim().is_empty() || !shown.contains(bad.trim()), "{shown}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ── Graph's errors ───────────────────────────────────────────────────────────

#[tokio::test]
async fn graphs_answers_map_to_the_error_a_caller_can_act_on() {
    for (response, kind, retry) in [
        (
            graph_error(401, "InvalidAuthenticationToken", "Access token has expired."),
            ErrorKind::ReconnectRequired,
            Retry::Never,
        ),
        // The code decides, whatever the status around it.
        (
            graph_error(400, "InvalidAuthenticationToken", "CompactToken parsing failed."),
            ErrorKind::ReconnectRequired,
            Retry::Never,
        ),
        (
            graph_error(403, "Authorization_RequestDenied", "Insufficient privileges."),
            ErrorKind::AccessDenied,
            Retry::Never,
        ),
        (
            graph_error(
                404,
                "ErrorItemNotFound",
                "The specified object was not found in the store.",
            ),
            ErrorKind::NotFound,
            Retry::Never,
        ),
        (
            graph_error(400, "BadRequest", "Invalid filter clause."),
            ErrorKind::InvalidInput,
            Retry::Never,
        ),
        (
            graph_error(429, "TooManyRequests", "Too many requests.").insert_header("retry-after", "120"),
            ErrorKind::RateLimited,
            Retry::After(Duration::from_secs(120)),
        ),
        // A busy service says when to come back, as throttling does.
        (
            graph_error(503, "serviceNotAvailable", "The service is temporarily unavailable.")
                .insert_header("retry-after", "120"),
            ErrorKind::Unexpected,
            Retry::After(Duration::from_secs(120)),
        ),
        (
            graph_error(503, "serviceNotAvailable", "The service is temporarily unavailable."),
            ErrorKind::Unexpected,
            Retry::Later,
        ),
        (
            graph_error(503, "serviceNotAvailable", "Unavailable.").insert_header("retry-after", "soon"),
            ErrorKind::Unexpected,
            Retry::Later,
        ),
        (
            graph_error(500, "generalException", "An unspecified error has occurred."),
            ErrorKind::Unexpected,
            Retry::Later,
        ),
    ] {
        let (server, socket, key) = microsoft().await;
        Mock::given(path("/v1.0/me"))
            .respond_with(response)
            .mount(&server)
            .await;
        let err = identity(&socket, &key).await.unwrap_err();
        assert_eq!((err.kind(), err.retry()), (kind, retry), "{err}");
    }
}

#[tokio::test]
async fn a_wait_stated_as_a_date_is_read_as_well_as_one_in_seconds() {
    let unavailable = |when: SystemTime| {
        graph_error(503, "serviceNotAvailable", "The service is temporarily unavailable.")
            .insert_header("retry-after", httpdate::fmt_http_date(when).as_str())
    };
    let (server, socket, key) = microsoft().await;
    Mock::given(path("/v1.0/me"))
        .respond_with(unavailable(SystemTime::now() + Duration::from_secs(300)))
        .mount(&server)
        .await;
    let err = identity(&socket, &key).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unexpected);
    let Retry::After(wait) = err.retry() else {
        panic!("the wait Graph asked for is passed on: {:?}", err.retry())
    };
    assert!(
        (Duration::from_secs(290)..=Duration::from_secs(300)).contains(&wait),
        "{wait:?}"
    );

    // A date already past means "now".
    let (server, socket, key) = microsoft().await;
    Mock::given(path("/v1.0/me"))
        .respond_with(unavailable(SystemTime::now() - Duration::from_secs(60)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/v1.0/me"))
        .respond_with(ok(json!({ "id": "u-1", "displayName": "Ada" })))
        .mount(&server)
        .await;
    assert_eq!(
        identity(&socket, &key).await.unwrap()["id"],
        "u-1",
        "a read is tried again at once"
    );
}

#[tokio::test]
async fn graphs_own_reason_reaches_the_caller() {
    let (server, socket, key) = microsoft().await;
    Mock::given(path("/v1.0/me"))
        .respond_with(graph_error(
            403,
            "Authorization_RequestDenied",
            "Insufficient privileges to complete the operation.",
        ))
        .mount(&server)
        .await;
    let err = identity(&socket, &key).await.unwrap_err();
    assert!(
        err.message()
            .ends_with("Insufficient privileges to complete the operation."),
        "{}",
        err.message()
    );
}

#[tokio::test]
async fn microsoft_built_with_a_token_calls_graph_with_it_and_needs_no_stored_connection() {
    let server = MockServer::start().await;
    Mock::given(path("/v1.0/me"))
        .and(header("authorization", "Bearer eyJ.given"))
        .respond_with(ok(json!({ "id": "u-1", "displayName": "Ada" })))
        .expect(1)
        .mount(&server)
        .await;
    let microsoft = Microsoft::with_spec(point_at(provider(), &server)).token("eyJ.given");
    let socket = Socket::in_memory().integration(Arc::new(microsoft)).build().unwrap();
    let anyone = ConnectionKey::new(provider().id, "anyone");
    assert_eq!(identity(&socket, &anyone).await.unwrap()["id"], "u-1");

    let given = Microsoft::with_token("eyJ.given");
    assert_eq!(given.fixed_token().unwrap().access_token.expose(), "eyJ.given");
    assert!(given.oauth_client().is_none());
}

#[tokio::test]
async fn the_integration_keeps_the_provider_id_its_operations_are_named_after() {
    let mut spec = provider();
    spec.id = socketkit_core::ProviderId::new("entra").unwrap();
    let err = Socket::in_memory().integration(build(spec)).build().unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Config);
}
