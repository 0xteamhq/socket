//! Socket integration for Google.
//!
//! One provider covers Google's products, because they share one OAuth
//! provider. Offers the provider definition, `google.identity.get` and
//! `google.resource.resolve` (a Drive file or folder, which includes Docs and Sheets).

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Access, Account, AuthScheme, ClientAuth, Connection, Error, ErrorKind, Integration, OAuth2Spec, OAuthClient,
    OperationInfo, ProviderId, ProviderSpec, RawRequest, Resource, Result, SecretString, TokenSet, identity_operation,
    resolve_input, resolve_operation, to_output,
};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "google";

/// Google's definition: where its APIs live and how they authenticate.
///
/// # Panics
/// Never in practice: the URLs are constants that parse.
pub fn provider() -> ProviderSpec {
    ProviderSpec {
        id: ProviderId::new(PROVIDER_ID).expect("a valid provider id"),
        display_name: "Google".into(),
        api_base: "https://www.googleapis.com/".parse().expect("a valid URL"),
        // oauth2.googleapis.com serves the token endpoint.
        allowed_hosts: vec![
            "www.googleapis.com".into(),
            "oauth2.googleapis.com".into(),
            // The Docs and Sheets APIs, which the default scopes cover, live on their own hosts.
            "docs.googleapis.com".into(),
            "sheets.googleapis.com".into(),
        ],
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: "https://accounts.google.com/o/oauth2/v2/auth"
                .parse()
                .expect("a valid URL"),
            token_url: "https://oauth2.googleapis.com/token".parse().expect("a valid URL"),
            default_scopes: vec![
                "https://www.googleapis.com/auth/drive.readonly".into(),
                "https://www.googleapis.com/auth/documents.readonly".into(),
            ],
            scope_separator: " ".into(),
            pkce: false,
            client_auth: ClientAuth::Body,
            // Without these Google issues no refresh token.
            extra_authorize_params: vec![
                ("access_type".into(), "offline".into()),
                ("prompt".into(), "consent".into()),
            ],
        }),
    }
}

/// Reads a Drive or Docs URL, or a bare file id.
pub fn parse_file_id(input: &str) -> Result<String> {
    let trimmed = input.trim();
    let candidate = ["/folders/", "/d/", "id="]
        .iter()
        .find_map(|marker| trimmed.split_once(marker).map(|(_, rest)| rest))
        .unwrap_or(trimmed);
    let id: String = candidate
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .collect();
    let is_whole_input = candidate.len() == trimmed.len();
    if id.len() >= 10 && (!is_whole_input || id.len() == trimmed.len()) {
        Ok(id)
    } else {
        Err(Error::new(
            ErrorKind::InvalidInput,
            format!("\"{trimmed}\" is not a Google Drive file or folder; paste its URL or id"),
        ))
    }
}

fn describe(mime_type: &str) -> &'static str {
    match mime_type {
        "application/vnd.google-apps.folder" => "Google Drive folder",
        "application/vnd.google-apps.document" => "Google Doc",
        "application/vnd.google-apps.spreadsheet" => "Google Sheet",
        "application/vnd.google-apps.presentation" => "Google Slides presentation",
        _ => "Google Drive file",
    }
}

/// OAuth settings for Google. A plain [`OAuthClient`] converts into this with
/// the defaults, so `Google::with_oauth(client)` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct GoogleOAuth {
    /// The application's own OAuth app.
    pub client: OAuthClient,
    /// Scopes to ask for in place of the defaults.
    pub scopes: Option<Vec<String>>,
    /// The Google Workspace domain accounts must belong to, such as `acme.example`.
    ///
    /// It does two things. It is sent as Google's `hd` parameter, which only
    /// filters the sign-in page: a person can still approve with another
    /// account. And `google.identity.get` refuses an account unless Google
    /// itself reports it as managed by that Workspace, which is the actual
    /// check. Connecting does not run that check by itself: call
    /// `google.identity.get` after `complete_authorization`, and delete the
    /// connection if it is refused.
    pub hosted_domain: Option<String>,
    /// Prefills the account chooser (Google's `login_hint` parameter).
    pub login_hint: Option<String>,
}

impl From<OAuthClient> for GoogleOAuth {
    fn from(client: OAuthClient) -> Self {
        Self {
            client,
            scopes: None,
            hosted_domain: None,
            login_hint: None,
        }
    }
}

/// A Google access token. A plain string converts into this, so
/// `Google::with_token("…")` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct GoogleToken {
    pub token: SecretString,
}

impl From<String> for GoogleToken {
    fn from(token: String) -> Self {
        Self {
            token: SecretString::new(token),
        }
    }
}

impl From<&str> for GoogleToken {
    fn from(token: &str) -> Self {
        token.to_owned().into()
    }
}

/// The Google integration.
#[derive(Debug, Clone)]
pub struct Google {
    spec: ProviderSpec,
    access: Access,
    /// The Workspace domain an account must belong to, lowercased.
    hosted_domain: Option<String>,
    /// What is wrong with the settings it was created with, if anything.
    problem: Option<String>,
}

impl Default for Google {
    fn default() -> Self {
        Self::new()
    }
}

impl Google {
    /// Google with no connection details of its own: the OAuth app is set on the
    /// `Socket` builder and tokens come from the application's token store.
    pub fn new() -> Self {
        Self::with_spec(provider())
    }

    /// Google with the application's OAuth app, for connecting users through OAuth.
    /// Takes an [`OAuthClient`], or a [`GoogleOAuth`] for the settings only Google has.
    pub fn with_oauth(settings: impl Into<GoogleOAuth>) -> Self {
        let settings = settings.into();
        let mut this = Self::new();
        if let AuthScheme::OAuth2(oauth) = &mut this.spec.auth {
            if let Some(scopes) = settings.scopes {
                oauth.default_scopes = scopes;
            }
            if let Some(hint) = settings.login_hint {
                oauth.extra_authorize_params.push(("login_hint".into(), hint));
            }
        }
        let this = match settings.hosted_domain {
            Some(domain) => this.hosted_domain(domain),
            None => this,
        };
        this.oauth(settings.client)
    }

    /// Requires accounts to belong to one Google Workspace domain.
    /// `google.identity.get` refuses any other account.
    ///
    /// Sends the domain as the sign-in page's `hd` hint, and adds the `openid`
    /// and `email` scopes, which reading Google's verified domain needs.
    pub fn hosted_domain(mut self, domain: impl Into<String>) -> Self {
        let domain = domain.into().trim().trim_start_matches('@').to_ascii_lowercase();
        // A domain is labels joined by dots. Anything else (blank, an email
        // address, a URL) could never match what Google reports, so it is a
        // mistake to be told about, not a restriction to apply silently.
        let is_label = |label: &str| {
            (1..=63).contains(&label.len()) && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        };
        let valid = domain.contains('.') && domain.split('.').all(is_label);
        if !valid {
            self.problem = Some(format!(
                "{domain:?} is not a Google Workspace domain; give the domain alone, such as acme.example"
            ));
        }
        // Kept even when invalid, so that identity refuses every account
        // instead of letting every account through.
        self.hosted_domain = Some(domain.clone());
        if let AuthScheme::OAuth2(oauth) = &mut self.spec.auth {
            // One `hd` hint for the sign-in page, replacing any set earlier.
            oauth.extra_authorize_params.retain(|(name, _)| name != "hd");
            if valid {
                oauth.extra_authorize_params.push(("hd".into(), domain));
            }
            for scope in ["openid", "email"] {
                if !oauth.default_scopes.iter().any(|s| s == scope) {
                    oauth.default_scopes.push(scope.to_owned());
                }
            }
        }
        self
    }

    /// Checks the account against Google's own statement of its Workspace
    /// domain, the `hd` claim.
    ///
    /// The email address is not evidence: a personal Google account can be
    /// registered with a work address. Google reports `hd` only for an account
    /// that is managed by that Workspace. Any failure to read it refuses.
    async fn require_workspace(&self, connection: &Connection, required: &str) -> Result<()> {
        let body = connection.request(RawRequest::get("oauth2/v3/userinfo")).await?.body;
        let verified = body["hd"].as_str().map(str::to_ascii_lowercase);
        if verified.as_deref() == Some(required) {
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::AccessDenied,
                format!("this Google account is not in the {required} workspace"),
            )
            .with_provider(self.spec.id.clone()))
        }
    }

    /// Google with a token the application already holds. Every call uses it.
    /// Takes a string, or a [`GoogleToken`] for the settings only Google has.
    pub fn with_token(settings: impl Into<GoogleToken>) -> Self {
        let settings = settings.into();
        let mut this = Self::new();
        this.access.token = Some(TokenSet {
            access_token: settings.token,
            refresh_token: None,
            expires_at: None,
            scopes: Vec::new(),
        });
        this
    }

    /// Uses another definition, for a test server.
    pub fn with_spec(spec: ProviderSpec) -> Self {
        Self {
            spec,
            access: Access::default(),
            hosted_domain: None,
            problem: None,
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

    /// The account the connection is authorised as.
    ///
    /// Read from Drive's `about` resource, which the Drive scopes cover. When a
    /// Workspace domain is required, the account's verified domain is also
    /// read from the userinfo endpoint, and any other account is refused.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let request = RawRequest::get("drive/v3/about").with_query("fields", "user");
        let body = connection.request(request).await?.body;
        let user = &body["user"];
        let email = user["emailAddress"].as_str().filter(|e| !e.is_empty());
        let Some(id) = user["permissionId"].as_str().filter(|id| !id.is_empty()).or(email) else {
            return Err(
                Error::new(ErrorKind::Decode, "google answered without an account").with_provider(self.spec.id.clone())
            );
        };
        if let Some(required) = &self.hosted_domain {
            self.require_workspace(connection, required).await?;
        }
        let name = user["displayName"]
            .as_str()
            .filter(|n| !n.is_empty())
            .or(email)
            .unwrap_or(id);
        Ok(Account {
            id: id.to_owned(),
            name: name.to_owned(),
            email: email.map(str::to_owned),
        })
    }

    /// Confirms a Drive file or folder exists and the account can open it.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        let id = parse_file_id(input).map_err(|e| e.with_provider(self.spec.id.clone()))?;
        let request = RawRequest::get(format!("drive/v3/files/{id}"))
            .with_query("fields", "id,name,mimeType")
            .with_query("supportsAllDrives", "true");
        let body = match connection.request(request).await {
            Ok(response) => response.body,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Err(
                    Error::new(ErrorKind::NotFound, format!("Google Drive item {id} was not found"))
                        .with_provider(self.spec.id.clone()),
                );
            }
            Err(e) => return Err(e),
        };
        if body["id"] != id.as_str() {
            return Err(
                Error::new(ErrorKind::Decode, "google answered without a file").with_provider(self.spec.id.clone())
            );
        }
        let name = body["name"].as_str().unwrap_or("Untitled");
        Ok(Resource::new(
            id,
            name,
            describe(body["mimeType"].as_str().unwrap_or_default()),
        ))
    }
}

#[async_trait]
impl Integration for Google {
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
            resolve_operation(&self.spec.id, "a Google Drive, Docs or Sheets URL, or a file id"),
        ]
    }

    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value> {
        let id = &self.spec.id;
        match operation.strip_prefix(&format!("{id}.")) {
            Some("identity.get") => to_output(id, &self.identity(&connection).await?),
            Some("resource.resolve") => to_output(id, &self.resolve(&connection, &resolve_input(id, &input)?).await?),
            _ => Err(
                Error::new(ErrorKind::Unsupported, format!("google has no operation {operation:?}"))
                    .with_provider(id.clone()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_file_id_accepts_urls_and_bare_ids() {
        let id = "1AbC_dEf-GhIjKlMnOpQrStUvWxYz012345";
        for input in [
            id.to_owned(),
            format!("  {id} "),
            format!("https://docs.google.com/document/d/{id}/edit"),
            format!("https://docs.google.com/spreadsheets/d/{id}/edit#gid=0"),
            format!("https://drive.google.com/drive/folders/{id}?usp=sharing"),
            format!("https://drive.google.com/open?id={id}"),
            format!("https://drive.google.com/file/d/{id}/view"),
        ] {
            assert_eq!(parse_file_id(&input).unwrap(), id, "{input}");
        }
    }

    #[test]
    fn parse_file_id_refuses_what_is_not_one_id() {
        for input in [
            "",
            "short",
            "two words that are long enough",
            "https://example.test/nothing",
            "../../etc/passwd",
        ] {
            assert_eq!(
                parse_file_id(input).unwrap_err().kind(),
                ErrorKind::InvalidInput,
                "{input:?}"
            );
        }
    }

    #[test]
    fn mime_types_are_described_in_words() {
        assert_eq!(describe("application/vnd.google-apps.folder"), "Google Drive folder");
        assert_eq!(describe("application/vnd.google-apps.document"), "Google Doc");
        assert_eq!(describe("application/vnd.google-apps.spreadsheet"), "Google Sheet");
        assert_eq!(describe("application/pdf"), "Google Drive file");
        assert_eq!(describe(""), "Google Drive file");
    }
}
