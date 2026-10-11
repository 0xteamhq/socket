//! Socket integration for Pipedrive.
//!
//! Offers the provider definition in its two forms (OAuth, and a personal
//! API token), identity, lookup of a record by its link, and typed methods
//! grouped as Pipedrive groups them: `deals`, `persons`, `organizations`,
//! `leads`, `activities`, `notes`, `pipelines`, `fields`, `users` and
//! `search`. Every typed method is also a named operation. See
//! `docs/integrations/pipedrive.md`.

mod client;
pub mod models;
mod operations;

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Access, Account, ApiKeySpec, AuthScheme, AuthorizationRequest, Classifier, ClientAuth, Connection, Error,
    ErrorKind, Integration, KeyPlacement, OAuth2Spec, OAuthClient, OAuthContext, OAuthFlow, OperationInfo, ProviderId,
    ProviderSpec, RawResponse, Resource, Result, Retry, SecretString, StandardClassifier, StandardOAuth, TokenSet,
    identity_operation, provider_message, resolve_input, resolve_operation, standard_token_response, to_output,
};
use url::Url;

pub use client::{Activities, Deals, Fields, Leads, Notes, Organizations, Persons, Pipelines, Search, Users};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "pipedrive";

/// The host that serves every company, and the one sign-in goes through.
const API_HOST: &str = "api.pipedrive.com";
const SIGN_IN_HOST: &str = "oauth.pipedrive.com";
/// Each company's own host is under this domain.
const COMPANY_DOMAIN: &str = "pipedrive.com";
/// The header a personal API token travels in.
const API_TOKEN_HEADER: &str = "x-api-token";

/// Pipedrive's definition for connecting users through OAuth.
///
/// Sign-in is at one host for everyone. The API is at each company's own,
/// which the token response names (`api_domain`); `*.pipedrive.com` lets a
/// connection call the host its own authorisation named, and no other
/// company's. `api_base` is where a connection without one calls.
///
/// No scopes are listed. Pipedrive takes an application's scopes from its
/// settings in Developer Hub and not from the sign-in address; `base`, which
/// identifies the account, is always granted.
///
/// # Panics
/// Never in practice: the URLs are constants that parse.
pub fn provider() -> ProviderSpec {
    let sign_in = |step: &str| {
        format!("https://{SIGN_IN_HOST}/oauth/{step}")
            .parse()
            .expect("a valid URL")
    };
    ProviderSpec {
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: sign_in("authorize"),
            token_url: sign_in("token"),
            default_scopes: Vec::new(),
            scope_separator: " ".into(),
            pkce: false,
            client_auth: ClientAuth::Basic,
            extra_authorize_params: Vec::new(),
        }),
        allowed_hosts: vec![API_HOST.into(), SIGN_IN_HOST.into(), format!("*.{COMPANY_DOMAIN}")],
        ..api_token_provider()
    }
}

/// Pipedrive's definition for a personal API token.
///
/// A definition has one way of authenticating, and Pipedrive has two that
/// are sent differently: an OAuth token as a bearer, a personal API token
/// bare in the `x-api-token` header. So the two forms are two definitions
/// with the same id, and a `Socket` holds one of them. This one has no
/// sign-in: the token is given to [`Pipedrive::with_token`], or saved in the
/// token store for each tenant.
///
/// # Panics
/// Never in practice: the URL is a constant that parses.
pub fn api_token_provider() -> ProviderSpec {
    ProviderSpec {
        id: ProviderId::new(PROVIDER_ID).expect("a valid provider id"),
        display_name: "Pipedrive".into(),
        // Both versions of the API are below this: `v1/…` and `v2/…`.
        api_base: format!("https://{API_HOST}/api/").parse().expect("a valid URL"),
        allowed_hosts: vec![API_HOST.into(), format!("*.{COMPANY_DOMAIN}")],
        content_hosts: Vec::new(),
        auth: AuthScheme::ApiKey(ApiKeySpec {
            placement: KeyPlacement::Header {
                name: API_TOKEN_HEADER.into(),
                prefix: None,
            },
        }),
    }
}

/// Pipedrive follows HTTP conventions, with its own body: `success`, and
/// when that is false `error`, sometimes `error_info` and a `code`.
///
/// What differs from the standard reading: an answer that says it failed is
/// a failure whatever its status; a throttle may state its wait in
/// `x-ratelimit-reset`; and a 403 is always a refusal, because Pipedrive
/// sends its rate-limit headers on every answer and an exhausted one beside
/// a 403 does not make it a throttle.
#[derive(Debug, Clone, Copy, Default)]
pub struct PipedriveClassifier;

/// The longest wait read from `x-ratelimit-reset`: the daily budget is new
/// every day, so nothing Pipedrive means by it is longer.
const LONGEST_RESET: u64 = 24 * 60 * 60;

impl Classifier for PipedriveClassifier {
    fn classify(&self, provider: &ProviderId, response: &RawResponse) -> Result<()> {
        let error = |kind, message: String| Error::new(kind, message).with_provider(provider.clone());
        let body = &response.body;
        let succeeded = (200..300).contains(&response.status);
        if succeeded && body["success"] != false {
            return Ok(());
        }
        // A failure written in a success is read by the status its body
        // names, when it names one.
        let status = if succeeded {
            let named = body["errorCode"].as_u64().filter(|code| (400..600).contains(code));
            match named.and_then(|code| u16::try_from(code).ok()) {
                Some(status) => status,
                None => {
                    return Err(error(
                        ErrorKind::Unexpected,
                        format!(
                            "{provider} answered that the request failed: {}",
                            provider_message(body)
                        ),
                    ));
                }
            }
        } else {
            response.status
        };
        match status {
            // Pipedrive documents no `Retry-After`. When it sends none, the
            // time left in its window is the wait.
            429 if response.header("retry-after").is_none() => {
                let reset = response
                    .header("x-ratelimit-reset")
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .filter(|seconds| *seconds <= LONGEST_RESET);
                let retry = reset.map_or(Retry::Later, |seconds| Retry::After(Duration::from_secs(seconds)));
                Err(error(ErrorKind::RateLimited, format!("{provider} is rate limiting requests")).with_retry(retry))
            }
            402 => Err(error(
                ErrorKind::AccessDenied,
                format!("the company's {provider} account is not open: its trial has ended or payment is missing"),
            )),
            403 => Err(error(ErrorKind::AccessDenied, refusal(provider, body))),
            410 => Err(error(
                ErrorKind::Unexpected,
                format!("{provider} has retired this part of its API (HTTP 410)"),
            )),
            _ => StandardClassifier.classify(
                provider,
                &RawResponse {
                    status,
                    headers: response.headers.clone(),
                    body: body.clone(),
                },
            ),
        }
    }
}

/// What a 403 means, in words a caller can act on.
fn refusal(provider: &ProviderId, body: &Value) -> String {
    // After repeated throttling Pipedrive's front door answers with a page
    // that is not JSON, which arrives here as no body at all.
    if body.is_null() {
        return format!(
            "{provider} denied the request without a reason; it blocks a client this way for a while after repeated rate limiting"
        );
    }
    let reason = provider_message(body);
    // A limit of the company's plan carries a code: `feature_capping_deals_limit`.
    let code = body["code"].as_str().unwrap_or_default();
    let is_code = (1..=60).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
    if is_code && code.starts_with("feature_capping") {
        return format!("{provider} denied the request: the company's plan has reached a limit ({code}). {reason}");
    }
    if reason.to_ascii_lowercase().contains("scope") {
        return format!(
            "{provider} denied the request: the connection lacks a scope this operation needs ({reason}); add the scope to the app in Developer Hub and connect again"
        );
    }
    format!("{provider} denied the request: {reason}")
}

/// The API address of the company a token response names in `api_domain`.
///
/// Pipedrive names the company's host, `https://acme.pipedrive.com`, and
/// both versions of its API are below `/api/` there. Anything that is not a
/// bare host is refused: a path, a query or a username would be something
/// this crate does not know how to call. Whether the host is one of
/// Pipedrive's is for the core to decide, against the definition.
fn company_api_base(api_domain: &str) -> Option<Url> {
    let mut base = Url::parse(api_domain.trim()).ok()?;
    let bare = matches!(base.scheme(), "https" | "http")
        && base.host_str().is_some()
        && base.username().is_empty()
        && base.password().is_none()
        && matches!(base.path(), "" | "/")
        && base.query().is_none()
        && base.fragment().is_none();
    if !bare {
        return None;
    }
    base.set_path("/api/");
    Some(base)
}

/// The API address of a company named by the first part of its Pipedrive
/// address: `acme`, or `acme.pipedrive.com` as it is often copied.
///
/// The name becomes part of the host the token is sent to, so it has to be
/// one name of a host and nothing else.
fn named_company_api_base(company: &str) -> Option<Url> {
    let company = company.trim().to_ascii_lowercase();
    let name = company.strip_suffix(&format!(".{COMPANY_DOMAIN}")).unwrap_or(&company);
    let is_label = (1..=63).contains(&name.len())
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    if !is_label {
        return None;
    }
    format!("https://{name}.{COMPANY_DOMAIN}/api/").parse().ok()
}

/// A record `resolve` can look up.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Record {
    Deal(u64),
    Person(u64),
    Organization(u64),
    Lead(String),
}

/// Reads a link to a record in Pipedrive, or the short form `deal/42`, and
/// returns the record with the host of the link, if it was one.
fn parse_record(input: &str) -> Result<(Record, Option<String>)> {
    // The input is not repeated: a link may carry more than the record.
    let refused = || {
        Error::new(
            ErrorKind::InvalidInput,
            "that is not a Pipedrive record; paste the link to a deal, a person, an organization or a lead, or write it as deal/42",
        )
    };
    let trimmed = input.trim();
    let (path, host) = if trimmed.contains("://") {
        let link = Url::parse(trimmed).map_err(|_| refused())?;
        let host = link.host_str().unwrap_or_default().to_ascii_lowercase();
        let ours = link.scheme() == "https"
            && host.ends_with(&format!(".{COMPANY_DOMAIN}"))
            && link.username().is_empty()
            && link.password().is_none();
        if !ours {
            return Err(refused());
        }
        (link.path().to_owned(), Some(host))
    } else {
        (trimmed.to_owned(), None)
    };
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let number = |id: &str| {
        let digits = !id.is_empty() && id.len() <= 19 && id.bytes().all(|b| b.is_ascii_digit());
        digits.then(|| id.parse::<u64>().ok()).flatten().ok_or_else(refused)
    };
    let lead = |id: &str| {
        client::is_uuid(id)
            .then(|| Record::Lead(id.to_ascii_lowercase()))
            .ok_or_else(refused)
    };
    let record = match parts.as_slice() {
        ["deal" | "deals", id] => Record::Deal(number(id)?),
        ["person" | "persons", id] => Record::Person(number(id)?),
        ["organization" | "organizations", id] => Record::Organization(number(id)?),
        // The inbox is where the web application shows a lead.
        ["lead" | "leads", id] | ["leads", "inbox", id] => lead(id)?,
        _ => return Err(refused()),
    };
    Ok((record, host))
}

/// OAuth settings for Pipedrive. A plain [`OAuthClient`] converts into this,
/// so `Pipedrive::with_oauth(client)` works.
///
/// There is no setting for scopes: an application's scopes are chosen in
/// Pipedrive's Developer Hub, and the sign-in address does not carry them.
#[derive(Debug, Clone)]
pub struct PipedriveOAuth {
    /// The application's own app, from Pipedrive's Developer Hub.
    pub client: OAuthClient,
}

impl From<OAuthClient> for PipedriveOAuth {
    fn from(client: OAuthClient) -> Self {
        Self { client }
    }
}

/// A personal API token. A plain string converts into this, so
/// `Pipedrive::with_token("…")` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct PipedriveToken {
    /// The token, from a user's personal preferences in Pipedrive.
    pub token: SecretString,
    /// The company's own name in its Pipedrive address: `acme` for
    /// `acme.pipedrive.com`. Calls then go to that host, as Pipedrive's
    /// documentation shows them. Without it they go to `api.pipedrive.com`.
    pub company_domain: Option<String>,
}

impl From<String> for PipedriveToken {
    fn from(token: String) -> Self {
        Self {
            token: SecretString::new(token),
            company_domain: None,
        }
    }
}

impl From<&str> for PipedriveToken {
    fn from(token: &str) -> Self {
        token.to_owned().into()
    }
}

/// The Pipedrive integration.
#[derive(Debug, Clone)]
pub struct Pipedrive {
    spec: ProviderSpec,
    access: Access,
    /// What is wrong with the settings it was created with, if anything.
    problem: Option<String>,
}

impl Default for Pipedrive {
    fn default() -> Self {
        Self::new()
    }
}

impl Pipedrive {
    /// Pipedrive for connecting users through OAuth, with no connection
    /// details of its own: the OAuth app is set on the `Socket` builder and
    /// tokens come from the application's token store.
    pub fn new() -> Self {
        Self::with_spec(provider())
    }

    /// Pipedrive with the application's app, for connecting users through OAuth.
    /// Takes an [`OAuthClient`], or a [`PipedriveOAuth`].
    pub fn with_oauth(settings: impl Into<PipedriveOAuth>) -> Self {
        Self::new().oauth(settings.into().client)
    }

    /// Pipedrive with a personal API token. Every call uses it, sent in the
    /// `x-api-token` header and never in an address.
    /// Takes a string, or a [`PipedriveToken`] to name the company's host.
    ///
    /// This uses [`api_token_provider`], the definition without a sign-in.
    /// A company name that is not one is reported when the `Socket` is
    /// built, and never reaches an address.
    pub fn with_token(settings: impl Into<PipedriveToken>) -> Self {
        let settings = settings.into();
        let mut this = Self::with_spec(api_token_provider());
        let api_base = match settings.company_domain.as_deref().map(named_company_api_base) {
            Some(None) => {
                this.problem = Some(
                    "that is not a company's name in a Pipedrive address; use the first part of it, such as acme for acme.pipedrive.com"
                        .to_owned(),
                );
                None
            }
            Some(named) => named,
            None => None,
        };
        this.access.token = Some(TokenSet {
            access_token: settings.token,
            refresh_token: None,
            expires_at: None,
            scopes: Vec::new(),
            api_base,
        });
        this
    }

    /// Uses another definition: [`api_token_provider`] to keep each tenant's
    /// API token in the token store, or either one aimed at a test server.
    pub fn with_spec(spec: ProviderSpec) -> Self {
        Self {
            spec,
            access: Access::default(),
            problem: None,
        }
    }

    /// Sets the application's app.
    pub fn oauth(mut self, client: OAuthClient) -> Self {
        self.access.oauth = Some(client);
        self
    }

    /// Sets a token the application already holds. It is sent the way the
    /// definition says: bare in `x-api-token` under [`api_token_provider`],
    /// as a bearer under [`provider`].
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.access.token = Some(TokenSet::bearer(token));
        self
    }

    /// Deals.
    pub fn deals<'a>(&self, connection: &'a Connection) -> Deals<'a> {
        Deals(client::Api { connection })
    }

    /// Persons: the people a company deals with.
    pub fn persons<'a>(&self, connection: &'a Connection) -> Persons<'a> {
        Persons(client::Api { connection })
    }

    /// Organisations: the companies a company deals with.
    pub fn organizations<'a>(&self, connection: &'a Connection) -> Organizations<'a> {
        Organizations(client::Api { connection })
    }

    /// Leads: possible deals that are not yet in a pipeline.
    pub fn leads<'a>(&self, connection: &'a Connection) -> Leads<'a> {
        Leads(client::Api { connection })
    }

    /// Activities: calls, meetings, tasks and emails logged against a record.
    pub fn activities<'a>(&self, connection: &'a Connection) -> Activities<'a> {
        Activities(client::Api { connection })
    }

    /// Notes written on a deal, a person, an organisation or a lead.
    pub fn notes<'a>(&self, connection: &'a Connection) -> Notes<'a> {
        Notes(client::Api { connection })
    }

    /// Pipelines, and the stages a deal moves through in each.
    pub fn pipelines<'a>(&self, connection: &'a Connection) -> Pipelines<'a> {
        Pipelines(client::Api { connection })
    }

    /// The fields a company's deals, persons and organisations have.
    pub fn fields<'a>(&self, connection: &'a Connection) -> Fields<'a> {
        Fields(client::Api { connection })
    }

    /// The users of the company's Pipedrive.
    pub fn users<'a>(&self, connection: &'a Connection) -> Users<'a> {
        Users(client::Api { connection })
    }

    /// One search across every kind of record.
    pub fn search<'a>(&self, connection: &'a Connection) -> Search<'a> {
        Search(client::Api { connection })
    }

    fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.spec.id.clone())
    }

    /// The host of the company the connection is to, as Pipedrive names the
    /// company for the signed-in user: `acme.pipedrive.com`.
    ///
    /// Without it a link cannot be told from another company's, so an answer
    /// that names no company is an error and not a reason to go on.
    async fn company_host(&self, connection: &Connection) -> Result<String> {
        let me = self.users(connection).me().await?;
        let company = me.company_domain.unwrap_or_default().trim().to_ascii_lowercase();
        let suffix = format!(".{COMPANY_DOMAIN}");
        let name = company.strip_suffix(&suffix).unwrap_or(&company);
        if name.is_empty() {
            return Err(self.error(
                ErrorKind::Decode,
                "pipedrive answered without the company this connection is to, so the link cannot be checked; use the short form, such as deal/42",
            ));
        }
        Ok(format!("{name}{suffix}"))
    }

    /// The account the connection is authorised as: the signed-in user, in
    /// the company the connection is to. Needs only the `base` scope, which
    /// every connection has.
    ///
    /// The id is the user's. The name carries the company's beside the
    /// user's, because one person can be a user of several companies and a
    /// connection is to one of them. `users().me()` returns the company in full.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let me = self.users(connection).me().await?;
        let filled = |text: Option<String>| text.filter(|text| !text.trim().is_empty());
        let email = filled(me.email);
        let name = filled(me.name)
            .or_else(|| email.clone())
            .unwrap_or_else(|| me.id.to_string());
        let name = match filled(me.company_name) {
            Some(company) => format!("{name} ({company})"),
            None => name,
        };
        Ok(Account {
            id: me.id.to_string(),
            name,
            email,
        })
    }

    /// Confirms a deal, a person, an organisation or a lead exists and the
    /// account can see it.
    ///
    /// Accepts the link to the record in Pipedrive, or the short form
    /// `deal/42`, `person/7`, `organization/3`, `lead/<uuid>`. The
    /// resource's id is that short form.
    ///
    /// A link names a company, and is refused unless it is the company the
    /// connection is to: the same number is a different record in another
    /// company's Pipedrive, and would otherwise be reported found here. A
    /// connection that calls its company's own host knows which it is. One
    /// that calls the shared host asks Pipedrive (`users/me`), which is one
    /// more request for a link and none for the short form.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        let (record, link_host) = parse_record(input).map_err(|e| e.with_provider(self.spec.id.clone()))?;
        if let Some(linked) = &link_host {
            let own = connection.api_base().host_str().map(str::to_ascii_lowercase);
            let own = match own.filter(|host| host != API_HOST && host.ends_with(&format!(".{COMPANY_DOMAIN}"))) {
                Some(own) => own,
                None => self.company_host(connection).await?,
            };
            if *linked != own {
                return Err(self.error(
                    ErrorKind::InvalidInput,
                    "that link is to another company's Pipedrive than the one this connection is to",
                ));
            }
        }
        let named = |name: Option<String>| {
            name.filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| "Untitled".into())
        };
        Ok(match record {
            Record::Deal(id) => {
                let deal = self.deals(connection).get(id).await?;
                Resource::new(format!("deal/{}", deal.id), named(deal.title), "Pipedrive deal")
            }
            Record::Person(id) => {
                let person = self.persons(connection).get(id).await?;
                Resource::new(format!("person/{}", person.id), named(person.name), "Pipedrive person")
            }
            Record::Organization(id) => {
                let organization = self.organizations(connection).get(id).await?;
                Resource::new(
                    format!("organization/{}", organization.id),
                    named(organization.name),
                    "Pipedrive organization",
                )
            }
            Record::Lead(id) => {
                let lead = self.leads(connection).get(&id).await?;
                Resource::new(format!("lead/{}", lead.id), named(lead.title), "Pipedrive lead")
            }
        })
    }
}

#[async_trait]
impl Integration for Pipedrive {
    fn provider(&self) -> ProviderSpec {
        self.spec.clone()
    }

    /// Pipedrive's operations are named `pipedrive.…`, so the definition
    /// must keep that id. And a personal API token goes in a header: a
    /// definition that would write it into every address is refused.
    fn check(&self) -> Result<()> {
        if let Some(problem) = &self.problem {
            return Err(self.error(ErrorKind::Config, problem.clone()));
        }
        if self.spec.id.as_str() != PROVIDER_ID {
            return Err(self.error(
                ErrorKind::Config,
                format!(
                    "the Pipedrive integration needs the provider id {PROVIDER_ID:?}, not {:?}",
                    self.spec.id.as_str()
                ),
            ));
        }
        match &self.spec.auth {
            AuthScheme::OAuth2(_)
            | AuthScheme::ApiKey(ApiKeySpec {
                placement: KeyPlacement::Header { .. },
            }) => Ok(()),
            AuthScheme::ApiKey(_) => Err(self.error(
                ErrorKind::Config,
                format!(
                    "the Pipedrive integration sends an API token in the {API_TOKEN_HEADER} header and no other way"
                ),
            )),
        }
    }

    fn oauth_client(&self) -> Option<OAuthClient> {
        self.access.oauth.clone()
    }

    fn fixed_token(&self) -> Option<TokenSet> {
        self.access.token.clone()
    }

    fn operations(&self) -> Vec<OperationInfo> {
        // The two every integration offers say which scopes they need, so
        // that what an app has to be given can be read from the catalogue.
        let needing = |scopes: &[&str], operation: OperationInfo| OperationInfo {
            required_scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
            ..operation
        };
        let mut resolve = needing(
            &["deals:read", "contacts:read", "leads:read"],
            resolve_operation(
                &self.spec.id,
                "the link to a deal, a person, an organization or a lead in Pipedrive, or the short form deal/42",
            ),
        );
        // The scopes are listed together and never all needed at once.
        resolve.description.push_str(
            " Of the scopes listed, only the one for the kind of record given is needed: deals:read for a deal, contacts:read for a person or an organization, leads:read for a lead.",
        );
        let mut operations = vec![needing(&["base"], identity_operation(&self.spec.id)), resolve];
        operations.extend(operations::all().iter().map(|operation| operation.info.clone()));
        operations
    }

    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value> {
        let id = &self.spec.id;
        match operation.strip_prefix(&format!("{id}.")) {
            Some("identity.get") => to_output(id, &self.identity(&connection).await?),
            Some("resource.resolve") => to_output(id, &self.resolve(&connection, &resolve_input(id, &input)?).await?),
            _ => match operations::all().iter().find(|known| known.info.name == operation) {
                Some(known) => known.run(self.clone(), connection, input).await,
                None => Err(self.error(
                    ErrorKind::Unsupported,
                    format!("pipedrive has no operation {operation:?}"),
                )),
            },
        }
    }

    fn classifier(&self) -> Arc<dyn Classifier> {
        Arc::new(PipedriveClassifier)
    }

    fn oauth_flow(&self) -> Arc<dyn OAuthFlow> {
        Arc::new(self.clone())
    }
}

/// Pipedrive follows the standard flow, with the client's id and secret as
/// HTTP Basic. Two things differ: the sign-in address carries no scopes, and
/// the token response names the company's own API host.
#[async_trait]
impl OAuthFlow for Pipedrive {
    /// An application's scopes are set in Pipedrive's Developer Hub. The
    /// sign-in address has no parameter for them, so none is written,
    /// whatever was asked for.
    fn authorization_url(&self, context: OAuthContext, mut request: AuthorizationRequest) -> Result<Url> {
        request.scopes.clear();
        StandardOAuth.authorization_url(context, request)
    }

    /// Reads `api_domain`, the company's host, as the connection's own API
    /// address. Socket refuses one outside `*.pipedrive.com` before anything
    /// is stored, and keeps it through a refresh.
    ///
    /// An answer that names no host leaves the connection on
    /// `api.pipedrive.com`, as Pipedrive's own client library does. One that
    /// names something that is not a host is refused.
    fn parse_token_response(&self, provider: ProviderId, raw: Value, now: SystemTime) -> Result<TokenSet> {
        let mut tokens = standard_token_response(&provider, &raw, now)?;
        tokens.api_base = match raw.get("api_domain").filter(|named| !named.is_null()) {
            None => None,
            Some(named) => Some(named.as_str().and_then(company_api_base).ok_or_else(|| {
                Error::new(
                    ErrorKind::Decode,
                    format!("{provider} named an API address for the company that is not one"),
                )
                .with_provider(provider.clone())
            })?),
        };
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_companys_host_becomes_the_address_both_versions_are_below() {
        let base = |domain: &str| company_api_base(domain).map(String::from);
        assert_eq!(
            base("https://acme.pipedrive.com").as_deref(),
            Some("https://acme.pipedrive.com/api/")
        );
        assert_eq!(
            base(" https://acme.pipedrive.com/ ").as_deref(),
            Some("https://acme.pipedrive.com/api/")
        );
        // Which hosts are Pipedrive's is the definition's to say, not this function's.
        assert_eq!(
            base("http://127.0.0.1:8080").as_deref(),
            Some("http://127.0.0.1:8080/api/")
        );
        for not_a_host in [
            "",
            "acme.pipedrive.com",
            "acme",
            "ftp://acme.pipedrive.com",
            "https://acme.pipedrive.com/api/v1",
            "https://acme.pipedrive.com/?next=x",
            "https://acme.pipedrive.com/#x",
            "https://user:pw@acme.pipedrive.com",
            "https://user@acme.pipedrive.com",
        ] {
            assert_eq!(base(not_a_host), None, "{not_a_host:?}");
        }
    }

    #[test]
    fn a_company_is_named_by_one_name_of_a_host_and_nothing_else() {
        let base = |company: &str| named_company_api_base(company).map(String::from);
        assert_eq!(base("acme").as_deref(), Some("https://acme.pipedrive.com/api/"));
        assert_eq!(
            base(" Acme-Ltd ").as_deref(),
            Some("https://acme-ltd.pipedrive.com/api/")
        );
        assert_eq!(
            base("acme.pipedrive.com").as_deref(),
            Some("https://acme.pipedrive.com/api/")
        );
        for bad in [
            "",
            "-acme",
            "acme-",
            "acme.evil.test",
            "evil.test/acme",
            "acme/",
            "acme@evil.test",
            "acme:8443",
            "acme.pipedrive.com.evil.test",
            "https://acme.pipedrive.com",
            "a b",
            &"a".repeat(64),
        ] {
            assert_eq!(base(bad), None, "{bad:?}");
        }
        assert!(base(&"a".repeat(63)).is_some());
    }

    #[test]
    fn a_record_is_read_from_its_link_or_its_short_form() {
        let lead = "adf21080-0e10-11eb-879b-05d71fb426ec";
        let record = |input: &str| parse_record(input).unwrap();
        let acme = Some("acme.pipedrive.com".to_owned());
        assert_eq!(record("deal/42"), (Record::Deal(42), None));
        assert_eq!(record(" /person/7/ "), (Record::Person(7), None));
        assert_eq!(record("organizations/3"), (Record::Organization(3), None));
        assert_eq!(record(&format!("lead/{lead}")), (Record::Lead(lead.into()), None));
        assert_eq!(
            record("https://acme.pipedrive.com/deal/42"),
            (Record::Deal(42), acme.clone())
        );
        assert_eq!(
            record("https://ACME.pipedrive.com/organization/3?tab=notes#x"),
            (Record::Organization(3), acme.clone())
        );
        assert_eq!(
            record(&format!(
                "https://acme.pipedrive.com/leads/inbox/{}",
                lead.to_uppercase()
            )),
            (Record::Lead(lead.into()), acme)
        );
    }

    #[test]
    fn what_is_not_a_record_is_refused_without_repeating_it() {
        for bad in [
            "",
            "42",
            "deal",
            "deal/",
            "deal/abc",
            "deal/-1",
            "deal/42/notes",
            "deal/99999999999999999999",
            "note/42",
            "lead/42",
            "leads/inbox/42",
            "person/adf21080-0e10-11eb-879b-05d71fb426ec",
            "http://acme.pipedrive.com/deal/42",
            "https://pipedrive.com/deal/42",
            "https://acme.pipedrive.com.evil.test/deal/42",
            "https://secret-user:secret-pw@acme.pipedrive.com/deal/42",
            "https://acme.pipedrive.com/pipeline/1",
            "https://acme.pipedrive.com/deal/42/../../person/7/x",
        ] {
            let error = parse_record(bad).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::InvalidInput, "{bad:?}");
            assert!(!error.message().contains("secret"), "{}", error.message());
        }
    }

    #[test]
    fn a_refusal_says_whether_a_scope_a_plan_or_the_front_door_is_behind_it() {
        let provider = ProviderId::new(PROVIDER_ID).unwrap();
        let said = |body: Value| refusal(&provider, &body);
        assert!(
            said(serde_json::json!({ "success": false, "error": "Scope and URL mismatch" })).contains("lacks a scope")
        );
        let capped = said(
            serde_json::json!({ "success": false, "error": "Deals limit reached", "code": "feature_capping_deals_limit" }),
        );
        assert!(
            capped.contains("plan has reached a limit (feature_capping_deals_limit)"),
            "{capped}"
        );
        assert!(said(Value::Null).contains("repeated rate limiting"));
        assert_eq!(
            said(serde_json::json!({ "success": false, "error": "You do not have permission" })),
            "pipedrive denied the request: You do not have permission"
        );
        // A code is repeated only when it looks like one.
        let odd = said(serde_json::json!({ "error": "No", "code": "feature_capping <script>" }));
        assert_eq!(odd, "pipedrive denied the request: No");
    }
}
