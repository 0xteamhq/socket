//! GitHub against a local server that answers as GitHub does.

use std::sync::Arc;

use serde_json::json;
use socketkit_core::{ErrorKind, Integration, ProviderSpec};
use socketkit_github::{GitHub, provider};
use socketkit_testkit::wiremock::matchers::{header, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{conformance, connect, point_at};

fn build(spec: ProviderSpec) -> Arc<dyn Integration> {
    Arc::new(GitHub::with_spec(spec))
}

async fn github() -> (MockServer, socketkit_core::Socket, socketkit_core::ConnectionKey) {
    let server = MockServer::start().await;
    let (socket, key) = connect(build(point_at(provider(), &server)), "good-token").await;
    (server, socket, key)
}

#[tokio::test]
async fn passes_the_conformance_suite() {
    conformance::all(provider(), build).await;
}

#[tokio::test]
async fn identity_returns_the_account_and_sends_githubs_headers() {
    let (server, socket, key) = github().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", "Bearer good-token"))
        .and(header("accept", "application/vnd.github+json"))
        .and(header("x-github-api-version", "2022-11-28"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "id": 583231, "login": "octocat", "name": "The Octocat", "email": null })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let account = socket
        .invoke(key, "github.identity.get".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(account, json!({ "id": "583231", "name": "The Octocat", "email": null }));
}

#[tokio::test]
async fn an_account_without_a_display_name_is_shown_by_its_login() {
    let (server, socket, key) = github().await;
    Mock::given(path("/user"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "id": 7, "login": "ada", "name": null, "email": "ada@example.test" })),
        )
        .mount(&server)
        .await;
    let account = socket
        .invoke(key, "github.identity.get".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(
        account,
        json!({ "id": "7", "name": "ada", "email": "ada@example.test" })
    );
}

#[tokio::test]
async fn a_reachable_repository_resolves_to_its_canonical_name() {
    let (server, socket, key) = github().await;
    // GitHub answers with the canonical casing.
    Mock::given(path("/repos/acme/frontend"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "full_name": "Acme/Frontend" })))
        .mount(&server)
        .await;
    for input in ["acme/frontend", "https://github.com/acme/frontend/issues/3"] {
        let resolved = socket
            .invoke(key.clone(), "github.resource.resolve".into(), json!({ "input": input }))
            .await
            .unwrap();
        assert_eq!(
            resolved,
            json!({ "id": "Acme/Frontend", "label": "Acme/Frontend", "description": "GitHub repository" })
        );
    }
}

#[tokio::test]
async fn a_missing_or_private_repository_is_not_found_by_name() {
    let (server, socket, key) = github().await;
    Mock::given(path("/repos/acme/secret"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "message": "Not Found" })))
        .mount(&server)
        .await;
    let err = socket
        .invoke(key, "github.resource.resolve".into(), json!({ "input": "acme/secret" }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(err.message(), "GitHub repository acme/secret was not found");
}

#[tokio::test]
async fn an_org_that_restricts_oauth_apps_is_reported_with_githubs_reason() {
    let (server, socket, key) = github().await;
    let reason = "the `locked-org` organization has enabled OAuth App access restrictions";
    Mock::given(path("/repos/locked-org/api"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({ "message": reason })))
        .mount(&server)
        .await;
    let err = socket
        .invoke(
            key,
            "github.resource.resolve".into(),
            json!({ "input": "locked-org/api" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert!(err.message().ends_with(reason), "{}", err.message());
}

#[tokio::test]
async fn a_403_with_an_exhausted_quota_is_throttling_not_a_refusal() {
    let (server, socket, key) = github().await;
    Mock::given(path("/repos/acme/frontend"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .set_body_json(json!({ "message": "API rate limit exceeded" })),
        )
        .mount(&server)
        .await;
    let err = socket
        .invoke(
            key,
            "github.resource.resolve".into(),
            json!({ "input": "acme/frontend" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimited);
}

#[tokio::test]
async fn bad_input_is_refused_without_calling_github() {
    let (server, socket, key) = github().await;
    for input in [
        json!({ "input": "not a repo" }),
        json!({ "input": "acme/.." }),
        json!({}),
        json!({ "input": 7 }),
    ] {
        let err = socket
            .invoke(key.clone(), "github.resource.resolve".into(), input)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}
