//! Socket integration for Zoom.
//!
//! Offers the provider definition, `zoom.identity.get` and
//! `zoom.resource.resolve` (a user's cloud recordings).

use async_trait::async_trait;
use serde_json::Value;
use socketkit_core::{
    Account, AuthScheme, ClientAuth, Connection, Error, ErrorKind, Integration, OAuth2Spec, OperationInfo, ProviderId,
    ProviderSpec, RawRequest, Resource, Result, identity_operation, resolve_input, resolve_operation, to_output,
};

/// This provider's id, as used in connection keys and operation names.
pub const PROVIDER_ID: &str = "zoom";

/// Zoom's definition: where its API lives and how it authenticates.
///
/// # Panics
/// Never in practice: the URLs are constants that parse.
pub fn provider() -> ProviderSpec {
    ProviderSpec {
        id: ProviderId::new(PROVIDER_ID).expect("a valid provider id"),
        display_name: "Zoom".into(),
        api_base: "https://api.zoom.us/v2/".parse().expect("a valid URL"),
        // zoom.us serves the OAuth token endpoint.
        allowed_hosts: vec!["api.zoom.us".into(), "zoom.us".into()],
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: "https://zoom.us/oauth/authorize".parse().expect("a valid URL"),
            token_url: "https://zoom.us/oauth/token".parse().expect("a valid URL"),
            default_scopes: vec![
                "user:read:user".into(),
                "recording:read:list_user_recordings:admin".into(),
                "recording:read:recording_token:admin".into(),
            ],
            scope_separator: " ".into(),
            pkce: false,
            client_auth: ClientAuth::Basic,
            extra_authorize_params: Vec::new(),
        }),
    }
}

/// Reads `me`, for the authorised account, or the email of a user in it.
pub fn parse_user(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("me") {
        return Ok("me".into());
    }
    let is_email = trimmed.matches('@').count() == 1
        && !trimmed.starts_with('@')
        && !trimmed.ends_with('@')
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-' | '+'));
    if is_email {
        Ok(trimmed.to_ascii_lowercase())
    } else {
        Err(Error::new(
            ErrorKind::InvalidInput,
            format!("\"{trimmed}\" is not a Zoom user; use \"me\" or the user's email"),
        ))
    }
}

/// The Zoom integration.
#[derive(Debug, Clone)]
pub struct Zoom {
    spec: ProviderSpec,
}

impl Default for Zoom {
    fn default() -> Self {
        Self::new()
    }
}

impl Zoom {
    pub fn new() -> Self {
        Self { spec: provider() }
    }

    /// Uses another definition, for a test server.
    pub fn with_spec(spec: ProviderSpec) -> Self {
        Self { spec }
    }

    /// The account the connection is authorised as. Needs the `user:read:user` scope.
    pub async fn identity(&self, connection: &Connection) -> Result<Account> {
        let body = connection.request(RawRequest::get("users/me")).await?.body;
        let Some(id) = body["id"].as_str().filter(|id| !id.is_empty()) else {
            return Err(
                Error::new(ErrorKind::Decode, "zoom answered without an account").with_provider(self.spec.id.clone())
            );
        };
        let email = body["email"].as_str().filter(|e| !e.is_empty());
        let full_name = [body["first_name"].as_str(), body["last_name"].as_str()]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let name = body["display_name"]
            .as_str()
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
            .or_else(|| (!full_name.is_empty()).then_some(full_name))
            .or_else(|| email.map(str::to_owned))
            .unwrap_or_else(|| id.to_owned());
        Ok(Account {
            id: id.to_owned(),
            name,
            email: email.map(str::to_owned),
        })
    }

    /// Confirms a user's cloud recordings can be read.
    ///
    /// Checked through the recordings list itself: that is what a Zoom
    /// connection reads, and it is the endpoint the recording scopes cover.
    pub async fn resolve(&self, connection: &Connection, input: &str) -> Result<Resource> {
        let user = parse_user(input).map_err(|e| e.with_provider(self.spec.id.clone()))?;
        // The `@` and `+` of an email are legal in a path segment; nothing else gets this far.
        let request = RawRequest::get(format!("users/{user}/recordings")).with_query("page_size", "1");
        match connection.request(request).await {
            Ok(_) => {
                let label = if user == "me" {
                    "My Zoom recordings".to_owned()
                } else {
                    user.clone()
                };
                Ok(Resource::new(user, label, "Zoom cloud recordings"))
            }
            // Zoom answers 400 (code 1001) or 404 for a user outside the account.
            Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::InvalidInput) => Err(Error::new(
                ErrorKind::NotFound,
                format!("Zoom user {user} was not found"),
            )
            .with_provider(self.spec.id.clone())),
            Err(e) => Err(e),
        }
    }
}

#[async_trait]
impl Integration for Zoom {
    fn provider(&self) -> ProviderSpec {
        self.spec.clone()
    }

    fn operations(&self) -> Vec<OperationInfo> {
        vec![
            identity_operation(&self.spec.id),
            resolve_operation(&self.spec.id, "\"me\" or the email of a user in the account"),
        ]
    }

    async fn invoke(&self, connection: Connection, operation: String, input: Value) -> Result<Value> {
        let id = &self.spec.id;
        match operation.strip_prefix(&format!("{id}.")) {
            Some("identity.get") => to_output(id, &self.identity(&connection).await?),
            Some("resource.resolve") => to_output(id, &self.resolve(&connection, &resolve_input(id, &input)?).await?),
            _ => Err(
                Error::new(ErrorKind::Unsupported, format!("zoom has no operation {operation:?}"))
                    .with_provider(id.clone()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_user_accepts_me_and_an_email() {
        assert_eq!(parse_user(" ME ").unwrap(), "me");
        assert_eq!(
            parse_user("Ada.Lovelace+zoom@Example.test").unwrap(),
            "ada.lovelace+zoom@example.test"
        );
    }

    #[test]
    fn parse_user_refuses_anything_that_could_change_the_path() {
        for bad in [
            "",
            "ada",
            "@example.test",
            "ada@",
            "a@b@c",
            "ada@example.test/../../accounts",
            "a b@c.test",
            "me/recordings",
        ] {
            assert_eq!(parse_user(bad).unwrap_err().kind(), ErrorKind::InvalidInput, "{bad:?}");
        }
    }
}
