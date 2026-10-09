//! Linear against a local server that answers as Linear does.

use std::sync::Arc;

use serde_json::json;
use socketkit_core::{ConnectionKey, ErrorKind, Integration, ProviderSpec, Socket};
use socketkit_linear::{Linear, provider};
use socketkit_testkit::wiremock::matchers::{body_string_contains, header, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{conformance, connect, point_at};

fn build(spec: ProviderSpec) -> Arc<dyn Integration> {
    Arc::new(Linear::with_spec(spec))
}

async fn linear(response: ResponseTemplate) -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(response)
        .mount(&server)
        .await;
    let (socket, key) = connect(build(point_at(provider(), &server)), "lin-good").await;
    (server, socket, key)
}

fn ok(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

async fn resolve(socket: &Socket, key: &ConnectionKey, input: &str) -> socketkit_core::Result<serde_json::Value> {
    socket
        .invoke(key.clone(), "linear.resource.resolve".into(), json!({ "input": input }))
        .await
}

#[tokio::test]
async fn passes_the_conformance_suite() {
    conformance::all(provider(), build, "ENG").await;
}

#[tokio::test]
async fn identity_asks_for_the_viewer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", "Bearer lin-good"))
        .and(body_string_contains("viewer"))
        .respond_with(ok(
            json!({ "data": { "viewer": { "id": "u-1", "name": "Ada", "email": "ada@example.test" } } }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let (socket, key) = connect(build(point_at(provider(), &server)), "lin-good").await;
    let account = socket
        .invoke(key, "linear.identity.get".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(
        account,
        json!({ "id": "u-1", "name": "Ada", "email": "ada@example.test" })
    );
}

#[tokio::test]
async fn a_team_resolves_to_its_id_and_the_key_is_sent_as_a_variable() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains(r#""variables":{"key":"ENG"}"#))
        .respond_with(ok(
            json!({ "data": { "teams": { "nodes": [{ "id": "team-uuid", "key": "ENG", "name": "Engineering" }] } } }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let (socket, key) = connect(build(point_at(provider(), &server)), "lin-good").await;
    let resolved = resolve(&socket, &key, " eng ").await.unwrap();
    assert_eq!(
        resolved,
        json!({ "id": "team-uuid", "label": "Engineering (ENG)", "description": "Linear team" })
    );
}

#[tokio::test]
async fn no_matching_team_is_not_found_by_key() {
    let (_server, socket, key) = linear(ok(json!({ "data": { "teams": { "nodes": [] } } }))).await;
    let err = resolve(&socket, &key, "ops").await.unwrap_err();
    assert_eq!(
        (err.kind(), err.message()),
        (ErrorKind::NotFound, "Linear team OPS was not found")
    );
}

#[tokio::test]
async fn a_graphql_error_is_never_mistaken_for_no_such_team() {
    let failure =
        json!({ "data": null, "errors": [{ "message": "boom", "extensions": { "code": "INTERNAL_SERVER_ERROR" } }] });
    let (_server, socket, key) = linear(ok(failure)).await;
    let err = resolve(&socket, &key, "ENG").await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unexpected);
}

#[tokio::test]
async fn an_authentication_error_in_a_400_requires_reconnecting() {
    let body = json!({ "errors": [{ "message": "Authentication required", "extensions": { "code": "AUTHENTICATION_ERROR" } }] });
    let (_server, socket, key) = linear(ResponseTemplate::new(400).set_body_json(body)).await;
    let err = socket
        .invoke(key, "linear.identity.get".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ReconnectRequired);
}

#[tokio::test]
async fn bad_input_is_refused_without_calling_linear() {
    let (server, socket, key) = linear(ok(json!({}))).await;
    assert_eq!(
        resolve(&socket, &key, "ENG-123").await.unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_team_other_than_the_one_asked_for_is_refused() {
    let wrong = json!({ "data": { "teams": { "nodes": [{ "id": "team-ops", "key": "OPS", "name": "Operations" }] } } });
    let (_server, socket, key) = linear(ok(wrong)).await;
    assert_eq!(
        resolve(&socket, &key, "ENG").await.unwrap_err().kind(),
        ErrorKind::Decode
    );

    let blank = json!({ "data": { "teams": { "nodes": [{ "id": "", "key": "ENG", "name": "Engineering" }] } } });
    let (_server, socket, key) = linear(ok(blank)).await;
    assert_eq!(
        resolve(&socket, &key, "ENG").await.unwrap_err().kind(),
        ErrorKind::Decode
    );
}

#[tokio::test]
async fn a_personal_api_key_is_sent_bare_and_an_oauth_token_as_bearer() {
    for (token, header_value) in [
        ("lin_api_abc123", "lin_api_abc123"),
        ("lin_oauth_xyz", "Bearer lin_oauth_xyz"),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("authorization", header_value))
            .respond_with(ok(
                json!({ "data": { "viewer": { "id": "u-1", "name": "Ada", "email": null } } }),
            ))
            .expect(1)
            .mount(&server)
            .await;
        // The real constructor decides how the token is sent; only the address moves to the local server.
        let moved = point_at(Linear::with_token(token).provider(), &server);
        let integration: Arc<dyn Integration> = Arc::new(Linear::with_spec(moved).token(token));
        let socket = Socket::in_memory().integration(integration).build().unwrap();
        let key = ConnectionKey::new(provider().id, "anyone");
        socket
            .invoke(key, "linear.identity.get".into(), json!({}))
            .await
            .unwrap_or_else(|e| panic!("{token}: {e}"));
    }
}
