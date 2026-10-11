//! Socket integration for Salesforce.
//!
//! Every Salesforce organisation is shaped by its customer, so the client is
//! generic: queries in SOQL (`query`), text search (`search`), records of
//! any object type (`records`), and the description of an organisation's
//! objects and fields (`sobjects`), which is how a caller learns what there
//! is to ask for. `limits` reports what is left of the daily allowance.
//! Every typed method is also a named operation.
//!
//! Each organisation has its own API host, which Salesforce names when a
//! person authorises. A connection keeps that address with its tokens and
//! calls nothing else. See `docs/integrations/salesforce.md`.

mod client;
mod escape;
pub mod models;
mod operations;

use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Access, Account, AuthScheme, AuthorizationRequest, Classifier, ClientAuth, CodeGrant, Connection, Error, ErrorKind,
    Grant, Integration, OAuth2Spec, OAuthClient, OAuthContext, OAuthFlow, OperationInfo, ProviderId, ProviderSpec,
    RawResponse, Resource, Result, Retry, StandardClassifier, StandardOAuth, TokenSet, identity_operation,
    provider_message, resolve_input, resolve_operation, standard_token_response, to_output,
};
use url::Url;

pub use client::{Limits, Query, Records, SObjects, Search};
pub use escape::{escape_soql, escape_soql_like, escape_sosl, quote_soql};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "salesforce";

/// The version of Salesforce's REST API that is called when none is chosen:
/// Summer '26. An organisation answers every version up to its own release,
/// so this is one every organisation has.
pub const DEFAULT_API_VERSION: &str = "67.0";

/// Where a person signs in to a production organisation, and to a sandbox.
/// Neither serves any organisation's data.
const PRODUCTION_LOGIN: &str = "login.salesforce.com";
const SANDBOX_LOGIN: &str = "test.salesforce.com";

/// The scopes without either of which Salesforce issues no refresh token.
const REFRESH_TOKEN: &str = "refresh_token";
const OFFLINE_ACCESS: &str = "offline_access";

/// Salesforce's definition: where a person signs in, and which hosts an
/// organisation's API may be on.
///
/// Every organisation has a host of its own under `my.salesforce.com`, a
/// sandbox included (`acme--uat.sandbox.my.salesforce.com`), and Salesforce
/// names it when a person authorises. The definition lists that domain as a
/// rule, so a connection calls the host its own authorisation named and no
/// other. Hosts outside it are not covered: the older instance hosts such as
/// `na1.salesforce.com`, and the clouds with a domain of their own.
///
/// `api_base` here is not anybody's API. A definition needs an address, and
/// the sign-in host is the only one that is not some customer's, so it
/// carries the API version and nothing is ever read from it: a connection
/// without an address of its own is refused before a request is made.
///
/// # Panics
/// Never in practice: the URLs are constants that parse.
pub fn provider() -> ProviderSpec {
    let at = |path: &str| {
        format!("https://{PRODUCTION_LOGIN}{path}")
            .parse()
            .expect("a valid URL")
    };
    ProviderSpec {
        id: ProviderId::new(PROVIDER_ID).expect("a valid provider id"),
        display_name: "Salesforce".into(),
        api_base: at(&format!("/services/data/v{DEFAULT_API_VERSION}/")),
        // login.salesforce.com serves the token endpoint.
        allowed_hosts: vec![PRODUCTION_LOGIN.into(), "*.my.salesforce.com".into()],
        content_hosts: Vec::new(),
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: at("/services/oauth2/authorize"),
            token_url: at("/services/oauth2/token"),
            default_scopes: vec!["api".into(), REFRESH_TOKEN.into(), "id".into()],
            scope_separator: " ".into(),
            pkce: true,
            client_auth: ClientAuth::Body,
            extra_authorize_params: Vec::new(),
        }),
    }
}

/// True for a host people sign in at and no organisation's data is on.
pub(crate) fn is_sign_in_host(host: &str) -> bool {
    host.eq_ignore_ascii_case(PRODUCTION_LOGIN) || host.eq_ignore_ascii_case(SANDBOX_LOGIN)
}

/// True for the shape of a record id: 15 or 18 letters and digits.
pub(crate) fn is_record_id(id: &str) -> bool {
    matches!(id.len(), 15 | 18) && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// Salesforce follows HTTP conventions, and says more in the code it gives
/// each refusal. The code decides where the status would mislead: a used-up
/// allowance and a missing permission both arrive as a 403, and a missing
/// permission also arrives as a 400.
#[derive(Debug, Clone, Copy, Default)]
pub struct SalesforceClassifier;

/// The codes that say the account may not do what was asked, whatever the
/// status: a record it has no access to, and an organisation whose edition
/// or settings give no API access.
///
/// `INVALID_FIELD_FOR_INSERT_UPDATE` is not among them. Salesforce sends it
/// when field-level security refuses a write, and also for any field nobody
/// may write: an id, a formula, the external id repeated among the fields of
/// an upsert. The second is the caller's to correct, and the code does not
/// say which it was, so it stays a refused request with Salesforce's reason.
fn denies(code: &str) -> bool {
    code.starts_with("INSUFFICIENT_ACCESS") || matches!(code, "API_DISABLED_FOR_ORG" | "API_CURRENTLY_DISABLED")
}

/// What Salesforce said about a refusal: its code, and its own words.
///
/// The data API answers with a list of `{ errorCode, message }`. The one
/// that decides the kind of error is taken, or the first. The sign-in
/// endpoints answer with `error` and `error_description`, which have no
/// code of this kind. A code is repeated only when it looks like one.
fn refusal(body: &Value) -> (String, String) {
    let code_of = |error: &Value| error["errorCode"].as_str().unwrap_or_default().to_owned();
    let decisive = |error: &&Value| {
        let code = code_of(error);
        code == "INVALID_SESSION_ID" || code == "REQUEST_LIMIT_EXCEEDED" || denies(&code)
    };
    let errors: Vec<&Value> = body.as_array().into_iter().flatten().collect();
    let Some(error) = errors
        .iter()
        .copied()
        .find(decisive)
        .or_else(|| errors.first().copied())
    else {
        return (String::new(), provider_message(body));
    };
    let code = code_of(error);
    let is_code = (1..=80).contains(&code.len())
        && code
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
    let said = error["message"].as_str().map_or_else(
        || "no reason given".to_owned(),
        |text| match text.char_indices().nth(MAX_SAID) {
            Some((cut, _)) => format!("{}…", &text[..cut]),
            None => text.to_owned(),
        },
    );
    (if is_code { code } else { String::new() }, said)
}

/// The most of Salesforce's own words that is copied into an error message.
const MAX_SAID: usize = 300;

impl Classifier for SalesforceClassifier {
    fn classify(&self, provider: &ProviderId, response: &RawResponse) -> Result<()> {
        let status = response.status;
        // A success, and throttling as HTTP writes it, are read as anywhere
        // else: a 429, or a 403 that says when to come back. Those signs win
        // over everything below, so a throttle is never taken for a refusal.
        let throttled = status == 429
            || (status == 403
                && (response.header("retry-after").is_some() || response.header("x-ratelimit-remaining") == Some("0")));
        if (200..300).contains(&status) || throttled {
            return StandardClassifier.classify(provider, response);
        }
        let error = |kind, message: String| Error::new(kind, message).with_provider(provider.clone());
        let (code, said) = refusal(&response.body);
        let named = if code.is_empty() {
            String::new()
        } else {
            format!(" ({code})")
        };
        if code == "INVALID_SESSION_ID" {
            return Err(error(
                ErrorKind::ReconnectRequired,
                format!("{provider} rejected the stored authorization"),
            ));
        }
        // The organisation's allowance, not the account's rights: the same
        // call succeeds once earlier requests are a day old, or once fewer
        // long requests are running. Salesforce does not say when.
        if code == "REQUEST_LIMIT_EXCEEDED" {
            return Err(error(
                ErrorKind::RateLimited,
                format!(
                    "{provider} is refusing requests: the organisation has used up an API allowance{named}: {said}"
                ),
            )
            .with_retry(Retry::Later));
        }
        if denies(&code) {
            return Err(error(
                ErrorKind::AccessDenied,
                format!("{provider} denied the request{named}: {said}"),
            ));
        }
        // The identity service refuses a token it does not accept with a 403
        // and one word of plain text, `Bad_OAuth_Token`, where the data API
        // answers 401. The transport does not keep an error body that is not
        // JSON, so the word cannot be read: what is left of it is an answer
        // that says it is plain text and carries nothing that could be read.
        // Only that is taken for a refused token. A 403 with no body, or
        // with a page from a proxy or a network rule, is a refusal like any
        // other: renewing the token would not change it.
        let plain_text = response
            .header("content-type")
            .and_then(|kind| kind.split(';').next())
            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("text/plain"));
        let rejected_token = response.body.is_null() && plain_text;
        match status {
            403 if rejected_token => Err(error(
                ErrorKind::ReconnectRequired,
                format!("{provider} rejected the stored authorization"),
            )),
            403 => Err(error(
                ErrorKind::AccessDenied,
                format!("{provider} denied the request{named}: {said}"),
            )),
            // Salesforce lists the records that share the external id. They
            // are not repeated.
            300 => Err(error(
                ErrorKind::InvalidInput,
                format!("{provider} found more than one record with that external id, so none was read or written"),
            )),
            400 | 405 | 409 | 412 | 415 | 422 => Err(error(
                ErrorKind::InvalidInput,
                format!("{provider} rejected the request{named}: {said}"),
            )),
            414 | 431 => Err(error(
                ErrorKind::InvalidInput,
                format!(
                    "{provider} rejected the request: it is too long; a query and its headers have about 16,000 bytes between them"
                ),
            )),
            _ => StandardClassifier.classify(provider, response),
        }
    }
}

/// Reads what a person gave to name a record: its object type and its id as
/// `Account/001xx000003DGb2AAG`, or the link to it in Lightning,
/// `https://acme.lightning.force.com/lightning/r/Account/001xx000003DGb2AAG/view`.
/// Returns the object type and the id.
pub fn parse_record_reference(input: &str) -> Result<(String, String)> {
    let trimmed = input.trim();
    let from_link = || {
        let link = Url::parse(trimmed).ok().filter(|link| link.scheme() == "https")?;
        let parts: Vec<&str> = link.path_segments()?.collect();
        match parts.as_slice() {
            ["lightning", "r", object, id, ..] => Some(((*object).to_owned(), (*id).to_owned())),
            _ => None,
        }
    };
    let from_pair = || {
        let (object, id) = trimmed.split_once('/')?;
        Some((object.trim().to_owned(), id.trim().to_owned()))
    };
    let named = if trimmed.contains("://") {
        from_link()
    } else {
        from_pair()
    };
    let is_name = |name: &str| {
        name.starts_with(|first: char| first.is_ascii_alphabetic())
            && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    };
    named
        .filter(|(object, id)| is_name(object) && is_record_id(id))
        // The input is not repeated: a link may carry more than the record.
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                "that does not name a Salesforce record; give its object type and id as `Account/001…`, or the link to it in Lightning",
            )
        })
}

/// Where people sign in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginHost {
    /// `login.salesforce.com`: production and Developer Edition
    /// organisations. What is used when nothing is chosen.
    Production,
    /// `test.salesforce.com`: sandboxes.
    Sandbox,
    /// The organisation's own My Domain host, such as
    /// `acme.my.salesforce.com` or `acme--uat.sandbox.my.salesforce.com`,
    /// for an organisation that does not let its people sign in anywhere else.
    MyDomain(String),
}

/// OAuth settings for Salesforce. A plain [`OAuthClient`] converts into this
/// with the defaults, so `Salesforce::with_oauth(client)` works when nothing
/// else is needed.
#[derive(Debug, Clone)]
pub struct SalesforceOAuth {
    /// The application's own connected app (or external client app).
    pub client: OAuthClient,
    /// Scopes to ask for in place of the defaults, `api`, `refresh_token`
    /// and `id`. `refresh_token` is always asked for as well, unless
    /// `offline_access` is: without either Salesforce issues no refresh token.
    pub scopes: Option<Vec<String>>,
    /// Where people sign in. Production when not given.
    pub login: Option<LoginHost>,
    /// The version of the REST API to call, such as `"66.0"`.
    /// [`DEFAULT_API_VERSION`] when not given.
    pub api_version: Option<String>,
}

impl From<OAuthClient> for SalesforceOAuth {
    fn from(client: OAuthClient) -> Self {
        Self {
            client,
            scopes: None,
            login: None,
            api_version: None,
        }
    }
}

/// The host a setting names, when it names one and nothing else.
///
/// A host is letters, digits, hyphens and dots. A whole address is taken
/// too, as an administrator copies it, as long as it is `https` and says
/// nothing but the host.
fn host_named(given: &str) -> Option<String> {
    let given = given.trim();
    let host = match given.strip_prefix("https://") {
        Some(rest) => rest.strip_suffix('/').unwrap_or(rest),
        None => given,
    };
    let plain = (1..=253).contains(&host.len())
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'));
    plain.then(|| host.to_ascii_lowercase())
}

/// The My Domain host `given` names, when the definition allows it as an
/// organisation's own.
///
/// What is allowed is decided by the definition's own rules for an
/// organisation's host, and by nothing written here: the host is put into an
/// address by the URL parser, and that address is asked of
/// [`ProviderSpec::allows_api_base`]. A value is never joined into an address
/// as text.
fn my_domain(spec: &ProviderSpec, given: &str) -> Option<String> {
    let host = host_named(given)?;
    let mut address: Url = format!("https://{PRODUCTION_LOGIN}/").parse().ok()?;
    address.set_host(Some(&host)).ok()?;
    let unchanged = address.host_str() == Some(host.as_str()) && address.port().is_none();
    (unchanged && spec.allows_api_base(&address)).then_some(host)
}

/// The path of the data API at `version`: `/services/data/v67.0/`.
///
/// A version is a number with one decimal, with or without a `v` before it.
/// Nothing older than 46.0 is taken: that is the first whose answer to an
/// upsert says which record was written and whether it was created, which
/// this integration reads.
fn version_path(version: &str) -> Option<String> {
    let version = version.trim();
    let number = version.strip_prefix(['v', 'V']).unwrap_or(version);
    let (major, minor) = number.split_once('.')?;
    let digits = |part: &str, most: usize| (1..=most).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit());
    if !digits(major, 3) || !digits(minor, 1) || major.starts_with('0') {
        return None;
    }
    major.parse::<u32>().ok().filter(|major| *major >= 46)?;
    Some(format!("/services/data/v{major}.{minor}/"))
}

/// The API address of the organisation at `instance_url`, when it is one the
/// definition allows: the organisation's host, with the path of the API
/// version the definition calls.
///
/// A sign-in host is refused though the definition lists it: no
/// organisation's data is there, and a token must not be sent to it.
fn instance_base(spec: &ProviderSpec, instance_url: &str) -> Option<Url> {
    let given = instance_url.trim();
    let written = if given.contains("://") {
        given.to_owned()
    } else {
        format!("https://{given}")
    };
    let mut base = Url::parse(&written).ok()?;
    base.set_path(spec.api_base.path());
    let an_organisation = !base.host_str().is_some_and(is_sign_in_host);
    (an_organisation && spec.allows_api_base(&base)).then_some(base)
}

/// What is wrong with the settings an integration was created with, each
/// decided afresh when its setting is given again.
#[derive(Debug, Clone, Default)]
struct Problems {
    login: Option<String>,
    version: Option<String>,
    instance: Option<String>,
}

/// The Salesforce integration.
#[derive(Debug, Clone)]
pub struct Salesforce {
    spec: ProviderSpec,
    access: Access,
    /// The host the login setting added to the definition's allowed hosts.
    login_host: Option<String>,
    problems: Problems,
}

impl Default for Salesforce {
    fn default() -> Self {
        Self::new()
    }
}

impl Salesforce {
    /// Salesforce with no connection details of its own: the OAuth app is set
    /// on the `Socket` builder and tokens come from the application's token store.
    pub fn new() -> Self {
        Self::with_spec(provider())
    }

    /// Salesforce with the application's connected app, for connecting users
    /// through OAuth. Takes an [`OAuthClient`], or a [`SalesforceOAuth`] for
    /// the settings only Salesforce has.
    pub fn with_oauth(settings: impl Into<SalesforceOAuth>) -> Self {
        let settings = settings.into();
        let mut this = Self::new();
        if let (AuthScheme::OAuth2(oauth), Some(scopes)) = (&mut this.spec.auth, settings.scopes) {
            oauth.default_scopes = scopes;
        }
        if let Some(login) = settings.login {
            this = this.login(login);
        }
        if let Some(version) = settings.api_version {
            this = this.api_version(&version);
        }
        this.oauth(settings.client)
    }

    /// Salesforce with a token the application already holds, and the
    /// address of the organisation it belongs to, as Salesforce gave it in
    /// `instance_url`: `https://acme.my.salesforce.com`. Every call uses them.
    ///
    /// The address is not optional. A token is good for one organisation
    /// only, and nothing but the address says which. One that is not an
    /// organisation's own host is reported when the `Socket` is built.
    pub fn with_token(token: impl Into<String>, instance_url: &str) -> Self {
        Self::new().token(token, instance_url)
    }

    /// Uses another definition, for a test server.
    pub fn with_spec(spec: ProviderSpec) -> Self {
        Self {
            spec,
            access: Access::default(),
            login_host: None,
            problems: Problems::default(),
        }
    }

    /// Sets the application's connected app.
    pub fn oauth(mut self, client: OAuthClient) -> Self {
        self.access.oauth = Some(client);
        self
    }

    /// Sets a token the application already holds, with the address of the
    /// organisation it belongs to. See [`Salesforce::with_token`].
    pub fn token(mut self, token: impl Into<String>, instance_url: &str) -> Self {
        let base = instance_base(&self.spec, instance_url);
        self.problems.instance = base.is_none().then(|| {
            // The value is not repeated: it is refused for what it holds.
            "the instance URL given with the Salesforce token is not an organisation's own address, \
             such as https://acme.my.salesforce.com"
                .to_owned()
        });
        // Without its address the token is not kept, so it can go nowhere.
        self.access.token = base.map(|base| TokenSet::bearer(token).with_api_base(base));
        self
    }

    /// Chooses where people sign in: production, a sandbox, or the
    /// organisation's own My Domain host.
    ///
    /// The host is part of the address of the sign-in page and of the token
    /// endpoint, which receives the client secret. A My Domain host is
    /// checked against the hosts this definition allows for an organisation.
    /// One that is not such a host is reported when the `Socket` is built,
    /// and never reaches either address.
    pub fn login(mut self, login: LoginHost) -> Self {
        let host = match &login {
            LoginHost::Production => Some(PRODUCTION_LOGIN.to_owned()),
            LoginHost::Sandbox => Some(SANDBOX_LOGIN.to_owned()),
            LoginHost::MyDomain(named) => my_domain(&self.spec, named),
        };
        let moved = host.and_then(|host| {
            let AuthScheme::OAuth2(oauth) = &mut self.spec.auth else {
                return None;
            };
            let (mut authorize, mut token) = (oauth.authorize_url.clone(), oauth.token_url.clone());
            authorize.set_host(Some(&host)).ok()?;
            token.set_host(Some(&host)).ok()?;
            (oauth.authorize_url, oauth.token_url) = (authorize, token);
            Some(host)
        });
        // Decided afresh on every call, so correcting a bad value clears the problem.
        self.problems.login = moved.is_none().then(|| {
            // The value is not repeated: it is refused for what it holds.
            "that is not a Salesforce My Domain host; give the organisation's own, such as acme.my.salesforce.com"
                .to_owned()
        });
        let Some(host) = moved else {
            return self;
        };
        // A host that was allowed for an earlier choice does not stay allowed.
        if let Some(previous) = self.login_host.take() {
            self.spec.allowed_hosts.retain(|allowed| *allowed != previous);
        }
        // The token endpoint has to be a host the definition lists.
        let listed = |allowed: &String| allowed.eq_ignore_ascii_case(&host);
        if !self.spec.allowed_hosts.iter().any(listed) {
            self.spec.allowed_hosts.push(host.clone());
            self.login_host = Some(host);
        }
        self
    }

    /// Chooses the version of Salesforce's REST API to call, such as
    /// `"66.0"`. [`DEFAULT_API_VERSION`] when not chosen.
    ///
    /// The version is part of the address of every call. One that is not a
    /// version number, or is older than 46.0, is reported when the `Socket`
    /// is built. A connection made through OAuth keeps the version it was
    /// made with until its token is next renewed.
    pub fn api_version(mut self, version: &str) -> Self {
        let path = version_path(version);
        self.problems.version = path.is_none().then(|| {
            "that is not a version of the Salesforce API this integration can call; give a number from 46.0, such as 67.0"
                .to_owned()
        });
        if let Some(path) = path {
            self.spec.api_base.set_path(&path);
            if let Some(base) = self.access.token.as_mut().and_then(|token| token.api_base.as_mut()) {
                base.set_path(&path);
            }
        }
        self
    }

    /// Queries in SOQL.
    pub fn query<'a>(&self, connection: &'a Connection) -> Query<'a> {
        Query(client::Api { connection })
    }

    /// Text search across object types.
    pub fn search<'a>(&self, connection: &'a Connection) -> Search<'a> {
        Search(client::Api { connection })
    }

    /// The organisation's object types, and what each is made of.
    pub fn sobjects<'a>(&self, connection: &'a Connection) -> SObjects<'a> {
        SObjects(client::Api { connection })
    }

    /// Records of any object type: reading, creating, changing and deleting them.
    pub fn records<'a>(&self, connection: &'a Connection) -> Records<'a> {
        Records(client::Api { connection })
    }

    /// What is left of the organisation's allowances.
    pub fn limits<'a>(&self, connection: &'a Connection) -> Limits<'a> {
        Limits(client::Api { connection })
    }

    fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.spec.id.clone())
    }

    /// The account the connection is authorised as, read from the
    /// organisation's own host. Needs the `id` scope, which every other
    /// scope includes.
    ///
    /// A user id is only unique within its organisation, and a sandbox
    /// copies those of production. So the account's id is both, written as
    /// Salesforce's own identity address ends:
    /// `{organisation id}/{user id}`.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let api = client::Api { connection };
        let body = api.read_at_instance("/services/oauth2/userinfo").await?;
        let filled = |value: &Value| value.as_str().filter(|s| !s.trim().is_empty()).map(str::to_owned);
        let (Some(user), Some(organisation)) = (filled(&body["user_id"]), filled(&body["organization_id"])) else {
            return Err(self.error(ErrorKind::Decode, "salesforce answered without an account"));
        };
        let email = filled(&body["email"]);
        let name = filled(&body["name"])
            .or_else(|| filled(&body["preferred_username"]))
            .or_else(|| email.clone())
            .unwrap_or_else(|| user.clone());
        Ok(Account {
            id: format!("{organisation}/{user}"),
            name,
            email,
        })
    }

    /// Confirms a record exists and the account can read it.
    ///
    /// Accepts what [`parse_record_reference`] reads. The resource's id is
    /// `{object type}/{record id}`, with the id in the 18 characters
    /// Salesforce writes, so it can be used in a later request.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        let (object, id) = parse_record_reference(input).map_err(|e| e.with_provider(self.spec.id.clone()))?;
        let record = match self
            .records(connection)
            .get(&object, &id, models::GetRecord::default())
            .await
        {
            Ok(record) => record,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Err(self.error(ErrorKind::NotFound, "that Salesforce record was not found"));
            }
            Err(e) => return Err(e),
        };
        // Most objects name their records in `Name`. The ones that do not
        // have a subject, a title or a number in its place.
        let label = [
            "Name",
            "Subject",
            "Title",
            "CaseNumber",
            "ContractNumber",
            "OrderNumber",
        ]
        .into_iter()
        .find_map(|field| {
            record
                .fields
                .get(field)?
                .as_str()
                .filter(|text| !text.trim().is_empty())
        })
        .map(str::to_owned);
        let id = record.id.unwrap_or(id);
        let description = format!("{} record", record.object_type);
        Ok(Resource::new(
            format!("{}/{id}", record.object_type),
            label.unwrap_or_else(|| id.clone()),
            description,
        ))
    }
}

#[async_trait]
impl Integration for Salesforce {
    fn provider(&self) -> ProviderSpec {
        self.spec.clone()
    }

    /// Salesforce's operations are named `salesforce.…`, so the definition must keep that id.
    fn check(&self) -> Result<()> {
        let Problems {
            login,
            version,
            instance,
        } = &self.problems;
        if let Some(problem) = login.as_ref().or(version.as_ref()).or(instance.as_ref()) {
            return Err(self.error(ErrorKind::Config, problem.clone()));
        }
        if self.spec.id.as_str() == PROVIDER_ID {
            return Ok(());
        }
        Err(self.error(
            ErrorKind::Config,
            format!(
                "the Salesforce integration needs the provider id {PROVIDER_ID:?}, not {:?}",
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
        // The two every integration offers say which scope they need, so
        // that what to ask for at sign-in can be read from the catalogue.
        let needing = |scope: &str, operation: OperationInfo| OperationInfo {
            required_scopes: vec![scope.to_owned()],
            ..operation
        };
        let mut operations = vec![
            needing("id", identity_operation(&self.spec.id)),
            needing(
                "api",
                resolve_operation(
                    &self.spec.id,
                    "a record's object type and id as `Account/001…`, or the link to the record in Lightning",
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
                    format!("salesforce has no operation {operation:?}"),
                )),
            },
        }
    }

    fn classifier(&self) -> Arc<dyn Classifier> {
        Arc::new(SalesforceClassifier)
    }

    fn oauth_flow(&self) -> Arc<dyn OAuthFlow> {
        Arc::new(self.clone())
    }
}

/// Salesforce follows the standard flow. Three things are added: a refresh
/// token is always asked for, the organisation's own address is read from
/// the token response, and an authorisation that names none is refused.
#[async_trait]
impl OAuthFlow for Salesforce {
    fn authorization_url(&self, context: OAuthContext, mut request: AuthorizationRequest) -> Result<Url> {
        // Salesforce does not say how long an access token lasts; it stops
        // working when the organisation's session policy says so. Without a
        // refresh token the connection stops with it.
        let asks = |scope: &str| request.scopes.iter().any(|asked| asked.eq_ignore_ascii_case(scope));
        if !asks(REFRESH_TOKEN) && !asks(OFFLINE_ACCESS) {
            request.scopes.push(REFRESH_TOKEN.into());
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
        let response = context.post_token(form).await?;
        let now = SystemTime::now();
        let body = context.granted(response, Grant::Code)?;
        let tokens = self.parse_token_response(context.provider().id.clone(), body, now)?;
        // A connection that does not know its organisation could call
        // nothing, so it is not made.
        if tokens.api_base.is_none() {
            return Err(self.error(
                ErrorKind::Decode,
                "salesforce answered without the address of the organisation",
            ));
        }
        Ok(tokens)
    }

    /// Reads the tokens, and the address of the organisation they are for:
    /// `instance_url`, with the path of the API version this integration
    /// calls. Socket refuses an address outside the definition's hosts.
    ///
    /// Salesforce states no lifetime, so the token has no expiry here and is
    /// used until Salesforce rejects it, when Socket renews it.
    fn parse_token_response(&self, provider: ProviderId, raw: Value, now: SystemTime) -> Result<TokenSet> {
        let mut tokens = standard_token_response(&provider, &raw, now)?;
        if let Some(named) = raw["instance_url"].as_str().filter(|named| !named.trim().is_empty()) {
            // The address is not repeated: it is refused for what it holds.
            let mut base = Url::parse(named.trim()).map_err(|_| {
                Error::new(
                    ErrorKind::Decode,
                    format!("{provider} named an organisation's address that is not a URL"),
                )
                .with_provider(provider.clone())
            })?;
            base.set_path(self.spec.api_base.path());
            tokens.api_base = Some(base);
        }
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn classified(status: u16, body: Value) -> Result<()> {
        answered(status, &[], body)
    }

    /// Classifies a response as the transport hands one over: header names
    /// in lowercase, and `null` for a body that was empty or was not JSON.
    fn answered(status: u16, headers: &[(&str, &str)], body: Value) -> Result<()> {
        let response = RawResponse {
            status,
            headers: headers
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
            body,
        };
        SalesforceClassifier.classify(&ProviderId::new(PROVIDER_ID).unwrap(), &response)
    }

    fn refused(code: &str, message: &str) -> Value {
        json!([{ "message": message, "errorCode": code }])
    }

    #[test]
    fn the_code_decides_where_the_status_would_mislead() {
        let kind = |status, body| classified(status, body).unwrap_err().kind();
        assert_eq!(
            kind(401, refused("INVALID_SESSION_ID", "Session expired or invalid")),
            ErrorKind::ReconnectRequired
        );
        assert_eq!(
            kind(403, refused("REQUEST_LIMIT_EXCEEDED", "TotalRequests Limit exceeded.")),
            ErrorKind::RateLimited
        );
        assert_eq!(kind(403, refused("INSUFFICIENT_ACCESS", "x")), ErrorKind::AccessDenied);
        // A missing permission also arrives as a 400.
        for code in [
            "INSUFFICIENT_ACCESS_ON_CROSS_REFERENCE_ENTITY",
            "INSUFFICIENT_ACCESS_OR_READONLY",
            "API_DISABLED_FOR_ORG",
            "API_CURRENTLY_DISABLED",
        ] {
            assert_eq!(kind(400, refused(code, "x")), ErrorKind::AccessDenied, "{code}");
        }
        assert_eq!(
            kind(403, refused("FUNCTIONALITY_NOT_ENABLED", "x")),
            ErrorKind::AccessDenied
        );
        // A field that cannot be written is refused for the account's
        // rights or for what the field is, and the code does not say which:
        // the status decides, and Salesforce's own words say the rest.
        assert_eq!(
            kind(400, refused("INVALID_FIELD_FOR_INSERT_UPDATE", "x")),
            ErrorKind::InvalidInput
        );
        assert_eq!(
            kind(403, refused("INVALID_FIELD_FOR_INSERT_UPDATE", "x")),
            ErrorKind::AccessDenied
        );
        assert_eq!(
            kind(400, refused("MALFORMED_QUERY", "unexpected token: FORM")),
            ErrorKind::InvalidInput
        );
        assert_eq!(
            kind(404, refused("NOT_FOUND", "The requested resource does not exist")),
            ErrorKind::NotFound
        );
        assert_eq!(
            kind(300, json!(["/services/data/v67.0/sobjects/Account/001xx000003DGb2AAG"])),
            ErrorKind::InvalidInput
        );
        assert_eq!(kind(431, json!(null)), ErrorKind::InvalidInput);
        assert_eq!(kind(500, refused("UNKNOWN_EXCEPTION", "x")), ErrorKind::Unexpected);
        assert!(classified(200, json!({})).is_ok());
        assert!(classified(204, json!(null)).is_ok());
    }

    #[test]
    fn the_error_that_decides_is_found_among_several() {
        let body = json!([
            { "message": "Required fields are missing: [LastName]", "errorCode": "REQUIRED_FIELD_MISSING", "fields": ["LastName"] },
            { "message": "insufficient access rights on object id", "errorCode": "INSUFFICIENT_ACCESS_OR_READONLY", "fields": [] }
        ]);
        let err = classified(400, body).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::AccessDenied);
        assert!(
            err.message().contains("(INSUFFICIENT_ACCESS_OR_READONLY)"),
            "{}",
            err.message()
        );
        assert!(
            err.message().ends_with("insufficient access rights on object id"),
            "{}",
            err.message()
        );
    }

    #[test]
    fn only_the_identity_services_plain_text_refusal_is_a_token_to_renew() {
        let kind = |headers: &[(&str, &str)], body: Value| answered(403, headers, body).unwrap_err().kind();
        // `Bad_OAuth_Token` as the transport hands it over: plain text, and
        // no body, because the word is not JSON.
        assert_eq!(
            kind(&[("content-type", "text/plain")], json!(null)),
            ErrorKind::ReconnectRequired
        );
        assert_eq!(
            kind(&[("content-type", "Text/Plain; charset=UTF-8")], json!(null)),
            ErrorKind::ReconnectRequired
        );

        // Anything else without Salesforce's own error is a refusal of the
        // request, which a new token would not change.
        for headers in [
            &[][..],
            &[("content-type", "text/html; charset=UTF-8")][..],
            &[("content-type", "application/json")][..],
            &[("content-type", "text/plainer")][..],
            &[("content-length", "0")][..],
        ] {
            assert_eq!(kind(headers, json!(null)), ErrorKind::AccessDenied, "{headers:?}");
        }
        // Plain text is not enough when Salesforce did say something.
        let explained = kind(&[("content-type", "text/plain")], json!({ "error": "Wrong_Org" }));
        assert_eq!(explained, ErrorKind::AccessDenied);
        let refused = kind(&[("content-type", "text/plain")], refused("INSUFFICIENT_ACCESS", "x"));
        assert_eq!(refused, ErrorKind::AccessDenied);
    }

    #[test]
    fn a_403_that_says_when_to_come_back_is_a_throttle_whatever_else_it_carries() {
        for (headers, body) in [
            (&[("retry-after", "120")][..], json!(null)),
            (
                &[("retry-after", "120"), ("content-type", "text/plain")][..],
                json!(null),
            ),
            (
                &[("retry-after", "120"), ("content-type", "text/html")][..],
                json!(null),
            ),
            (&[("x-ratelimit-remaining", "0")][..], json!(null)),
            (&[("retry-after", "120")][..], refused("INSUFFICIENT_ACCESS", "x")),
            (&[("retry-after", "120")][..], refused("REQUEST_LIMIT_EXCEEDED", "x")),
        ] {
            let err = answered(403, headers, body).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::RateLimited, "{headers:?}");
            if headers[0].0 == "retry-after" {
                assert_eq!(
                    err.retry(),
                    Retry::After(std::time::Duration::from_secs(120)),
                    "{headers:?}"
                );
            }
        }
        // The wait is only read on a status that can be a throttle.
        let not_found = answered(404, &[("retry-after", "120")], json!(null)).unwrap_err();
        assert_eq!(not_found.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn only_what_looks_like_a_code_is_repeated_and_long_words_are_cut() {
        let (code, said) = refusal(&refused("client_secret=shh", "x"));
        assert_eq!((code.as_str(), said.as_str()), ("", "x"));
        let (code, _) = refusal(&refused("DUPLICATE_VALUE", "x"));
        assert_eq!(code, "DUPLICATE_VALUE");
        let (_, said) = refusal(&refused("X", &"é".repeat(400)));
        assert_eq!(
            said.chars().count(),
            MAX_SAID + 1,
            "cut on a character, with a mark that it was"
        );
        assert_eq!(refusal(&json!([])).1, "no reason given");
        assert_eq!(refusal(&json!([{ "errorCode": "X" }])).1, "no reason given");
        assert_eq!(
            refusal(&json!({ "error": "invalid_grant", "error_description": "expired" })).1,
            "expired"
        );
    }

    #[test]
    fn a_record_is_named_by_type_and_id_or_by_its_link() {
        let named = |input: &str| parse_record_reference(input).unwrap();
        let account = ("Account".to_owned(), "001xx000003DGb2AAG".to_owned());
        assert_eq!(named("Account/001xx000003DGb2AAG"), account);
        assert_eq!(named("  Account / 001xx000003DGb2AAG "), account);
        assert_eq!(
            named("https://acme.lightning.force.com/lightning/r/Account/001xx000003DGb2AAG/view"),
            account
        );
        assert_eq!(
            named(
                "https://acme--uat.sandbox.lightning.force.com/lightning/r/Invoice__c/a01xx0000012345/view?ws=%2Flightning"
            ),
            ("Invoice__c".to_owned(), "a01xx0000012345".to_owned())
        );
        for bad in [
            "",
            "Account",
            "001xx000003DGb2AAG",
            "Account/",
            "/001xx000003DGb2AAG",
            "Account/001",
            "Account/001xx000003DGb2AAG/describe",
            "Account/../User/005xx000001SvogAAC",
            "Acc ount/001xx000003DGb2AAG",
            "Account?x/001xx000003DGb2AAG",
            "http://acme.lightning.force.com/lightning/r/Account/001xx000003DGb2AAG/view",
            "https://acme.lightning.force.com/lightning/o/Account/list",
            "https://acme.lightning.force.com/lightning/r/001xx000003DGb2AAG/view",
            "https://user:hunter2@acme.lightning.force.com/lightning/r/Acc%20ount/001xx000003DGb2AAG/view",
        ] {
            let err = parse_record_reference(bad).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{bad:?}");
            assert!(!err.message().contains("hunter2"), "{}", err.message());
        }
    }

    #[test]
    fn a_my_domain_host_is_one_the_definition_allows_for_an_organisation() {
        let spec = provider();
        let allowed = |given: &str| my_domain(&spec, given);
        assert_eq!(
            allowed("acme.my.salesforce.com").as_deref(),
            Some("acme.my.salesforce.com")
        );
        assert_eq!(
            allowed(" Acme.My.Salesforce.com ").as_deref(),
            Some("acme.my.salesforce.com")
        );
        assert_eq!(
            allowed("https://acme.my.salesforce.com/").as_deref(),
            Some("acme.my.salesforce.com")
        );
        assert_eq!(
            allowed("acme--uat.sandbox.my.salesforce.com").as_deref(),
            Some("acme--uat.sandbox.my.salesforce.com")
        );
        assert_eq!(
            allowed("acme-dev-ed.develop.my.salesforce.com").as_deref(),
            Some("acme-dev-ed.develop.my.salesforce.com")
        );
        for bad in [
            "",
            " ",
            "acme",
            "my.salesforce.com",
            ".my.salesforce.com",
            "acme..my.salesforce.com",
            "-acme.my.salesforce.com",
            "acme.my.salesforce.com.",
            "acme.my.salesforce.com.evil.example",
            "evil.example",
            "acme.my.salesforce.com@evil.example",
            "evil.example/acme.my.salesforce.com",
            "evil.example#.my.salesforce.com",
            "evil.example?.my.salesforce.com",
            "acme.my.salesforce.com:8443",
            "acme.my.salesforce.com/services/oauth2/token",
            "http://acme.my.salesforce.com",
            "https://acme.my.salesforce.com/setup",
            "https://user:hunter2@acme.my.salesforce.com",
            "acme.my.salesforce.com\n.evil.example",
            "acme.lightning.force.com",
            "na1.salesforce.com",
            "acme.my.salesforce.mil",
            "acm\u{00e9}.my.salesforce.com",
            "127.0.0.1",
        ] {
            assert_eq!(allowed(bad), None, "{bad:?}");
        }
        assert_eq!(
            allowed(&format!("{}.my.salesforce.com", "a".repeat(63))).map(|h| h.len()),
            Some(81)
        );
        assert_eq!(
            allowed(&format!("{}.my.salesforce.com", "a".repeat(64))),
            None,
            "a label of 64"
        );
    }

    #[test]
    fn a_version_is_a_number_from_46_with_one_decimal() {
        assert_eq!(version_path("67.0").as_deref(), Some("/services/data/v67.0/"));
        assert_eq!(version_path(" v66.0 ").as_deref(), Some("/services/data/v66.0/"));
        assert_eq!(version_path("V46.0").as_deref(), Some("/services/data/v46.0/"));
        assert_eq!(version_path("100.0").as_deref(), Some("/services/data/v100.0/"));
        assert_eq!(
            version_path(DEFAULT_API_VERSION).as_deref(),
            Some(provider().api_base.path()),
            "the default is what the definition carries"
        );
        for bad in [
            "",
            "67",
            "v67",
            "67.",
            ".0",
            "67.00",
            "67.0.1",
            "45.0",
            "9.0",
            "067.0",
            "1000.0",
            "latest",
            "67,0",
            "67.0/",
            "67.0/../../oauth2",
            "../67.0",
            "67.0?x",
            "vv67.0",
            "-67.0",
            "6 7.0",
            "67.x",
        ] {
            assert_eq!(version_path(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn an_instance_address_is_an_organisations_own_host_and_keeps_nothing_else() {
        let spec = provider();
        let base = |given: &str| instance_base(&spec, given).map(String::from);
        let acme = Some("https://acme.my.salesforce.com/services/data/v67.0/".to_owned());
        assert_eq!(base("https://acme.my.salesforce.com"), acme);
        assert_eq!(base("https://acme.my.salesforce.com/"), acme);
        assert_eq!(base(" acme.my.salesforce.com "), acme);
        // An address copied from a browser or a response: only its host is kept.
        assert_eq!(
            base("https://acme.my.salesforce.com/services/data/v52.0/sobjects"),
            acme
        );
        assert_eq!(
            base("https://acme--uat.sandbox.my.salesforce.com").as_deref(),
            Some("https://acme--uat.sandbox.my.salesforce.com/services/data/v67.0/")
        );
        for bad in [
            "",
            "https://",
            "https://login.salesforce.com",
            "https://test.salesforce.com",
            "login.salesforce.com",
            "http://acme.my.salesforce.com",
            "https://acme.my.salesforce.com:8443",
            "https://user:hunter2@acme.my.salesforce.com",
            "https://acme.my.salesforce.com?x=1",
            "https://acme.my.salesforce.com/#x",
            "https://evil.example/acme.my.salesforce.com",
            "https://acme.my.salesforce.com.evil.example",
            "https://na1.salesforce.com",
            "ftp://acme.my.salesforce.com",
            "not a url",
        ] {
            assert_eq!(base(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_sign_in_hosts_are_known_whatever_their_case() {
        assert!(is_sign_in_host("login.salesforce.com"));
        assert!(is_sign_in_host("Test.Salesforce.com"));
        assert!(!is_sign_in_host("acme.my.salesforce.com"));
        assert!(!is_sign_in_host("login.salesforce.com.evil.example"));
        assert!(!is_sign_in_host("127.0.0.1"));
    }
}
