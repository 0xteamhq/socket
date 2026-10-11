//! Attio against a local server that answers as Attio does: the definition,
//! connecting, identity and lookup. The typed operations are in `operations.rs`.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use socketkit_attio::{Attio, AttioOAuth, TokenLevel, provider};
use socketkit_core::{
    AuthScheme, ConnectionKey, ErrorKind, Integration, MemoryTokenStore, OAuthClient, RetryPolicy, SecretString,
    Socket, TokenStore,
};
use socketkit_testkit::wiremock::matchers::{any, body_string_contains, header, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{TENANT, conformance, point_at};

mod support;
use support::{TOKEN, answering, attio, attio_error, build, object, only_request};

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

fn authorize_url(attio: Attio) -> url::Url {
    Socket::in_memory()
        .integration(Arc::new(attio))
        .build()
        .unwrap()
        .begin_authorization(key(), None)
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

/// What Attio says of a token it accepts.
fn token_info() -> Value {
    json!({
        "active": true, "scope": "record_permission:read object_configuration:read", "client_id": "app-1",
        "token_type": "Bearer", "exp": null, "iat": 1_760_000_000, "sub": "ws-1", "aud": "app-1", "iss": "attio.com",
        "token_level": "workspace", "authorized_by_workspace_member_id": "mem-1",
        "workspace_id": "ws-1", "workspace_name": "Acme", "workspace_slug": "acme", "workspace_logo_url": null
    })
}

// ── The definition and its settings ───────────────────────────────────────────

#[tokio::test]
async fn passes_the_conformance_suite() {
    conformance::all(provider(), build, "people").await;
}

#[tokio::test]
async fn the_definition_is_attios_v2_api_signed_in_through_its_app_with_pkce_and_no_scopes() {
    let spec = provider();
    assert_eq!(spec.api_base.as_str(), "https://api.attio.com/v2/");
    let AuthScheme::OAuth2(oauth) = &spec.auth else {
        panic!("attio uses OAuth")
    };
    assert_eq!(oauth.authorize_url.as_str(), "https://app.attio.com/authorize");
    assert_eq!(oauth.token_url.as_str(), "https://app.attio.com/oauth/token");
    // Attio takes no scopes at sign-in: they are set on the application.
    assert!(oauth.default_scopes.is_empty());
    assert!(oauth.pkce);

    let allows = |u: &str| spec.allows_host(&u.parse().unwrap());
    assert!(allows("https://api.attio.com/v2/self"));
    assert!(allows("https://app.attio.com/oauth/token"));
    for elsewhere in [
        "https://attio.com/",
        "https://files.attio.com/x",
        "https://api.attio.com.evil.example/",
    ] {
        assert!(!allows(elsewhere), "{elsewhere}");
    }
}

#[tokio::test]
async fn the_integration_keeps_the_provider_id_its_operations_are_named_after() {
    let mut spec = provider();
    spec.id = socketkit_core::ProviderId::new("attio-eu").unwrap();
    let err = Socket::in_memory()
        .integration(build(spec.clone()))
        .build()
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Config);
    // The integration says so itself, whatever order a host checks things in.
    let err = Attio::with_spec(spec).check().unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Config);
    assert!(err.message().contains("\"attio\""), "{}", err.message());
}

#[tokio::test]
async fn sign_in_asks_for_no_scopes_and_for_the_token_level_that_was_set() {
    let plain = authorize_url(Attio::with_oauth(client()));
    assert_eq!(plain.host_str(), Some("app.attio.com"));
    assert_eq!(plain.path(), "/authorize");
    assert_eq!(param(&plain, "client_id").as_deref(), Some("client-id"));
    assert_eq!(param(&plain, "response_type").as_deref(), Some("code"));
    assert_eq!(param(&plain, "scope"), None, "attio takes no scope parameter");
    assert_eq!(
        param(&plain, "token_level"),
        None,
        "attio's own default is a workspace token"
    );
    assert_eq!(param(&plain, "code_challenge_method").as_deref(), Some("S256"));
    assert!(param(&plain, "code_challenge").is_some_and(|c| !c.is_empty()));
    assert!(!plain.as_str().contains("client-secret"));

    for (level, written) in [(TokenLevel::User, "user"), (TokenLevel::Workspace, "workspace")] {
        let settings = AttioOAuth {
            client: client(),
            token_level: Some(level),
        };
        let url = authorize_url(Attio::with_oauth(settings));
        assert_eq!(param(&url, "token_level").as_deref(), Some(written));
        // A token for one person needs PKCE, which is on for every sign-in.
        assert!(param(&url, "code_challenge").is_some());
    }
}

// ── Connecting ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn connecting_exchanges_the_code_with_the_apps_credentials_in_the_body_and_keeps_a_token_that_does_not_expire() {
    let server = MockServer::start().await;
    let store = Arc::new(MemoryTokenStore::new());
    let attio = Attio::with_spec(point_at(provider(), &server)).oauth(client());
    let socket = Socket::builder(store.clone())
        .integration(Arc::new(attio))
        .retry(RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(50),
        })
        .build()
        .unwrap();

    let authorization = socket.begin_authorization(key(), None).unwrap();
    assert_eq!(authorization.url.path(), "/authorize");
    let verifier = authorization.pending.pkce_verifier.clone().expect("PKCE is on");

    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains(format!("code_verifier={}", verifier.expose())))
        .and(body_string_contains("client_id=client-id"))
        .and(body_string_contains("client_secret=client-secret"))
        // Attio answers with a token and its type, and nothing else.
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
    assert!(granted.refresh_token.is_none(), "attio issues no refresh token");
    assert!(granted.expires_at.is_none(), "an attio token does not expire");
    assert_eq!(store.load(key()).await.unwrap(), Some(granted));

    // The stored token is what the next call carries.
    Mock::given(method("GET"))
        .and(path("/v2/self"))
        .and(header("authorization", "Bearer at-first"))
        .respond_with(ok(token_info()))
        .expect(1)
        .mount(&server)
        .await;
    let account = identity(&socket, &key()).await.unwrap();
    assert_eq!(account["id"], "ws-1");
}

#[tokio::test]
async fn a_workspaces_api_key_is_sent_as_a_bearer_token_on_every_call() {
    let server = MockServer::start().await;
    Mock::given(any()).respond_with(ok(token_info())).mount(&server).await;
    let attio = Attio::with_spec(point_at(provider(), &server)).token("key-for-acme");
    let socket = Socket::in_memory().integration(Arc::new(attio)).build().unwrap();
    for tenant in ["one", "another"] {
        let key = ConnectionKey::new(provider().id, tenant);
        identity(&socket, &key).await.unwrap();
    }
    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 2);
    for request in received {
        assert_eq!(request.headers.get("authorization").unwrap(), "Bearer key-for-acme");
    }
    assert_eq!(
        Attio::with_token("key-for-acme")
            .fixed_token()
            .unwrap()
            .access_token
            .expose(),
        "key-for-acme"
    );
}

// ── Identity ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn identity_is_the_workspace_the_token_belongs_to() {
    let (server, socket, key) = attio().await;
    Mock::given(method("GET"))
        .and(path("/v2/self"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ok(token_info()))
        .expect(1)
        .mount(&server)
        .await;
    let account = identity(&socket, &key).await.unwrap();
    assert_eq!(account, json!({ "id": "ws-1", "name": "Acme", "email": null }));
    let request = only_request(&server).await;
    assert_eq!(request.url.query(), None);
    assert!(
        !request.url.as_str().contains(TOKEN),
        "the token is never in the address"
    );
}

#[tokio::test]
async fn a_workspace_without_a_name_is_shown_by_its_slug_and_then_by_its_id() {
    let mut info = token_info();
    info["workspace_name"] = json!("  ");
    let (_server, socket, key) = answering(200, info.clone()).await;
    assert_eq!(identity(&socket, &key).await.unwrap()["name"], "acme");

    info["workspace_slug"] = json!(null);
    let (_server, socket, key) = answering(200, info).await;
    assert_eq!(identity(&socket, &key).await.unwrap()["name"], "ws-1");
}

#[tokio::test]
async fn a_token_attio_no_longer_accepts_is_a_connection_to_renew_though_attio_answers_200() {
    // Attio does not answer 401 for a revoked token here: it answers 200 and says so.
    let (_server, socket, key) = answering(200, json!({ "active": false })).await;
    let err = identity(&socket, &key).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ReconnectRequired, "{err}");

    // An answer that names no workspace is not an account either.
    for body in [
        json!({ "active": true }),
        json!({ "active": true, "workspace_id": "" }),
        json!({}),
    ] {
        let (_server, socket, key) = answering(200, body.clone()).await;
        let err = identity(&socket, &key).await.expect_err(&body.to_string());
        assert_eq!(err.kind(), ErrorKind::Decode, "{body}: {err}");
    }
}

// ── Lookup ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_object_resolves_by_its_slug_to_what_every_other_method_takes() {
    let (server, socket, key) = attio().await;
    Mock::given(method("GET"))
        .and(path("/v2/objects/people"))
        .respond_with(ok(json!({ "data": object() })))
        .expect(1)
        .mount(&server)
        .await;
    let resolved = resolve(&socket, &key, " people ").await.unwrap();
    assert_eq!(
        resolved,
        json!({ "id": "people", "label": "People", "description": "Attio object" })
    );
}

#[tokio::test]
async fn an_object_without_a_slug_or_a_name_resolves_by_its_id() {
    let bare = json!({ "data": { "id": { "workspace_id": "ws-1", "object_id": "obj-9" }, "api_slug": null, "plural_noun": null, "singular_noun": "Deal" } });
    let (_server, socket, key) = answering(200, bare).await;
    let resolved = resolve(&socket, &key, "obj-9").await.unwrap();
    assert_eq!(
        resolved,
        json!({ "id": "obj-9", "label": "Deal", "description": "Attio object" })
    );
}

#[tokio::test]
async fn an_object_that_is_not_there_or_cannot_be_read_is_reported_as_that() {
    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(attio_error(
            404,
            "invalid_request_error",
            "not_found",
            "Object not found",
        ))
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, "peopel").await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(err.message(), "that Attio object was not found");

    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(attio_error(
            403,
            "auth_error",
            "unauthorized",
            "Missing object_configuration:read",
        ))
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, "people").await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert!(err.message().contains("object_configuration:read"), "{}", err.message());

    for blank in ["", "   ", ".."] {
        let (server, socket, key) = answering(200, json!({ "data": object() })).await;
        let err = resolve(&socket, &key, blank).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{blank:?}");
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}

// ── The typed methods ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    use socketkit_attio::models::{
        AttributeTarget, CreateEntry, CreateNote, CreateTask, ListAttributes, ListNotes, Paging, Query, Sort,
        SortDirection, UpdateTask, WriteEntry, WriteRecord,
    };
    use support::{attribute, body_of, entry, note, query_of, record, task};

    let server = MockServer::start().await;
    let attio = Attio::with_spec(point_at(provider(), &server));
    let (socket, key) = socketkit_testkit::connect(Arc::new(attio.clone()), TOKEN).await;
    let connection = socket.connection(key).await.unwrap();
    let mount = |verb: &'static str, at: &'static str, body: Value| {
        let server = &server;
        async move {
            Mock::given(method(verb))
                .and(path(format!("/v2{at}")))
                .respond_with(ok(body))
                .mount(server)
                .await;
        }
    };
    mount("GET", "/objects/people/attributes", json!({ "data": [attribute()] })).await;
    mount("POST", "/objects/people/records/query", json!({ "data": [record()] })).await;
    mount("PUT", "/objects/people/records", json!({ "data": record() })).await;
    mount("PATCH", "/objects/people/records/rec-1", json!({ "data": record() })).await;
    mount("GET", "/objects/people/records/rec-1/entries", json!({ "data": [] })).await;
    mount("POST", "/lists/sales/entries", json!({ "data": entry() })).await;
    mount("PATCH", "/lists/sales/entries/ent-1", json!({ "data": entry() })).await;
    mount("GET", "/notes", json!({ "data": [note()] })).await;
    mount("POST", "/notes", json!({ "data": note() })).await;
    mount("POST", "/tasks", json!({ "data": task() })).await;
    mount("PATCH", "/tasks/task-1", json!({ "data": task() })).await;
    mount("DELETE", "/tasks/task-1", json!({})).await;

    let attributes = attio
        .attributes(&connection)
        .list(AttributeTarget::Objects, "people", ListAttributes::default())
        .await
        .unwrap();
    assert_eq!(attributes.items[0].api_slug, "email_addresses");
    assert_eq!(attributes.items[0].kind, "email-address");
    assert!(attributes.items[0].is_unique);

    let records = attio.records(&connection);
    let query = Query {
        sorts: Some(vec![Sort::by("name", SortDirection::Desc)]),
        limit: Some(1),
        ..Query::default()
    };
    let page = records.query("people", query).await.unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("1"));
    let ada = &page.items[0];
    assert_eq!(ada.id.record_id, "rec-1");
    assert_eq!(ada.current["name"], "Ada Lovelace");
    assert_eq!(ada.values["name"][0].attribute_type, "personal-name");
    assert_eq!(ada.values["name"][0].fields["first_name"], "Ada");
    assert!(ada.values["name"][0].is_current());

    let person = WriteRecord::with("name", "Ada Lovelace").and("email_addresses", json!(["ada@example.test"]));
    let asserted = records.assert("people", "email_addresses", person).await.unwrap();
    assert_eq!(asserted.current["email_addresses"], "ada@example.test");
    let updated = records
        .update("people", "rec-1", WriteRecord::with("job_title", "Analyst"))
        .await
        .unwrap();
    assert_eq!(updated.id.record_id, "rec-1");
    assert!(
        records
            .entries("people", "rec-1", Paging::default())
            .await
            .unwrap()
            .items
            .is_empty()
    );

    let entries = attio.entries(&connection);
    let added = entries
        .create("sales", CreateEntry::of("companies", "rec-2"))
        .await
        .unwrap();
    assert_eq!(added.current["stage"], "In progress");
    entries
        .update("sales", "ent-1", WriteEntry::with("stage", "Won"))
        .await
        .unwrap();

    let notes = attio.notes(&connection);
    assert_eq!(
        notes.list(ListNotes::of("people", "rec-1")).await.unwrap().items[0].title,
        "Call"
    );
    let written = notes
        .create(CreateNote::on("people", "rec-1", "Call", "Asked about pricing."))
        .await
        .unwrap();
    assert_eq!(written.id.note_id, "note-1");

    let tasks = attio.tasks(&connection);
    assert_eq!(
        tasks
            .create(CreateTask::saying("Send the contract"))
            .await
            .unwrap()
            .id
            .task_id,
        "task-1"
    );
    let done = UpdateTask {
        is_completed: Some(true),
        deadline_at: Some(None),
        ..UpdateTask::default()
    };
    tasks.update("task-1", done).await.unwrap();
    tasks.delete("task-1").await.unwrap();

    let received = server.received_requests().await.unwrap();
    let sent: Vec<(String, String, Value, Value)> = received
        .iter()
        .map(|r| (r.method.to_string(), r.url.path().to_owned(), query_of(r), body_of(r)))
        .collect();
    let expected = [
        ("GET", "/v2/objects/people/attributes", json!({}), json!(null)),
        (
            "POST",
            "/v2/objects/people/records/query",
            json!({}),
            json!({ "sorts": [{ "direction": "desc", "attribute": "name" }], "limit": 1, "offset": 0 }),
        ),
        (
            "PUT",
            "/v2/objects/people/records",
            json!({ "matching_attribute": "email_addresses" }),
            json!({ "data": { "values": { "name": "Ada Lovelace", "email_addresses": ["ada@example.test"] } } }),
        ),
        (
            "PATCH",
            "/v2/objects/people/records/rec-1",
            json!({}),
            json!({ "data": { "values": { "job_title": "Analyst" } } }),
        ),
        (
            "GET",
            "/v2/objects/people/records/rec-1/entries",
            json!({ "limit": "100", "offset": "0" }),
            json!(null),
        ),
        (
            "POST",
            "/v2/lists/sales/entries",
            json!({}),
            json!({ "data": { "parent_object": "companies", "parent_record_id": "rec-2", "entry_values": {} } }),
        ),
        (
            "PATCH",
            "/v2/lists/sales/entries/ent-1",
            json!({}),
            json!({ "data": { "entry_values": { "stage": "Won" } } }),
        ),
        (
            "GET",
            "/v2/notes",
            json!({ "limit": "10", "offset": "0", "parent_object": "people", "parent_record_id": "rec-1" }),
            json!(null),
        ),
        (
            "POST",
            "/v2/notes",
            json!({}),
            json!({ "data": { "parent_object": "people", "parent_record_id": "rec-1", "title": "Call", "format": "plaintext", "content": "Asked about pricing." } }),
        ),
        (
            "POST",
            "/v2/tasks",
            json!({}),
            json!({ "data": { "content": "Send the contract", "format": "plaintext", "deadline_at": null, "is_completed": false, "linked_records": [], "assignees": [] } }),
        ),
        (
            "PATCH",
            "/v2/tasks/task-1",
            json!({}),
            json!({ "data": { "deadline_at": null, "is_completed": true } }),
        ),
        ("DELETE", "/v2/tasks/task-1", json!({}), json!(null)),
    ];
    assert_eq!(sent.len(), expected.len());
    for (sent, (verb, at, query, body)) in sent.iter().zip(expected) {
        assert_eq!((sent.0.as_str(), sent.1.as_str()), (verb, at));
        assert_eq!(sent.2, query, "{verb} {at}");
        assert_eq!(sent.3, body, "{verb} {at}");
    }
}
