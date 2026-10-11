//! Attio against a local server that answers as Attio's API and its sign-in do.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use socketkit_attio::{Attio, AttioOAuth, TokenLevel, provider};
use socketkit_core::{
    AuthScheme, ConnectionKey, ErrorKind, Integration, MemoryTokenStore, OAuthClient, Retry, RetryPolicy, SecretString,
    Socket, TokenSet, TokenStore,
};
use socketkit_testkit::wiremock::matchers::{any, body_string_contains, header, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{TENANT, conformance, point_at};

mod support;
use support::{
    ENTRY, LIST, MEETING, MEMBER, NOTE, OBJECT, RECORD, TASK, THREAD, WORKSPACE, answer, answering, attio, attio_error,
    body_of, build, invoke, list, meeting, note, object, only_request, query_of, record, token,
};

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

/// A `Socket` whose Attio integration holds the application's OAuth app and
/// signs people in on `server`.
async fn with_oauth_app(server: &MockServer, stored: Option<TokenSet>) -> (Socket, Arc<MemoryTokenStore>) {
    let store = Arc::new(MemoryTokenStore::new());
    if let Some(tokens) = stored {
        store.save(key(), tokens).await.unwrap();
    }
    let attio = Attio::with_spec(point_at(provider(), server)).oauth(client());
    let retry = RetryPolicy {
        max_attempts: 2,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(50),
    };
    let socket = Socket::builder(store.clone())
        .integration(Arc::new(attio))
        .retry(retry)
        .build()
        .unwrap();
    (socket, store)
}

fn authorize_url(attio: Attio, scopes: Option<Vec<String>>) -> url::Url {
    Socket::in_memory()
        .integration(Arc::new(attio))
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
    socket.invoke(key.clone(), "attio.identity.get".into(), json!({})).await
}

async fn resolve(socket: &Socket, key: &ConnectionKey, input: &str) -> socketkit_core::Result<Value> {
    socket
        .invoke(key.clone(), "attio.resource.resolve".into(), json!({ "input": input }))
        .await
}

// ── The definition and its settings ───────────────────────────────────────────

#[tokio::test]
async fn passes_the_conformance_suite() {
    conformance::all(provider(), build, "people").await;
}

#[tokio::test]
async fn the_definition_is_attios_v2_api_signed_in_at_app_attio_com() {
    let spec = provider();
    assert_eq!(spec.id.as_str(), "attio");
    assert_eq!(spec.api_base.as_str(), "https://api.attio.com/v2/");
    let AuthScheme::OAuth2(oauth) = &spec.auth else {
        panic!("attio uses OAuth")
    };
    assert_eq!(oauth.authorize_url.as_str(), "https://app.attio.com/authorize");
    assert_eq!(oauth.token_url.as_str(), "https://app.attio.com/oauth/token");
    // An Attio app's scopes are set in Attio's console, not asked for at sign-in.
    assert!(oauth.default_scopes.is_empty());
    assert!(oauth.pkce);

    let allows = |u: &str| spec.allows_host(&u.parse().unwrap());
    assert!(allows("https://api.attio.com/v2/self"));
    assert!(allows("https://app.attio.com/oauth/token"));
    for elsewhere in [
        "https://attio.com/",
        "https://video.attio.com/call-recording/x.mp4",
        "https://api.attio.com.evil.test/v2/self",
        "http://api.attio.com/v2/self",
    ] {
        assert!(!allows(elsewhere), "{elsewhere}");
    }
}

#[tokio::test]
async fn the_sign_in_page_is_asked_for_a_code_with_pkce_and_never_for_scopes() {
    let url = authorize_url(Attio::with_oauth(client()), None);
    assert_eq!(url.host_str(), Some("app.attio.com"));
    assert_eq!(url.path(), "/authorize");
    assert_eq!(param(&url, "response_type").as_deref(), Some("code"));
    assert_eq!(param(&url, "client_id").as_deref(), Some("client-id"));
    assert_eq!(
        param(&url, "redirect_uri").as_deref(),
        Some("https://app.example.test/callback")
    );
    assert!(param(&url, "state").is_some_and(|state| !state.is_empty()));
    assert_eq!(param(&url, "code_challenge_method").as_deref(), Some("S256"));
    assert!(param(&url, "code_challenge").is_some());
    assert_eq!(param(&url, "scope"), None);
    assert_eq!(param(&url, "token_level"), None, "Attio's default is left to Attio");

    // Scopes passed when the authorisation begins are not sent either: the
    // sign-in page has no parameter for them.
    let asked = authorize_url(
        Attio::with_oauth(client()),
        Some(vec!["record_permission:read".into(), "note:read-write".into()]),
    );
    assert_eq!(param(&asked, "scope"), None);
    assert_eq!(asked.query_pairs().count(), url.query_pairs().count());
}

#[tokio::test]
async fn the_token_level_setting_chooses_whose_permissions_a_token_has() {
    for (level, sent) in [(TokenLevel::Workspace, "workspace"), (TokenLevel::User, "user")] {
        let settings = AttioOAuth {
            client: client(),
            token_level: Some(level),
        };
        let url = authorize_url(Attio::with_oauth(settings), None);
        assert_eq!(param(&url, "token_level").as_deref(), Some(sent));
        // A token that acts as one member needs PKCE, which is always on.
        assert!(param(&url, "code_challenge").is_some());
    }
}

// ── Connecting ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn connecting_exchanges_the_code_for_a_token_that_does_not_expire() {
    let server = MockServer::start().await;
    let (socket, store) = with_oauth_app(&server, None).await;
    let authorization = socket.begin_authorization(key(), None).unwrap();
    assert_eq!(authorization.url.path(), "/authorize");
    let verifier = authorization.pending.pkce_verifier.clone().expect("PKCE is on");

    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains(
            "redirect_uri=https%3A%2F%2Fapp.example.test%2Fcallback",
        ))
        .and(body_string_contains(format!("code_verifier={}", verifier.expose())))
        // The client's id and secret go in the form, as Attio's tutorial sends them.
        .and(body_string_contains("client_id=client-id"))
        .and(body_string_contains("client_secret=client-secret"))
        .respond_with(ok(json!({ "access_token": "at-first", "token_type": "Bearer" })))
        .expect(1)
        .mount(&server)
        .await;

    let state = authorization.pending.state.clone();
    let granted = socket
        .complete_authorization(authorization.pending, "the-code".into(), state)
        .await
        .unwrap();
    assert_eq!(granted.access_token.expose(), "at-first");
    // Attio states no lifetime and issues no refresh token.
    assert_eq!(granted.expires_at, None);
    assert_eq!(granted.refresh_token, None);
    assert!(granted.scopes.is_empty());
    assert_eq!(store.load(key()).await.unwrap(), Some(granted));

    let request = only_request(&server).await;
    assert!(request.headers.get("authorization").is_none(), "no Basic credentials");
    assert!(
        !String::from_utf8_lossy(&request.body).contains("scope="),
        "no scopes are asked for"
    );
}

#[tokio::test]
async fn a_code_attio_refuses_is_an_error_and_nothing_is_stored() {
    let server = MockServer::start().await;
    Mock::given(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant" })))
        .mount(&server)
        .await;
    let (socket, store) = with_oauth_app(&server, None).await;
    let authorization = socket.begin_authorization(key(), None).unwrap();
    let state = authorization.pending.state.clone();
    let err = socket
        .complete_authorization(authorization.pending, "the-code".into(), state)
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(!err.message().contains("the-code"), "{}", err.message());
    assert_eq!(store.load(key()).await.unwrap(), None);
}

#[tokio::test]
async fn a_token_attio_rejects_means_reconnect_because_there_is_nothing_to_refresh_it_with() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/self"))
        .respond_with(attio_error(401, "auth_error", "unauthorized", "Invalid access token"))
        .mount(&server)
        .await;
    let (socket, store) = with_oauth_app(&server, Some(TokenSet::bearer("at-revoked"))).await;
    let err = identity(&socket, &key()).await.unwrap_err();
    assert_eq!((err.kind(), err.retry()), (ErrorKind::ReconnectRequired, Retry::Never));
    let received = server.received_requests().await.unwrap();
    assert!(
        received.iter().all(|request| request.url.path() == "/v2/self"),
        "the token endpoint is not asked for a refresh Attio does not offer"
    );
    assert_eq!(
        store.load(key()).await.unwrap().unwrap().access_token.expose(),
        "at-revoked"
    );
}

#[tokio::test]
async fn attio_built_with_a_token_calls_attio_with_it_and_needs_no_stored_connection() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/self"))
        .and(header("authorization", "Bearer at-workspace-key"))
        .respond_with(ok(token()))
        .expect(1)
        .mount(&server)
        .await;
    let attio = Attio::with_spec(point_at(provider(), &server)).token("at-workspace-key");
    let socket = Socket::in_memory().integration(Arc::new(attio)).build().unwrap();
    let anyone = ConnectionKey::new(provider().id, "anyone");
    assert_eq!(identity(&socket, &anyone).await.unwrap()["id"], WORKSPACE);

    let given = Attio::with_token("at-workspace-key");
    assert_eq!(given.fixed_token().unwrap().access_token.expose(), "at-workspace-key");
    assert!(given.oauth_client().is_none());
    assert!(
        !format!("{given:?}").contains("at-workspace-key"),
        "a token is not printed"
    );
    let with_app = Attio::with_oauth(client());
    assert!(with_app.fixed_token().is_none());
    assert_eq!(with_app.oauth_client().unwrap().client_id, "client-id");
}

#[tokio::test]
async fn the_integration_keeps_the_provider_id_its_operations_are_named_after() {
    let mut spec = provider();
    spec.id = socketkit_core::ProviderId::new("crm").unwrap();
    let err = Socket::in_memory().integration(build(spec)).build().unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Config);
}

// ── Identity ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn identity_is_the_workspace_the_token_belongs_to() {
    let (server, socket, key) = attio().await;
    Mock::given(method("GET"))
        .and(path("/v2/self"))
        .and(header("authorization", "Bearer at-good"))
        .respond_with(ok(token()))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        identity(&socket, &key).await.unwrap(),
        json!({ "id": WORKSPACE, "name": "Acme", "email": null })
    );
}

#[tokio::test]
async fn a_workspace_without_a_name_is_known_by_its_slug_and_then_by_its_id() {
    for (name, slug, shown) in [
        (json!(""), json!("acme"), "acme"),
        (json!(null), json!("acme"), "acme"),
        (json!(" "), json!(null), WORKSPACE),
    ] {
        let mut described = token();
        described["workspace_name"] = name;
        described["workspace_slug"] = slug;
        let (_server, socket, key) = answering(200, described.clone()).await;
        assert_eq!(
            identity(&socket, &key).await.unwrap(),
            json!({ "id": WORKSPACE, "name": shown, "email": null }),
            "{described}"
        );
    }
}

#[tokio::test]
async fn a_token_attio_calls_inactive_in_a_success_means_reconnect() {
    // Attio answers 200 for a token it does not know, and says so in the body.
    let (_server, socket, key) = answering(200, json!({ "active": false })).await;
    for outcome in [
        identity(&socket, &key).await,
        invoke(&socket, &key, "meta.identify", json!({})).await,
    ] {
        let err = outcome.unwrap_err();
        assert_eq!((err.kind(), err.retry()), (ErrorKind::ReconnectRequired, Retry::Never));
        assert_eq!(err.message(), "attio rejected the stored authorization");
    }

    // An answer that says the token is active and names no workspace is not an account.
    for odd in [
        json!({ "active": true }),
        json!({ "active": true, "workspace_id": " " }),
        json!({ "workspace_id": WORKSPACE }),
    ] {
        let (_server, socket, key) = answering(200, odd.clone()).await;
        let err = identity(&socket, &key).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{odd}");
    }
}

#[tokio::test]
async fn a_token_with_no_scopes_describes_itself_with_an_empty_list() {
    let mut bare = token();
    bare.as_object_mut().unwrap().remove("scope");
    bare.as_object_mut()
        .unwrap()
        .remove("authorized_by_workspace_member_id");
    let (_server, socket, key) = answering(200, bare).await;
    let described = invoke(&socket, &key, "meta.identify", json!({})).await.unwrap();
    assert_eq!(described["scopes"], json!([]));
    assert_eq!(described["authorized_by_workspace_member_id"], Value::Null);
}

// ── Resource lookup ──────────────────────────────────────────────────────────

#[tokio::test]
async fn an_object_or_a_list_resolves_to_attios_own_id_for_it() {
    for (input, at, body, resolved) in [
        (
            " people ",
            "/v2/objects/people",
            json!({ "data": object() }),
            json!({ "id": OBJECT, "label": "People", "description": "Attio object" }),
        ),
        (
            "objects/people",
            "/v2/objects/people",
            json!({ "data": object() }),
            json!({ "id": OBJECT, "label": "People", "description": "Attio object" }),
        ),
        (
            "lists/enterprise_sales",
            "/v2/lists/enterprise_sales",
            json!({ "data": list() }),
            json!({ "id": LIST, "label": "Enterprise sales", "description": "Attio list" }),
        ),
    ] {
        let (server, socket, key) = attio().await;
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(ok(body))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(resolve(&socket, &key, input).await.unwrap(), resolved, "{input}");
    }

    // An object the workspace gave no names to is known by its slug.
    let mut unnamed = object();
    unnamed["plural_noun"] = json!(null);
    unnamed["singular_noun"] = json!("");
    let (_server, socket, key) = answering(200, json!({ "data": unnamed })).await;
    assert_eq!(resolve(&socket, &key, "people").await.unwrap()["label"], "people");
}

#[tokio::test]
async fn what_is_not_an_object_or_a_list_is_refused_without_calling_attio() {
    let (server, socket, key) = attio().await;
    for bad in [
        "",
        "  ",
        "records/people",
        "people/records",
        "objects/people/records",
        "lists/",
        "objects/",
        "../self",
        "people?limit=1",
        "people#x",
        "https://app.attio.com/acme/person/abc",
        "user:secret@people",
    ] {
        let err = resolve(&socket, &key, bad).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{bad:?}");
        let shown = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!shown.contains("secret"), "{shown}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_object_attio_does_not_have_is_not_found_with_attios_word_for_what_was_missing() {
    let (server, socket, key) = attio().await;
    Mock::given(path("/v2/objects/peple"))
        .respond_with(attio_error(
            404,
            "invalid_request_error",
            "not_found",
            "Object with slug/ID \"peple\" not found.",
        ))
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, "peple").await.unwrap_err();
    assert_eq!(
        (err.kind(), err.message()),
        (
            ErrorKind::NotFound,
            "attio has no such resource: Object with slug/ID \"peple\" not found."
        )
    );
}

// ── Attio's errors ───────────────────────────────────────────────────────────

#[tokio::test]
async fn attios_answers_map_to_the_error_a_caller_can_act_on() {
    for (response, kind, retry) in [
        (
            attio_error(401, "auth_error", "unauthorized", "Invalid access token"),
            ErrorKind::ReconnectRequired,
            Retry::Never,
        ),
        // A token without the scope, or a member without the permission.
        (
            attio_error(
                403,
                "auth_error",
                "unauthorized",
                "You do not have the necessary permissions.",
            ),
            ErrorKind::AccessDenied,
            Retry::Never,
        ),
        (
            attio_error(404, "invalid_request_error", "not_found", "Record not found."),
            ErrorKind::NotFound,
            Retry::Never,
        ),
        (ResponseTemplate::new(404), ErrorKind::NotFound, Retry::Never),
        // Validation: a value Attio does not have, a filter it cannot read.
        (
            attio_error(
                400,
                "invalid_request_error",
                "value_not_found",
                "Cannot find the option.",
            ),
            ErrorKind::InvalidInput,
            Retry::Never,
        ),
        (
            attio_error(400, "invalid_request_error", "filter_error", "Error in filter."),
            ErrorKind::InvalidInput,
            Retry::Never,
        ),
        (
            attio_error(400, "invalid_request_error", "validation_type", "Invalid value."),
            ErrorKind::InvalidInput,
            Retry::Never,
        ),
        (
            attio_error(422, "invalid_request_error", "validation_type", "Invalid value."),
            ErrorKind::InvalidInput,
            Retry::Never,
        ),
        // A value another record already holds, where it has to be unique.
        (
            attio_error(
                409,
                "invalid_request_error",
                "uniqueness_conflict",
                "A record with this value exists.",
            ),
            ErrorKind::InvalidInput,
            Retry::Never,
        ),
        // Two writes to one record at once: Attio carried this one out not
        // at all, and asks for it again.
        (
            attio_error(
                409,
                "invalid_request_error",
                "concurrent_write_conflict",
                "The record was modified by another request while this write was being validated. Please try again.",
            ),
            ErrorKind::Unexpected,
            Retry::Later,
        ),
        (
            attio_error(413, "invalid_request_error", "validation_type", "Too large."),
            ErrorKind::InvalidInput,
            Retry::Never,
        ),
        (
            attio_error(
                429,
                "rate_limit_error",
                "rate_limit_exceeded",
                "Rate limit exceeded, please try again later",
            )
            .insert_header("retry-after", "120"),
            ErrorKind::RateLimited,
            Retry::After(Duration::from_secs(120)),
        ),
        (
            attio_error(
                429,
                "rate_limit_error",
                "rate_limit_exceeded",
                "Rate limit exceeded, please try again later",
            ),
            ErrorKind::RateLimited,
            Retry::Later,
        ),
        (
            attio_error(500, "api_error", "internal", "Something went wrong."),
            ErrorKind::Unexpected,
            Retry::Later,
        ),
    ] {
        // A write, so that the transport does not try again and each answer is seen once.
        let (server, socket, key) = attio().await;
        Mock::given(any()).respond_with(response).mount(&server).await;
        let input = json!({ "object": "people", "values": { "name": "Ada" } });
        let err = invoke(&socket, &key, "records.create", input).await.unwrap_err();
        assert_eq!((err.kind(), err.retry()), (kind, retry), "{err}");
    }
}

#[tokio::test]
async fn the_wait_attio_states_as_a_date_reaches_the_caller() {
    // Attio writes `Retry-After` as the time its limit resets, not as seconds.
    let limited = |when: SystemTime| {
        attio_error(
            429,
            "rate_limit_error",
            "rate_limit_exceeded",
            "Rate limit exceeded, please try again later",
        )
        .insert_header("retry-after", httpdate::fmt_http_date(when).as_str())
    };
    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(limited(SystemTime::now() + Duration::from_secs(300)))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "objects.list", json!({})).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimited);
    assert_eq!(err.message(), "attio is rate limiting requests");
    let Retry::After(wait) = err.retry() else {
        panic!("the wait Attio asked for is passed on: {:?}", err.retry())
    };
    assert!(
        (Duration::from_secs(290)..=Duration::from_secs(300)).contains(&wait),
        "{wait:?}"
    );

    // The usual case: the limit resets within the second, and the request
    // is sent again, a write included, because Attio did not carry it out.
    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(limited(SystemTime::now() - Duration::from_secs(1)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(ok(json!({ "data": record() })))
        .mount(&server)
        .await;
    let input = json!({ "object": "people", "values": { "name": "Ada" } });
    let created = invoke(&socket, &key, "records.create", input).await.unwrap();
    assert_eq!(created["id"]["record_id"], RECORD);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_write_that_met_another_write_is_sent_again_only_where_that_is_safe() {
    let conflict = || {
        attio_error(
            409,
            "invalid_request_error",
            "concurrent_write_conflict",
            "The record was modified by another request while this write was being validated. Please try again.",
        )
    };
    // An assert writes the same values to the same record however often it is sent.
    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(conflict())
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(ok(json!({ "data": record() })))
        .mount(&server)
        .await;
    let input = json!({ "object": "people", "matching_attribute": "email_addresses", "values": { "email_addresses": ["ada@example.com"] } });
    invoke(&socket, &key, "records.assert", input).await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 2);

    // A create is the caller's to send again: the error says it can be.
    let (server, socket, key) = attio().await;
    Mock::given(any()).respond_with(conflict()).mount(&server).await;
    let input = json!({ "object": "people", "values": { "name": "Ada" } });
    let err = invoke(&socket, &key, "records.create", input).await.unwrap_err();
    assert_eq!(err.retry(), Retry::Later);
    assert!(err.message().ends_with("try again"), "{}", err.message());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_token_attio_echoes_in_an_error_is_not_shown() {
    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(attio_error(
            403,
            "auth_error",
            "unauthorized",
            "Token at-good lacks the scope note:read",
        ))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "notes.get", json!({ "note": NOTE }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert_eq!(
        err.message(),
        "attio denied the request: Token [redacted] lacks the scope note:read"
    );
}

// ── Paging ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_list_paged_by_offset_goes_on_while_its_pages_are_full() {
    let notes = |count: usize| json!({ "data": vec![note(); count] });
    // A full page may have more behind it: the cursor is where the next one starts.
    let (server, socket, key) = answering(200, notes(2)).await;
    let first = invoke(&socket, &key, "notes.list", json!({ "limit": 2 }))
        .await
        .unwrap();
    assert_eq!(first["next_cursor"], "2");
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "limit": "2" }),
        "no offset on the first page"
    );

    // Passed back, it becomes the offset, beside the same page size.
    let (server, socket, key) = answering(200, notes(2)).await;
    let second = invoke(&socket, &key, "notes.list", json!({ "limit": 2, "cursor": "2" }))
        .await
        .unwrap();
    assert_eq!(second["next_cursor"], "4");
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "limit": "2", "offset": "2" })
    );

    // A page that is not full is the last.
    let (_server, socket, key) = answering(200, notes(1)).await;
    let last = invoke(&socket, &key, "notes.list", json!({ "limit": 2, "cursor": "4" }))
        .await
        .unwrap();
    assert_eq!(last["items"].as_array().unwrap().len(), 1);
    assert_eq!(last["next_cursor"], Value::Null);

    // A list that ends on a full page ends with an empty one, which has no cursor.
    let (_server, socket, key) = answering(200, notes(0)).await;
    let after = invoke(&socket, &key, "notes.list", json!({ "limit": 2, "cursor": "4" }))
        .await
        .unwrap();
    assert_eq!(after, json!({ "items": [], "next_cursor": null }));

    // The page size not given is this crate's, and is what "full" is measured by.
    let (server, socket, key) = answering(200, notes(50)).await;
    let unsized_ = invoke(&socket, &key, "notes.list", json!({})).await.unwrap();
    assert_eq!(unsized_["next_cursor"], "50");
    assert_eq!(query_of(&only_request(&server).await), json!({ "limit": "50" }));
}

#[tokio::test]
async fn a_query_pages_in_its_body_the_same_way() {
    let records = |count: usize| json!({ "data": vec![record(); count] });
    let (server, socket, key) = answering(200, records(25)).await;
    let first = invoke(&socket, &key, "records.query", json!({ "object": "people" }))
        .await
        .unwrap();
    assert_eq!(first["next_cursor"], "25");
    let request = only_request(&server).await;
    assert_eq!(body_of(&request), json!({ "limit": 25 }));
    assert_eq!(query_of(&request), json!({}), "nothing of the paging is in the address");

    let (server, socket, key) = answering(200, records(3)).await;
    let input = json!({ "object": "people", "cursor": " 25 ", "filter": { "name": "Ada" } });
    let second = invoke(&socket, &key, "records.query", input).await.unwrap();
    assert_eq!(second["next_cursor"], Value::Null);
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "filter": { "name": "Ada" }, "limit": 25, "offset": 25 })
    );

    let (server, socket, key) = answering(200, json!({ "data": [] })).await;
    let input = json!({ "list": "enterprise_sales", "limit": 500, "cursor": "1000" });
    invoke(&socket, &key, "entries.query", input).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "limit": 500, "offset": 1000 })
    );
}

#[tokio::test]
async fn a_list_paged_by_attios_cursor_passes_it_on_as_a_parameter_and_nothing_else() {
    // Whatever a cursor holds, it reaches Attio as the value of `cursor`: it
    // cannot change the path, add a parameter, or send the token elsewhere.
    for cursor in [
        "eyJvZmZzZXQiOjUwfQ==",
        "../../self",
        "x&limit=9999&linked_object=companies",
        "https://evil.test/v2/meetings?x=1",
        "a b+c/d?e#f",
    ] {
        let (server, socket, key) = answering(
            200,
            json!({ "data": [meeting()], "pagination": { "next_cursor": "next" } }),
        )
        .await;
        let page = invoke(&socket, &key, "meetings.list", json!({ "cursor": cursor, "limit": 1 }))
            .await
            .unwrap();
        assert_eq!(page["next_cursor"], "next");
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), "/v2/meetings", "{cursor}");
        assert_eq!(
            query_of(&request),
            json!({ "limit": "1", "cursor": cursor }),
            "{cursor}"
        );
        assert_eq!(request.url.fragment(), None, "{cursor}");
    }

    // The last page has no cursor, however Attio writes that.
    for pagination in [
        json!({ "next_cursor": null }),
        json!({ "next_cursor": "" }),
        json!({}),
        json!(null),
    ] {
        let (_server, socket, key) = answering(200, json!({ "data": [meeting()], "pagination": pagination })).await;
        let page = invoke(&socket, &key, "meetings.list", json!({})).await.unwrap();
        assert_eq!(page["next_cursor"], Value::Null, "{pagination}");
        let comments = json!({ "data": { "id": { "workspace_id": WORKSPACE, "thread_id": THREAD }, "comments": [] }, "pagination": pagination });
        let (_server, socket, key) = answering(200, comments).await;
        let thread = invoke(&socket, &key, "threads.get", json!({ "thread": THREAD }))
            .await
            .unwrap();
        assert_eq!(thread["next_cursor"], Value::Null, "{pagination}");
    }

    // A blank cursor is the first page.
    let (server, socket, key) = answering(200, json!({ "data": [], "pagination": { "next_cursor": null } })).await;
    invoke(&socket, &key, "meetings.list", json!({ "cursor": "  " }))
        .await
        .unwrap();
    assert_eq!(query_of(&only_request(&server).await), json!({ "limit": "50" }));
}

#[tokio::test]
async fn a_cursor_or_a_page_size_that_cannot_be_right_is_refused_without_calling_attio() {
    let (server, socket, key) = attio().await;
    let on_person = json!({ "object": "people", "record_id": RECORD });
    let with = |mut input: Value, name: &str, value: Value| {
        input[name] = value;
        input
    };
    let mut bad = Vec::new();
    // The cursor of a list paged by offset is the number this crate gave.
    for cursor in [
        "next",
        "-1",
        "1.5",
        "0x10",
        "99999999999999999999999",
        "2; drop",
        "../self",
    ] {
        bad.push(("notes.list", json!({ "cursor": cursor })));
        bad.push(("records.query", json!({ "object": "people", "cursor": cursor })));
        bad.push(("tasks.list", json!({ "cursor": cursor })));
        bad.push(("threads.list", with(on_person.clone(), "cursor", json!(cursor))));
        bad.push((
            "records.entries",
            json!({ "object": "people", "record": RECORD, "cursor": cursor }),
        ));
    }
    // Each list takes from one to the most Attio does.
    for (name, input, most) in [
        ("notes.list", json!({}), 50),
        ("threads.list", on_person.clone(), 50),
        ("threads.get", json!({ "thread": THREAD }), 250),
        ("meetings.list", json!({}), 200),
        ("call_recordings.list", json!({ "meeting": MEETING }), 200),
        ("records.entries", json!({ "object": "people", "record": RECORD }), 1000),
        ("records.query", json!({ "object": "people" }), 500),
        ("entries.query", json!({ "list": "enterprise_sales" }), 500),
        ("tasks.list", json!({}), 500),
        (
            "attributes.list",
            json!({ "target": "objects", "identifier": "people" }),
            500,
        ),
    ] {
        bad.push((name, with(input.clone(), "limit", json!(0))));
        bad.push((name, with(input.clone(), "limit", json!(most + 1))));
        bad.push((name, with(input.clone(), "limit", json!(-1))));
        // The most it takes is taken.
        let (_server, socket, key) = answering(200, json!({ "data": [], "pagination": { "next_cursor": null } })).await;
        let outcome = invoke(&socket, &key, name, with(input, "limit", json!(most))).await;
        assert!(
            !matches!(&outcome, Err(e) if e.kind() == ErrorKind::InvalidInput),
            "{name} takes {most}: {outcome:?}"
        );
    }
    for (name, input) in bad {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
        // A cursor is the caller's own text, and is not repeated.
        assert!(
            !err.message().contains("drop") && !err.message().contains("self"),
            "{}",
            err.message()
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ── What a caller passes as an id ─────────────────────────────────────────────

#[tokio::test]
async fn an_id_that_would_add_to_the_path_is_refused_and_never_sent() {
    let (server, socket, key) = attio().await;
    let attacks = [
        "people/records",
        "../lists",
        "..",
        ".",
        "people%2Frecords",
        "people?limit=1",
        "people#",
        "people records",
        "people\\records",
        "people;x",
        "pe\u{f6}ple",
        "people\n",
        "",
        "  ",
    ];
    for attack in attacks {
        let ok = "people";
        let inputs = [
            ("objects.get", json!({ "object": attack })),
            ("records.query", json!({ "object": attack })),
            ("records.get", json!({ "object": attack, "record": RECORD })),
            ("records.get", json!({ "object": ok, "record": attack })),
            ("records.entries", json!({ "object": ok, "record": attack })),
            (
                "records.create",
                json!({ "object": attack, "values": { "name": "Ada" } }),
            ),
            (
                "records.update",
                json!({ "object": ok, "record": attack, "values": { "name": "Ada" } }),
            ),
            (
                "records.assert",
                json!({ "object": attack, "matching_attribute": "email_addresses", "values": { "name": "Ada" } }),
            ),
            (
                "records.assert",
                json!({ "object": ok, "matching_attribute": attack, "values": { "name": "Ada" } }),
            ),
            ("records.delete", json!({ "object": ok, "record": attack })),
            ("records.delete", json!({ "object": attack, "record": RECORD })),
            ("attributes.list", json!({ "target": "objects", "identifier": attack })),
            (
                "attributes.get",
                json!({ "target": "lists", "identifier": ok, "attribute": attack }),
            ),
            (
                "attributes.options",
                json!({ "target": "objects", "identifier": attack, "attribute": "stage" }),
            ),
            (
                "attributes.statuses",
                json!({ "target": "lists", "identifier": ok, "attribute": attack }),
            ),
            ("lists.get", json!({ "list": attack })),
            ("entries.query", json!({ "list": attack })),
            ("entries.get", json!({ "list": "enterprise_sales", "entry": attack })),
            (
                "entries.create",
                json!({ "list": attack, "parent_object": ok, "parent_record": RECORD }),
            ),
            (
                "entries.create",
                json!({ "list": "enterprise_sales", "parent_object": attack, "parent_record": RECORD }),
            ),
            (
                "entries.create",
                json!({ "list": "enterprise_sales", "parent_object": ok, "parent_record": attack }),
            ),
            (
                "entries.update",
                json!({ "list": "enterprise_sales", "entry": attack, "entry_values": { "stage": "Won" } }),
            ),
            ("entries.delete", json!({ "list": attack, "entry": ENTRY })),
            ("notes.get", json!({ "note": attack })),
            (
                "notes.create",
                json!({ "parent_object": attack, "parent_record": RECORD, "title": "t", "content": "c" }),
            ),
            (
                "notes.create",
                json!({ "parent_object": ok, "parent_record": RECORD, "title": "t", "content": "c", "meeting_id": attack }),
            ),
            ("notes.delete", json!({ "note": attack })),
            ("tasks.get", json!({ "task": attack })),
            ("tasks.update", json!({ "task": attack, "is_completed": true })),
            ("tasks.delete", json!({ "task": attack })),
            (
                "tasks.create",
                json!({ "content": "Call", "linked_records": [{ "target_object": attack, "target_record_id": RECORD }] }),
            ),
            ("threads.get", json!({ "thread": attack })),
            (
                "threads.comment",
                json!({ "author": attack, "thread_id": THREAD, "content": "Hello" }),
            ),
            (
                "threads.comment",
                json!({ "author": MEMBER, "thread_id": attack, "content": "Hello" }),
            ),
            (
                "threads.comment",
                json!({ "author": MEMBER, "entry": { "list": attack, "entry_id": ENTRY }, "content": "Hello" }),
            ),
            ("workspace_members.get", json!({ "member": attack })),
            ("meetings.get", json!({ "meeting": attack })),
            ("call_recordings.list", json!({ "meeting": attack })),
            (
                "call_recordings.get",
                json!({ "meeting": MEETING, "recording": attack }),
            ),
        ];
        for (name, input) in inputs {
            let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
            // What was passed is not repeated in the refusal.
            assert!(
                attack.trim().is_empty() || !err.message().contains(attack),
                "{name}: {}",
                err.message()
            );
        }
    }
    // An id given as a filter is held to the same rule, though it goes in the query.
    for (name, input) in [
        ("notes.list", json!({ "parent_object": "people/x" })),
        (
            "notes.list",
            json!({ "parent_object": "people", "parent_record_id": "../x" }),
        ),
        (
            "tasks.list",
            json!({ "linked_object": "a b", "linked_record_id": RECORD }),
        ),
        ("threads.list", json!({ "object": "people", "record_id": "x/y" })),
        ("threads.list", json!({ "list": "a?b", "entry_id": ENTRY })),
        (
            "meetings.list",
            json!({ "linked_object": "people", "linked_record_id": "x#y" }),
        ),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}");
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "nothing reached Attio"
    );
}

#[tokio::test]
async fn a_slug_and_an_id_are_each_sent_as_the_one_segment_they_are() {
    for (name, input, at) in [
        (
            "objects.get",
            json!({ "object": OBJECT }),
            format!("/v2/objects/{OBJECT}"),
        ),
        (
            "objects.get",
            json!({ "object": "custom_object_2" }),
            "/v2/objects/custom_object_2".to_owned(),
        ),
        (
            "objects.get",
            json!({ "object": "Custom-Object" }),
            "/v2/objects/Custom-Object".to_owned(),
        ),
        ("tasks.get", json!({ "task": TASK }), format!("/v2/tasks/{TASK}")),
    ] {
        let (server, socket, key) = answering(404, json!(null)).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::NotFound);
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), at);
        assert_eq!(request.url.query(), None);
    }
    // An id is at most as long as anything Attio issues could be.
    let (server, socket, key) = attio().await;
    let err = invoke(&socket, &key, "objects.get", json!({ "object": "a".repeat(129) }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ── Fields an operation does not know ────────────────────────────────────────

#[tokio::test]
async fn a_field_an_operation_does_not_know_is_refused_and_named() {
    let (server, socket, key) = attio().await;
    for (name, input, named) in [
        // An operation that takes nothing refuses anything.
        ("objects.list", json!({ "limit": 5 }), "`limit`"),
        ("lists.list", json!({ "cursor": "2" }), "`cursor`"),
        (
            "workspace_members.list",
            json!({ "show_archived": true }),
            "`show_archived`",
        ),
        ("meta.identify", json!({ "token": "at-good" }), "`token`"),
        // A misspelt option would be dropped in silence, and with it what it asked for.
        (
            "records.query",
            json!({ "object": "people", "filters": { "name": "Ada" } }),
            "`filters`",
        ),
        ("records.query", json!({ "object": "people", "sort": [] }), "`sort`"),
        (
            "records.query",
            json!({ "object": "people", "sorts": [{ "direction": "asc", "atribute": "name" }] }),
            "`sorts[0].atribute`",
        ),
        (
            "records.update",
            json!({ "object": "people", "record": RECORD, "values": { "a": 1 }, "value": {} }),
            "`value`",
        ),
        (
            "records.assert",
            json!({ "object": "people", "matching_attribute": "name", "values": { "a": 1 }, "matching": "x" }),
            "`matching`",
        ),
        (
            "entries.create",
            json!({ "list": "l", "parent_object": "people", "parent_record": RECORD, "values": {} }),
            "`values`",
        ),
        (
            "notes.create",
            json!({ "parent_object": "people", "parent_record": RECORD, "title": "t", "content": "c", "body": "x" }),
            "`body`",
        ),
        (
            "notes.list",
            json!({ "parent_object": "people", "record": RECORD }),
            "`record`",
        ),
        (
            "tasks.create",
            json!({ "content": "Call", "deadline": "2026-11-01" }),
            "`deadline`",
        ),
        (
            "tasks.create",
            json!({ "content": "Call", "assignees": [{ "email": "susan@example.com" }] }),
            "`assignees[0].email`",
        ),
        (
            "tasks.create",
            json!({ "content": "Call", "linked_records": [{ "target_object": "people", "target_record_id": RECORD, "id": 1 }] }),
            "`linked_records[0].id`",
        ),
        (
            "tasks.update",
            json!({ "task": TASK, "content": "New text" }),
            "`content`",
        ),
        (
            "threads.comment",
            json!({ "author": MEMBER, "content": "Hi", "record": { "object": "people", "record_id": RECORD, "entry_id": ENTRY } }),
            "`record.entry_id`",
        ),
        (
            "threads.list",
            json!({ "object": "people", "record": RECORD }),
            "`record`",
        ),
        (
            "meetings.list",
            json!({ "participant": "ada@example.com" }),
            "`participant`",
        ),
        (
            "attributes.options",
            json!({ "target": "objects", "identifier": "people", "attribute": "stage", "limit": 5 }),
            "`limit`",
        ),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}");
        assert!(
            err.message().starts_with(named) && err.message().contains("is not a field"),
            "{name}: {}",
            err.message()
        );
    }
    // A name that does not look like one is not repeated.
    let odd = json!({ "object": "people", "at-good <script>": 1 });
    let err = invoke(&socket, &key, "objects.get", odd).await.unwrap_err();
    assert_eq!(err.message(), "the input has a field this operation does not know");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn the_fields_of_a_filter_and_of_a_records_values_are_the_workspaces_own_and_pass_as_given() {
    // No schema of this crate lists them, so none of them is "unknown".
    let (server, socket, key) = answering(200, json!({ "data": record() })).await;
    let values = json!({ "any_slug_at_all": "x", "41252299-f8c7-4b5e-99c9-4ff8321d2f96": [{ "nested": { "freely": true } }], "cleared": null });
    let input = json!({ "object": "people", "record": RECORD, "values": values.clone() });
    invoke(&socket, &key, "records.update", input).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "data": { "values": values } })
    );

    let (server, socket, key) = answering(200, json!({ "data": [] })).await;
    let filter = json!({ "$and": [{ "stage": "Won" }, { "path": [["candidates", "parent_record"]], "constraints": { "value": null } }] });
    let input = json!({ "list": "enterprise_sales", "filter": filter.clone() });
    invoke(&socket, &key, "entries.query", input).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "filter": filter, "limit": 25 })
    );
}

#[tokio::test]
async fn an_input_of_the_wrong_type_is_refused_without_repeating_what_was_in_it() {
    let (server, socket, key) = attio().await;
    for (name, input, message) in [
        ("records.get", json!({ "object": "people" }), "missing field `record`"),
        (
            "records.get",
            json!({ "object": 7, "record": RECORD }),
            "`object` has the wrong type",
        ),
        (
            "notes.create",
            json!({ "parent_object": "people", "parent_record": RECORD, "title": ["secret plan"], "content": "c" }),
            // An option sits beside the arguments, and its place is not known once it fails to read.
            "the input has a field of the wrong type",
        ),
    ] {
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(err.message().contains(message), "{}", err.message());
        assert!(!err.message().contains("secret plan"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_operation_attio_does_not_have_is_unsupported() {
    let (_server, socket, key) = answering(200, json!({ "data": [] })).await;
    let err = invoke(&socket, &key, "records.merge", json!({})).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unsupported);
    // A deleted thing answers with an empty object, and that is a success.
    let (_server, socket, key) = answering(200, json!({})).await;
    let done = invoke(&socket, &key, "notes.delete", json!({ "note": NOTE }))
        .await
        .unwrap();
    assert_eq!(done, Value::Null);
    let (_server, socket, key) = answer_nothing().await;
    let done = invoke(&socket, &key, "tasks.delete", json!({ "task": TASK }))
        .await
        .unwrap();
    assert_eq!(done, Value::Null);
}

/// A server that answers every request with a success and no body.
async fn answer_nothing() -> (MockServer, Socket, ConnectionKey) {
    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(answer(204, &json!(null)))
        .mount(&server)
        .await;
    (server, socket, key)
}
