use std::fmt;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{Error, ErrorKind, Result};

/// A provider's identifier, such as `"slack"`.
///
/// Lowercase ASCII letters, digits, `-` and `_`; it must start with a letter.
/// It cannot contain `.`, which separates it from the rest of an operation name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        let starts_with_letter = id.chars().next().is_some_and(|c| c.is_ascii_lowercase());
        let all_allowed = id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        if starts_with_letter && all_allowed {
            Ok(Self(id))
        } else {
            Err(Error::new(ErrorKind::Config, format!("invalid provider id {id:?}")))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ProviderId {
    type Error = Error;

    fn try_from(id: String) -> Result<Self> {
        Self::new(id)
    }
}

impl From<ProviderId> for String {
    fn from(id: ProviderId) -> Self {
        id.0
    }
}

/// A service's identity and how it authenticates. Plain data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSpec {
    pub id: ProviderId,
    pub display_name: String,
    /// Overridable for self-hosted instances.
    pub api_base: Url,
    /// The only hosts that may receive this provider's credentials.
    ///
    /// An entry is a host, compared exactly: `"slack.com"`. An entry that
    /// starts with `*.` is a rule for a service that gives each customer
    /// their own host: `"*.my.salesforce.com"` admits any host under
    /// `my.salesforce.com`, and only as the API host of the connection whose
    /// authorisation named it. See [`ProviderSpec::allows_api_base`].
    pub allowed_hosts: Vec<String>,
    /// Other hosts this provider keeps content on: files, recordings, exports.
    /// Only a content request goes to them. See [`ContentHost`].
    #[serde(default)]
    pub content_hosts: Vec<ContentHost>,
    pub auth: AuthScheme,
}

/// A host a provider serves content from that is not its API.
///
/// A content request may be sent there, and a redirect there is followed.
/// No other request is: the API's own calls stay on `allowed_hosts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentHost {
    /// The host, written and matched as an entry of `allowed_hosts` is:
    /// the exact name, reached over https on port 443.
    pub host: String,
    /// Whether the connection's credential goes with a request to this host.
    ///
    /// True for a host that asks for the token, as Slack's file host does.
    /// False for a host reached by an address that is already signed, which
    /// must never be given the token as well.
    pub credentials: bool,
}

/// Where a customer's own name goes in the host of a definition's addresses,
/// for a service whose sign-in address is the customer's own:
/// `https://{tenant}.zendesk.com/oauth/authorizations/new`.
/// [`ProviderSpec::with_tenant`] fills it in.
pub const TENANT_PLACEHOLDER: &str = "{tenant}";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
// Boxing the larger variant would make every integration write `Box::new` for no gain:
// a spec is built once and shared behind an `Arc`.
#[allow(clippy::large_enum_variant)]
pub enum AuthScheme {
    #[serde(rename = "oauth2")]
    OAuth2(OAuth2Spec),
    ApiKey(ApiKeySpec),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OAuth2Spec {
    pub authorize_url: Url,
    pub token_url: Url,
    pub default_scopes: Vec<String>,
    /// Usually a space. Some providers use a comma.
    pub scope_separator: String,
    pub pkce: bool,
    /// How the client id and secret are sent to the token endpoint.
    #[serde(default)]
    pub client_auth: ClientAuth,
    /// Extra query parameters for the authorization URL, such as Google's `access_type=offline`.
    #[serde(default)]
    pub extra_authorize_params: Vec<(String, String)>,
}

/// How an OAuth client authenticates itself to the token endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientAuth {
    /// `client_id` and `client_secret` as form fields.
    #[default]
    Body,
    /// HTTP Basic auth.
    Basic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiKeySpec {
    pub placement: KeyPlacement,
}

/// Where an API key goes on a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "in", rename_all = "snake_case", deny_unknown_fields)]
pub enum KeyPlacement {
    Header {
        name: String,
        prefix: Option<String>,
    },
    Query {
        name: String,
    },
    /// Written `Basic {}` so that stray fields beside it are rejected when read from data.
    Basic {},
}

impl ProviderSpec {
    /// True when credentials for this provider may be sent to `url`.
    ///
    /// The scheme must be `https`, the port must be 443, and the host must
    /// equal an allowed host, compared without regard to case. A subdomain of
    /// an allowed host is not allowed.
    ///
    /// One exception serves local development and tests: plain `http` to a
    /// loopback address is allowed when the list names that address with its
    /// port, for example `"127.0.0.1:8080"`. Use it only for tests and local
    /// development: a configured HTTP proxy would still receive such a request.
    pub fn allows_host(&self, url: &Url) -> bool {
        // A rule is not a host. The URL parser accepts `*` in a host name, so
        // without this the text of a rule would itself be a host to call.
        self.allowed_hosts
            .iter()
            .filter(|allowed| !allowed.contains('*'))
            .any(|allowed| names(allowed, url))
    }

    /// Whether content may be fetched from `url`, and if it may, whether the
    /// credential goes with the request.
    ///
    /// `Some(true)` for the API's own hosts and for a content host marked to
    /// receive the credential, `Some(false)` for a content host marked not
    /// to, and `None` for anywhere else: nothing is sent there at all. A
    /// content host is matched by the same rule as [`ProviderSpec::allows_host`].
    pub fn content_credentials(&self, url: &Url) -> Option<bool> {
        if self.allows_host(url) {
            return Some(true);
        }
        self.content_hosts
            .iter()
            .find(|content| names(&content.host, url))
            .map(|content| content.credentials)
    }

    /// True when `base` may be the API address of one connection: the
    /// address its authorisation named, where each customer has their own.
    ///
    /// It is a host credentials may go to anyway, or an `https` host on port
    /// 443 that falls under one of the `*.` rules of the allowed hosts. It
    /// carries no username, password, query or fragment.
    ///
    /// A rule admits a host only for the connection that was given it. A
    /// request of another connection to that host is still refused, so one
    /// customer's token is never sent to another customer's address.
    pub fn allows_api_base(&self, base: &Url) -> bool {
        if !base.username().is_empty() || base.password().is_some() || base.query().is_some() {
            return false;
        }
        if base.fragment().is_some() {
            return false;
        }
        if self.allows_host(base) {
            return true;
        }
        let under_a_rule = |host: &str| self.allowed_hosts.iter().any(|entry| falls_under(entry, host));
        base.scheme() == "https"
            && base.port_or_known_default() == Some(443)
            && base.host_str().is_some_and(under_a_rule)
    }

    /// This definition for one customer: `tenant` written wherever a host
    /// holds [`TENANT_PLACEHOLDER`], in the API base, the sign-in addresses,
    /// the allowed hosts and the content hosts.
    ///
    /// The value becomes part of the address the client secret is sent to,
    /// so it has to be one name of a host and nothing else: letters, digits
    /// and hyphens, at most 63, with no hyphen first or last. Anything else
    /// is refused, and never reaches an address.
    pub fn with_tenant(mut self, tenant: &str) -> Result<Self> {
        let tenant = tenant.trim().to_ascii_lowercase();
        let config = |message: String| Error::new(ErrorKind::Config, message).with_provider(self.id.clone());
        if !is_label(&tenant) {
            // The value is not repeated: it is refused for what it holds.
            return Err(config(format!(
                "that is not a name {} can put in a host; use letters, digits and hyphens only",
                self.id
            )));
        }
        let fill = |url: &mut Url| -> Result<()> {
            let Some(host) = url.host_str().filter(|host| host.contains(TENANT_PLACEHOLDER)) else {
                return Ok(());
            };
            let host = host.replace(TENANT_PLACEHOLDER, &tenant);
            url.set_host(Some(&host))
                .map_err(|_| config(format!("provider {} has an address that cannot take a tenant", self.id)))
        };
        fill(&mut self.api_base)?;
        if let AuthScheme::OAuth2(oauth) = &mut self.auth {
            fill(&mut oauth.authorize_url)?;
            fill(&mut oauth.token_url)?;
        }
        for entry in &mut self.allowed_hosts {
            *entry = entry.replace(TENANT_PLACEHOLDER, &tenant);
        }
        for content in &mut self.content_hosts {
            content.host = content.host.replace(TENANT_PLACEHOLDER, &tenant);
        }
        Ok(self)
    }

    /// Checks the rules a spec must meet before it is registered.
    pub fn validate(&self) -> Result<()> {
        let fail = |message: String| Err(Error::new(ErrorKind::Config, message).with_provider(self.id.clone()));
        if self.allowed_hosts.is_empty() {
            return fail(format!("provider {} has no allowed hosts", self.id));
        }
        // A rule has to keep to one organisation's hosts. `*.com` would not.
        if let Some(rule) = self
            .allowed_hosts
            .iter()
            .find(|entry| entry.contains('*') && !is_host_rule(entry))
        {
            return fail(format!(
                "provider {} has the host rule {rule:?}; a rule is `*.` and then a domain of at least two names",
                self.id
            ));
        }
        let unfilled = |host: Option<&str>| host.is_some_and(|host| host.contains(TENANT_PLACEHOLDER));
        let sign_in = match &self.auth {
            AuthScheme::OAuth2(oauth) => vec![&oauth.authorize_url, &oauth.token_url],
            AuthScheme::ApiKey(_) => Vec::new(),
        };
        if unfilled(self.api_base.host_str())
            || sign_in.into_iter().any(|url| unfilled(url.host_str()))
            || self.allowed_hosts.iter().any(|entry| unfilled(Some(entry)))
            || self.content_hosts.iter().any(|content| unfilled(Some(&content.host)))
        {
            return fail(format!(
                "provider {} is defined for one customer at a time and names none; give it the customer's tenant",
                self.id
            ));
        }
        for (at, content) in self.content_hosts.iter().enumerate() {
            let host = content.host.as_str();
            if !is_host_entry(host) {
                return fail(format!(
                    "provider {} has a content host that is not a host name: {host:?}",
                    self.id
                ));
            }
            // Listed twice, a host would be given the credential by one entry
            // and denied it by the other.
            let again = self.content_hosts[..at]
                .iter()
                .map(|earlier| &earlier.host)
                .chain(&self.allowed_hosts)
                .any(|other| other.eq_ignore_ascii_case(host));
            if again {
                return fail(format!("provider {} lists the host {host:?} more than once", self.id));
            }
        }
        let carries_credentials = |url: &Url| !url.username().is_empty() || url.password().is_some();
        let oauth_urls = match &self.auth {
            AuthScheme::OAuth2(oauth) => vec![&oauth.authorize_url, &oauth.token_url],
            AuthScheme::ApiKey(_) => Vec::new(),
        };
        if carries_credentials(&self.api_base) || oauth_urls.into_iter().any(carries_credentials) {
            return fail(format!(
                "provider {} has a URL that carries a username or password",
                self.id
            ));
        }
        if !self.allows_host(&self.api_base) {
            return fail(format!(
                "provider {} has an api_base outside its allowed hosts",
                self.id
            ));
        }
        if let AuthScheme::OAuth2(oauth) = &self.auth {
            // Socket writes these itself. A second copy from the definition
            // would make the request ambiguous, or override the state check.
            const WRITTEN_BY_SOCKET: [&str; 7] = [
                "client_id",
                "redirect_uri",
                "response_type",
                "state",
                "scope",
                "code_challenge",
                "code_challenge_method",
            ];
            let own_query = oauth.authorize_url.query_pairs().map(|(name, _)| name.into_owned());
            let extra = oauth.extra_authorize_params.iter().map(|(name, _)| name.clone());
            if let Some(name) = own_query
                .chain(extra)
                .find(|name| WRITTEN_BY_SOCKET.contains(&name.as_str()))
            {
                return fail(format!(
                    "provider {} sets the OAuth parameter {name:?}, which Socket writes itself",
                    self.id
                ));
            }
            // The user's browser visits the authorize URL; no credential is sent there.
            if oauth.authorize_url.scheme() != "https" && !self.allows_host(&oauth.authorize_url) {
                return fail(format!("provider {} has an authorize_url that is not https", self.id));
            }
            // The token endpoint receives the client secret and refresh tokens.
            if !self.allows_host(&oauth.token_url) {
                return fail(format!(
                    "provider {} has a token_url outside its allowed hosts",
                    self.id
                ));
            }
        }
        Ok(())
    }
}

/// True when `entry`, a line of a host list, names the host `url` is on.
fn names(entry: &str, url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    match (url.scheme(), url.port_or_known_default()) {
        ("https", Some(443)) => entry.eq_ignore_ascii_case(host),
        ("http", Some(port)) if is_loopback(url) => entry.eq_ignore_ascii_case(&format!("{host}:{port}")),
        _ => false,
    }
}

/// True when `entry` is written as [`names`] will match it: a host with no
/// port, or a loopback address with one. Not a URL, a path or a pattern, and
/// not a public host with a port, which would be accepted here and then
/// match nothing.
fn is_host_entry(entry: &str) -> bool {
    let plain = !entry.is_empty()
        && entry
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'));
    let read_as = |scheme: &str| Url::parse(&format!("{scheme}://{entry}/")).ok();
    let a_host =
        read_as("https").is_some_and(|url| url.host_str().is_some_and(|host| entry.eq_ignore_ascii_case(host)));
    let a_loopback_port = read_as("http").is_some_and(|url| {
        let with_port = url
            .host_str()
            .zip(url.port_or_known_default())
            .map(|(host, port)| format!("{host}:{port}"));
        is_loopback(&url) && with_port.is_some_and(|written| entry.eq_ignore_ascii_case(&written))
    });
    plain && (a_host || a_loopback_port)
}

/// True for one name of a host: letters, digits and hyphens, at most 63,
/// with no hyphen first or last.
fn is_label(label: &str) -> bool {
    (1..=63).contains(&label.len())
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// The domain a `*.` entry of the allowed hosts covers, if `entry` is one.
fn rule_domain(entry: &str) -> Option<&str> {
    entry.strip_prefix("*.")
}

/// True when `entry` is a well-formed rule: `*.` and then a domain of at
/// least two names.
fn is_host_rule(entry: &str) -> bool {
    rule_domain(entry).is_some_and(|domain| domain.split('.').count() >= 2 && domain.split('.').all(is_label))
}

/// True when `host` is under the domain of the rule `entry`, by one name or more.
fn falls_under(entry: &str, host: &str) -> bool {
    let Some(domain) = rule_domain(entry).filter(|_| is_host_rule(entry)) else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    let own = host
        .strip_suffix(&domain.to_ascii_lowercase())
        .and_then(|rest| rest.strip_suffix('.'));
    own.is_some_and(|own| own.split('.').all(is_label))
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slack() -> ProviderSpec {
        ProviderSpec {
            id: ProviderId::new("slack").unwrap(),
            display_name: "Slack".into(),
            api_base: Url::parse("https://slack.com/api/").unwrap(),
            allowed_hosts: vec!["slack.com".into()],
            content_hosts: Vec::new(),
            auth: AuthScheme::OAuth2(OAuth2Spec {
                authorize_url: Url::parse("https://slack.com/oauth/v2/authorize").unwrap(),
                token_url: Url::parse("https://slack.com/api/oauth.v2.access").unwrap(),
                default_scopes: vec!["chat:write".into()],
                scope_separator: ",".into(),
                pkce: false,
                client_auth: ClientAuth::Body,
                extra_authorize_params: Vec::new(),
            }),
        }
    }

    #[test]
    fn provider_ids_are_lowercase_and_cannot_contain_a_dot() {
        assert!(ProviderId::new("slack").is_ok());
        assert!(ProviderId::new("google-drive_2").is_ok());
        for bad in ["", "Slack", "slack.chat", "2fa", "sla ck", "slack/", "-slack"] {
            let err = ProviderId::new(bad).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Config, "{bad:?}");
        }
    }

    #[test]
    fn a_provider_id_is_validated_when_read_from_data() {
        assert!(serde_json::from_str::<ProviderId>("\"github\"").is_ok());
        assert!(serde_json::from_str::<ProviderId>("\"Git Hub\"").is_err());
    }

    #[test]
    fn a_spec_round_trips_through_json() {
        let spec = slack();
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["auth"]["type"], "oauth2");
        let back: ProviderSpec = serde_json::from_value(json).unwrap();
        assert_eq!(back, spec);
    }

    #[test]
    fn an_api_key_spec_reads_from_data() {
        let spec: ProviderSpec = serde_json::from_str(
            r#"{
                "id": "sendgrid",
                "display_name": "SendGrid",
                "api_base": "https://api.sendgrid.com/v3/",
                "allowed_hosts": ["api.sendgrid.com"],
                "auth": { "type": "api_key", "placement": { "in": "header", "name": "Authorization", "prefix": "Bearer " } }
            }"#,
        )
        .unwrap();
        assert_eq!(
            spec.auth,
            AuthScheme::ApiKey(ApiKeySpec {
                placement: KeyPlacement::Header {
                    name: "Authorization".into(),
                    prefix: Some("Bearer ".into())
                }
            })
        );
        spec.validate().unwrap();
    }

    #[test]
    fn a_spec_with_a_misspelt_field_or_a_bad_url_is_rejected() {
        let misspelt = r#"{"id":"x","display_name":"X","api_base":"https://x.test/","allowed_host":["x.test"],"auth":{"type":"api_key","placement":{"in":"basic"}}}"#;
        assert!(serde_json::from_str::<ProviderSpec>(misspelt).is_err());
        let bad_url = r#"{"id":"x","display_name":"X","api_base":"not a url","allowed_hosts":["x.test"],"auth":{"type":"api_key","placement":{"in":"basic"}}}"#;
        assert!(serde_json::from_str::<ProviderSpec>(bad_url).is_err());
    }

    #[test]
    fn credentials_go_only_to_an_exact_allowed_host_over_https() {
        let spec = slack();
        let allows = |u: &str| spec.allows_host(&Url::parse(u).unwrap());
        assert!(allows("https://slack.com/api/chat.postMessage"));
        assert!(allows("https://SLACK.com/api/"), "host comparison ignores case");
        assert!(allows("https://slack.com:443/api/"));
        assert!(
            !allows("https://slack.com:8443/api/"),
            "another port is another service"
        );
        assert!(!allows("http://slack.com/api/"), "plain http is refused");
        assert!(!allows("https://slack.com.evil.test/api/"));
        assert!(!allows("https://evil-slack.com/"));
        assert!(!allows("https://files.slack.com/"), "a subdomain must be listed itself");
        assert!(
            !allows("https://slack.com@evil.test/"),
            "userinfo does not change the host"
        );
    }

    #[test]
    fn plain_http_is_allowed_only_to_a_loopback_address_listed_with_its_port() {
        let mut spec = slack();
        spec.allowed_hosts = vec!["slack.com".into(), "127.0.0.1:8080".into(), "localhost:9000".into()];
        let allows = |u: &str| spec.allows_host(&Url::parse(u).unwrap());
        assert!(allows("http://127.0.0.1:8080/api/"));
        assert!(allows("http://localhost:9000/"));
        assert!(!allows("http://127.0.0.1:8081/"), "another port is another program");
        assert!(!allows("http://127.0.0.1/"), "the port must be listed");
        assert!(
            !allows("http://slack.com/"),
            "listing a public host never allows plain http"
        );
        assert!(!allows("https://127.0.0.1:8080/"), "the exception is for http only");

        spec.allowed_hosts = vec!["10.0.0.5:8080".into(), "evil.test:80".into()];
        let allows = |u: &str| spec.allows_host(&Url::parse(u).unwrap());
        assert!(!allows("http://10.0.0.5:8080/"), "a private address is not loopback");
        assert!(
            !allows("http://evil.test/"),
            "a public host with a port entry is still refused over http"
        );
    }

    #[test]
    fn oauth_settings_have_defaults_when_read_from_data() {
        let oauth: OAuth2Spec = serde_json::from_str(
            r#"{"authorize_url":"https://x.test/a","token_url":"https://x.test/t","default_scopes":[],"scope_separator":" ","pkce":true}"#,
        )
        .unwrap();
        assert_eq!(oauth.client_auth, ClientAuth::Body);
        assert!(oauth.extra_authorize_params.is_empty());
        let basic: ClientAuth = serde_json::from_str(r#""basic""#).unwrap();
        assert_eq!(basic, ClientAuth::Basic);
    }

    #[test]
    fn an_allowed_host_entry_matches_without_regard_to_its_own_case() {
        let mut spec = slack();
        spec.allowed_hosts = vec!["Slack.COM".into()];
        assert!(spec.allows_host(&Url::parse("https://slack.com/api/").unwrap()));
    }

    #[test]
    fn unknown_fields_are_rejected_at_every_level_of_a_spec() {
        let with = |extra_top: &str, placement: &str| {
            format!(
                r#"{{"id":"x","display_name":"X","api_base":"https://x.test/","allowed_hosts":["x.test"]{extra_top},"auth":{{"type":"api_key","placement":{placement}}}}}"#
            )
        };
        let ok = with("", r#"{"in":"basic"}"#);
        assert!(serde_json::from_str::<ProviderSpec>(&ok).is_ok());
        for bad in [
            with(r#","extra":1"#, r#"{"in":"basic"}"#),
            with("", r#"{"in":"header","name":"A","prefx":"B "}"#),
            with("", r#"{"in":"basic","name":"X-Api-Key"}"#),
        ] {
            assert!(serde_json::from_str::<ProviderSpec>(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_token_endpoint_must_be_an_allowed_host_and_both_oauth_endpoints_https() {
        let with = |authorize: &str, token: &str| {
            let mut spec = slack();
            if let AuthScheme::OAuth2(oauth) = &mut spec.auth {
                oauth.authorize_url = Url::parse(authorize).unwrap();
                oauth.token_url = Url::parse(token).unwrap();
            }
            spec.validate()
        };
        let authorize = "https://slack.com/oauth/v2/authorize";
        let token = "https://slack.com/api/oauth.v2.access";
        with(authorize, token).unwrap();
        with("https://login.elsewhere.test/authorize", token).unwrap();
        for (a, t) in [
            ("http://slack.com/oauth/v2/authorize", token),
            (authorize, "https://evil.test/token"),
            (authorize, "https://slack.com:8443/token"),
        ] {
            assert_eq!(with(a, t).unwrap_err().kind(), ErrorKind::Config, "{a} {t}");
        }
    }

    #[test]
    fn a_definition_cannot_set_the_oauth_parameters_socket_writes() {
        for reserved in [
            "state",
            "redirect_uri",
            "client_id",
            "response_type",
            "scope",
            "code_challenge",
            "code_challenge_method",
        ] {
            let mut extra = slack();
            if let AuthScheme::OAuth2(oauth) = &mut extra.auth {
                oauth.extra_authorize_params.push((reserved.into(), "x".into()));
            }
            assert_eq!(
                extra.validate().unwrap_err().kind(),
                ErrorKind::Config,
                "{reserved} as an extra parameter"
            );

            let mut in_url = slack();
            if let AuthScheme::OAuth2(oauth) = &mut in_url.auth {
                oauth.authorize_url =
                    Url::parse(&format!("https://slack.com/oauth/v2/authorize?{reserved}=x")).unwrap();
            }
            assert_eq!(
                in_url.validate().unwrap_err().kind(),
                ErrorKind::Config,
                "{reserved} on the authorize URL"
            );
        }
        let mut fine = slack();
        if let AuthScheme::OAuth2(oauth) = &mut fine.auth {
            oauth
                .extra_authorize_params
                .push(("access_type".into(), "offline".into()));
        }
        fine.validate().unwrap();
    }

    #[test]
    fn a_definition_whose_urls_carry_credentials_is_invalid() {
        let mut base = slack();
        base.api_base = Url::parse("https://user:pw@slack.com/api/").unwrap();
        assert_eq!(base.validate().unwrap_err().kind(), ErrorKind::Config);
        let mut token = slack();
        if let AuthScheme::OAuth2(oauth) = &mut token.auth {
            oauth.token_url = Url::parse("https://user@slack.com/api/oauth.v2.access").unwrap();
        }
        assert_eq!(token.validate().unwrap_err().kind(), ErrorKind::Config);
    }

    fn per_customer() -> ProviderSpec {
        let mut spec = slack();
        spec.allowed_hosts = vec!["slack.com".into(), "*.my.salesforce.com".into()];
        spec
    }

    #[test]
    fn a_host_rule_admits_a_customers_host_as_an_api_base_and_never_as_a_listed_host() {
        let spec = per_customer();
        spec.validate().unwrap();
        let base = |u: &str| spec.allows_api_base(&Url::parse(u).unwrap());
        assert!(base("https://acme.my.salesforce.com/services/data/v62.0/"));
        assert!(base("https://ACME.My.Salesforce.com/"), "host comparison ignores case");
        assert!(
            base("https://acme--dev.sandbox.my.salesforce.com/"),
            "a host may be more than one name under the rule"
        );
        assert!(base("https://slack.com/api/"), "a listed host is an API base too");
        for outside in [
            "https://my.salesforce.com/",
            "https://evilmy.salesforce.com/",
            "https://acme.my.salesforce.com.evil.test/",
            "https://salesforce.com/",
            "http://acme.my.salesforce.com/",
            "https://acme.my.salesforce.com:8443/",
            "https://user@acme.my.salesforce.com/",
            "https://user:pw@acme.my.salesforce.com/",
            "https://acme.my.salesforce.com/?to=elsewhere",
            "https://acme.my.salesforce.com/#fragment",
            "https://slack.com@acme.my.salesforce.com/",
            "https://*.my.salesforce.com/",
            "https://acme.*.my.salesforce.com/",
            "https://acme.my.salesforce.com./",
        ] {
            assert!(!base(outside), "{outside}");
        }
        assert!(
            !spec.allows_host(&Url::parse("https://*.my.salesforce.com/").unwrap()),
            "the text of a rule is not itself a host"
        );
        assert!(
            !spec.allows_host(&Url::parse("https://acme.my.salesforce.com/").unwrap()),
            "a rule does not make a customer's host one that any connection may call"
        );
    }

    #[test]
    fn a_name_under_a_rule_is_made_of_whole_host_names() {
        assert!(falls_under("*.pipedrive.com", "acme.pipedrive.com"));
        assert!(falls_under("*.PipeDrive.com", "acme.pipedrive.com"));
        for (rule, host) in [
            ("*.pipedrive.com", "pipedrive.com"),
            ("*.pipedrive.com", ".pipedrive.com"),
            ("*.pipedrive.com", "a..pipedrive.com"),
            ("*.pipedrive.com", "-a.pipedrive.com"),
            ("*.pipedrive.com", "a_b.pipedrive.com"),
            ("*.pipedrive.com", "acmepipedrive.com"),
            ("pipedrive.com", "acme.pipedrive.com"),
            ("*.com", "pipedrive.com"),
        ] {
            assert!(!falls_under(rule, host), "{rule} {host}");
        }
    }

    #[test]
    fn a_host_rule_must_name_a_domain_of_its_own() {
        for rule in [
            "*",
            "*.",
            "*.com",
            "*.a..com",
            "a.*.com",
            "*x.example.com",
            "**.example.com",
            "*.example.com:443",
        ] {
            let mut spec = slack();
            spec.allowed_hosts.push(rule.into());
            let err = spec.validate().unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Config, "{rule}");
        }
        let mut spec = slack();
        spec.allowed_hosts.push("*.pipedrive.com".into());
        spec.validate().unwrap();

        // A rule admits a customer's API host and nothing else: the token
        // endpoint and the definition's own base have to be listed exactly.
        let mut base_on_a_rule = per_customer();
        base_on_a_rule.api_base = Url::parse("https://*.my.salesforce.com/").unwrap();
        assert_eq!(base_on_a_rule.validate().unwrap_err().kind(), ErrorKind::Config);
        let mut token_on_a_rule = per_customer();
        if let AuthScheme::OAuth2(oauth) = &mut token_on_a_rule.auth {
            oauth.token_url = Url::parse("https://acme.my.salesforce.com/token").unwrap();
        }
        assert_eq!(token_on_a_rule.validate().unwrap_err().kind(), ErrorKind::Config);
    }

    #[test]
    fn a_loopback_address_listed_with_its_port_may_be_a_connections_api_base() {
        let mut spec = slack();
        spec.allowed_hosts.push("127.0.0.1:8080".into());
        assert!(spec.allows_api_base(&Url::parse("http://127.0.0.1:8080/api/").unwrap()));
        assert!(!spec.allows_api_base(&Url::parse("http://127.0.0.1:8081/api/").unwrap()));
    }

    fn for_one_customer() -> ProviderSpec {
        ProviderSpec {
            id: ProviderId::new("zendesk").unwrap(),
            display_name: "Zendesk".into(),
            api_base: Url::parse("https://{tenant}.zendesk.com/api/v2/").unwrap(),
            allowed_hosts: vec!["{tenant}.zendesk.com".into()],
            content_hosts: Vec::new(),
            auth: AuthScheme::OAuth2(OAuth2Spec {
                authorize_url: Url::parse("https://{tenant}.zendesk.com/oauth/authorizations/new").unwrap(),
                token_url: Url::parse("https://{tenant}.zendesk.com/oauth/tokens").unwrap(),
                default_scopes: vec!["read".into()],
                scope_separator: " ".into(),
                pkce: false,
                client_auth: ClientAuth::Body,
                extra_authorize_params: Vec::new(),
            }),
        }
    }

    #[test]
    fn a_tenant_fills_every_host_of_a_definition_written_for_one_customer() {
        let unfilled = for_one_customer().validate().unwrap_err();
        assert_eq!(unfilled.kind(), ErrorKind::Config);

        let spec = for_one_customer().with_tenant(" Acme-Support ").unwrap();
        spec.validate().unwrap();
        assert_eq!(spec.api_base.as_str(), "https://acme-support.zendesk.com/api/v2/");
        assert_eq!(spec.allowed_hosts, ["acme-support.zendesk.com"]);
        let AuthScheme::OAuth2(oauth) = &spec.auth else {
            panic!("the scheme is kept")
        };
        assert_eq!(
            oauth.authorize_url.as_str(),
            "https://acme-support.zendesk.com/oauth/authorizations/new"
        );
        assert_eq!(
            oauth.token_url.as_str(),
            "https://acme-support.zendesk.com/oauth/tokens"
        );

        // A definition with no placeholder is left as it was.
        assert_eq!(slack().with_tenant("acme").unwrap(), slack());
    }

    #[test]
    fn a_tenant_that_is_not_one_plain_host_name_is_refused_and_not_repeated() {
        let long = "a".repeat(64);
        for bad in [
            "",
            " ",
            "acme.evil",
            "acme/evil",
            "acme evil",
            "acme?x",
            "acme#x",
            "acme@evil.test",
            "acme:8443",
            "-acme",
            "acme-",
            "acme_support",
            "acme%2eevil",
            "{tenant}",
            "evil.test/",
            long.as_str(),
        ] {
            let err = for_one_customer().with_tenant(bad).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Config, "{bad:?}");
            assert!(
                bad.trim().is_empty() || !err.message().contains(bad.trim()),
                "{}",
                err.message()
            );
        }
        assert!(for_one_customer().with_tenant(&"a".repeat(63)).is_ok());
    }

    #[test]
    fn validate_rejects_specs_that_could_leak_or_never_work() {
        slack().validate().unwrap();

        let mut no_hosts = slack();
        no_hosts.allowed_hosts.clear();
        assert_eq!(no_hosts.validate().unwrap_err().kind(), ErrorKind::Config);

        let mut base_elsewhere = slack();
        base_elsewhere.api_base = Url::parse("https://example.test/").unwrap();
        assert_eq!(base_elsewhere.validate().unwrap_err().kind(), ErrorKind::Config);

        let mut http_token_url = slack();
        if let AuthScheme::OAuth2(oauth) = &mut http_token_url.auth {
            oauth.token_url = Url::parse("http://slack.com/api/oauth.v2.access").unwrap();
        }
        let err = http_token_url.validate().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Config);
        assert_eq!(err.provider().map(ProviderId::as_str), Some("slack"));
    }

    fn content(host: &str, credentials: bool) -> ContentHost {
        ContentHost {
            host: host.into(),
            credentials,
        }
    }

    #[test]
    fn content_is_fetched_only_from_a_declared_host_and_each_says_whether_it_gets_the_credential() {
        let mut spec = slack();
        spec.content_hosts = vec![content("files.slack.com", true), content("Signed.Example.test", false)];
        spec.validate().unwrap();
        let credentials = |u: &str| spec.content_credentials(&Url::parse(u).unwrap());
        assert_eq!(
            credentials("https://slack.com/api/files.info"),
            Some(true),
            "the API itself"
        );
        assert_eq!(credentials("https://files.slack.com/files-pri/T1-F1/a.pdf"), Some(true));
        assert_eq!(credentials("https://signed.example.test/blob?sig=1"), Some(false));
        for elsewhere in [
            "https://evil.test/blob",
            "http://files.slack.com/a.pdf",
            "https://files.slack.com:8443/a.pdf",
            "https://cdn.files.slack.com/a.pdf",
            "https://files.slack.com.evil.test/a.pdf",
            "https://signed.example.test.evil.test/blob",
        ] {
            assert_eq!(credentials(elsewhere), None, "{elsewhere}");
        }
        // A content host is not an API host: an ordinary request never goes there.
        assert!(!spec.allows_host(&Url::parse("https://files.slack.com/a.pdf").unwrap()));
    }

    #[test]
    fn a_definition_without_content_hosts_reads_as_one_with_none() {
        let json = serde_json::to_value(slack()).unwrap();
        let mut without = json.clone();
        without.as_object_mut().unwrap().remove("content_hosts");
        let spec: ProviderSpec = serde_json::from_value(without).unwrap();
        assert!(spec.content_hosts.is_empty());

        let mut with = json;
        with["content_hosts"] = serde_json::json!([{ "host": "files.slack.com", "credentials": true }]);
        let spec: ProviderSpec = serde_json::from_value(with.clone()).unwrap();
        assert_eq!(spec.content_hosts, vec![content("files.slack.com", true)]);
        // Whether a host gets the credential is never left to a default.
        for bad in [
            serde_json::json!([{ "host": "files.slack.com" }]),
            serde_json::json!([{ "host": "files.slack.com", "credentials": true, "extra": 1 }]),
            serde_json::json!(["files.slack.com"]),
        ] {
            with["content_hosts"] = bad.clone();
            assert!(serde_json::from_value::<ProviderSpec>(with.clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_content_host_must_be_a_host_and_be_listed_once() {
        let with = |hosts: Vec<ContentHost>| {
            let mut spec = slack();
            spec.content_hosts = hosts;
            spec.validate()
        };
        with(vec![
            content("files.slack.com", true),
            content("127.0.0.1:9000", false),
            content("LocalHost:8080", false),
            content("[::1]:9000", true),
        ])
        .unwrap();
        for bad in [
            vec![content("", false)],
            vec![content("https://files.slack.com", true)],
            vec![content("files.slack.com/files", true)],
            vec![content("*.slack.com", false)],
            vec![content("user@files.slack.com", true)],
            vec![content("files slack.com", true)],
            // A port is for a loopback address. On any other host the entry would match nothing.
            vec![content("files.slack.com:443", true)],
            vec![content("files.slack.com:8443", true)],
            vec![content("10.0.0.5:8080", false)],
            vec![content("127.0.0.1:", false)],
            vec![content(":9000", false)],
            // Named twice, or named as an API host too, one entry would contradict the other.
            vec![content("files.slack.com", true), content("FILES.slack.com", false)],
            vec![content("Slack.com", false)],
        ] {
            let err = with(bad.clone()).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Config, "{bad:?}");
            assert_eq!(err.provider().map(ProviderId::as_str), Some("slack"));
        }
    }
}
