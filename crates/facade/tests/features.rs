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
    feature = "attio",
    feature = "github",
    feature = "google",
    feature = "hubspot",
    feature = "linear",
    feature = "microsoft",
    feature = "notion",
    feature = "pipedrive",
    feature = "salesforce",
    feature = "slack",
    feature = "zoom"
))]
#[test]
fn all_eleven_integrations_register_together_and_each_offers_identity_and_lookup() {
    use std::sync::Arc;

    let socket = socketkit::Socket::builder(Arc::new(socketkit::MemoryTokenStore::new()))
        .integration(Arc::new(socketkit::attio::Attio::new()))
        .integration(Arc::new(socketkit::github::GitHub::new()))
        .integration(Arc::new(socketkit::google::Google::new()))
        .integration(Arc::new(socketkit::hubspot::HubSpot::new()))
        .integration(Arc::new(socketkit::linear::Linear::new()))
        .integration(Arc::new(socketkit::microsoft::Microsoft::new()))
        .integration(Arc::new(socketkit::notion::Notion::new()))
        .integration(Arc::new(socketkit::pipedrive::Pipedrive::new()))
        .integration(Arc::new(socketkit::salesforce::Salesforce::new()))
        .integration(Arc::new(socketkit::slack::Slack::new()))
        .integration(Arc::new(socketkit::zoom::Zoom::new()))
        .build()
        .unwrap();
    let ids: Vec<String> = socket.providers().into_iter().map(|p| p.id.to_string()).collect();
    assert_eq!(
        ids,
        [
            "attio",
            "github",
            "google",
            "hubspot",
            "linear",
            "microsoft",
            "notion",
            "pipedrive",
            "salesforce",
            "slack",
            "zoom"
        ]
    );
    let names: Vec<String> = socket.operations().into_iter().map(|o| o.name).collect();
    // Every integration has these two; Slack, Microsoft and the four CRMs have typed operations besides.
    assert!(names.len() >= 22);
    assert!(names.contains(&"attio.records.query".to_owned()));
    assert!(names.contains(&"hubspot.objects.search".to_owned()));
    assert!(names.contains(&"pipedrive.deals.list".to_owned()));
    assert!(names.contains(&"salesforce.query.run".to_owned()));
    assert!(names.contains(&"slack.chat.post_message".to_owned()));
    assert!(names.contains(&"microsoft.events.create".to_owned()));
    assert!(names.contains(&"microsoft.mail.send".to_owned()));
    assert!(names.contains(&"microsoft.chats.send".to_owned()));
    assert!(names.contains(&"microsoft.transcripts.content".to_owned()));
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
    feature = "attio",
    feature = "github",
    feature = "google",
    feature = "hubspot",
    feature = "linear",
    feature = "microsoft",
    feature = "notion",
    feature = "pipedrive",
    feature = "salesforce",
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
            Arc::new(socketkit::attio::Attio::with_oauth(client("attio"))),
            Arc::new(socketkit::github::GitHub::with_oauth(client("github"))),
            Arc::new(socketkit::google::Google::with_oauth(client("google"))),
            Arc::new(socketkit::hubspot::HubSpot::with_oauth(client("hubspot"))),
            Arc::new(socketkit::linear::Linear::with_oauth(client("linear"))),
            Arc::new(socketkit::microsoft::Microsoft::with_oauth(client("microsoft"))),
            Arc::new(socketkit::notion::Notion::with_oauth(client("notion"))),
            Arc::new(socketkit::pipedrive::Pipedrive::with_oauth(client("pipedrive"))),
            Arc::new(socketkit::salesforce::Salesforce::with_oauth(client("salesforce"))),
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
            Arc::new(socketkit::attio::Attio::with_token("t-attio")),
            Arc::new(socketkit::github::GitHub::with_token("t-github")),
            Arc::new(socketkit::google::Google::with_token("t-google")),
            Arc::new(socketkit::hubspot::HubSpot::with_token("t-hubspot")),
            Arc::new(socketkit::linear::Linear::with_token("t-linear")),
            Arc::new(socketkit::microsoft::Microsoft::with_token("t-microsoft")),
            Arc::new(socketkit::notion::Notion::with_token("t-notion")),
            Arc::new(socketkit::pipedrive::Pipedrive::with_token("t-pipedrive")),
            // A Salesforce token is for one organisation, so it comes with that organisation's address.
            Arc::new(socketkit::salesforce::Salesforce::with_token(
                "t-salesforce",
                "https://acme.my.salesforce.com",
            )),
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

    fn authorize_url(integration: Arc<dyn Integration>) -> url::Url {
        let key = ConnectionKey::new(integration.provider().id, "user-1");
        Socket::in_memory()
            .integration(integration)
            .build()
            .unwrap()
            .begin_authorization(key, None)
            .unwrap()
            .url
    }

    fn param(url: &url::Url, name: &str) -> Option<String> {
        url.query_pairs().find(|(n, _)| n == name).map(|(_, v)| v.into_owned())
    }

    #[test]
    fn slack_takes_user_scopes_as_its_own_parameter() {
        let settings = socketkit::slack::SlackOAuth {
            client: client("slack"),
            scopes: Some(vec!["chat:write".into()]),
            user_scopes: vec!["search:read".into(), "users:read".into()],
        };
        let url = authorize_url(Arc::new(socketkit::slack::Slack::with_oauth(settings)));
        assert_eq!(
            param(&url, "scope").as_deref(),
            Some("chat:write"),
            "the given scopes replace the defaults"
        );
        assert_eq!(param(&url, "user_scope").as_deref(), Some("search:read,users:read"));

        let plain = authorize_url(Arc::new(socketkit::slack::Slack::with_oauth(client("slack"))));
        assert_eq!(param(&plain, "user_scope"), None);
        assert!(
            param(&plain, "scope").unwrap().contains("channels:read"),
            "the defaults stay when none are given"
        );
    }

    #[test]
    fn google_takes_a_workspace_domain_and_a_login_hint_and_keeps_asking_for_a_refresh_token() {
        let settings = socketkit::google::GoogleOAuth {
            client: client("google"),
            scopes: None,
            hosted_domain: Some("acme.example".into()),
            login_hint: Some("ada@acme.example".into()),
        };
        let url = authorize_url(Arc::new(socketkit::google::Google::with_oauth(settings)));
        assert_eq!(param(&url, "hd").as_deref(), Some("acme.example"));
        assert_eq!(param(&url, "login_hint").as_deref(), Some("ada@acme.example"));
        assert_eq!(param(&url, "access_type").as_deref(), Some("offline"));
    }

    #[test]
    fn microsoft_takes_a_tenant_a_login_hint_and_a_prompt_and_always_asks_for_a_refresh_token() {
        let settings = socketkit::microsoft::MicrosoftOAuth {
            client: client("microsoft"),
            scopes: Some(vec!["Calendars.Read".into()]),
            tenant: Some("contoso.onmicrosoft.com".into()),
            login_hint: Some("ada@contoso.example".into()),
            prompt: Some(socketkit::microsoft::Prompt::SelectAccount),
        };
        let url = authorize_url(Arc::new(socketkit::microsoft::Microsoft::with_oauth(settings)));
        assert_eq!(url.host_str(), Some("login.microsoftonline.com"));
        assert_eq!(url.path(), "/contoso.onmicrosoft.com/oauth2/v2.0/authorize");
        assert_eq!(param(&url, "login_hint").as_deref(), Some("ada@contoso.example"));
        assert_eq!(param(&url, "prompt").as_deref(), Some("select_account"));
        assert_eq!(param(&url, "scope").as_deref(), Some("Calendars.Read offline_access"));
    }

    #[test]
    fn github_takes_an_enterprise_server_host_and_sends_nothing_to_github_com() {
        let oauth = socketkit::github::GitHubOAuth {
            client: client("github"),
            scopes: Some(vec!["read:org".into()]),
            host: Some("GitHub.Acme.Example".into()),
        };
        let integration = socketkit::github::GitHub::with_oauth(oauth);
        let spec = integration.provider();
        assert_eq!(spec.api_base.as_str(), "https://github.acme.example/api/v3/");
        assert!(spec.allows_host(&"https://github.acme.example/api/v3/user".parse().unwrap()));
        assert!(
            !spec.allows_host(&"https://api.github.com/user".parse().unwrap()),
            "an Enterprise token never goes to github.com"
        );
        let url = authorize_url(Arc::new(integration));
        assert_eq!(url.host_str(), Some("github.acme.example"));
        assert_eq!(url.path(), "/login/oauth/authorize");
        assert_eq!(param(&url, "scope").as_deref(), Some("read:org"));

        let token = socketkit::github::GitHubToken {
            token: SecretString::new("ghp_x"),
            host: Some("github.acme.example".into()),
        };
        let with_token = socketkit::github::GitHub::with_token(token);
        assert_eq!(with_token.provider().allowed_hosts, ["github.acme.example"]);
    }

    #[test]
    fn a_github_host_that_is_not_a_bare_host_name_is_refused_when_the_socket_is_built() {
        for host in [
            "https://github.acme.example",
            "github.acme.example/api",
            "evil.test#",
            "a..b",
            "host:8443",
            "user@host",
            "",
            " ",
            "-bad.example",
        ] {
            let token = socketkit::github::GitHubToken {
                token: SecretString::new("ghp_x"),
                host: Some(host.into()),
            };
            let integration: Arc<dyn Integration> = Arc::new(socketkit::github::GitHub::with_token(token));
            let err = Socket::in_memory().integration(integration).build().unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Config, "{host:?}");
        }
    }

    #[test]
    fn hubspot_asks_for_optional_scopes_as_their_own_parameter() {
        let settings = socketkit::hubspot::HubSpotOAuth {
            client: client("hubspot"),
            scopes: Some(vec!["crm.objects.contacts.read".into()]),
            optional_scopes: vec!["crm.objects.custom.read".into()],
        };
        let url = authorize_url(Arc::new(socketkit::hubspot::HubSpot::with_oauth(settings)));
        assert_eq!(url.host_str(), Some("app.hubspot.com"));
        let required = param(&url, "scope").unwrap();
        assert!(required.contains("crm.objects.contacts.read"), "{required}");
        assert!(
            !required.contains("crm.objects.custom.read"),
            "a scope only some plans have is not required: {required}"
        );
        assert_eq!(
            param(&url, "optional_scope").as_deref(),
            Some("crm.objects.custom.read")
        );

        let plain = authorize_url(Arc::new(socketkit::hubspot::HubSpot::with_oauth(client("hubspot"))));
        assert_eq!(param(&plain, "optional_scope"), None);
    }

    #[test]
    fn salesforce_signs_in_at_the_host_it_is_told_and_a_token_keeps_to_its_own_organisation() {
        use socketkit::salesforce::{LoginHost, Salesforce, SalesforceOAuth};

        let at = |login: Option<LoginHost>| {
            let settings = SalesforceOAuth {
                login,
                ..client("salesforce").into()
            };
            authorize_url(Arc::new(Salesforce::with_oauth(settings)))
        };
        assert_eq!(at(None).host_str(), Some("login.salesforce.com"));
        assert_eq!(at(Some(LoginHost::Sandbox)).host_str(), Some("test.salesforce.com"));
        let own = at(Some(LoginHost::MyDomain("Acme.my.salesforce.com".into())));
        assert_eq!(own.host_str(), Some("acme.my.salesforce.com"));
        assert_eq!(own.path(), "/services/oauth2/authorize");
        assert!(param(&own, "code_challenge").is_some(), "PKCE is on for Salesforce");

        // A host that is not Salesforce's is refused when the socket is built.
        for host in [
            "evil.test",
            "acme.my.salesforce.com.evil.test",
            "acme.my.salesforce.com/x",
            "",
        ] {
            let settings = SalesforceOAuth {
                login: Some(LoginHost::MyDomain(host.into())),
                ..client("salesforce").into()
            };
            let integration: Arc<dyn Integration> = Arc::new(Salesforce::with_oauth(settings));
            let err = Socket::in_memory().integration(integration).build().unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Config, "{host:?}");
        }

        // A token given directly calls the organisation it was given with.
        let given = Salesforce::with_token("t", "https://acme.my.salesforce.com");
        let base = given.fixed_token().unwrap().api_base.unwrap();
        assert_eq!(base.host_str(), Some("acme.my.salesforce.com"));
        assert!(base.path().starts_with("/services/data/v"), "{base}");
        let elsewhere: Arc<dyn Integration> = Arc::new(Salesforce::with_token("t", "https://evil.test"));
        assert_eq!(
            Socket::in_memory().integration(elsewhere).build().unwrap_err().kind(),
            ErrorKind::Config
        );
    }

    #[test]
    fn a_pipedrive_api_token_goes_in_a_header_and_never_in_the_address() {
        use socketkit::{ApiKeySpec, AuthScheme, KeyPlacement};

        let integration = socketkit::pipedrive::Pipedrive::with_token("t-pipedrive");
        let AuthScheme::ApiKey(ApiKeySpec { placement }) = integration.provider().auth else {
            panic!("an API token is not an OAuth connection")
        };
        assert!(
            matches!(placement, KeyPlacement::Header { ref name, prefix: None } if name == "x-api-token"),
            "{placement:?}"
        );
        // Signing in through OAuth is still the definition a plain `new` gives.
        assert!(matches!(
            socketkit::pipedrive::Pipedrive::new().provider().auth,
            AuthScheme::OAuth2(_)
        ));
    }

    #[test]
    fn scopes_can_be_replaced_on_an_integration_with_no_other_settings() {
        let settings = socketkit::linear::LinearOAuth {
            client: client("linear"),
            scopes: Some(vec!["read".into()]),
        };
        let url = authorize_url(Arc::new(socketkit::linear::Linear::with_oauth(settings)));
        assert_eq!(param(&url, "scope").as_deref(), Some("read"));
    }
}
