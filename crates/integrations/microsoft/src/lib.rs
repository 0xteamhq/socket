//! Socket integration for Microsoft Graph.
//!
//! One provider covers Outlook, Teams, OneDrive, SharePoint and Entra ID,
//! because they share one sign-in. Offers the provider definition,
//! `microsoft.identity.get` and `microsoft.resource.resolve` (a OneDrive or
//! SharePoint sharing link). See `docs/integrations/microsoft.md`.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;
use socketkit_core::{
    Access, Account, AuthScheme, AuthorizationRequest, Classifier, ClientAuth, CodeGrant, Connection, Error, ErrorKind,
    Grant, Integration, OAuth2Spec, OAuthClient, OAuthContext, OAuthFlow, OperationInfo, ProviderId, ProviderSpec,
    RawRequest, RawResponse, Resource, Result, Retry, SecretString, StandardClassifier, StandardOAuth, TokenSet,
    identity_operation, resolve_input, resolve_operation, to_output,
};
use url::Url;

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "microsoft";

/// The tenant that lets both work or school accounts and personal accounts sign in.
const DEFAULT_TENANT: &str = "common";

/// The scope without which Microsoft issues no refresh token.
const OFFLINE_ACCESS: &str = "offline_access";

/// Microsoft's definition: where Graph lives and how it authenticates.
///
/// This is the worldwide service. The national clouds use other hosts and are
/// not covered.
///
/// # Panics
/// Never in practice: the URLs are constants that parse.
pub fn provider() -> ProviderSpec {
    let sign_in = |step: &str| {
        format!("https://login.microsoftonline.com/{DEFAULT_TENANT}/oauth2/v2.0/{step}")
            .parse()
            .expect("a valid URL")
    };
    ProviderSpec {
        id: ProviderId::new(PROVIDER_ID).expect("a valid provider id"),
        display_name: "Microsoft".into(),
        api_base: "https://graph.microsoft.com/v1.0/".parse().expect("a valid URL"),
        // login.microsoftonline.com serves the token endpoint.
        allowed_hosts: vec!["graph.microsoft.com".into(), "login.microsoftonline.com".into()],
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: sign_in("authorize"),
            token_url: sign_in("token"),
            default_scopes: vec![OFFLINE_ACCESS.into(), "User.Read".into()],
            scope_separator: " ".into(),
            pkce: true,
            client_auth: ClientAuth::Body,
            extra_authorize_params: Vec::new(),
        }),
    }
}

/// Graph follows HTTP conventions, with two additions: a token it calls
/// invalid is one to renew whatever the status, and a busy service says when
/// to come back.
#[derive(Debug, Clone, Copy, Default)]
pub struct MicrosoftClassifier;

impl Classifier for MicrosoftClassifier {
    fn classify(&self, provider: &ProviderId, response: &RawResponse) -> Result<()> {
        let error = |kind, message: String| Error::new(kind, message).with_provider(provider.clone());
        let failed = !(200..300).contains(&response.status);
        let code = response.body["error"]["code"].as_str().unwrap_or_default();
        if failed && code.eq_ignore_ascii_case("InvalidAuthenticationToken") {
            return Err(error(
                ErrorKind::ReconnectRequired,
                format!("{provider} rejected the stored authorization"),
            ));
        }
        // Graph states the wait in seconds, on a 503 as on a 429.
        let wait = response
            .header("retry-after")
            .and_then(|value| value.trim().parse().ok())
            .map(Duration::from_secs);
        if let (503, Some(wait)) = (response.status, wait) {
            return Err(
                error(ErrorKind::Unexpected, format!("{provider} returned HTTP 503")).with_retry(Retry::After(wait))
            );
        }
        StandardClassifier.classify(provider, response)
    }
}

/// Reads a OneDrive or SharePoint sharing link and returns it as Graph
/// addresses it: `u!`, then the whole link in base64url without padding.
pub fn parse_sharing_link(input: &str) -> Result<String> {
    let trimmed = input.trim();
    let is_link = Url::parse(trimmed).is_ok_and(|url| {
        url.scheme() == "https" && url.host_str().is_some() && url.username().is_empty() && url.password().is_none()
    });
    if is_link {
        Ok(format!("u!{}", URL_SAFE_NO_PAD.encode(trimmed)))
    } else {
        Err(Error::new(
            ErrorKind::InvalidInput,
            format!("\"{trimmed}\" is not a OneDrive or SharePoint sharing link; paste the whole link"),
        ))
    }
}

fn describe(drive_type: &str, is_folder: bool) -> String {
    let place = match drive_type {
        "documentLibrary" => "SharePoint",
        "personal" | "business" => "OneDrive",
        _ => "OneDrive or SharePoint",
    };
    format!("{place} {}", if is_folder { "folder" } else { "file" })
}

/// A tenant is `common`, `organizations`, `consumers`, a tenant id or a
/// domain. It becomes a path segment of the sign-in address, so anything else
/// is refused.
fn is_tenant(tenant: &str) -> bool {
    let is_label = |label: &str| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    tenant.len() <= 253 && tenant.split('.').all(is_label)
}

/// What the sign-in page asks of the person (Microsoft's `prompt` parameter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    /// Enter the credentials again, even when already signed in.
    Login,
    /// Show nothing; fail when the person would have to act.
    None,
    /// Show the permissions again, even when already granted.
    Consent,
    /// Choose among the signed-in accounts.
    SelectAccount,
}

impl Prompt {
    fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::None => "none",
            Self::Consent => "consent",
            Self::SelectAccount => "select_account",
        }
    }
}

/// OAuth settings for Microsoft. A plain [`OAuthClient`] converts into this with
/// the defaults, so `Microsoft::with_oauth(client)` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct MicrosoftOAuth {
    /// The application's own app registration.
    pub client: OAuthClient,
    /// Graph permissions to ask for in place of the defaults, such as
    /// `Calendars.Read`. `offline_access` is always asked for as well: without
    /// it Microsoft issues no refresh token.
    pub scopes: Option<Vec<String>>,
    /// Who may sign in: `common` (the default) for any account,
    /// `organizations` for work or school accounts, `consumers` for personal
    /// accounts, or a tenant id or domain for one organisation.
    pub tenant: Option<String>,
    /// Prefills the sign-in page (Microsoft's `login_hint` parameter).
    pub login_hint: Option<String>,
    /// What the sign-in page asks of the person.
    pub prompt: Option<Prompt>,
}

impl From<OAuthClient> for MicrosoftOAuth {
    fn from(client: OAuthClient) -> Self {
        Self {
            client,
            scopes: None,
            tenant: None,
            login_hint: None,
            prompt: None,
        }
    }
}

/// A Microsoft Graph access token. A plain string converts into this, so
/// `Microsoft::with_token("…")` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct MicrosoftToken {
    pub token: SecretString,
}

impl From<String> for MicrosoftToken {
    fn from(token: String) -> Self {
        Self {
            token: SecretString::new(token),
        }
    }
}

impl From<&str> for MicrosoftToken {
    fn from(token: &str) -> Self {
        token.to_owned().into()
    }
}

/// The Microsoft integration.
#[derive(Debug, Clone)]
pub struct Microsoft {
    spec: ProviderSpec,
    access: Access,
    /// What is wrong with the settings it was created with, if anything.
    problem: Option<String>,
}

impl Default for Microsoft {
    fn default() -> Self {
        Self::new()
    }
}

impl Microsoft {
    /// Microsoft with no connection details of its own: the OAuth app is set on the
    /// `Socket` builder and tokens come from the application's token store.
    pub fn new() -> Self {
        Self::with_spec(provider())
    }

    /// Microsoft with the application's app registration, for connecting users through OAuth.
    /// Takes an [`OAuthClient`], or a [`MicrosoftOAuth`] for the settings only Microsoft has.
    pub fn with_oauth(settings: impl Into<MicrosoftOAuth>) -> Self {
        let settings = settings.into();
        let mut this = Self::new();
        if let AuthScheme::OAuth2(oauth) = &mut this.spec.auth {
            if let Some(scopes) = settings.scopes {
                oauth.default_scopes = scopes;
            }
            if let Some(hint) = settings.login_hint {
                oauth.extra_authorize_params.push(("login_hint".into(), hint));
            }
            if let Some(prompt) = settings.prompt {
                oauth
                    .extra_authorize_params
                    .push(("prompt".into(), prompt.as_str().into()));
            }
        }
        let this = match settings.tenant {
            Some(tenant) => this.tenant(tenant),
            None => this,
        };
        this.oauth(settings.client)
    }

    /// Chooses who may sign in: `common`, `organizations`, `consumers`, or a
    /// tenant id or domain for one organisation.
    ///
    /// The tenant is part of the address of the sign-in page and of the token
    /// endpoint. A value that is none of those is reported when the `Socket`
    /// is built, and never reaches either address.
    pub fn tenant(mut self, tenant: impl Into<String>) -> Self {
        let tenant = tenant.into().trim().to_ascii_lowercase();
        // Decided afresh on every call, so correcting a bad value clears the problem.
        self.problem = (!is_tenant(&tenant)).then(|| {
            format!(
                "{tenant:?} is not a Microsoft tenant; use common, organizations, consumers, or a tenant id or domain"
            )
        });
        if self.problem.is_none() {
            if let AuthScheme::OAuth2(oauth) = &mut self.spec.auth {
                oauth
                    .authorize_url
                    .set_path(&format!("/{tenant}/oauth2/v2.0/authorize"));
                oauth.token_url.set_path(&format!("/{tenant}/oauth2/v2.0/token"));
            }
        }
        self
    }

    /// Microsoft with a token the application already holds. Every call uses it.
    /// Takes a string, or a [`MicrosoftToken`] for the settings only Microsoft has.
    pub fn with_token(settings: impl Into<MicrosoftToken>) -> Self {
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
            problem: None,
        }
    }

    /// Sets the application's app registration.
    pub fn oauth(mut self, client: OAuthClient) -> Self {
        self.access.oauth = Some(client);
        self
    }

    /// Sets a token the application already holds.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.access.token = Some(TokenSet::bearer(token));
        self
    }

    fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.spec.id.clone())
    }

    /// The account the connection is authorised as. Needs the `User.Read` permission.
    ///
    /// The email is the mailbox address, or the sign-in name for an account
    /// without a mailbox.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let body = connection.request(RawRequest::get("me")).await?.body;
        let filled = |value: &Value| value.as_str().filter(|s| !s.trim().is_empty()).map(str::to_owned);
        let Some(id) = filled(&body["id"]) else {
            return Err(self.error(ErrorKind::Decode, "microsoft answered without an account"));
        };
        let email = filled(&body["mail"]).or_else(|| filled(&body["userPrincipalName"]));
        let name = filled(&body["displayName"])
            .or_else(|| email.clone())
            .unwrap_or_else(|| id.clone());
        Ok(Account { id, name, email })
    }

    /// Confirms a OneDrive or SharePoint sharing link leads to a file or
    /// folder the account can open.
    ///
    /// The resource's id is the Graph path that addresses the item,
    /// `drives/{drive id}/items/{item id}`, so it can be used in a later request.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        let share = parse_sharing_link(input).map_err(|e| e.with_provider(self.spec.id.clone()))?;
        let request = RawRequest::get(format!("shares/{share}/driveItem"));
        let body = match connection.request(request).await {
            Ok(response) => response.body,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Err(self.error(ErrorKind::NotFound, "that OneDrive or SharePoint link was not found"));
            }
            // Graph does not say whether the person or the connection lacks the access.
            Err(e) if e.kind() == ErrorKind::AccessDenied => {
                return Err(self.error(
                    ErrorKind::AccessDenied,
                    format!(
                        "{}; if this account can open the link in a browser, the connection is missing the Files.ReadWrite permission",
                        e.message().trim_end_matches('.')
                    ),
                ));
            }
            Err(e) => return Err(e),
        };
        let filled = |value: &Value| value.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
        let Some(item) = filled(&body["id"]) else {
            return Err(self.error(ErrorKind::Decode, "microsoft answered without a file or folder"));
        };
        let parent = &body["parentReference"];
        let id = match filled(&parent["driveId"]) {
            Some(drive) => format!("drives/{drive}/items/{item}"),
            // Without its drive an item id is ambiguous; the link still addresses it.
            None => format!("shares/{share}/driveItem"),
        };
        let name = filled(&body["name"]).unwrap_or_else(|| "Untitled".into());
        let description = describe(
            parent["driveType"].as_str().unwrap_or_default(),
            body["folder"].is_object(),
        );
        Ok(Resource::new(id, name, description))
    }

    /// Posts `form` to the token endpoint and reads the answer, telling a
    /// permission that still needs approval apart from a refused grant.
    async fn granted(
        &self,
        context: &OAuthContext,
        form: Vec<(String, String)>,
        grant: Grant,
    ) -> Result<(Value, SystemTime)> {
        let response = context.post_token(form).await?;
        // The lifetime Microsoft states is counted from when its answer arrived.
        let now = SystemTime::now();
        if let Some(refusal) = awaiting_consent(&context.provider().id, &response) {
            return Err(refusal);
        }
        Ok((context.granted(response, grant)?, now))
    }
}

/// The AADSTS codes that say a permission has not been approved: by the
/// person or an administrator (65001, 90008), or by an administrator alone
/// (90094, 90095).
const NOT_CONSENTED: [u64; 2] = [65001, 90008];
const ADMIN_CONSENT_REQUIRED: [u64; 2] = [90094, 90095];

/// The refusal for a token response that asks for consent, if it is one.
///
/// Microsoft reports it under several `error` values, `invalid_grant` among
/// them, so its numbered code is what tells it apart from a grant that has
/// expired. Reconnecting does not fix it, and for many Graph permissions only
/// an administrator can.
fn awaiting_consent(provider: &ProviderId, response: &RawResponse) -> Option<Error> {
    if (200..300).contains(&response.status) {
        return None;
    }
    let code = response.body["error_codes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .find(|code| NOT_CONSENTED.contains(code) || ADMIN_CONSENT_REQUIRED.contains(code));
    let who = match code {
        Some(code) if ADMIN_CONSENT_REQUIRED.contains(&code) => {
            "an administrator of the organisation has to approve it"
        }
        Some(_) => "the person has to accept it at sign-in, or an administrator of the organisation has to approve it",
        None if response.body["error"] == "consent_required" => {
            "the person has to accept it at sign-in, or an administrator of the organisation has to approve it"
        }
        None => return None,
    };
    // Only the code is repeated. Microsoft's own text carries request ids.
    let numbered = code.map(|code| format!(" (AADSTS{code})")).unwrap_or_default();
    Some(
        Error::new(
            ErrorKind::AccessDenied,
            format!("{provider} has not been given consent for a permission this connection asks for{numbered}; {who}"),
        )
        .with_provider(provider.clone()),
    )
}

#[async_trait]
impl Integration for Microsoft {
    fn provider(&self) -> ProviderSpec {
        self.spec.clone()
    }

    /// Microsoft's operations are named `microsoft.…`, so the definition must keep that id.
    fn check(&self) -> Result<()> {
        if let Some(problem) = &self.problem {
            return Err(self.error(ErrorKind::Config, problem.clone()));
        }
        if self.spec.id.as_str() == PROVIDER_ID {
            return Ok(());
        }
        Err(self.error(
            ErrorKind::Config,
            format!(
                "the Microsoft integration needs the provider id {PROVIDER_ID:?}, not {:?}",
                self.spec.id.as_str()
            ),
        ))
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
            resolve_operation(&self.spec.id, "a OneDrive or SharePoint sharing link"),
        ]
    }

    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value> {
        let id = &self.spec.id;
        match operation.strip_prefix(&format!("{id}.")) {
            Some("identity.get") => to_output(id, &self.identity(&connection).await?),
            Some("resource.resolve") => to_output(id, &self.resolve(&connection, &resolve_input(id, &input)?).await?),
            _ => Err(self.error(
                ErrorKind::Unsupported,
                format!("microsoft has no operation {operation:?}"),
            )),
        }
    }

    fn classifier(&self) -> Arc<dyn Classifier> {
        Arc::new(MicrosoftClassifier)
    }

    fn oauth_flow(&self) -> Arc<dyn OAuthFlow> {
        Arc::new(self.clone())
    }
}

/// Microsoft follows the standard flow. Two things are added: a refresh token
/// is always asked for, and a token response that asks for consent is
/// reported as that.
#[async_trait]
impl OAuthFlow for Microsoft {
    fn authorization_url(&self, context: OAuthContext, mut request: AuthorizationRequest) -> Result<Url> {
        if !request
            .scopes
            .iter()
            .any(|scope| scope.eq_ignore_ascii_case(OFFLINE_ACCESS))
        {
            request.scopes.push(OFFLINE_ACCESS.into());
        }
        StandardOAuth.authorization_url(context, request)
    }

    async fn exchange_code(&self, context: OAuthContext, grant: CodeGrant) -> Result<TokenSet> {
        let mut form = vec![
            ("grant_type".to_owned(), "authorization_code".to_owned()),
            ("code".to_owned(), grant.code),
            ("redirect_uri".to_owned(), context.client().redirect_uri.to_string()),
        ];
        if let Some(verifier) = &grant.pkce_verifier {
            form.push(("code_verifier".to_owned(), verifier.expose().to_owned()));
        }
        let (body, now) = self.granted(&context, form, Grant::Code).await?;
        self.parse_token_response(context.provider().id.clone(), body, now)
    }

    async fn refresh(&self, context: OAuthContext, current: TokenSet) -> Result<TokenSet> {
        let provider = context.provider().id.clone();
        let Some(refresh_token) = current.refresh_token.clone() else {
            return Err(Error::new(
                ErrorKind::ReconnectRequired,
                format!("the authorization for {provider} has expired"),
            )
            .with_provider(provider));
        };
        let form = vec![
            ("grant_type".to_owned(), "refresh_token".to_owned()),
            ("refresh_token".to_owned(), refresh_token.expose().to_owned()),
        ];
        let (body, now) = self.granted(&context, form, Grant::Refresh).await?;
        let mut fresh = self.parse_token_response(provider, body, now)?;
        // Microsoft sends a new refresh token each time; the stored one is
        // kept only if it ever does not.
        if fresh.refresh_token.is_none() {
            fresh.refresh_token = current.refresh_token;
        }
        if fresh.scopes.is_empty() {
            fresh.scopes = current.scopes;
        }
        Ok(fresh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sharing_link_is_encoded_as_graph_documents() {
        assert_eq!(
            parse_sharing_link(" https://contoso.sharepoint.com/:w:/s/team/EabcDEF?e=x1 ").unwrap(),
            "u!aHR0cHM6Ly9jb250b3NvLnNoYXJlcG9pbnQuY29tLzp3Oi9zL3RlYW0vRWFiY0RFRj9lPXgx"
        );
    }

    #[test]
    fn a_tenant_is_a_keyword_an_id_or_a_domain_and_nothing_else() {
        for tenant in [
            "common",
            "organizations",
            "consumers",
            "contoso.onmicrosoft.com",
            "8eaef023-2b34-4da1-9baa-8bc8c9d6a490",
        ] {
            assert!(is_tenant(tenant), "{tenant}");
        }
        for bad in [
            "", "a/b", "..", "a b", "a?b", "a#", "a@b", ".a", "a.", "-a", "a-", "a%2Fb",
        ] {
            assert!(!is_tenant(bad), "{bad:?}");
        }
        assert!(!is_tenant(&"a".repeat(254)));
    }

    #[test]
    fn an_item_is_described_in_words() {
        assert_eq!(describe("documentLibrary", false), "SharePoint file");
        assert_eq!(describe("business", true), "OneDrive folder");
        assert_eq!(describe("", false), "OneDrive or SharePoint file");
    }
}
