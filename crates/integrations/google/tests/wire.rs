//! Google against a local server that answers as Google does.

use std::sync::Arc;

use serde_json::json;
use socketkit_core::{ConnectionKey, ErrorKind, Integration, ProviderSpec, Socket};
use socketkit_google::{Google, provider};
use socketkit_testkit::wiremock::matchers::{header, path, query_param};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{conformance, connect, point_at};

const FILE: &str = "1AbC_dEf-GhIjKlMnOpQrStUvWxYz012345";

fn build(spec: ProviderSpec) -> Arc<dyn Integration> {
    Arc::new(Google::with_spec(spec))
}

async fn google() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let (socket, key) = connect(build(point_at(provider(), &server)), "ya29.good").await;
    (server, socket, key)
}

async fn resolve(socket: &Socket, key: &ConnectionKey, input: &str) -> socketkit_core::Result<serde_json::Value> {
    socket
        .invoke(key.clone(), "google.resource.resolve".into(), json!({ "input": input }))
        .await
}

#[tokio::test]
async fn passes_the_conformance_suite() {
    conformance::all(provider(), build).await;
}

#[tokio::test]
async fn the_definition_asks_google_for_a_refresh_token() {
    let socketkit_core::AuthScheme::OAuth2(oauth) = provider().auth else {
        panic!("google uses OAuth")
    };
    assert!(
        oauth
            .extra_authorize_params
            .contains(&("access_type".into(), "offline".into()))
    );
    assert!(
        oauth
            .extra_authorize_params
            .contains(&("prompt".into(), "consent".into()))
    );
}

#[tokio::test]
async fn identity_reads_the_drive_user() {
    let (server, socket, key) = google().await;
    Mock::given(path("/drive/v3/about"))
        .and(query_param("fields", "user"))
        .and(header("authorization", "Bearer ya29.good"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user": { "displayName": "Ada Lovelace", "emailAddress": "ada@example.test", "permissionId": "0123456789" }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let account = socket
        .invoke(key, "google.identity.get".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(
        account,
        json!({ "id": "0123456789", "name": "Ada Lovelace", "email": "ada@example.test" })
    );
}

#[tokio::test]
async fn a_document_resolves_with_its_name_and_kind() {
    let (server, socket, key) = google().await;
    Mock::given(path(format!("/drive/v3/files/{FILE}")))
        .and(query_param("supportsAllDrives", "true"))
        .and(query_param("fields", "id,name,mimeType"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": FILE, "name": "Design notes", "mimeType": "application/vnd.google-apps.document"
        })))
        .mount(&server)
        .await;
    let url = format!("https://docs.google.com/document/d/{FILE}/edit");
    let resolved = resolve(&socket, &key, &url).await.unwrap();
    assert_eq!(
        resolved,
        json!({ "id": FILE, "label": "Design notes", "description": "Google Doc" })
    );
}

#[tokio::test]
async fn a_file_the_account_cannot_see_is_not_found_by_id() {
    let (server, socket, key) = google().await;
    Mock::given(path(format!("/drive/v3/files/{FILE}")))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({ "error": { "code": 404, "message": "File not found" } })),
        )
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, FILE).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(err.message(), format!("Google Drive item {FILE} was not found"));
}

#[tokio::test]
async fn a_disabled_api_is_reported_with_googles_reason() {
    let (server, socket, key) = google().await;
    let reason = "Google Drive API has not been used in project 1 before or it is disabled.";
    Mock::given(path(format!("/drive/v3/files/{FILE}")))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({ "error": { "code": 403, "message": reason } })))
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, FILE).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert!(err.message().ends_with(reason), "{}", err.message());
}

#[tokio::test]
async fn bad_input_is_refused_without_calling_google() {
    let (server, socket, key) = google().await;
    assert_eq!(
        resolve(&socket, &key, "short").await.unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}
