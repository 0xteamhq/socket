//! Zoom against a local server that answers as Zoom does.

use std::sync::Arc;

use serde_json::json;
use socketkit_core::{ConnectionKey, ErrorKind, Integration, ProviderSpec, Socket};
use socketkit_testkit::wiremock::matchers::{header, path, query_param};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{conformance, connect, point_at};
use socketkit_zoom::{Zoom, provider};

fn build(spec: ProviderSpec) -> Arc<dyn Integration> {
    Arc::new(Zoom::with_spec(spec))
}

async fn zoom() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let (socket, key) = connect(build(point_at(provider(), &server)), "zm-good").await;
    (server, socket, key)
}

async fn resolve(socket: &Socket, key: &ConnectionKey, input: &str) -> socketkit_core::Result<serde_json::Value> {
    socket
        .invoke(key.clone(), "zoom.resource.resolve".into(), json!({ "input": input }))
        .await
}

#[tokio::test]
async fn passes_the_conformance_suite() {
    conformance::all(provider(), build).await;
}

#[tokio::test]
async fn identity_prefers_the_display_name_and_falls_back_to_first_and_last() {
    let (server, socket, key) = zoom().await;
    Mock::given(path("/v2/users/me"))
        .and(header("authorization", "Bearer zm-good"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "KDcuGIm1QgePTO8WbOqwIQ", "first_name": "Ada", "last_name": "Lovelace", "email": "ada@example.test"
        })))
        .mount(&server)
        .await;
    let account = socket.invoke(key, "zoom.identity.get".into(), json!({})).await.unwrap();
    assert_eq!(
        account,
        json!({ "id": "KDcuGIm1QgePTO8WbOqwIQ", "name": "Ada Lovelace", "email": "ada@example.test" })
    );
}

#[tokio::test]
async fn my_own_recordings_resolve_when_the_list_can_be_read() {
    let (server, socket, key) = zoom().await;
    Mock::given(path("/v2/users/me/recordings"))
        .and(query_param("page_size", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "meetings": [] })))
        .expect(1)
        .mount(&server)
        .await;
    let resolved = resolve(&socket, &key, "me").await.unwrap();
    assert_eq!(
        resolved,
        json!({ "id": "me", "label": "My Zoom recordings", "description": "Zoom cloud recordings" })
    );
}

#[tokio::test]
async fn another_users_recordings_resolve_by_email() {
    let (server, socket, key) = zoom().await;
    Mock::given(path("/v2/users/grace@example.test/recordings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "meetings": [] })))
        .expect(1)
        .mount(&server)
        .await;
    let resolved = resolve(&socket, &key, "Grace@Example.test").await.unwrap();
    assert_eq!(resolved["id"], "grace@example.test");
    assert_eq!(resolved["label"], "grace@example.test");
}

#[tokio::test]
async fn a_user_outside_the_account_is_not_found_whether_zoom_says_400_or_404() {
    for status in [400, 404] {
        let (server, socket, key) = zoom().await;
        Mock::given(path("/v2/users/nobody@example.test/recordings"))
            .respond_with(
                ResponseTemplate::new(status).set_body_json(json!({ "code": 1001, "message": "User does not exist" })),
            )
            .mount(&server)
            .await;
        let err = resolve(&socket, &key, "nobody@example.test").await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::NotFound, "HTTP {status}");
        assert_eq!(err.message(), "Zoom user nobody@example.test was not found");
    }
}

#[tokio::test]
async fn a_missing_scope_is_a_refusal_with_zooms_reason() {
    let (server, socket, key) = zoom().await;
    Mock::given(path("/v2/users/me/recordings"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({ "code": 4711, "message": "Invalid access token, does not contain scopes" })),
        )
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, "me").await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert!(err.message().contains("does not contain scopes"), "{}", err.message());
}

#[tokio::test]
async fn bad_input_is_refused_without_calling_zoom() {
    let (server, socket, key) = zoom().await;
    assert_eq!(
        resolve(&socket, &key, "me/recordings").await.unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}
