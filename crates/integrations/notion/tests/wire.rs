//! Notion against a local server that answers as Notion does.

use std::sync::Arc;

use serde_json::json;
use socketkit_core::{ConnectionKey, ErrorKind, Integration, ProviderSpec, Socket};
use socketkit_notion::{Notion, provider};
use socketkit_testkit::wiremock::matchers::{header, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{conformance, connect, point_at};

const ID: &str = "0123abcd-4567-89ab-cdef-0123456789ab";

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
        .and(header("notion-version", "2022-06-28"))
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
    // Notion answers 400 when a database id is asked for as a page.
    Mock::given(path(format!("/v1/pages/{ID}")))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(json!({ "code": "validation_error", "message": "is a database" })),
        )
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
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        2,
        "pages, then databases"
    );
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
