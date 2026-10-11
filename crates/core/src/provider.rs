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
        self.allowed_hosts.iter().any(|allowed| names(allowed, url))
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

    /// Checks the rules a spec must meet before it is registered.
    pub fn validate(&self) -> Result<()> {
        let fail = |message: String| Err(Error::new(ErrorKind::Config, message).with_provider(self.id.clone()));
        if self.allowed_hosts.is_empty() {
            return fail(format!("provider {} has no allowed hosts", self.id));
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
