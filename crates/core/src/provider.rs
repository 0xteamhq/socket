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
    pub auth: AuthScheme,
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
    /// port, for example `"127.0.0.1:8080"`. Credentials sent there never
    /// leave the machine.
    pub fn allows_host(&self, url: &Url) -> bool {
        let Some(host) = url.host_str() else {
            return false;
        };
        let listed = |entry: &str| {
            self.allowed_hosts
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(entry))
        };
        match (url.scheme(), url.port_or_known_default()) {
            ("https", Some(443)) => listed(host),
            ("http", Some(port)) if is_loopback(url) => listed(&format!("{host}:{port}")),
            _ => false,
        }
    }

    /// Checks the rules a spec must meet before it is registered.
    pub fn validate(&self) -> Result<()> {
        let fail = |message: String| Err(Error::new(ErrorKind::Config, message).with_provider(self.id.clone()));
        if self.allowed_hosts.is_empty() {
            return fail(format!("provider {} has no allowed hosts", self.id));
        }
        if !self.allows_host(&self.api_base) {
            return fail(format!(
                "provider {} has an api_base outside its allowed hosts",
                self.id
            ));
        }
        if let AuthScheme::OAuth2(oauth) = &self.auth {
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
}
