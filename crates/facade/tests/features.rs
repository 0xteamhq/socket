//! Each feature switches on exactly its own integration crate.

#[test]
fn the_core_is_always_re_exported() {
    let id = socketkit::ProviderId::new("slack").unwrap();
    assert_eq!(id.as_str(), "slack");
}

#[cfg(feature = "slack")]
#[test]
fn the_slack_feature_exposes_the_slack_crate() {
    let id = socketkit::ProviderId::new(socketkit::slack::PROVIDER_ID).unwrap();
    assert_eq!(id.as_str(), "slack");
}

#[cfg(all(
    feature = "github",
    feature = "google",
    feature = "linear",
    feature = "notion",
    feature = "slack",
    feature = "zoom"
))]
#[test]
fn all_six_integrations_register_together_with_two_operations_each() {
    use std::sync::Arc;

    let socket = socketkit::Socket::builder(Arc::new(socketkit::MemoryTokenStore::new()))
        .integration(Arc::new(socketkit::github::GitHub::new()))
        .integration(Arc::new(socketkit::google::Google::new()))
        .integration(Arc::new(socketkit::linear::Linear::new()))
        .integration(Arc::new(socketkit::notion::Notion::new()))
        .integration(Arc::new(socketkit::slack::Slack::new()))
        .integration(Arc::new(socketkit::zoom::Zoom::new()))
        .build()
        .unwrap();
    let ids: Vec<String> = socket.providers().into_iter().map(|p| p.id.to_string()).collect();
    assert_eq!(ids, ["github", "google", "linear", "notion", "slack", "zoom"]);
    let names: Vec<String> = socket.operations().into_iter().map(|o| o.name).collect();
    assert_eq!(names.len(), 12);
    for id in &ids {
        assert!(names.contains(&format!("{id}.identity.get")), "{id}");
        assert!(names.contains(&format!("{id}.resource.resolve")), "{id}");
    }
}

#[cfg(all(feature = "github", feature = "slack"))]
#[test]
fn connection_details_are_given_to_each_integration() {
    use std::sync::Arc;

    use socketkit::{ConnectionKey, OAuthClient, ProviderId, SecretString, Socket};

    let socket = Socket::in_memory()
        .integration(Arc::new(socketkit::slack::Slack::with_token("xoxb-a-token")))
        .integration(Arc::new(socketkit::github::GitHub::with_oauth(OAuthClient {
            client_id: "gh-client-id".into(),
            client_secret: SecretString::new("gh-client-secret"),
            redirect_uri: "https://statustool.example/oauth/callback".parse().unwrap(),
        })))
        .build()
        .unwrap();

    // GitHub can start connecting a user with nothing else configured.
    let github = ConnectionKey::new(ProviderId::new("github").unwrap(), "user-42");
    let url = socket.begin_authorization(github, None).unwrap().url;
    assert_eq!(url.host_str(), Some("github.com"));
    assert!(url.query_pairs().any(|(n, v)| n == "client_id" && v == "gh-client-id"));
    assert!(!url.as_str().contains("gh-client-secret"));

    // Slack was given a token, not an OAuth app, so there is no OAuth flow to start.
    let slack = ConnectionKey::new(ProviderId::new("slack").unwrap(), "user-42");
    assert_eq!(
        socket.begin_authorization(slack, None).unwrap_err().kind(),
        socketkit::ErrorKind::Config
    );
}

#[cfg(all(
    feature = "github",
    feature = "google",
    feature = "linear",
    feature = "notion",
    feature = "slack",
    feature = "zoom"
))]
mod every_integration {
    use std::sync::Arc;

    use socketkit::{ConnectionKey, ErrorKind, Integration, OAuthClient, ProviderId, SecretString, Socket};

    fn client(id: &str) -> OAuthClient {
        OAuthClient {
            client_id: format!("{id}-client-id"),
            client_secret: SecretString::new(format!("{id}-client-secret")),
            redirect_uri: "https://app.example.test/oauth/callback".parse().unwrap(),
        }
    }

    #[test]
    fn takes_an_oauth_app_and_can_start_connecting_a_user() {
        let integrations: Vec<Arc<dyn Integration>> = vec![
            Arc::new(socketkit::github::GitHub::with_oauth(client("github"))),
            Arc::new(socketkit::google::Google::with_oauth(client("google"))),
            Arc::new(socketkit::linear::Linear::with_oauth(client("linear"))),
            Arc::new(socketkit::notion::Notion::with_oauth(client("notion"))),
            Arc::new(socketkit::slack::Slack::with_oauth(client("slack"))),
            Arc::new(socketkit::zoom::Zoom::with_oauth(client("zoom"))),
        ];
        let mut builder = Socket::in_memory();
        for integration in &integrations {
            builder = builder.integration(integration.clone());
        }
        let socket = builder.build().unwrap();
        for integration in &integrations {
            let id = integration.provider().id;
            let key = ConnectionKey::new(id.clone(), "user-1");
            let url = socket.begin_authorization(key, None).unwrap().url;
            let has_own_client = url
                .query_pairs()
                .any(|(n, v)| n == "client_id" && v == format!("{id}-client-id"));
            assert!(has_own_client, "{id}: {url}");
            assert!(
                !url.as_str().contains("client-secret"),
                "{id}: the secret never goes in the URL"
            );
        }
    }

    #[test]
    fn takes_a_token_and_reports_it_as_the_one_to_use() {
        let integrations: Vec<Arc<dyn Integration>> = vec![
            Arc::new(socketkit::github::GitHub::with_token("t-github")),
            Arc::new(socketkit::google::Google::with_token("t-google")),
            Arc::new(socketkit::linear::Linear::with_token("t-linear")),
            Arc::new(socketkit::notion::Notion::with_token("t-notion")),
            Arc::new(socketkit::slack::Slack::with_token("t-slack")),
            Arc::new(socketkit::zoom::Zoom::with_token("t-zoom")),
        ];
        let mut builder = Socket::in_memory();
        for integration in &integrations {
            let id = integration.provider().id;
            let token = integration
                .fixed_token()
                .unwrap_or_else(|| panic!("{id} kept its token"));
            assert_eq!(token.access_token.expose(), format!("t-{id}"));
            assert!(integration.oauth_client().is_none(), "{id}");
            builder = builder.integration(integration.clone());
        }
        let socket = builder.build().unwrap();
        // A token is not an OAuth app: there is no flow to start.
        let key = ConnectionKey::new(ProviderId::new("zoom").unwrap(), "user-1");
        assert_eq!(
            socket.begin_authorization(key, None).unwrap_err().kind(),
            ErrorKind::Config
        );
    }

    #[test]
    fn refuses_a_blank_token() {
        let blank: Arc<dyn Integration> = Arc::new(socketkit::linear::Linear::with_token("  "));
        assert_eq!(
            Socket::in_memory().integration(blank).build().unwrap_err().kind(),
            ErrorKind::Config
        );
    }
}
