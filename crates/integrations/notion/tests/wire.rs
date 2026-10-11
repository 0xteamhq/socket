//! Notion against a local server that answers as Notion does.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use socketkit_core::{ConnectionKey, ErrorKind, Integration, ProviderId, ProviderSpec, Retry, Socket};
use socketkit_notion::{Notion, provider};
use socketkit_testkit::wiremock::matchers::{any, header, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{conformance, connect, point_at};

mod support;
use support::notion_error;

const ID: &str = "0123abcd-4567-89ab-cdef-0123456789ab";

/// What Notion answers when an id belongs to another kind of object.
const IS_A_DATABASE: &str = "Provided ID 0123abcd-4567-89ab-cdef-0123456789ab is a database, not a page. Use the retrieve database API instead: https://developers.notion.com/reference/retrieve-a-database";

fn build(spec: ProviderSpec) -> Arc<dyn Integration> {
    Arc::new(Notion::with_spec(spec))
}

async fn notion() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let (socket, key) = connect(build(point_at(provider(), &server)), "ntn-good").await;
    (server, socket, key)
}

async fn resolve(socket: &Socket, key: &ConnectionKey, input: &str) -> socketkit_core::Result<serde_json::Value> {
    socket
        .invoke(key.clone(), "notion.resource.resolve".into(), json!({ "input": input }))
        .await
}

#[tokio::test]
async fn passes_the_conformance_suite() {
    conformance::all(provider(), build, "0123abcd456789abcdef0123456789ab").await;
}

#[tokio::test]
async fn identity_shows_the_person_who_owns_the_integration_and_sends_the_version() {
    let (server, socket, key) = notion().await;
    Mock::given(path("/v1/users/me"))
        .and(header("notion-version", "2026-03-11"))
        .and(header("authorization", "Bearer ntn-good"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "user", "id": "bot-1", "type": "bot", "name": "Acme bot",
            "bot": { "workspace_name": "Acme", "owner": { "type": "user", "user": {
                "id": "u-1", "name": "Ada Lovelace", "person": { "email": "ada@example.test" } } } }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let account = socket
        .invoke(key, "notion.identity.get".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(
        account,
        json!({ "id": "bot-1", "name": "Ada Lovelace", "email": "ada@example.test" })
    );
}

#[tokio::test]
async fn a_workspace_owned_integration_is_shown_by_its_workspace() {
    let (server, socket, key) = notion().await;
    Mock::given(path("/v1/users/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "bot-2", "name": "Acme bot", "bot": { "workspace_name": "Acme", "owner": { "type": "workspace", "workspace": true } }
        })))
        .mount(&server)
        .await;
    let account = socket
        .invoke(key, "notion.identity.get".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(account, json!({ "id": "bot-2", "name": "Acme", "email": null }));
}

#[tokio::test]
async fn a_page_resolves_with_its_title() {
    let (server, socket, key) = notion().await;
    Mock::given(path(format!("/v1/pages/{ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "page",
            "properties": { "Name": { "type": "title", "title": [{ "plain_text": "Roadmap" }] } }
        })))
        .mount(&server)
        .await;
    let resolved = resolve(
        &socket,
        &key,
        "https://www.notion.so/acme/Roadmap-0123abcd456789abcdef0123456789ab",
    )
    .await
    .unwrap();
    assert_eq!(
        resolved,
        json!({ "id": ID, "label": "Roadmap", "description": "Notion page" })
    );
}

#[tokio::test]
async fn an_id_that_is_not_a_page_is_tried_as_a_database() {
    let (server, socket, key) = notion().await;
    // Notion answers 400 when a database id is asked for as a page, and says so.
    Mock::given(path(format!("/v1/pages/{ID}")))
        .respond_with(notion_error(400, "validation_error", IS_A_DATABASE))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path(format!("/v1/databases/{ID}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "object": "database", "title": [{ "plain_text": "Tasks" }] })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let resolved = resolve(&socket, &key, ID).await.unwrap();
    assert_eq!(
        resolved,
        json!({ "id": ID, "label": "Tasks", "description": "Notion database" })
    );
}

#[tokio::test]
async fn an_id_that_is_neither_is_not_found() {
    let (server, socket, key) = notion().await;
    Mock::given(socketkit_testkit::wiremock::matchers::any())
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({ "code": "object_not_found", "message": "Could not find" })),
        )
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, ID).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    // Notion answers the same for what does not exist and for what was not
    // shared, so the person is told of both.
    assert_eq!(
        err.message(),
        format!("Notion page or database {ID} was not found, or it was not shared with this integration")
    );
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        2,
        "pages, then databases"
    );
}

#[tokio::test]
async fn a_400_that_is_not_about_the_kind_of_id_is_reported_and_not_as_not_found() {
    for (code, message) in [
        ("missing_version", "Notion-Version header should be defined"),
        (
            "validation_error",
            "path failed validation: path.page_id should be a valid uuid",
        ),
        ("invalid_request", "This API is not supported"),
    ] {
        // The page is refused, and there is no database with the id either.
        let (server, socket, key) = notion().await;
        Mock::given(path(format!("/v1/pages/{ID}")))
            .respond_with(notion_error(400, code, message))
            .mount(&server)
            .await;
        Mock::given(path(format!("/v1/databases/{ID}")))
            .respond_with(notion_error(404, "object_not_found", "Could not find database"))
            .mount(&server)
            .await;
        let err = resolve(&socket, &key, ID).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{message}");
        assert_eq!(err.message(), format!("notion rejected the request: {message}"));

        // Every request is refused the same way, as when the version is wrong.
        let (server, socket, key) = notion().await;
        Mock::given(any())
            .respond_with(notion_error(400, code, message))
            .mount(&server)
            .await;
        let err = resolve(&socket, &key, ID).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{message}");
        assert_eq!(err.message(), format!("notion rejected the request: {message}"));
    }
}

#[tokio::test]
async fn a_400_on_the_database_is_reported_too() {
    let (server, socket, key) = notion().await;
    Mock::given(path(format!("/v1/pages/{ID}")))
        .respond_with(notion_error(404, "object_not_found", "Could not find page"))
        .mount(&server)
        .await;
    Mock::given(path(format!("/v1/databases/{ID}")))
        .respond_with(notion_error(
            400,
            "missing_version",
            "Notion-Version header should be defined",
        ))
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, ID).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(err.message().ends_with("Notion-Version header should be defined"));
}

#[tokio::test]
async fn a_database_resolves_however_notion_words_its_refusal_of_the_page() {
    // What Notion says when a database's id is asked for as a page is not
    // in its documentation, so nothing is read from the words.
    let (server, socket, key) = notion().await;
    Mock::given(path(format!("/v1/pages/{ID}")))
        .respond_with(notion_error(400, "validation_error", "That is not a page."))
        .mount(&server)
        .await;
    Mock::given(path(format!("/v1/databases/{ID}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "object": "database", "title": [{ "plain_text": "Tasks" }] })),
        )
        .mount(&server)
        .await;
    let resolved = resolve(&socket, &key, ID).await.unwrap();
    assert_eq!(resolved["description"], "Notion database");
}

#[tokio::test]
async fn an_id_of_another_kind_is_refused_in_notions_own_words() {
    let (server, socket, key) = notion().await;
    Mock::given(any())
        .respond_with(notion_error(400, "validation_error", IS_A_DATABASE))
        .mount(&server)
        .await;
    let err = socket
        .invoke(key, "notion.pages.get".into(), json!({ "page": ID }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(err.message().contains("is a database, not a page"), "{}", err.message());
}

#[tokio::test]
async fn what_is_not_found_may_only_be_unshared_and_the_caller_is_told_so() {
    for (operation, input) in [
        ("notion.pages.get", json!({ "page": ID })),
        ("notion.blocks.children", json!({ "block": ID })),
        ("notion.databases.query", json!({ "data_source": ID })),
        ("notion.comments.list", json!({ "block": ID })),
    ] {
        let (server, socket, key) = notion().await;
        let message = "Could not find page with ID: 0123abcd. Make sure the relevant pages and databases are shared with your integration.";
        Mock::given(any())
            .respond_with(notion_error(404, "object_not_found", message))
            .mount(&server)
            .await;
        let err = socket.invoke(key, operation.into(), input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::NotFound, "{operation}");
        assert_eq!(
            err.message(),
            "notion has nothing with that id, or it was not shared with this integration",
            "{operation}"
        );
        assert_eq!(err.retry(), Retry::Never, "{operation}");
    }
}

#[tokio::test]
async fn an_overloaded_notion_is_a_rate_limit_with_the_wait_it_asks_for() {
    let (server, socket, key) = notion().await;
    Mock::given(any())
        .respond_with(notion_error(529, "service_overload", "Notion is overloaded").insert_header("retry-after", "120"))
        .mount(&server)
        .await;
    let err = socket
        .invoke(key.clone(), "notion.pages.get".into(), json!({ "page": ID }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimited);
    assert_eq!(err.retry(), Retry::After(Duration::from_secs(120)));
    // A write that was throttled was not carried out, and says the same.
    let err = socket
        .invoke(key, "notion.pages.archive".into(), json!({ "page": ID }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimited);
}

#[tokio::test]
async fn a_missing_capability_is_a_refusal_in_notions_words() {
    let (server, socket, key) = notion().await;
    Mock::given(any())
        .respond_with(notion_error(
            403,
            "restricted_resource",
            "Insufficient permissions for this endpoint.",
        ))
        .mount(&server)
        .await;
    let err = socket
        .invoke(key, "notion.comments.list".into(), json!({ "block": ID }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert!(
        err.message().ends_with("Insufficient permissions for this endpoint."),
        "{}",
        err.message()
    );
}

#[tokio::test]
async fn a_server_error_repeats_a_read_and_never_a_write_that_may_have_happened() {
    for (operation, input, times) in [
        ("notion.pages.get", json!({ "page": ID }), 2),
        // The two reads sent as POST are not repeated either: the transport
        // goes by the verb.
        ("notion.search.run", json!({}), 1),
        ("notion.pages.create", json!({ "parent": { "page_id": ID } }), 1),
        ("notion.pages.update", json!({ "page": ID, "icon": null }), 1),
        ("notion.pages.archive", json!({ "page": ID }), 1),
        (
            "notion.blocks.append",
            json!({ "block": ID, "children": [{ "divider": {} }] }),
            1,
        ),
        (
            "notion.comments.create",
            json!({ "discussion_id": ID, "markdown": "ok" }),
            1,
        ),
    ] {
        let (server, socket, key) = notion().await;
        Mock::given(any())
            .respond_with(notion_error(503, "service_unavailable", "Notion is unavailable"))
            .mount(&server)
            .await;
        let err = socket.invoke(key, operation.into(), input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{operation}");
        assert_eq!(server.received_requests().await.unwrap().len(), times, "{operation}");
    }
}

#[tokio::test]
async fn notion_under_another_provider_id_is_refused_when_the_socket_is_built() {
    // The operations are named `notion.…`, so the definition has to keep that id.
    let mut spec = provider();
    spec.id = ProviderId::new("notion_eu").unwrap();
    let err = Socket::in_memory()
        .integration(Arc::new(Notion::with_spec(spec)))
        .build()
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Config);
    assert!(err.message().contains("must be named notion_eu."), "{}", err.message());
}

#[tokio::test]
async fn a_refusal_on_the_page_is_reported_and_the_database_is_not_tried() {
    let (server, socket, key) = notion().await;
    Mock::given(path(format!("/v1/pages/{ID}")))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({ "code": "restricted_resource", "message": "Insufficient permissions" })),
        )
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, ID).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert!(err.message().ends_with("Insufficient permissions"), "{}", err.message());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn bad_input_is_refused_without_calling_notion() {
    let (server, socket, key) = notion().await;
    assert_eq!(
        resolve(&socket, &key, "roadmap").await.unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn notion_built_with_a_token_and_a_version_sends_both() {
    use socketkit_core::SecretString;
    use socketkit_notion::NotionToken;

    let server = MockServer::start().await;
    Mock::given(path("/v1/users/me"))
        .and(header("authorization", "Bearer secret_abc"))
        .and(header("notion-version", "2025-09-03"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "bot-1", "name": "Acme bot" })))
        .expect(1)
        .mount(&server)
        .await;
    let settings = NotionToken {
        token: SecretString::new("secret_abc"),
        version: Some("2025-09-03".into()),
    };
    // Built by the real constructor, so this fails if `with_token` stops applying
    // the token or the version. Only the address is moved to the local server.
    let moved = Notion::with_token(settings).spec(point_at(provider(), &server));
    assert_eq!(
        format!("{:?}", moved.fixed_token().unwrap().access_token),
        "SecretString(***)"
    );
    let socket = Socket::in_memory().integration(Arc::new(moved)).build().unwrap();
    let key = ConnectionKey::new(provider().id, "anyone");
    socket
        .invoke(key, "notion.identity.get".into(), json!({}))
        .await
        .unwrap();
}
