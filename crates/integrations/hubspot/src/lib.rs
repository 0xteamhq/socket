//! Socket integration for HubSpot.
//!
//! Offers the provider definition, identity (the account, which HubSpot
//! calls a portal), lookup of a record by its link, and typed methods for
//! the CRM grouped by area: records of any object type (`objects`), the
//! links between them (`associations`), an object type's fields
//! (`properties`), the stages of deals and tickets (`pipelines`) and the
//! people records are assigned to (`owners`). Every typed method is also a
//! named operation. See `docs/integrations/hubspot.md`.

mod client;
pub mod models;
mod operations;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Access, Account, AuthScheme, AuthorizationRequest, Classifier, ClientAuth, Connection, Effect, Error, ErrorKind,
    Integration, OAuth2Spec, OAuthClient, OAuthContext, OAuthFlow, OperationInfo, ProviderId, ProviderSpec, RawRequest,
    RawResponse, Resource, Result, Retry, SecretString, StandardClassifier, StandardOAuth, TokenSet,
    identity_operation, resolve_input, resolve_operation, standard_token_response, to_output,
};
use url::Url;

pub use client::{Associations, Objects, Owners, Pipelines, Properties};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "hubspot";

/// The version of HubSpot's API every call is made at. HubSpot names a
/// version by the month it was released and supports it for eighteen months.
pub const API_VERSION: &str = "2026-09";

/// The scope every HubSpot application has, and all that identity needs.
const OAUTH: &str = "oauth";

/// The scope beside the contacts one that an email logged on a record needs.
const EMAIL_ENGAGEMENTS: &str = "sales-email-read";

/// HubSpot's definition: where its API lives and how it authenticates.
///
/// # Panics
/// Never in practice: the URLs are constants that parse.
pub fn provider() -> ProviderSpec {
    ProviderSpec {
        id: ProviderId::new(PROVIDER_ID).expect("a valid provider id"),
        display_name: "HubSpot".into(),
        api_base: "https://api.hubapi.com/".parse().expect("a valid URL"),
        // The API and the token endpoint are one host. The page a person
        // approves on, app.hubspot.com, is opened by their browser and is
        // never sent a credential, so it is not listed.
        allowed_hosts: vec!["api.hubapi.com".into()],
        content_hosts: Vec::new(),
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: "https://app.hubspot.com/oauth/authorize".parse().expect("a valid URL"),
            token_url: format!("https://api.hubapi.com/oauth/{API_VERSION}/token")
                .parse()
                .expect("a valid URL"),
            default_scopes: vec![OAUTH.into()],
            scope_separator: " ".into(),
            // HubSpot's sign-in page documents no code challenge.
            pkce: false,
            client_auth: ClientAuth::Body,
            extra_authorize_params: Vec::new(),
        }),
    }
}

/// Which of HubSpot's request limits a 429 was for.
///
/// It travels on the error as its cause, so that a search can tell its own
/// limit from the others by what HubSpot said, and not by the words of a
/// message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Limit {
    /// The account's requests for the day.
    Daily,
    /// Requests in any ten seconds, of every kind of call but a search.
    TenSeconds,
    /// Requests in one second. HubSpot no longer holds ordinary calls to
    /// such a limit; a search is held to five a second.
    Second,
    /// HubSpot named no limit, or one this crate does not know.
    Unnamed,
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Daily => "the daily request limit",
            Self::TenSeconds => "the ten-second request limit",
            Self::Second => "the one-second request limit",
            Self::Unnamed => "a request limit that was not named",
        })
    }
}

impl std::error::Error for Limit {}

/// The limit `error` was for, when it is a 429 this crate's classifier read.
pub(crate) fn limit_of(error: &Error) -> Option<Limit> {
    std::error::Error::source(error)?.downcast_ref::<Limit>().copied()
}

/// HubSpot follows HTTP conventions, with four additions: a 429 names which
/// of its limits was reached, a 403 for a missing permission names the
/// scope, a 423 is records locked for two seconds, and a 477 is an account
/// being moved between data centres.
///
/// Only a 429 is `RateLimited`. The transport sends a rate-limited request
/// again whatever its verb, because a request that was limited was not
/// carried out; HubSpot says that of no other answer.
#[derive(Debug, Clone, Copy, Default)]
pub struct HubSpotClassifier;

impl Classifier for HubSpotClassifier {
    fn classify(&self, provider: &ProviderId, response: &RawResponse) -> Result<()> {
        let error = |kind, message: String| Error::new(kind, message).with_provider(provider.clone());
        let body = &response.body;
        // Where HubSpot documents a wait, for an account that is being
        // moved, it is in seconds. HTTP also allows a date, and a date
        // already past means "now".
        let wait = response.header("retry-after").map(str::trim).and_then(|value| {
            let seconds = value.parse().ok().map(Duration::from_secs);
            seconds.or_else(|| {
                let when = httpdate::parse_http_date(value).ok()?;
                Some(when.duration_since(SystemTime::now()).unwrap_or(Duration::ZERO))
            })
        });
        match response.status {
            429 => {
                let policy = body["policyName"].as_str().unwrap_or_default();
                // A private app's answers also count down the day's requests.
                let none_left = response.header("x-hubspot-ratelimit-daily-remaining").map(str::trim) == Some("0");
                // The policy is HubSpot's own text, so only the names it
                // documents are read, and none is repeated.
                let limit = match policy.to_ascii_uppercase().as_str() {
                    "DAILY" => Limit::Daily,
                    "TEN_SECONDLY_ROLLING" => Limit::TenSeconds,
                    "SECONDLY" => Limit::Second,
                    "" if none_left => Limit::Daily,
                    _ => Limit::Unnamed,
                };
                let (message, unstated) = match limit {
                    // Trying again at once cannot succeed, and each refusal
                    // counts against the account. Without a wait from
                    // HubSpot there is none to give: the day starts again at
                    // midnight in a time zone only the account knows.
                    Limit::Daily => (
                        format!(
                            "{provider}'s daily request limit for this account is used up; \
                             it starts again at midnight in the account's time zone"
                        ),
                        Retry::Never,
                    ),
                    Limit::TenSeconds => (
                        format!("{provider} is rate limiting requests: its limit for any ten seconds was reached"),
                        Retry::Later,
                    ),
                    Limit::Second => (
                        format!("{provider} is rate limiting requests: its limit for one second was reached"),
                        Retry::Later,
                    ),
                    Limit::Unnamed => (format!("{provider} is rate limiting requests"), Retry::Later),
                };
                Err(error(ErrorKind::RateLimited, message)
                    .with_retry(wait.map_or(unstated, Retry::After))
                    .with_source(limit))
            }
            // HubSpot locks records for two seconds while a large change is
            // applied, and asks for at least that long between requests. It
            // does not say that nothing of the request was done, so this is
            // not `RateLimited`: a write is reported, and not sent again.
            423 => Err(error(
                ErrorKind::Unexpected,
                format!(
                    "{provider} has locked the records while a large change is applied to them; \
                     wait two seconds before the next request"
                ),
            )
            .with_retry(Retry::After(wait.unwrap_or(Duration::from_secs(2))))),
            477 => Err(error(
                ErrorKind::Unexpected,
                format!("{provider} is moving this account between its data centres and is not answering for it"),
            )
            .with_retry(wait.map_or(Retry::Later, Retry::After))),
            403 if body["category"] == "MISSING_SCOPES" => {
                let scopes = missing_scopes(body);
                let named = if scopes.is_empty() {
                    "The scope is that of the object type, such as crm.objects.contacts.read.".to_owned()
                } else {
                    format!("HubSpot names {}.", scopes.join(", "))
                };
                // How a scope is granted depends on the kind of token, and
                // the classifier does not know which this is.
                Err(error(
                    ErrorKind::AccessDenied,
                    format!(
                        "{provider} denied the request: the connection was not granted a scope this call needs. \
                         {named} Connect again asking for it or, for a private app's token, add it to the app in HubSpot"
                    ),
                ))
            }
            _ => StandardClassifier.classify(provider, response),
        }
    }
}

/// The scopes a refusal for a missing permission names, wherever HubSpot
/// put them. Only what has the shape of a scope is kept, and only a few:
/// the text is HubSpot's and goes into an error.
fn missing_scopes(body: &Value) -> Vec<String> {
    const NAMED_UNDER: [&str; 3] = ["missingScopes", "requiredScopes", "requiredGranularScopes"];
    const MOST: usize = 8;
    let is_scope = |text: &str| {
        (1..=80).contains(&text.len())
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    };
    let details = body["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|detail| &detail["context"]);
    let mut scopes: Vec<String> = Vec::new();
    for context in std::iter::once(&body["context"]).chain(details) {
        let named = NAMED_UNDER
            .iter()
            .flat_map(|name| context[*name].as_array().into_iter().flatten())
            .filter_map(Value::as_str);
        for scope in named {
            if is_scope(scope) && !scopes.iter().any(|known| known == scope) && scopes.len() < MOST {
                scopes.push(scope.to_owned());
            }
        }
    }
    scopes
}

/// The scopes a call on one object type needs: `Effect::Read` to read its
/// records, anything else to change them.
///
/// HubSpot grants access object by object, so an application that uses the
/// generic operations on one object type can ask for exactly this. Notes,
/// calls, meetings and tasks go by the contacts scope; an email logged on a
/// record needs `sales-email-read` as well; a custom object needs the
/// `custom` scope, which only an Enterprise account has, so it belongs
/// among the optional scopes. An object type this crate does not know
/// returns nothing.
pub fn object_scopes(object_type: &str, effect: Effect) -> Vec<String> {
    let access = if effect == Effect::Read { "read" } else { "write" };
    let object_type = object_type.trim().to_ascii_lowercase();
    let scope = |object: &str| format!("crm.objects.{object}.{access}");
    let is_custom = object_type.starts_with("2-")
        || object_type
            .strip_prefix('p')
            .and_then(|rest| rest.split_once('_'))
            .is_some_and(|(account, _)| !account.is_empty() && account.bytes().all(|b| b.is_ascii_digit()));
    match object_type.as_str() {
        "contacts" | "contact" | "0-1" => vec![scope("contacts")],
        "companies" | "company" | "0-2" => vec![scope("companies")],
        "deals" | "deal" | "0-3" => vec![scope("deals")],
        "tickets" | "ticket" | "0-5" => vec![scope("tickets")],
        "notes" | "note" | "0-46" | "calls" | "call" | "0-48" | "meetings" | "meeting" | "0-47" | "tasks" | "task"
        | "0-27" => vec![scope("contacts")],
        "emails" | "email" | "0-49" => vec![scope("contacts"), EMAIL_ENGAGEMENTS.to_owned()],
        _ if is_custom => vec![scope("custom")],
        _ => Vec::new(),
    }
}

/// A record a person named: its object type and its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRef {
    pub object_type: String,
    pub id: String,
    /// The id of the HubSpot account a link was in. `None` when the record
    /// was given by its object type and id, which name no account.
    pub account: Option<String>,
}

/// Reads a record's link from HubSpot, or its object type and id written as
/// `contacts/12345`.
///
/// A link is `https://app.hubspot.com/contacts/{account}/record/{object
/// type id}/{record id}`, on HubSpot's own host or a regional one such as
/// `app-eu1.hubspot.com`.
pub fn parse_record(input: &str) -> Result<RecordRef> {
    let trimmed = input.trim();
    // The input is not repeated: a link that is refused may carry a
    // username and password.
    let refused = || {
        Error::new(
            ErrorKind::InvalidInput,
            "that is not a HubSpot record; paste the record's link from HubSpot, \
             or give its object type and id as `contacts/12345`",
        )
    };
    let is_number = |text: &str| (1..=20).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_digit());
    let (object_type, id, account) = if trimmed.contains("://") {
        let link = Url::parse(trimmed).ok().filter(is_hubspot_page).ok_or_else(refused)?;
        let segments: Vec<&str> = link.path_segments().map(Iterator::collect).unwrap_or_default();
        match segments.as_slice() {
            [_, account, "record", object_type, id, ..] if is_number(account) => {
                ((*object_type).to_owned(), (*id).to_owned(), Some((*account).to_owned()))
            }
            _ => return Err(refused()),
        }
    } else {
        let (object_type, id) = trimmed.split_once('/').ok_or_else(refused)?;
        (object_type.to_owned(), id.to_owned(), None)
    };
    if client::is_object_type(&object_type) && is_number(&id) {
        Ok(RecordRef {
            object_type,
            id,
            account,
        })
    } else {
        Err(refused())
    }
}

/// True for a page of HubSpot's own application: `app.hubspot.com`, or a
/// regional host such as `app-eu1.hubspot.com`.
fn is_hubspot_page(link: &Url) -> bool {
    let on_hubspot = link.host_str().is_some_and(|host| {
        let host = host.to_ascii_lowercase();
        let region = host
            .strip_prefix("app-")
            .and_then(|rest| rest.strip_suffix(".hubspot.com"));
        host == "app.hubspot.com"
            || region.is_some_and(|region| !region.is_empty() && region.bytes().all(|b| b.is_ascii_alphanumeric()))
    });
    link.scheme() == "https" && on_hubspot && link.username().is_empty() && link.password().is_none()
}

/// The properties that name a record, across the object types. HubSpot
/// leaves out the ones an object type does not have.
const NAMING: [&str; 11] = [
    "firstname",
    "lastname",
    "name",
    "dealname",
    "subject",
    "hs_call_title",
    "hs_meeting_title",
    "hs_task_subject",
    "hs_email_subject",
    "email",
    "domain",
];

/// What to call a record: a person's name, or the first of the naming
/// properties that has a value.
fn label(properties: &BTreeMap<String, Option<String>>) -> Option<String> {
    let filled = |name: &str| {
        let value = properties.get(name)?.as_deref()?.trim();
        (!value.is_empty()).then(|| value.to_owned())
    };
    let person: Vec<String> = ["firstname", "lastname"].into_iter().filter_map(filled).collect();
    if !person.is_empty() {
        return Some(person.join(" "));
    }
    NAMING.into_iter().find_map(filled)
}

/// The word for one record of an object type, by its name or its type id.
fn noun(object_type: &str) -> &'static str {
    match object_type.to_ascii_lowercase().as_str() {
        "contacts" | "contact" | "0-1" => "contact",
        "companies" | "company" | "0-2" => "company",
        "deals" | "deal" | "0-3" => "deal",
        "tickets" | "ticket" | "0-5" => "ticket",
        "notes" | "note" | "0-46" => "note",
        "calls" | "call" | "0-48" => "call",
        "meetings" | "meeting" | "0-47" => "meeting",
        "emails" | "email" | "0-49" => "email",
        "tasks" | "task" | "0-27" => "task",
        _ => "record",
    }
}

/// OAuth settings for HubSpot. A plain [`OAuthClient`] converts into this with
/// the defaults, so `HubSpot::with_oauth(client)` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct HubSpotOAuth {
    /// The application's own HubSpot app.
    pub client: OAuthClient,
    /// Scopes the account has to grant, in place of the default, which is
    /// `oauth` alone. `oauth` is always asked for as well. HubSpot refuses
    /// the whole authorisation when the account's plan lacks one of these.
    pub scopes: Option<Vec<String>>,
    /// Scopes to ask for where the account has them: HubSpot's
    /// `optional_scope`. One the account's plan lacks is left out and the
    /// authorisation still goes through, so this is where a scope only some
    /// plans have belongs, such as `crm.objects.custom.read`. The scopes
    /// that were granted are on the stored tokens.
    pub optional_scopes: Vec<String>,
}

impl From<OAuthClient> for HubSpotOAuth {
    fn from(client: OAuthClient) -> Self {
        Self {
            client,
            scopes: None,
            optional_scopes: Vec::new(),
        }
    }
}

/// A HubSpot access token: a private app's, or the static token of an
/// application installed in one account. A plain string converts into this,
/// so `HubSpot::with_token("…")` works when nothing else is needed.
#[derive(Debug, Clone)]
pub struct HubSpotToken {
    pub token: SecretString,
}

impl From<String> for HubSpotToken {
    fn from(token: String) -> Self {
        Self {
            token: SecretString::new(token),
        }
    }
}

impl From<&str> for HubSpotToken {
    fn from(token: &str) -> Self {
        token.to_owned().into()
    }
}

/// The HubSpot integration.
#[derive(Debug, Clone)]
pub struct HubSpot {
    spec: ProviderSpec,
    access: Access,
    /// Scopes asked for where the account has them.
    optional_scopes: Vec<String>,
}

impl Default for HubSpot {
    fn default() -> Self {
        Self::new()
    }
}

impl HubSpot {
    /// HubSpot with no connection details of its own: the OAuth app is set on the
    /// `Socket` builder and tokens come from the application's token store.
    pub fn new() -> Self {
        Self::with_spec(provider())
    }

    /// HubSpot with the application's own app, for connecting accounts through OAuth.
    /// Takes an [`OAuthClient`], or a [`HubSpotOAuth`] for the settings only HubSpot has.
    pub fn with_oauth(settings: impl Into<HubSpotOAuth>) -> Self {
        let settings = settings.into();
        let mut this = Self::new().optional_scopes(settings.optional_scopes);
        if let (AuthScheme::OAuth2(oauth), Some(scopes)) = (&mut this.spec.auth, settings.scopes) {
            oauth.default_scopes = scopes;
        }
        this.oauth(settings.client)
    }

    /// HubSpot with a token the application already holds, such as a private
    /// app's. Every call uses it, as a bearer token.
    /// Takes a string, or a [`HubSpotToken`] for the settings only HubSpot has.
    pub fn with_token(settings: impl Into<HubSpotToken>) -> Self {
        let settings = settings.into();
        let mut this = Self::new();
        this.access.token = Some(TokenSet {
            access_token: settings.token,
            refresh_token: None,
            expires_at: None,
            scopes: Vec::new(),
            api_base: None,
        });
        this
    }

    /// Uses another definition, for a test server.
    pub fn with_spec(spec: ProviderSpec) -> Self {
        Self {
            spec,
            access: Access::default(),
            optional_scopes: Vec::new(),
        }
    }

    /// Sets the application's own HubSpot app.
    pub fn oauth(mut self, client: OAuthClient) -> Self {
        self.access.oauth = Some(client);
        self
    }

    /// Sets a token the application already holds.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.access.token = Some(TokenSet::bearer(token));
        self
    }

    /// Sets the scopes to ask for where the account has them. See
    /// [`HubSpotOAuth::optional_scopes`].
    pub fn optional_scopes(mut self, scopes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.optional_scopes = scopes.into_iter().map(Into::into).collect();
        self
    }

    /// Records of any object type: contacts, companies, deals, tickets,
    /// notes, calls, meetings, emails, tasks and custom objects.
    pub fn objects<'a>(&self, connection: &'a Connection) -> Objects<'a> {
        Objects(client::Api { connection })
    }

    /// The associations between records.
    pub fn associations<'a>(&self, connection: &'a Connection) -> Associations<'a> {
        Associations(client::Api { connection })
    }

    /// The fields of an object type, in this account.
    pub fn properties<'a>(&self, connection: &'a Connection) -> Properties<'a> {
        Properties(client::Api { connection })
    }

    /// The stages a deal or a ticket moves through.
    pub fn pipelines<'a>(&self, connection: &'a Connection) -> Pipelines<'a> {
        Pipelines(client::Api { connection })
    }

    /// The people and queues a record can be assigned to.
    pub fn owners<'a>(&self, connection: &'a Connection) -> Owners<'a> {
        Owners(client::Api { connection })
    }

    fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.spec.id.clone())
    }

    /// The account the connection is authorised for. Needs the `oauth`
    /// scope, which every HubSpot application has.
    ///
    /// The token travels in the `Authorization` header, as on every call.
    /// HubSpot's older way of asking about a token writes the token into the
    /// address, and its newer one needs the application's client secret, so
    /// neither is used. HubSpot names no person here: the id is the
    /// account's, which HubSpot calls the portal id or Hub ID.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let request = RawRequest::get(format!("account-info/{API_VERSION}/details"));
        let body = connection.request(request).await?.body;
        let id = match &body["portalId"] {
            Value::Number(id) if id.as_u64().is_some_and(|id| id > 0) => id.to_string(),
            Value::String(id) if !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()) => id.clone(),
            _ => return Err(self.error(ErrorKind::Decode, "hubspot answered without an account")),
        };
        let name = body["portalName"]
            .as_str()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map_or_else(|| format!("HubSpot account {id}"), str::to_owned);
        Ok(Account { id, name, email: None })
    }

    /// Confirms a record exists and the account can read it.
    ///
    /// The resource's id is the object type and the record id,
    /// `contacts/12345`, which are the two arguments `objects.get` takes.
    ///
    /// A link names the account its record is in, and record ids are only
    /// unique within one account. So a link is first checked against the
    /// connection's own account, which takes one more call: a link from
    /// another account would otherwise resolve to whatever record has the
    /// same id here.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        let record = parse_record(input).map_err(|e| e.with_provider(self.spec.id.clone()))?;
        if let Some(named) = &record.account {
            let own = self.identity(connection).await?.id;
            if named.trim_start_matches('0') != own.trim_start_matches('0') {
                return Err(self.error(
                    ErrorKind::NotFound,
                    "that link is to a record in another HubSpot account than the one this connection is for",
                ));
            }
        }
        let naming = models::GetObject {
            properties: Some(NAMING.iter().map(|name| (*name).to_owned()).collect()),
            ..models::GetObject::default()
        };
        let found = match self
            .objects(connection)
            .get(&record.object_type, &record.id, naming)
            .await
        {
            Ok(found) => found,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Err(self.error(ErrorKind::NotFound, "that HubSpot record was not found"));
            }
            Err(e) => return Err(e),
        };
        let kind = noun(&record.object_type);
        let name = label(&found.properties).unwrap_or_else(|| format!("{kind} {}", found.id));
        Ok(Resource::new(
            format!("{}/{}", record.object_type, found.id),
            name,
            format!("HubSpot {kind}"),
        ))
    }
}

#[async_trait]
impl Integration for HubSpot {
    fn provider(&self) -> ProviderSpec {
        self.spec.clone()
    }

    /// HubSpot's operations are named `hubspot.…`, so the definition must keep that id.
    fn check(&self) -> Result<()> {
        if self.spec.id.as_str() == PROVIDER_ID {
            return Ok(());
        }
        Err(self.error(
            ErrorKind::Config,
            format!(
                "the HubSpot integration needs the provider id {PROVIDER_ID:?}, not {:?}",
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
        // The two every integration offers say which scopes they need, so
        // that what to ask for at sign-in can be read from the catalogue.
        let needing = |scopes: &[&str], operation: OperationInfo| OperationInfo {
            required_scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
            ..operation
        };
        let mut operations = vec![
            needing(&[OAUTH], identity_operation(&self.spec.id)),
            // A link is checked against the account, which is what `oauth` reads.
            needing(
                &[&[OAUTH], operations::READ].concat(),
                resolve_operation(
                    &self.spec.id,
                    "a record's link from HubSpot, or its object type and id as contacts/12345",
                ),
            ),
        ];
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
                    format!("hubspot has no operation {operation:?}"),
                )),
            },
        }
    }

    fn classifier(&self) -> Arc<dyn Classifier> {
        Arc::new(HubSpotClassifier)
    }

    fn oauth_flow(&self) -> Arc<dyn OAuthFlow> {
        Arc::new(self.clone())
    }
}

/// HubSpot follows the standard flow. Three things are its own: `oauth` is
/// always asked for, the scopes an account may lack go in `optional_scope`,
/// and the scopes that were granted come back as a list.
#[async_trait]
impl OAuthFlow for HubSpot {
    fn authorization_url(&self, context: OAuthContext, mut request: AuthorizationRequest) -> Result<Url> {
        // Every HubSpot application requires `oauth`, and HubSpot shows an
        // error for a link that leaves out a scope the application requires.
        if !request.scopes.iter().any(|scope| scope == OAUTH) {
            request.scopes.push(OAUTH.into());
        }
        // A scope asked for outright is not also asked for as optional.
        let mut optional: Vec<&str> = Vec::new();
        for scope in self.optional_scopes.iter().map(|scope| scope.trim()) {
            let asked = request.scopes.iter().any(|required| required == scope) || optional.contains(&scope);
            if !scope.is_empty() && !asked {
                optional.push(scope);
            }
        }
        let optional = optional.join(" ");
        let mut url = StandardOAuth.authorization_url(context, request)?;
        if !optional.is_empty() {
            url.query_pairs_mut().append_pair("optional_scope", &optional);
        }
        Ok(url)
    }

    /// HubSpot writes the granted scopes as a list under `scopes`, where
    /// the standard has one string under `scope`. With optional scopes this
    /// list is the only place that says which of them the account granted.
    fn parse_token_response(&self, provider: ProviderId, raw: Value, now: SystemTime) -> Result<TokenSet> {
        let mut tokens = standard_token_response(&provider, &raw, now)?;
        if tokens.scopes.is_empty() {
            tokens.scopes = raw["scopes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|scope| !scope.is_empty())
                .map(str::to_owned)
                .collect();
        }
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn hubspot() -> ProviderId {
        ProviderId::new(PROVIDER_ID).unwrap()
    }

    fn classify(status: u16, headers: &[(&str, &str)], body: Value) -> Error {
        let response = RawResponse {
            status,
            headers: headers
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
            body,
        };
        HubSpotClassifier.classify(&hubspot(), &response).unwrap_err()
    }

    fn limited(policy: &str) -> Value {
        json!({
            "status": "error", "message": "You have reached your limit.", "errorType": "RATE_LIMIT",
            "correlationId": "c033cdaa-2c40-4a64-ae48-b4cec88dad24", "policyName": policy,
            "requestId": "3d3e35b7-0dae-4b9f-a6e3-9c230cbcf8dd"
        })
    }

    #[test]
    fn each_limit_is_named_and_the_days_is_not_tried_again() {
        let rolling = classify(429, &[], limited("TEN_SECONDLY_ROLLING"));
        assert_eq!(
            (rolling.kind(), rolling.retry()),
            (ErrorKind::RateLimited, Retry::Later)
        );
        assert!(rolling.message().contains("ten seconds"), "{}", rolling.message());
        assert_eq!(limit_of(&rolling), Some(Limit::TenSeconds));

        let daily = classify(429, &[], limited("DAILY"));
        assert_eq!((daily.kind(), daily.retry()), (ErrorKind::RateLimited, Retry::Never));
        assert!(daily.message().contains("midnight"), "{}", daily.message());
        assert_eq!(limit_of(&daily), Some(Limit::Daily));

        // A private app's answers count the day down, when the body says nothing.
        let counted = classify(429, &[("x-hubspot-ratelimit-daily-remaining", "0")], Value::Null);
        assert_eq!(limit_of(&counted), Some(Limit::Daily));
        assert!(
            counted.message().contains("daily request limit"),
            "{}",
            counted.message()
        );
        let left = classify(429, &[("x-hubspot-ratelimit-daily-remaining", "41")], Value::Null);
        assert_eq!(limit_of(&left), Some(Limit::Unnamed));
        // The policy decides when both are there.
        let both = classify(
            429,
            &[("x-hubspot-ratelimit-daily-remaining", "0")],
            limited("TEN_SECONDLY_ROLLING"),
        );
        assert_eq!(limit_of(&both), Some(Limit::TenSeconds));
    }

    #[test]
    fn a_wait_hubspot_states_is_passed_on_for_every_limit() {
        for policy in ["DAILY", "TEN_SECONDLY_ROLLING", "SECONDLY", "", "SOMETHING_NEW"] {
            let waited = classify(429, &[("Retry-After", "7")], limited(policy));
            assert_eq!(waited.kind(), ErrorKind::RateLimited, "{policy}");
            assert_eq!(waited.retry(), Retry::After(Duration::from_secs(7)), "{policy}");
        }
        let when = httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(300));
        let Retry::After(wait) = classify(429, &[("retry-after", when.as_str())], limited("DAILY")).retry() else {
            panic!("a date is a wait")
        };
        assert!((290..=300).contains(&wait.as_secs()), "{wait:?}");
        let garbled = classify(429, &[("retry-after", "soon")], limited("TEN_SECONDLY_ROLLING"));
        assert_eq!(garbled.retry(), Retry::Later);
    }

    #[test]
    fn a_policy_hubspot_does_not_document_is_not_repeated() {
        let odd = classify(429, &[], limited("token=abc123 <script>"));
        assert_eq!(odd.message(), "hubspot is rate limiting requests");
        assert_eq!(limit_of(&odd), Some(Limit::Unnamed));
        let shown = format!("{odd} {odd:?} {:?}", odd.to_wire());
        assert!(!shown.contains("abc123"), "{shown}");

        // The limit is what HubSpot named, however it is written, and an
        // error that is not a 429 carries none.
        assert_eq!(limit_of(&classify(429, &[], limited("secondly"))), Some(Limit::Second));
        assert_eq!(limit_of(&classify(429, &[], Value::Null)), Some(Limit::Unnamed));
        assert_eq!(limit_of(&classify(500, &[], Value::Null)), None);
        assert_eq!(limit_of(&classify(403, &[("retry-after", "5")], Value::Null)), None);
    }

    #[test]
    fn locked_records_and_a_moving_account_say_how_long_to_wait() {
        let locked = classify(423, &[], json!({ "status": "error", "message": "Locked" }));
        // Not `RateLimited`: HubSpot does not say that nothing was written,
        // and the transport sends a rate-limited write again.
        assert_eq!(
            (locked.kind(), locked.retry()),
            (ErrorKind::Unexpected, Retry::After(Duration::from_secs(2)))
        );
        assert_eq!(limit_of(&locked), None);
        assert!(locked.message().contains("wait two seconds"), "{}", locked.message());
        let sooner = classify(423, &[("retry-after", "1")], Value::Null);
        assert_eq!(sooner.retry(), Retry::After(Duration::from_secs(1)));
        let moving = classify(477, &[("retry-after", "3600")], Value::Null);
        assert_eq!(
            (moving.kind(), moving.retry()),
            (ErrorKind::Unexpected, Retry::After(Duration::from_secs(3600)))
        );
        assert!(moving.message().contains("data centres"), "{}", moving.message());
        assert_eq!(classify(477, &[], Value::Null).retry(), Retry::Later);
    }

    #[test]
    fn a_missing_scope_is_named_from_wherever_hubspot_put_it() {
        let nested = json!({
            "status": "error", "category": "MISSING_SCOPES",
            "message": "This app hasn't been granted all required scopes to make this call.",
            "errors": [{ "message": "One or more of the following scopes are required.",
                         "context": { "requiredGranularScopes": ["crm.objects.deals.read", "crm.objects.deals.write"] } }]
        });
        let err = classify(403, &[], nested);
        assert_eq!(err.kind(), ErrorKind::AccessDenied);
        assert!(
            err.message()
                .contains("crm.objects.deals.read, crm.objects.deals.write"),
            "{}",
            err.message()
        );

        let flat = json!({ "category": "MISSING_SCOPES", "context": { "missingScopes": ["tickets", "tickets"] } });
        assert!(classify(403, &[], flat).message().contains("HubSpot names tickets."));

        // What is not a scope is not repeated, and the error still says what to do.
        let odd = json!({ "category": "MISSING_SCOPES", "context": { "missingScopes": ["a scope with spaces", 7] } });
        let message = classify(403, &[], odd).message().to_owned();
        assert!(!message.contains("spaces"), "{message}");
        assert!(message.contains("crm.objects.contacts.read"), "{message}");

        // Any other refusal keeps HubSpot's own reason.
        let other = classify(
            403,
            &[],
            json!({ "category": "FORBIDDEN", "message": "This account is suspended." }),
        );
        assert_eq!(
            other.message(),
            "hubspot denied the request: This account is suspended."
        );
    }

    #[test]
    fn the_scopes_of_an_object_type_follow_hubspots_rules() {
        let scopes = |object_type: &str, effect| object_scopes(object_type, effect);
        assert_eq!(scopes("contacts", Effect::Read), ["crm.objects.contacts.read"]);
        assert_eq!(scopes("0-2", Effect::Write), ["crm.objects.companies.write"]);
        assert_eq!(scopes("Deal", Effect::Destructive), ["crm.objects.deals.write"]);
        assert_eq!(scopes("tickets", Effect::Read), ["crm.objects.tickets.read"]);
        // An engagement goes by the contacts scope.
        for engagement in ["notes", "calls", "meetings", "tasks", "0-46", "0-48", "0-47", "0-27"] {
            assert_eq!(
                scopes(engagement, Effect::Read),
                ["crm.objects.contacts.read"],
                "{engagement}"
            );
            assert_eq!(
                scopes(engagement, Effect::Write),
                ["crm.objects.contacts.write"],
                "{engagement}"
            );
        }
        assert_eq!(
            scopes("emails", Effect::Read),
            ["crm.objects.contacts.read", "sales-email-read"]
        );
        assert_eq!(
            scopes("0-49", Effect::Write),
            ["crm.objects.contacts.write", "sales-email-read"]
        );
        for custom in ["2-3465404", "p12345_cars", "P12345_Cars"] {
            assert_eq!(scopes(custom, Effect::Read), ["crm.objects.custom.read"], "{custom}");
        }
        for unknown in ["", "line_items", "p_cars", "pets", "products", "2"] {
            assert!(scopes(unknown, Effect::Read).is_empty(), "{unknown}");
        }
    }

    #[test]
    fn a_record_is_read_from_its_link_or_from_its_type_and_id() {
        let record = |object_type: &str, id: &str, account: Option<&str>| RecordRef {
            object_type: object_type.into(),
            id: id.into(),
            account: account.map(str::to_owned),
        };
        for (input, expected) in [
            // An object type and an id name no account.
            ("contacts/12345", record("contacts", "12345", None)),
            ("  2-3465404/4388553737 ", record("2-3465404", "4388553737", None)),
            // A link does, and it is kept to be checked against the connection's.
            (
                "https://app.hubspot.com/contacts/8675309/record/0-1/12345",
                record("0-1", "12345", Some("8675309")),
            ),
            (
                "https://app-eu1.hubspot.com/contacts/8675309/record/0-3/987/?eschref=%2Fdeals",
                record("0-3", "987", Some("8675309")),
            ),
            (
                "https://APP.HubSpot.com/contacts/42/record/2-3465404/4388553737/view/1",
                record("2-3465404", "4388553737", Some("42")),
            ),
        ] {
            assert_eq!(parse_record(input).unwrap(), expected, "{input}");
        }
    }

    #[test]
    fn what_is_not_a_record_is_refused_without_being_repeated() {
        for bad in [
            "",
            "contacts",
            "contacts/",
            "/12345",
            "contacts/abc",
            "contacts/12/34",
            "contacts/../owners/1",
            "con tacts/1",
            "contacts/1?archived=true",
            "http://app.hubspot.com/contacts/8675309/record/0-1/12345",
            "https://app.hubspot.com.evil.test/contacts/8675309/record/0-1/12345",
            "https://evil.test/contacts/8675309/record/0-1/12345",
            "https://app-.hubspot.com/contacts/8675309/record/0-1/12345",
            "https://user:pw@app.hubspot.com/contacts/8675309/record/0-1/12345",
            "https://app.hubspot.com/contacts/8675309/objects/0-1/views/all/list",
            "https://app.hubspot.com/contacts/acme/record/0-1/12345",
            "https://app.hubspot.com/contacts/8675309/record/0-1/abc",
            "https://app.hubspot.com/contacts/8675309/record/0%2F1/12345",
        ] {
            let err = parse_record(bad).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{bad:?}");
            assert!(
                !err.message().contains("pw@") && !err.message().contains("evil"),
                "{err}"
            );
        }
    }

    #[test]
    fn a_record_is_called_by_its_name_whatever_kind_it_is() {
        let of = |pairs: &[(&str, Option<&str>)]| {
            pairs
                .iter()
                .map(|(name, value)| ((*name).to_owned(), value.map(str::to_owned)))
                .collect::<BTreeMap<_, _>>()
        };
        let contact = of(&[
            ("firstname", Some("Ada")),
            ("lastname", Some("Lovelace")),
            ("email", Some("ada@example.com")),
        ]);
        assert_eq!(label(&contact).as_deref(), Some("Ada Lovelace"));
        assert_eq!(
            label(&of(&[("firstname", None), ("lastname", Some(" Lovelace "))])).as_deref(),
            Some("Lovelace")
        );
        assert_eq!(
            label(&of(&[("firstname", Some("")), ("email", Some("ada@example.com"))])).as_deref(),
            Some("ada@example.com")
        );
        assert_eq!(
            label(&of(&[
                ("name", Some("Analytical Engines")),
                ("domain", Some("engines.example"))
            ]))
            .as_deref(),
            Some("Analytical Engines")
        );
        assert_eq!(label(&of(&[("dealname", Some("Renewal"))])).as_deref(), Some("Renewal"));
        assert_eq!(label(&of(&[("hs_object_id", Some("1")), ("subject", None)])), None);
        assert_eq!(
            (noun("0-5"), noun("Companies"), noun("2-3465404")),
            ("ticket", "company", "record")
        );
    }
}
