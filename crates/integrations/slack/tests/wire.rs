//! Slack against a local server that answers as Slack does.

use std::sync::Arc;

use serde_json::json;
use socketkit_core::{ConnectionKey, ErrorKind, Integration, ProviderSpec, Socket};
use socketkit_slack::{Slack, provider};
use socketkit_testkit::wiremock::matchers::{header, path, query_param};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{conformance, connect, point_at};

fn build(spec: ProviderSpec) -> Arc<dyn Integration> {
    Arc::new(Slack::with_spec(spec))
}

async fn slack() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let (socket, key) = connect(build(point_at(provider(), &server)), "xoxp-good").await;
    (server, socket, key)
}

fn ok(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

async fn resolve(socket: &Socket, key: &ConnectionKey, input: &str) -> socketkit_core::Result<serde_json::Value> {
    socket
        .invoke(key.clone(), "slack.resource.resolve".into(), json!({ "input": input }))
        .await
}

#[tokio::test]
async fn passes_the_conformance_suite() {
    conformance::all(provider(), build, "C0123ABCD").await;
}

#[tokio::test]
async fn identity_returns_the_user_behind_the_token() {
    let (server, socket, key) = slack().await;
    Mock::given(path("/api/auth.test"))
        .and(header("authorization", "Bearer xoxp-good"))
        .respond_with(ok(
            json!({ "ok": true, "user_id": "U123", "user": "ada", "team": "Acme", "team_id": "T1" }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let account = socket
        .invoke(key, "slack.identity.get".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(account, json!({ "id": "U123", "name": "ada", "email": null }));
}

#[tokio::test]
async fn a_revoked_token_reported_inside_a_200_requires_reconnecting() {
    let (server, socket, key) = slack().await;
    Mock::given(path("/api/auth.test"))
        .respond_with(ok(json!({ "ok": false, "error": "token_revoked" })))
        .mount(&server)
        .await;
    let err = socket
        .invoke(key, "slack.identity.get".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ReconnectRequired);
}

#[tokio::test]
async fn a_channel_id_is_looked_up_directly() {
    let (server, socket, key) = slack().await;
    Mock::given(path("/api/conversations.info"))
        .and(query_param("channel", "C0123ABCD"))
        .respond_with(ok(
            json!({ "ok": true, "channel": { "id": "C0123ABCD", "name": "eng" } }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let resolved = resolve(&socket, &key, "C0123ABCD").await.unwrap();
    assert_eq!(
        resolved,
        json!({ "id": "C0123ABCD", "label": "#eng", "description": "Slack channel" })
    );
}

#[tokio::test]
async fn a_channel_name_is_found_on_a_later_page() {
    let (server, socket, key) = slack().await;
    Mock::given(path("/api/conversations.list"))
        .and(query_param("cursor", ""))
        .and(query_param("types", "public_channel"))
        .respond_with(ok(json!({
            "ok": true,
            "channels": [{ "id": "C1", "name": "general" }],
            "response_metadata": { "next_cursor": "page-2" }
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/conversations.list"))
        .and(query_param("cursor", "page-2"))
        .respond_with(ok(json!({
            "ok": true,
            "channels": [{ "id": "C2", "name": "eng-backend" }],
            "response_metadata": { "next_cursor": "" }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let resolved = resolve(&socket, &key, "#Eng-Backend").await.unwrap();
    assert_eq!(resolved["id"], "C2");
    assert_eq!(resolved["label"], "#eng-backend");
}

#[tokio::test]
async fn a_name_on_no_page_and_an_unknown_id_are_not_found_by_name() {
    let (server, socket, key) = slack().await;
    Mock::given(path("/api/conversations.list"))
        .respond_with(ok(
            json!({ "ok": true, "channels": [{ "id": "C1", "name": "general" }], "response_metadata": {} }),
        ))
        .mount(&server)
        .await;
    Mock::given(path("/api/conversations.info"))
        .respond_with(ok(json!({ "ok": false, "error": "channel_not_found" })))
        .mount(&server)
        .await;

    let by_name = resolve(&socket, &key, "#nowhere").await.unwrap_err();
    assert_eq!(
        (by_name.kind(), by_name.message()),
        (ErrorKind::NotFound, "Slack channel #nowhere was not found")
    );
    let by_id = resolve(&socket, &key, "C0000000000").await.unwrap_err();
    assert_eq!(
        (by_id.kind(), by_id.message()),
        (ErrorKind::NotFound, "Slack channel C0000000000 was not found")
    );
}

#[tokio::test]
async fn paging_stops_at_the_limit_when_slack_never_ends_the_list() {
    let (server, socket, key) = slack().await;
    Mock::given(path("/api/conversations.list"))
        .respond_with(ok(
            json!({ "ok": true, "channels": [], "response_metadata": { "next_cursor": "again" } }),
        ))
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, "#nowhere").await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        20,
        "twenty pages, then stop"
    );
}

#[tokio::test]
async fn a_missing_scope_is_a_refusal_that_names_the_scope() {
    let (server, socket, key) = slack().await;
    Mock::given(path("/api/conversations.list"))
        .respond_with(ok(
            json!({ "ok": false, "error": "missing_scope", "needed": "channels:read" }),
        ))
        .mount(&server)
        .await;
    let err = resolve(&socket, &key, "#eng").await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert!(err.message().contains("channels:read"), "{}", err.message());
}

#[tokio::test]
async fn bad_input_is_refused_without_calling_slack() {
    let (server, socket, key) = slack().await;
    assert_eq!(
        resolve(&socket, &key, "has space").await.unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_lookup_that_answers_with_another_channel_or_a_blank_account_is_refused() {
    let (server, socket, key) = slack().await;
    Mock::given(path("/api/conversations.info"))
        .respond_with(ok(
            json!({ "ok": true, "channel": { "id": "C9999ZZZZ", "name": "other" } }),
        ))
        .mount(&server)
        .await;
    Mock::given(path("/api/auth.test"))
        .respond_with(ok(json!({ "ok": true, "user_id": "", "user": "ada" })))
        .mount(&server)
        .await;
    assert_eq!(
        resolve(&socket, &key, "C0123ABCD").await.unwrap_err().kind(),
        ErrorKind::Decode
    );
    let err = socket
        .invoke(key, "slack.identity.get".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
}
