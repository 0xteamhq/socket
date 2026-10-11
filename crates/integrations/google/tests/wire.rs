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
    conformance::all(provider(), build, "1AbC_dEf-GhIjKlMnOpQrStUvWxYz012345").await;
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

#[tokio::test]
async fn a_file_other_than_the_one_asked_for_is_refused() {
    let (server, socket, key) = google().await;
    Mock::given(path(format!("/drive/v3/files/{FILE}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "id": "another-file-id-0000", "name": "Other" })),
        )
        .mount(&server)
        .await;
    assert_eq!(
        resolve(&socket, &key, FILE).await.unwrap_err().kind(),
        ErrorKind::Decode
    );
}

#[tokio::test]
async fn the_hosts_of_the_apis_that_are_built_may_receive_the_token_and_no_other_google_host_may() {
    let spec = provider();
    let allows = |u: &str| spec.allows_host(&u.parse().unwrap());
    assert!(allows(
        "https://www.googleapis.com/calendar/v3/calendars/primary/events"
    ));
    assert!(allows("https://docs.googleapis.com/v1/documents/x"));
    assert!(allows("https://sheets.googleapis.com/v4/spreadsheets/x"));
    assert!(allows("https://gmail.googleapis.com/gmail/v1/users/me/profile"));
    assert!(allows("https://meet.googleapis.com/v2/conferenceRecords"));
    assert!(
        !allows("https://accounts.google.com/"),
        "the browser goes there, the token does not"
    );
    assert!(!allows("https://storage.googleapis.com/"));
}

fn restricted(server: &MockServer, domain: &str) -> Arc<dyn Integration> {
    Arc::new(Google::with_spec(point_at(provider(), server)).hosted_domain(domain))
}

/// Asks for the identity of an account whose email is `email` and whose
/// verified Workspace domain, as Google reports it, is `hd`.
async fn identity_as(email: &str, hd: serde_json::Value, required: &str) -> socketkit_core::Result<serde_json::Value> {
    let server = MockServer::start().await;
    Mock::given(path("/drive/v3/about"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user": { "displayName": "Someone", "emailAddress": email, "permissionId": "42" }
        })))
        .mount(&server)
        .await;
    let mut userinfo = json!({ "sub": "42", "email": email, "email_verified": true });
    if !hd.is_null() {
        userinfo["hd"] = hd;
    }
    Mock::given(path("/oauth2/v3/userinfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(userinfo))
        .mount(&server)
        .await;
    let (socket, key) = connect(restricted(&server, required), "ya29.good").await;
    socket.invoke(key, "google.identity.get".into(), json!({})).await
}

#[tokio::test]
async fn an_account_is_judged_by_googles_verified_domain_not_by_its_email_address() {
    // A personal Google account can be registered with a work email address. Its
    // email is in the domain, but Google reports no Workspace domain for it.
    let personal = identity_as("ada@acme.example", json!(null), "acme.example")
        .await
        .unwrap_err();
    assert_eq!(personal.kind(), ErrorKind::AccessDenied);

    for other in [
        json!("other.example"),
        json!("acme.example.evil.test"),
        json!(""),
        json!(7),
    ] {
        let err = identity_as("ada@acme.example", other.clone(), "acme.example")
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::AccessDenied, "{other}");
    }
}

#[tokio::test]
async fn an_account_in_the_required_workspace_is_accepted_whatever_the_case() {
    let account = identity_as("ada@acme.example", json!("ACME.example"), "Acme.Example")
        .await
        .unwrap();
    assert_eq!(account["email"], "ada@acme.example");
}

#[tokio::test]
async fn requiring_a_workspace_asks_for_the_scopes_the_check_needs_and_fails_closed_without_them() {
    let socketkit_core::AuthScheme::OAuth2(oauth) = Google::new().hosted_domain("acme.example").provider().auth else {
        panic!("google uses OAuth")
    };
    assert!(
        oauth.default_scopes.iter().any(|s| s == "openid"),
        "{:?}",
        oauth.default_scopes
    );
    assert!(oauth.default_scopes.iter().any(|s| s == "email"));
    assert!(
        oauth
            .extra_authorize_params
            .contains(&("hd".into(), "acme.example".into()))
    );

    // A token without those scopes cannot read the verified domain: the check refuses, it does not skip.
    let server = MockServer::start().await;
    Mock::given(path("/drive/v3/about"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "user": { "emailAddress": "ada@acme.example", "permissionId": "42" } })),
        )
        .mount(&server)
        .await;
    Mock::given(path("/oauth2/v3/userinfo"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({ "error": "insufficient_scope" })))
        .mount(&server)
        .await;
    let (socket, key) = connect(restricted(&server, "acme.example"), "ya29.good").await;
    assert!(
        socket
            .invoke(key, "google.identity.get".into(), json!({}))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_workspace_domain_that_is_not_a_domain_is_refused_when_the_socket_is_built() {
    for bad in [
        "",
        "@",
        "ada@acme.example",
        "https://acme.example",
        "acme",
        "acme .example",
        "-acme.example",
        "acme-.example",
    ] {
        let integration: Arc<dyn Integration> = Arc::new(Google::new().hosted_domain(bad));
        let err = Socket::in_memory().integration(integration).build().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Config, "{bad:?}");
    }
    let fine: Arc<dyn Integration> = Arc::new(Google::new().hosted_domain("@Acme.Example"));
    Socket::in_memory().integration(fine).build().unwrap();
}

#[tokio::test]
async fn correcting_a_bad_workspace_domain_clears_the_problem() {
    let corrected: Arc<dyn Integration> = Arc::new(
        Google::new()
            .hosted_domain("not a domain")
            .hosted_domain("acme.example"),
    );
    Socket::in_memory().integration(corrected).build().unwrap();
    let broken_again: Arc<dyn Integration> = Arc::new(Google::new().hosted_domain("acme.example").hosted_domain(""));
    assert_eq!(
        Socket::in_memory()
            .integration(broken_again)
            .build()
            .unwrap_err()
            .kind(),
        ErrorKind::Config
    );
}
