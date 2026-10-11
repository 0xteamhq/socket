use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::error::{Error, ErrorKind, Result, Retry};
use crate::provider::{ApiKeySpec, AuthScheme, KeyPlacement, ProviderId, ProviderSpec};
use crate::secret::TokenSet;

mod content;

pub use content::{Content, ContentRequest};

/// A request described as plain data. The transport adds the credentials.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawRequest {
    /// `GET`, `POST` and so on.
    pub method: String,
    /// A path relative to the provider's `api_base` (or to the connection's own, when its
    /// authorisation named one), or an absolute URL on one of the provider's allowed hosts.
    pub path: String,
    #[serde(default)]
    pub query: Vec<(String, String)>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// Sent as JSON when present.
    #[serde(default)]
    pub body: Option<Value>,
    /// Set when the answer is text and not JSON, such as a transcript. See [`RawRequest::as_text`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub text: bool,
}

impl RawRequest {
    pub fn new(method: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            text: false,
        }
    }

    pub fn get(path: impl Into<String>) -> Self {
        Self::new("GET", path)
    }

    pub fn post(path: impl Into<String>, body: Value) -> Self {
        Self::new("POST", path).with_body(body)
    }

    pub fn with_query(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.push((name.into(), value.into()));
        self
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn with_body(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }

    /// Says the answer is text, such as a transcript, and not JSON.
    ///
    /// A successful body then comes back as a JSON string holding the text
    /// exactly as it was sent, and is refused when it is not UTF-8. Nothing
    /// else changes: an error status is still read as the provider's JSON
    /// error, and the same size limit applies. Ask for the format with an
    /// `Accept` header where the provider needs one.
    pub fn as_text(mut self) -> Self {
        self.text = true;
        self
    }
}

/// A response as plain data. Header names are lowercase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// The JSON body, or `null` when the body was empty or (on an error status) not JSON.
    /// For a request sent with [`RawRequest::as_text`], a string holding the text.
    pub body: Value,
}

impl RawResponse {
    /// The first header called `name`, compared without regard to case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// One page of a listing. `next_cursor` is `None` on the last page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

/// How often and how long the transport retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total tries, including the first. `1` disables retrying.
    pub max_attempts: u32,
    /// First backoff delay; doubled on each further try.
    pub base_delay: Duration,
    /// The longest the transport will wait before one retry. A provider asking
    /// for longer gets no retry; the caller receives the rate-limit error.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
        }
    }
}

/// Decides whether a response is a success, and which error it is when it is not.
///
/// A provider that reports errors inside HTTP 200, or throttles with an
/// unusual status, supplies its own.
pub trait Classifier: Send + Sync {
    fn classify(&self, provider: &ProviderId, response: &RawResponse) -> Result<()>;
}

/// The classifier for providers that follow HTTP conventions.
#[derive(Debug, Clone, Copy, Default)]
pub struct StandardClassifier;

impl Classifier for StandardClassifier {
    fn classify(&self, provider: &ProviderId, response: &RawResponse) -> Result<()> {
        let status = response.status;
        let error = |kind, message: String| Error::new(kind, message).with_provider(provider.clone());
        let retry_after = response.header("retry-after");
        // GitHub throttles with a 403 marked by an exhausted quota header or a
        // Retry-After. That is not a refusal: retrying later succeeds.
        let throttled = status == 429
            || (status == 403 && (response.header("x-ratelimit-remaining") == Some("0") || retry_after.is_some()));
        if throttled {
            let retry = retry_guidance(retry_after);
            return Err(
                error(ErrorKind::RateLimited, format!("{provider} is rate limiting requests")).with_retry(retry),
            );
        }
        match status {
            200..=299 => Ok(()),
            401 => Err(error(
                ErrorKind::ReconnectRequired,
                format!("{provider} rejected the stored authorization"),
            )),
            // The provider's own words are the only place the reason is stated.
            403 => Err(error(
                ErrorKind::AccessDenied,
                format!("{provider} denied the request: {}", provider_message(&response.body)),
            )),
            404 => Err(error(ErrorKind::NotFound, format!("{provider} has no such resource"))),
            400 | 409 | 422 => Err(error(
                ErrorKind::InvalidInput,
                format!("{provider} rejected the request: {}", provider_message(&response.body)),
            )),
            // Only the statuses that are redirects. A 304 or a 300 is not one.
            301 | 302 | 303 | 307 | 308 => Err(error(
                ErrorKind::Unexpected,
                format!("{provider} redirected the request (HTTP {status}) to an address Socket does not follow"),
            )),
            500..=599 => {
                Err(error(ErrorKind::Unexpected, format!("{provider} returned HTTP {status}")).with_retry(Retry::Later))
            }
            _ => Err(error(
                ErrorKind::Unexpected,
                format!("{provider} returned HTTP {status}"),
            )),
        }
    }
}

/// Reads a `Retry-After` header: a number of seconds, or an HTTP date.
/// Anything else, or no header, means "later" with no time given.
pub(crate) fn retry_guidance(retry_after: Option<&str>) -> Retry {
    let Some(value) = retry_after.map(str::trim) else {
        return Retry::Later;
    };
    if let Ok(secs) = value.parse::<u64>() {
        return Retry::After(Duration::from_secs(secs));
    }
    match httpdate::parse_http_date(value) {
        // A date already past means "now".
        Ok(when) => Retry::After(
            when.duration_since(std::time::SystemTime::now())
                .unwrap_or(Duration::ZERO),
        ),
        Err(_) => Retry::Later,
    }
}

/// The provider's explanation from the usual places in an error body.
pub fn provider_message(body: &Value) -> String {
    [
        &body["message"],
        &body["error"]["message"],
        &body["error_description"],
        &body["error"],
    ]
    .into_iter()
    .find_map(Value::as_str)
    .map_or_else(
        || "no reason given".into(),
        |text| shortened(text, MAX_PROVIDER_MESSAGE),
    )
}

/// The most of a provider's own words that is copied into an error message.
const MAX_PROVIDER_MESSAGE: usize = 300;
/// The largest response body the transport will read.
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

/// `text`, cut to at most `limit` characters.
pub(crate) fn shortened(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_owned()
    } else {
        format!("{}…", text.chars().take(limit).collect::<String>())
    }
}

/// The one HTTP path every request takes.
#[derive(Debug, Clone)]
pub(crate) struct Transport {
    client: reqwest::Client,
    retry: RetryPolicy,
}

const USER_AGENT: &str = concat!("socketkit/", env!("CARGO_PKG_VERSION"));

impl Transport {
    pub(crate) fn new(client: reqwest::Client, retry: RetryPolicy) -> Self {
        Self { client, retry }
    }

    /// Sends `request` to `spec`'s API with `tokens` attached.
    pub(crate) async fn send(
        &self,
        spec: &ProviderSpec,
        tokens: &TokenSet,
        classifier: &dyn Classifier,
        request: RawRequest,
    ) -> Result<RawResponse> {
        let invalid = |message: String| Error::new(ErrorKind::InvalidInput, message).with_provider(spec.id.clone());
        let mut url = resolve_url(spec, tokens, &request.path)?;
        for (name, value) in &request.query {
            url.query_pairs_mut().append_pair(name, value);
        }
        let method = reqwest::Method::from_bytes(request.method.to_ascii_uppercase().as_bytes())
            .map_err(|_| invalid(format!("{:?} is not an HTTP method", request.method)))?;
        let idempotent = matches!(method.as_str(), "GET" | "HEAD" | "PUT" | "DELETE" | "OPTIONS");
        refuse_key_parameter(spec, &url)?;
        let headers = caller_headers(spec, &request.headers)?;

        let mut attempt = 1;
        let mut redirects = 0;
        loop {
            let outcome = self
                .send_once(spec, tokens, classifier, &method, url.clone(), &headers, &request)
                .await;
            let error = match outcome {
                Ok(Sent::Done(response)) => return Ok(response),
                // A read that the provider moved to another address it also
                // owns. `send_once` has already checked the new address
                // against the allowlist.
                Ok(Sent::Moved(next)) if redirects < MAX_REDIRECTS => {
                    redirects += 1;
                    url = next;
                    continue;
                }
                Ok(Sent::Moved(_)) => return Err(too_many_redirects(spec)),
                Err(error) => error,
            };
            // A throttled request was not processed, so any method may be retried.
            // Anything else is retried only when repeating it cannot do harm.
            let may_retry = error.kind() == ErrorKind::RateLimited || idempotent;
            match self.wait(&error, attempt) {
                Some(wait) if may_retry => {
                    tokio::time::sleep(wait).await;
                    attempt += 1;
                }
                _ => return Err(error),
            }
        }
    }

    /// How long to wait before trying again after `error` on try number
    /// `attempt`, or `None` when the request is not to be tried again.
    fn wait(&self, error: &Error, attempt: u32) -> Option<Duration> {
        if attempt >= self.retry.max_attempts {
            return None;
        }
        match error.retry() {
            Retry::Never => None,
            Retry::After(wait) => (wait <= self.retry.max_delay).then_some(wait),
            Retry::Later => Some(self.backoff(attempt)),
        }
    }

    fn backoff(&self, attempt: u32) -> Duration {
        let factor = 2u32.saturating_pow(attempt.saturating_sub(1));
        self.retry.base_delay.saturating_mul(factor).min(self.retry.max_delay)
    }

    #[allow(clippy::too_many_arguments)]
    async fn send_once(
        &self,
        spec: &ProviderSpec,
        tokens: &TokenSet,
        classifier: &dyn Classifier,
        method: &reqwest::Method,
        mut url: Url,
        caller: &HeaderMap,
        request: &RawRequest,
    ) -> Result<Sent> {
        let secret = tokens.access_token.expose();
        let asked = url.clone();
        if let Some(name) = key_parameter(spec) {
            url.query_pairs_mut().append_pair(name, secret);
        }
        // One map, filled in order: defaults, then the caller's headers, which
        // replace a default of the same name. `caller_headers` has already
        // refused any name that carries credentials, so the credentials added
        // by the builder below are the only ones on the request.
        let mut headers = HeaderMap::new();
        headers.insert(reqwest::header::USER_AGENT, HeaderValue::from_static(USER_AGENT));
        headers.insert(reqwest::header::ACCEPT, HeaderValue::from_static("application/json"));
        for (name, value) in caller {
            headers.insert(name.clone(), value.clone());
        }
        let body = match &request.body {
            Some(body) => {
                let bytes = serde_json::to_vec(body).map_err(|e| {
                    Error::new(ErrorKind::InvalidInput, "request body is not valid JSON").with_source(e)
                })?;
                // The caller's own content type wins; there is never more than one.
                headers
                    .entry(reqwest::header::CONTENT_TYPE)
                    .or_insert(HeaderValue::from_static("application/json"));
                Some(bytes)
            }
            None => None,
        };
        let mut builder = authorized(spec, self.client.request(method.clone(), url).headers(headers), secret);
        if let Some(bytes) = body {
            builder = builder.body(bytes);
        }
        let response = read(spec, builder, request.text).await?;
        if let Some(next) = redirect_target(spec, tokens, method, &asked, &response) {
            return Ok(Sent::Moved(next));
        }
        // A provider may echo the request, credential included, in its error text.
        classifier
            .classify(&spec.id, &response)
            .map_err(|e| e.map_message(|m| m.replace(secret, "[redacted]")))?;
        Ok(Sent::Done(response))
    }

    /// Posts a form to `url`, which must be one of `spec`'s allowed hosts.
    /// Used for the OAuth token endpoint. Never retried: a code or a rotating
    /// refresh token can be spent only once.
    pub(crate) async fn post_form(
        &self,
        spec: &ProviderSpec,
        url: &Url,
        form: &[(&str, &str)],
        basic: Option<(&str, &str)>,
    ) -> Result<RawResponse> {
        if !spec.allows_host(url) {
            return Err(refused(spec, url));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(Error::new(
                ErrorKind::Config,
                format!("the token URL of {} must not carry a username or password", spec.id),
            )
            .with_provider(spec.id.clone()));
        }
        // Built in its own block: the serializer is not `Send` and must not live across the await.
        let body = {
            let mut body = url::form_urlencoded::Serializer::new(String::new());
            for (name, value) in form {
                body.append_pair(name, value);
            }
            body.finish()
        };
        let mut builder = self
            .client
            .post(url.clone())
            .header("user-agent", USER_AGENT)
            .header("accept", "application/json")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body);
        if let Some((user, password)) = basic {
            builder = builder.basic_auth(user, Some(password));
        }
        read(spec, builder, false).await
    }
}

/// Puts the credential on a request, where the provider's scheme carries it
/// in a header. A key that goes in the query is written into the address.
fn authorized(spec: &ProviderSpec, builder: reqwest::RequestBuilder, secret: &str) -> reqwest::RequestBuilder {
    match &spec.auth {
        AuthScheme::OAuth2(_) => builder.bearer_auth(secret),
        AuthScheme::ApiKey(ApiKeySpec { placement }) => match placement {
            KeyPlacement::Header { name, prefix } => {
                builder.header(name.as_str(), format!("{}{secret}", prefix.as_deref().unwrap_or("")))
            }
            KeyPlacement::Basic {} => builder.basic_auth(secret, None::<&str>),
            KeyPlacement::Query { .. } => builder,
        },
    }
}

/// The query parameter that carries the API key, for a provider that puts it there.
fn key_parameter(spec: &ProviderSpec) -> Option<&str> {
    match &spec.auth {
        AuthScheme::ApiKey(ApiKeySpec {
            placement: KeyPlacement::Query { name },
        }) => Some(name),
        _ => None,
    }
}

/// True when `given` is the key's own parameter. Servers differ on case and
/// stray whitespace, so any spelling of the name counts.
fn is_key_parameter(given: &str, name: &str) -> bool {
    given.trim().eq_ignore_ascii_case(name.trim())
}

/// Refuses an address that already has a value under the API key's own name,
/// which a server could read in place of the real one.
fn refuse_key_parameter(spec: &ProviderSpec, url: &Url) -> Result<()> {
    match key_parameter(spec) {
        Some(name) if url.query_pairs().any(|(given, _)| is_key_parameter(&given, name)) => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("the query parameter {name:?} carries the credentials and cannot be set"),
        )
        .with_provider(spec.id.clone())),
        _ => Ok(()),
    }
}

/// Drops a copy of the API key that the provider echoed into `url`: the key
/// is added again on each attempt.
fn without_key_parameter(spec: &ProviderSpec, url: &mut Url) {
    let Some(name) = key_parameter(spec) else {
        return;
    };
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(given, _)| !is_key_parameter(given, name))
        .map(|(n, v)| (n.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(kept);
    }
}

fn too_many_redirects(spec: &ProviderSpec) -> Error {
    Error::new(
        ErrorKind::Unexpected,
        format!("{} redirected the request too many times", spec.id),
    )
    .with_provider(spec.id.clone())
}

/// What one attempt produced.
enum Sent {
    Done(RawResponse),
    /// The provider answered a read with a redirect to this address, which
    /// is one of its allowed hosts.
    Moved(Url),
}

/// How many redirects one request follows.
const MAX_REDIRECTS: u32 = 3;

/// Where a redirect may be followed to, if anywhere.
///
/// Only a read is followed, and only to an address that passes the same
/// allowlist as any other request: https, port 443, a listed host or the
/// connection's own, no username or password. Anything else is not followed,
/// and the classifier reports it as a redirect that was refused, so
/// credentials never follow a redirect off the provider's own hosts.
fn redirect_target(
    spec: &ProviderSpec,
    tokens: &TokenSet,
    method: &reqwest::Method,
    asked: &Url,
    response: &RawResponse,
) -> Option<Url> {
    if !matches!(response.status, 301 | 302 | 303 | 307 | 308) || !matches!(method.as_str(), "GET" | "HEAD") {
        return None;
    }
    let mut next = asked.join(response.header("location")?).ok()?;
    if !may_receive(spec, tokens, &next) || !next.username().is_empty() || next.password().is_some() {
        return None;
    }
    next.set_fragment(None);
    without_key_parameter(spec, &mut next);
    Some(next)
}

/// Headers a caller may never set: they carry credentials, choose where the
/// request is routed, or frame the body.
const RESERVED_HEADERS: [&str; 8] = [
    "authorization",
    "proxy-authorization",
    "cookie",
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "upgrade",
];

/// Checks and parses the headers a caller asked for.
fn caller_headers(spec: &ProviderSpec, requested: &[(String, String)]) -> Result<HeaderMap> {
    let invalid = |message: String| Error::new(ErrorKind::InvalidInput, message).with_provider(spec.id.clone());
    let key_header = match &spec.auth {
        AuthScheme::ApiKey(ApiKeySpec {
            placement: KeyPlacement::Header { name, .. },
        }) => Some(name.as_str()),
        _ => None,
    };
    let mut headers = HeaderMap::new();
    for (name, value) in requested {
        // Some servers treat `_` and `-` in a header name as the same character.
        let folded = |s: &str| s.trim().to_ascii_lowercase().replace('_', "-");
        let reserved = RESERVED_HEADERS.iter().any(|r| folded(name) == *r)
            || key_header.is_some_and(|key| folded(name) == folded(key));
        if reserved {
            return Err(invalid(format!(
                "the header {name:?} is set by Socket and cannot be supplied"
            )));
        }
        let parsed_name =
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| invalid(format!("{name:?} is not a header name")))?;
        // A value with a line break would start a new header.
        let parsed_value = HeaderValue::from_str(value)
            .map_err(|_| invalid(format!("the value of the header {name:?} is not valid")))?;
        headers.insert(parsed_name, parsed_value);
    }
    Ok(headers)
}

fn refused(spec: &ProviderSpec, url: &Url) -> Error {
    Error::new(
        ErrorKind::InvalidInput,
        format!(
            "refusing to send {} credentials to {}: it is not one of the provider's allowed hosts",
            spec.id,
            url.host_str().unwrap_or("that address")
        ),
    )
    .with_provider(spec.id.clone())
}

/// True when `tokens` may be sent to `url`: one of the provider's listed
/// hosts, or the API host this connection's authorisation named.
///
/// The connection's own host is checked against the provider's rules here as
/// well as when it was stored, so tokens whose address was altered in the
/// application's store still go nowhere the provider did not declare. And it
/// is this connection's host only: a rule such as `*.my.salesforce.com` does
/// not let one customer's token reach another customer's address.
fn may_receive(spec: &ProviderSpec, tokens: &TokenSet, url: &Url) -> bool {
    // Scheme, host and port are compared themselves. An origin would also
    // match a `blob:` address that only names the host inside its path.
    let same_place = |base: &Url| {
        base.scheme() == url.scheme()
            && base.host().is_some()
            && base.host() == url.host()
            && base.port_or_known_default() == url.port_or_known_default()
    };
    spec.allows_host(url)
        || tokens
            .api_base
            .as_ref()
            .is_some_and(|base| spec.allows_api_base(base) && same_place(base))
}

/// Whether content may be fetched from `url` by the connection `tokens` are
/// for, and if it may, whether the credential goes with the request.
///
/// As [`ProviderSpec::content_credentials`], with the connection's own API
/// host counted among the API's hosts.
fn content_credentials(spec: &ProviderSpec, tokens: &TokenSet, url: &Url) -> Option<bool> {
    if may_receive(spec, tokens, url) {
        return Some(true);
    }
    spec.content_credentials(url)
}

/// Turns a request path into a URL that may receive these credentials.
fn resolve_url(spec: &ProviderSpec, tokens: &TokenSet, path: &str) -> Result<Url> {
    let url = address(spec, tokens, path)?;
    if !may_receive(spec, tokens, &url) {
        return Err(refused(spec, &url));
    }
    Ok(url)
}

/// Turns a request path into a URL: the path joined to the API's address
/// (the connection's own, when its authorisation named one), or the address
/// itself when it is one. Which host it is on is for the caller to check.
fn address(spec: &ProviderSpec, tokens: &TokenSet, path: &str) -> Result<Url> {
    let invalid = |message: String| Error::new(ErrorKind::InvalidInput, message).with_provider(spec.id.clone());
    let url = if path.starts_with("https://") || path.starts_with("http://") {
        // The address is not repeated: one that is signed carries its own credential.
        Url::parse(path).map_err(|_| invalid("the address of the request is not a URL".into()))?
    } else {
        // `Url::join` drops the last segment of a base without a trailing slash,
        // and a leading slash on the path would discard the base's own path.
        let mut base = tokens.api_base.clone().unwrap_or_else(|| spec.api_base.clone());
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        base.join(path.trim_start_matches('/'))
            .map_err(|_| invalid("the path of the request is not valid".into()))?
    };
    // An HTTP client turns `user:password@host` into a Basic `Authorization`
    // header, which would travel beside or in place of the stored credentials.
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("a request URL must not carry a username or password".into()));
    }
    Ok(url)
}

/// Sends a built request and reads the response as data. Never includes the
/// request URL in an error: an API key may be in its query string.
///
/// `text` is true when the caller asked for text: a successful body is then
/// kept as a string and not parsed.
async fn read(spec: &ProviderSpec, builder: reqwest::RequestBuilder, text: bool) -> Result<RawResponse> {
    let response = builder.send().await.map_err(|e| unreached(spec, "reach", e))?;
    answer(spec, response, text, unreached).await
}

/// A request that got no answer, or whose answer broke off. Worth trying again.
fn unreached(spec: &ProviderSpec, what: &str, e: reqwest::Error) -> Error {
    Error::new(ErrorKind::Transport, format!("could not {what} {}", spec.id))
        .with_provider(spec.id.clone())
        .with_retry(Retry::Later)
        .with_source(e.without_url())
}

/// As [`unreached`], for a request that is given its time once. What did not
/// arrive in the time allowed will not arrive in it the next time either, so
/// running out of time is not worth trying again: the caller allows more.
fn unreached_once(spec: &ProviderSpec, what: &str, e: reqwest::Error) -> Error {
    let out_of_time = e.is_timeout();
    let error = unreached(spec, what, e);
    if out_of_time {
        error
            .map_message(|m| format!("{m} in the time allowed"))
            .with_retry(Retry::Never)
    } else {
        error
    }
}

/// The headers of a response as data. One that is not text is left out.
fn header_pairs(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned())))
        .collect()
}

/// Reads a response that has arrived as data: see [`read`]. `broken` makes
/// the error for an answer that breaks off.
async fn answer(
    spec: &ProviderSpec,
    mut response: reqwest::Response,
    text: bool,
    broken: fn(&ProviderSpec, &str, reqwest::Error) -> Error,
) -> Result<RawResponse> {
    let status = response.status().as_u16();
    let headers = header_pairs(response.headers());
    // Read in pieces so a broken or hostile provider cannot fill memory.
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| broken(spec, "read the response from", e))?
    {
        if bytes.len() + chunk.len() > MAX_BODY_BYTES {
            // An oversized error page still has a status and headers worth
            // acting on: a 429 stays a throttle. Its body is not read.
            if !(200..300).contains(&status) {
                return Ok(RawResponse {
                    status,
                    headers,
                    body: Value::Null,
                });
            }
            return Err(Error::new(
                ErrorKind::Decode,
                format!("{} answered with a response too large to read", spec.id),
            )
            .with_provider(spec.id.clone()));
        }
        bytes.extend_from_slice(&chunk);
    }
    if text && (200..300).contains(&status) && !bytes.is_empty() {
        // Lossy decoding would turn a recording or an archive into text that
        // looks plausible, so bytes that are not UTF-8 are refused.
        let body = String::from_utf8(bytes).map_err(|_| {
            Error::new(
                ErrorKind::Decode,
                format!("{} answered with something that is not text", spec.id),
            )
            .with_provider(spec.id.clone())
        })?;
        return Ok(RawResponse {
            status,
            headers,
            body: Value::String(body),
        });
    }
    let text = String::from_utf8_lossy(&bytes);
    let body = if text.trim().is_empty() {
        Value::Null
    } else {
        match serde_json::from_str(&text) {
            Ok(body) => body,
            // A success that is not JSON is not the provider confirming anything.
            Err(e) if (200..300).contains(&status) => {
                return Err(Error::new(
                    ErrorKind::Decode,
                    format!("{} answered with something that is not JSON", spec.id),
                )
                .with_provider(spec.id.clone())
                .with_source(e));
            }
            // An error page may be HTML or plain text; its status is what matters.
            Err(_) => Value::Null,
        }
    };
    Ok(RawResponse { status, headers, body })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn slack() -> ProviderId {
        ProviderId::new("slack").unwrap()
    }

    fn response(status: u16, headers: &[(&str, &str)], body: Value) -> RawResponse {
        RawResponse {
            status,
            headers: headers
                .iter()
                .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
                .collect(),
            body,
        }
    }

    fn classify(status: u16, headers: &[(&str, &str)], body: Value) -> Error {
        StandardClassifier
            .classify(&slack(), &response(status, headers, body))
            .unwrap_err()
    }

    #[test]
    fn success_statuses_are_not_errors() {
        for status in [200, 201, 204, 299] {
            StandardClassifier
                .classify(&slack(), &response(status, &[], Value::Null))
                .unwrap();
        }
    }

    #[test]
    fn throttling_is_told_apart_from_a_policy_refusal() {
        assert_eq!(classify(429, &[], Value::Null).kind(), ErrorKind::RateLimited);
        assert_eq!(
            classify(403, &[("x-ratelimit-remaining", "0")], Value::Null).kind(),
            ErrorKind::RateLimited
        );
        assert_eq!(
            classify(403, &[("retry-after", "5")], Value::Null).kind(),
            ErrorKind::RateLimited
        );
        assert_eq!(
            classify(403, &[("x-ratelimit-remaining", "57")], Value::Null).kind(),
            ErrorKind::AccessDenied,
            "a refusal with quota left is a refusal"
        );
        assert_eq!(
            classify(404, &[("x-ratelimit-remaining", "0")], Value::Null).kind(),
            ErrorKind::NotFound
        );
    }

    #[test]
    fn retry_after_seconds_become_retry_guidance_and_anything_else_means_later() {
        assert_eq!(
            classify(429, &[("Retry-After", "12")], Value::Null).retry(),
            Retry::After(Duration::from_secs(12))
        );
        assert_eq!(classify(429, &[], Value::Null).retry(), Retry::Later);
        let garbled = classify(429, &[("retry-after", "soon")], Value::Null);
        assert_eq!(garbled.retry(), Retry::Later);

        // An HTTP date is honoured too: the wait is the time left until it.
        let in_two_minutes = httpdate::fmt_http_date(std::time::SystemTime::now() + Duration::from_secs(120));
        let Retry::After(wait) = classify(429, &[("retry-after", in_two_minutes.as_str())], Value::Null).retry() else {
            panic!("a date in the future is a wait")
        };
        assert!((115..=120).contains(&wait.as_secs()), "{wait:?}");
        let past = classify(429, &[("retry-after", "Wed, 21 Oct 2015 07:28:00 GMT")], Value::Null);
        assert_eq!(
            past.retry(),
            Retry::After(Duration::ZERO),
            "a date already past means now"
        );
    }

    #[test]
    fn a_rejected_token_names_only_the_provider() {
        let err = classify(401, &[], json!({ "message": "Bad credentials: ghp_secret" }));
        assert_eq!(err.kind(), ErrorKind::ReconnectRequired);
        assert_eq!(err.message(), "slack rejected the stored authorization");
        assert_eq!(err.retry(), Retry::Never);
    }

    #[test]
    fn a_refusal_carries_the_providers_own_reason() {
        let github = classify(
            403,
            &[],
            json!({ "message": "the org has enabled OAuth App access restrictions" }),
        );
        assert_eq!(
            github.message(),
            "slack denied the request: the org has enabled OAuth App access restrictions"
        );
        let google = classify(
            403,
            &[],
            json!({ "error": { "code": 403, "message": "Drive API has not been used" } }),
        );
        assert_eq!(
            google.message(),
            "slack denied the request: Drive API has not been used"
        );
        let silent = classify(403, &[], Value::Null);
        assert_eq!(silent.message(), "slack denied the request: no reason given");
    }

    #[test]
    fn server_errors_may_be_retried_and_unknown_statuses_may_not() {
        let server = classify(503, &[], Value::Null);
        assert_eq!((server.kind(), server.retry()), (ErrorKind::Unexpected, Retry::Later));
        let odd = classify(418, &[], Value::Null);
        assert_eq!((odd.kind(), odd.retry()), (ErrorKind::Unexpected, Retry::Never));
        assert_eq!(
            classify(422, &[], json!({ "message": "title is too long" })).kind(),
            ErrorKind::InvalidInput
        );
    }

    fn per_customer() -> ProviderSpec {
        serde_json::from_value(json!({
            "id": "acme",
            "display_name": "Acme",
            "api_base": "https://login.acme.example/api/",
            "allowed_hosts": ["login.acme.example", "*.customers.example"],
            "auth": { "type": "api_key", "placement": { "in": "header", "name": "Authorization", "prefix": "Bearer " } }
        }))
        .unwrap()
    }

    fn at(base: &str) -> TokenSet {
        TokenSet::bearer("t").with_api_base(base.parse().unwrap())
    }

    #[test]
    fn a_connection_reaches_its_own_host_under_a_rule_and_no_other_customers() {
        let spec = per_customer();
        let acme = at("https://acme.customers.example/tenant-api/");
        let allowed = |tokens: &TokenSet, url: &str| may_receive(&spec, tokens, &Url::parse(url).unwrap());

        assert!(allowed(&acme, "https://acme.customers.example/tenant-api/things"));
        assert!(allowed(&acme, "https://ACME.customers.example:443/elsewhere"));
        assert!(allowed(&acme, "https://login.acme.example/userinfo"), "a listed host");
        for refused in [
            "https://globex.customers.example/tenant-api/things",
            "https://customers.example/",
            "http://acme.customers.example/tenant-api/things",
            "https://acme.customers.example:8443/tenant-api/things",
            "https://acme.customers.example./tenant-api/things",
            "https://*.customers.example/",
            "blob:https://acme.customers.example/things",
            "wss://acme.customers.example/things",
        ] {
            assert!(!allowed(&acme, refused), "{refused}");
        }

        // A rule gives nothing to a connection with no address of its own,
        // nor to one whose address is outside the rule.
        let bare = TokenSet::bearer("t");
        assert!(!allowed(&bare, "https://acme.customers.example/tenant-api/things"));
        assert!(!allowed(&bare, "https://*.customers.example/"));
        let altered = at("https://elsewhere.example/tenant-api/");
        assert!(!allowed(&altered, "https://elsewhere.example/tenant-api/things"));
    }

    #[test]
    fn content_on_a_connections_own_host_gets_its_token_and_another_customers_host_gets_nothing() {
        let mut spec = per_customer();
        spec.content_hosts = vec![crate::provider::ContentHost {
            host: "signed.acme.example".into(),
            credentials: false,
        }];
        let acme = at("https://acme.customers.example/tenant-api/");
        let credentials = |tokens: &TokenSet, url: &str| content_credentials(&spec, tokens, &Url::parse(url).unwrap());

        assert_eq!(credentials(&acme, "https://acme.customers.example/files/1"), Some(true));
        assert_eq!(credentials(&acme, "https://login.acme.example/files/1"), Some(true));
        assert_eq!(
            credentials(&acme, "https://signed.acme.example/blob?sig=1"),
            Some(false),
            "a signed address is still not given the token"
        );
        assert_eq!(credentials(&acme, "https://globex.customers.example/files/1"), None);
        assert_eq!(
            credentials(&TokenSet::bearer("t"), "https://acme.customers.example/files/1"),
            None,
            "a rule gives nothing to a connection with no address of its own"
        );
    }

    #[test]
    fn a_path_is_resolved_against_the_connections_own_address() {
        let spec = per_customer();
        let acme = at("https://acme.customers.example/tenant-api");
        let resolved = |tokens: &TokenSet, path: &str| resolve_url(&spec, tokens, path).map(String::from);

        assert_eq!(
            resolved(&acme, "/things/1").unwrap(),
            "https://acme.customers.example/tenant-api/things/1"
        );
        assert_eq!(
            resolved(&TokenSet::bearer("t"), "things/1").unwrap(),
            "https://login.acme.example/api/things/1",
            "a connection with no address of its own uses the provider's"
        );
        let other = resolved(&acme, "https://globex.customers.example/tenant-api/things/1").unwrap_err();
        assert_eq!(other.kind(), ErrorKind::InvalidInput);
        let altered = resolved(&at("https://elsewhere.example/"), "things/1").unwrap_err();
        assert_eq!(altered.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn a_redirect_is_followed_within_the_connections_own_host_and_not_to_another_customers() {
        let spec = per_customer();
        let acme = at("https://acme.customers.example/tenant-api/");
        let asked = Url::parse("https://acme.customers.example/tenant-api/things").unwrap();
        let to = |location: &str| {
            let moved = response(302, &[("location", location)], Value::Null);
            redirect_target(&spec, &acme, &reqwest::Method::GET, &asked, &moved).map(String::from)
        };
        assert_eq!(
            to("/tenant-api/v2/things").as_deref(),
            Some("https://acme.customers.example/tenant-api/v2/things")
        );
        assert_eq!(to("https://globex.customers.example/tenant-api/things"), None);
        assert_eq!(to("blob:https://acme.customers.example/things"), None);
        assert_eq!(to("http://acme.customers.example/tenant-api/things"), None);
    }

    #[test]
    fn a_raw_request_reads_from_json_with_only_method_and_path() {
        let request: RawRequest = serde_json::from_value(json!({ "method": "GET", "path": "users/me" })).unwrap();
        assert_eq!(request, RawRequest::get("users/me"));
    }
}
