//! Socket integration for GitHub.
//!
//! Offers the provider definition, `github.identity.get` and
//! `github.resource.resolve` (a repository). Typed operations arrive in phase 2.

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Account, AuthScheme, ClientAuth, Connection, Error, ErrorKind, Integration, OAuth2Spec, OperationInfo, ProviderId,
    ProviderSpec, RawRequest, Resource, Result, identity_operation, resolve_input, resolve_operation, to_output,
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

/// The GitHub integration.
#[derive(Debug, Clone)]
pub struct GitHub {
    spec: ProviderSpec,
}

impl Default for GitHub {
    fn default() -> Self {
        Self::new()
    }
}

impl GitHub {
    pub fn new() -> Self {
        Self { spec: provider() }
    }

    /// Uses another definition, for GitHub Enterprise Server or a test server.
    pub fn with_spec(spec: ProviderSpec) -> Self {
        Self { spec }
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
        let (Some(id), Some(login)) = (body["id"].as_u64(), body["login"].as_str()) else {
            return Err(
                Error::new(ErrorKind::Decode, "github answered without an account").with_provider(self.spec.id.clone())
            );
        };
        Ok(Account {
            id: id.to_string(),
            name: body["name"].as_str().unwrap_or(login).to_owned(),
            email: body["email"].as_str().map(str::to_owned),
        })
    }

    /// Confirms a repository exists and the account can reach it.
    ///
    /// Accepts `owner/repo`, a github.com URL, or an SSH remote. The result
    /// carries the name with GitHub's own casing.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        let (owner, repo) = parse_repo(input).map_err(|e| e.with_provider(self.spec.id.clone()))?;
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
        let full_name = response.body["full_name"].as_str().ok_or_else(|| {
            Error::new(ErrorKind::Decode, "github answered without a repository").with_provider(self.spec.id.clone())
        })?;
        Ok(Resource::new(full_name, full_name, "GitHub repository"))
    }
}

/// Reads `owner/repo`, a github.com URL, or an SSH remote.
pub fn parse_repo(input: &str) -> Result<(String, String)> {
    let trimmed = input.trim().trim_end_matches('/');
    let (path, is_url) = ["https://github.com/", "http://github.com/", "git@github.com:"]
        .iter()
        .find_map(|prefix| trimmed.strip_prefix(prefix))
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
