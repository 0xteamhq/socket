//! Socket integration for GitHub.
//!
//! Offers the provider definition, `github.identity.get` and
//! `github.resource.resolve` (a repository). Typed operations arrive in phase 2.

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Access, Account, AuthScheme, ClientAuth, Connection, Error, ErrorKind, Integration, OAuth2Spec, OAuthClient,
    OperationInfo, ProviderId, ProviderSpec, RawRequest, Resource, Result, SecretString, TokenSet, identity_operation,
    resolve_input, resolve_operation, to_output,
};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "github";

/// GitHub's definition: where its API lives and how it authenticates.
///
/// # Panics
/// Never in practice: the URLs are constants that parse.
pub fn provider() -> ProviderSpec {
    ProviderSpec {
        id: ProviderId::new(PROVIDER_ID).expect("a valid provider id"),
        display_name: "GitHub".into(),
        api_base: "https://api.github.com/".parse().expect("a valid URL"),
        // github.com serves the OAuth token endpoint.
        allowed_hosts: vec!["api.github.com".into(), "github.com".into()],
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: "https://github.com/login/oauth/authorize".parse().expect("a valid URL"),
            token_url: "https://github.com/login/oauth/access_token"
                .parse()
                .expect("a valid URL"),
            default_scopes: vec!["repo".into(), "read:user".into()],
            scope_separator: " ".into(),
            pkce: false,
            client_auth: ClientAuth::Body,
            extra_authorize_params: Vec::new(),
        }),
    }
}

/// OAuth settings for GitHub. A plain [`OAuthClient`] converts into this with
/// the defaults, so `GitHub::with_oauth(client)` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct GitHubOAuth {
    /// The application's own OAuth app.
    pub client: OAuthClient,
    /// Scopes to ask for in place of the defaults.
    pub scopes: Option<Vec<String>>,
    /// The host of a GitHub Enterprise Server, such as `github.acme.example`. `None` means github.com.
    pub host: Option<String>,
}

impl From<OAuthClient> for GitHubOAuth {
    fn from(client: OAuthClient) -> Self {
        Self {
            client,
            scopes: None,
            host: None,
        }
    }
}

/// A GitHub personal access token or app token. A plain string converts into this, so
/// `GitHub::with_token("…")` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct GitHubToken {
    pub token: SecretString,
    /// The host of a GitHub Enterprise Server, such as `github.acme.example`. `None` means github.com.
    pub host: Option<String>,
}

impl From<String> for GitHubToken {
    fn from(token: String) -> Self {
        Self {
            token: SecretString::new(token),
            host: None,
        }
    }
}

impl From<&str> for GitHubToken {
    fn from(token: &str) -> Self {
        token.to_owned().into()
    }
}

/// GitHub Enterprise Server's definition for `host`, or `None` when `host` is
/// not a bare host name.
pub fn provider_for_host(host: &str) -> Option<ProviderSpec> {
    let host = host.trim().to_ascii_lowercase();
    // A DNS name: labels of at most 63 characters, 253 in all.
    let is_label = |label: &str| {
        (1..=63).contains(&label.len())
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    if host.is_empty() || host.len() > 253 || !host.split('.').all(is_label) {
        return None;
    }
    let url = |path: &str| format!("https://{host}{path}").parse::<url::Url>().ok();
    let mut spec = provider();
    spec.api_base = url("/api/v3/")?;
    if let AuthScheme::OAuth2(oauth) = &mut spec.auth {
        oauth.authorize_url = url("/login/oauth/authorize")?;
        oauth.token_url = url("/login/oauth/access_token")?;
    }
    // The host as the URL parser wrote it: `127.1` becomes `127.0.0.1`, and
    // the allowlist must name the host requests will really go to.
    spec.allowed_hosts = vec![spec.api_base.host_str()?.to_owned()];
    Some(spec)
}

/// The GitHub integration.
#[derive(Debug, Clone)]
pub struct GitHub {
    spec: ProviderSpec,
    access: Access,
    /// What is wrong with the settings it was created with, if anything.
    problem: Option<String>,
    /// The hosts people paste repository URLs from: `github.com`, or an Enterprise Server's.
    web_hosts: Vec<String>,
}

impl Default for GitHub {
    fn default() -> Self {
        Self::new()
    }
}

impl GitHub {
    /// GitHub with no connection details of its own: the OAuth app is set on the
    /// `Socket` builder and tokens come from the application's token store.
    pub fn new() -> Self {
        Self::with_spec(provider())
    }

    /// GitHub with the application's OAuth app, for connecting users through OAuth.
    /// Takes an [`OAuthClient`], or a [`GitHubOAuth`] to set scopes or an Enterprise Server host.
    pub fn with_oauth(settings: impl Into<GitHubOAuth>) -> Self {
        let settings = settings.into();
        let mut this = Self::on_host(settings.host);
        if let (Some(scopes), AuthScheme::OAuth2(oauth)) = (settings.scopes, &mut this.spec.auth) {
            oauth.default_scopes = scopes;
        }
        this.oauth(settings.client)
    }

    /// GitHub with a token the application already holds. Every call uses it.
    /// Takes a string, or a [`GitHubToken`] to set an Enterprise Server host.
    pub fn with_token(settings: impl Into<GitHubToken>) -> Self {
        let settings = settings.into();
        let mut this = Self::on_host(settings.host);
        this.access.token = Some(TokenSet {
            access_token: settings.token,
            refresh_token: None,
            expires_at: None,
            scopes: Vec::new(),
        });
        this
    }

    fn on_host(host: Option<String>) -> Self {
        let Some(host) = host else {
            return Self::new();
        };
        match provider_for_host(&host) {
            Some(spec) => {
                // The host as the URL parser wrote it, which is what a pasted URL will carry.
                // A pasted URL may carry the host as it was typed (`127.1`) or
                // as the URL parser wrote it in the definition (`127.0.0.1`).
                // Both are recognised, whatever `with_spec` made of the address.
                let canonical = spec.allowed_hosts.first().cloned();
                let typed = host.trim().to_ascii_lowercase();
                let mut this = Self::with_spec(spec);
                for known in canonical.into_iter().chain([typed]) {
                    if !this.web_hosts.contains(&known) {
                        this.web_hosts.push(known);
                    }
                }
                this
            }
            None => {
                let mut this = Self::new();
                this.problem = Some(format!(
                    "{host:?} is not a host name; give the host alone, such as github.acme.example"
                ));
                this
            }
        }
    }

    /// Uses another definition, for GitHub Enterprise Server or a test server.
    pub fn with_spec(spec: ProviderSpec) -> Self {
        let web_hosts = web_hosts_of(&spec);
        Self {
            spec,
            access: Access::default(),
            problem: None,
            web_hosts,
        }
    }

    /// Sets the application's OAuth app.
    pub fn oauth(mut self, client: OAuthClient) -> Self {
        self.access.oauth = Some(client);
        self
    }

    /// Sets a token the application already holds.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.access.token = Some(TokenSet::bearer(token));
        self
    }

    fn request(path: String) -> RawRequest {
        RawRequest::get(path)
            .with_header("Accept", "application/vnd.github+json")
            .with_header("X-GitHub-Api-Version", "2022-11-28")
    }

    /// The account the connection is authorised as.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let body = connection.request(Self::request("user".into())).await?.body;
        // GitHub ids are numbers; the login is the stable name people recognise.
        let (Some(id), Some(login)) = (body["id"].as_u64(), body["login"].as_str().filter(|l| !l.is_empty())) else {
            return Err(
                Error::new(ErrorKind::Decode, "github answered without an account").with_provider(self.spec.id.clone())
            );
        };
        Ok(Account {
            id: id.to_string(),
            name: body["name"]
                .as_str()
                .filter(|n| !n.is_empty())
                .unwrap_or(login)
                .to_owned(),
            email: body["email"].as_str().map(str::to_owned),
        })
    }

    /// Confirms a repository exists and the account can reach it.
    ///
    /// Accepts `owner/repo`, a github.com URL, or an SSH remote. The result
    /// carries the name with GitHub's own casing.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        // A bare `owner/repo` reads the same for any host; a URL must be on one of this integration's.
        let first_host = self.web_hosts.first().map_or("github.com", String::as_str);
        let (owner, repo) = match self.web_hosts.iter().find_map(|host| parse_repo_on(input, host).ok()) {
            Some(parsed) => parsed,
            None => {
                return Err(parse_repo_on(input, first_host)
                    .err()
                    .unwrap_or_else(|| Error::new(ErrorKind::InvalidInput, "that is not a GitHub repository"))
                    .with_provider(self.spec.id.clone()));
            }
        };
        let response = connection
            .request(Self::request(format!("repos/{owner}/{repo}")))
            .await
            .map_err(|e| {
                if e.kind() == ErrorKind::NotFound {
                    // A private repository the account cannot see also answers 404.
                    Error::new(
                        ErrorKind::NotFound,
                        format!("GitHub repository {owner}/{repo} was not found"),
                    )
                    .with_provider(self.spec.id.clone())
                } else {
                    e
                }
            })?;
        // GitHub may answer with different casing, or a new name after a rename,
        // so the name is not compared with the input; it must still be one repository's name.
        let is_full_name = |name: &&str| name.split('/').count() == 2 && name.split('/').all(is_name);
        let full_name = response.body["full_name"]
            .as_str()
            .filter(is_full_name)
            .ok_or_else(|| {
                Error::new(ErrorKind::Decode, "github answered without a repository")
                    .with_provider(self.spec.id.clone())
            })?;
        Ok(Resource::new(full_name, full_name, "GitHub repository"))
    }
}

/// The hosts repository URLs are pasted from, for a definition.
///
/// github.com for GitHub's own API, and for a loopback address, which is a
/// test server standing in for it. Otherwise the definition's own host, which
/// is how a GitHub Enterprise Server given through `with_spec` is recognised.
fn web_hosts_of(spec: &ProviderSpec) -> Vec<String> {
    let host = spec.api_base.host_str().unwrap_or_default().to_ascii_lowercase();
    let stands_for_github = matches!(host.as_str(), "api.github.com" | "localhost" | "127.0.0.1" | "[::1]");
    if stands_for_github {
        vec!["github.com".to_owned()]
    } else {
        vec![host]
    }
}

/// Reads `owner/repo`, a github.com URL, or an SSH remote.
pub fn parse_repo(input: &str) -> Result<(String, String)> {
    parse_repo_on(input, "github.com")
}

/// Reads `owner/repo`, or a URL or SSH remote on `host`: `github.com`, or the
/// host of a GitHub Enterprise Server.
pub fn parse_repo_on(input: &str, host: &str) -> Result<(String, String)> {
    let trimmed = input.trim().trim_end_matches('/');
    // Host names are not case sensitive, so the prefix is matched without regard to case.
    let prefixes = [
        format!("https://{host}/"),
        format!("http://{host}/"),
        format!("git@{host}:"),
    ];
    let (path, is_url) = prefixes
        .iter()
        .find_map(|prefix| {
            let head = trimmed.get(..prefix.len())?;
            head.eq_ignore_ascii_case(prefix).then(|| &trimmed[prefix.len()..])
        })
        .map_or((trimmed, false), |rest| (rest, true));

    // A pasted URL often carries `?tab=…` or `#readme`.
    let path = if is_url {
        path.split(['?', '#']).next().unwrap_or_default().trim_end_matches('/')
    } else {
        path
    };
    let segments: Vec<&str> = path.split('/').collect();
    let well_formed = if is_url {
        segments.len() >= 2
    } else {
        segments.len() == 2
    };
    let invalid = || {
        Error::new(
            ErrorKind::InvalidInput,
            format!("\"{}\" is not a GitHub repository; use owner/repo", input.trim()),
        )
    };
    if !well_formed {
        return Err(invalid());
    }
    let owner = segments[0];
    let repo = segments[1].strip_suffix(".git").unwrap_or(segments[1]);
    if !is_name(owner) || !is_name(repo) {
        return Err(invalid());
    }
    Ok((owner.to_owned(), repo.to_owned()))
}

/// `.` and `..` are refused: as path segments they would climb out of
/// `/repos/` and reach a different GitHub endpoint.
fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars().any(|c| c != '.')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[async_trait]
impl Integration for GitHub {
    fn provider(&self) -> ProviderSpec {
        self.spec.clone()
    }

    fn check(&self) -> Result<()> {
        match &self.problem {
            Some(problem) => Err(Error::new(ErrorKind::Config, problem.clone()).with_provider(self.spec.id.clone())),
            None => Ok(()),
        }
    }

    fn oauth_client(&self) -> Option<OAuthClient> {
        self.access.oauth.clone()
    }

    fn fixed_token(&self) -> Option<TokenSet> {
        self.access.token.clone()
    }

    fn operations(&self) -> Vec<OperationInfo> {
        vec![
            identity_operation(&self.spec.id),
            resolve_operation(
                &self.spec.id,
                "a repository as owner/repo, a github.com URL, or an SSH remote",
            ),
        ]
    }

    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value> {
        let id = &self.spec.id;
        match operation.strip_prefix(&format!("{id}.")) {
            Some("identity.get") => to_output(id, &self.identity(&connection).await?),
            Some("resource.resolve") => to_output(id, &self.resolve(&connection, &resolve_input(id, &input)?).await?),
            _ => Err(
                Error::new(ErrorKind::Unsupported, format!("github has no operation {operation:?}"))
                    .with_provider(id.clone()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_repo_accepts_the_forms_people_paste() {
        let expected = ("acme".to_owned(), "front.end".to_owned());
        for input in [
            "acme/front.end",
            "  acme/front.end/ ",
            "https://github.com/acme/front.end",
            "https://github.com/acme/front.end.git",
            "https://github.com/acme/front.end/issues/12",
            "git@github.com:acme/front.end.git",
            "https://github.com/acme/front.end?tab=readme-ov-file",
            "https://github.com/acme/front.end/#readme",
            "https://github.com/acme/front.end.git?ref=main",
        ] {
            assert_eq!(parse_repo(input).unwrap(), expected, "{input}");
        }
    }

    #[test]
    fn an_enterprise_host_accepts_its_own_urls_and_not_github_coms() {
        let expected = ("acme".to_owned(), "api".to_owned());
        for input in [
            "acme/api",
            "https://github.acme.example/acme/api",
            "https://GitHub.Acme.Example/acme/api/pulls/3",
            "git@github.acme.example:acme/api.git",
        ] {
            assert_eq!(
                parse_repo_on(input, "github.acme.example").unwrap(),
                expected,
                "{input}"
            );
        }
        for input in [
            "https://github.com/acme/api",
            "https://github.acme.example.evil.test/acme/api",
        ] {
            let err = parse_repo_on(input, "github.acme.example").unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{input}");
        }
    }

    #[test]
    fn an_enterprise_definition_names_the_host_requests_really_go_to() {
        let spec = provider_for_host("GitHub.Acme.Example").unwrap();
        assert_eq!(spec.allowed_hosts, ["github.acme.example"]);
        spec.validate().unwrap();
        // A numeric host is rewritten by the URL parser; the allowlist follows it, so the definition is valid.
        let numeric = provider_for_host("127.1").unwrap();
        assert_eq!(numeric.allowed_hosts, ["127.0.0.1"]);
        numeric.validate().unwrap();
        let too_long_label = format!("{}.example", "a".repeat(64));
        let too_long = format!("{}.example", ["a".repeat(60).as_str(); 5].join("."));
        for bad in [
            too_long_label.as_str(),
            too_long.as_str(),
            "",
            "a..b",
            "-a.example",
            "a/b",
            "a:8443",
        ] {
            assert!(provider_for_host(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn the_hosts_urls_are_pasted_from_follow_the_definition_and_what_was_typed() {
        assert_eq!(GitHub::new().web_hosts, ["github.com"]);
        let given = GitHub::with_spec(provider_for_host("github.acme.example").unwrap());
        assert_eq!(given.web_hosts, ["github.acme.example"]);
        // A numeric shorthand is rewritten in the definition; both spellings are recognised in a pasted URL.
        let numeric = GitHub::with_token(GitHubToken {
            token: SecretString::new("t"),
            host: Some("127.1".into()),
        });
        // The integration tries each known host, so a URL in either spelling is read.
        let read = |input: &str| numeric.web_hosts.iter().any(|host| parse_repo_on(input, host).is_ok());
        assert!(read("https://127.1/acme/api") && read("https://127.0.0.1/acme/api"));
        assert!(
            !read("https://127.0.0.2/acme/api"),
            "a host that is neither spelling is refused"
        );
        for spelling in ["127.1", "127.0.0.1"] {
            assert!(
                numeric.web_hosts.contains(&spelling.to_owned()),
                "{spelling}: {:?}",
                numeric.web_hosts
            );
        }
    }

    #[test]
    fn parse_repo_refuses_anything_that_is_not_one_repository() {
        for input in [
            "",
            "acme",
            "acme/",
            "/repo",
            "acme/repo/extra",
            "acme/re po",
            "a/b?x=1",
            "../user",
            "acme/..",
            "./.",
            "https://github.com/../user",
        ] {
            assert_eq!(
                parse_repo(input).unwrap_err().kind(),
                ErrorKind::InvalidInput,
                "{input:?}"
            );
        }
    }
}
