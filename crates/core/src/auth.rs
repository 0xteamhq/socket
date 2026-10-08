//! The OAuth authorization-code flow: signed state, PKCE, the authorization
//! URL, and reading token responses.

use std::time::{Duration, SystemTime};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;

use crate::error::{Error, ErrorKind, Result};
use crate::provider::{OAuth2Spec, ProviderId};
use crate::secret::{SecretString, TokenSet};
use crate::store::ConnectionKey;

/// How long a person has to finish approving at the provider.
pub(crate) const STATE_LIFETIME: Duration = Duration::from_secs(600);
/// The shortest state-signing secret accepted.
pub(crate) const MIN_STATE_SECRET: usize = 32;

/// The application's own OAuth app at one provider.
#[derive(Debug, Clone)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_secret: SecretString,
    /// The application's callback route, exactly as registered with the provider.
    pub redirect_uri: Url,
}

/// What the application keeps, server-side, between sending a person to the
/// provider and receiving the callback.
///
/// Two rules are the application's to keep, because only it knows who is asking:
///
/// - **Tie the record to the session of the person who started the flow**, and
///   at the callback look it up by that session, never by the `state` value
///   in the URL. Otherwise someone can start a connection, send the link to a
///   victim, and have the victim's account stored under their own tenant.
/// - **Use it once.** Delete the record when the callback arrives. Socket
///   checks that the state is genuine and unexpired; it holds no memory of
///   which states were already used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingAuthorization {
    pub key: ConnectionKey,
    /// The signed `state` value that was put in the URL.
    pub state: String,
    /// Present when the provider uses PKCE. It never appears in the URL.
    pub pkce_verifier: Option<SecretString>,
}

/// The first half of a connection: where to send the person, and what to keep.
#[derive(Debug, Clone)]
pub struct Authorization {
    pub url: Url,
    pub pending: PendingAuthorization,
}

#[derive(Serialize, Deserialize)]
struct StatePayload {
    provider: String,
    tenant: String,
    nonce: String,
    /// Seconds since the Unix epoch at signing time.
    iat: u64,
}

fn epoch_secs(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn random_token(bytes: usize) -> Result<String> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer)
        .map_err(|e| Error::new(ErrorKind::Unexpected, format!("the system random source failed: {e}")))?;
    Ok(B64.encode(buffer))
}

fn mac(secret: &[u8], data: &[u8]) -> Hmac<Sha256> {
    // HMAC accepts a key of any length, so this cannot fail.
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(data);
    mac
}

/// Signs a `state` value that names the connection being made.
pub(crate) fn sign_state(key: &ConnectionKey, secret: &[u8], now: SystemTime) -> Result<String> {
    let payload = StatePayload {
        provider: key.provider.as_str().to_owned(),
        tenant: key.tenant.clone(),
        nonce: random_token(16)?,
        iat: epoch_secs(now),
    };
    let json = serde_json::to_vec(&payload)
        .map_err(|e| Error::new(ErrorKind::Unexpected, "could not encode the state").with_source(e))?;
    let data = B64.encode(json);
    let signature = B64.encode(mac(secret, data.as_bytes()).finalize().into_bytes());
    Ok(format!("{data}.{signature}"))
}

/// Checks a returned `state`: our signature, not expired, and for `expected`.
pub(crate) fn verify_state(state: &str, secret: &[u8], now: SystemTime, expected: &ConnectionKey) -> Result<()> {
    let invalid = |why: &str| {
        Error::new(ErrorKind::InvalidInput, format!("the authorization state {why}"))
            .with_provider(expected.provider.clone())
    };
    let (data, signature) = state.rsplit_once('.').ok_or_else(|| invalid("is malformed"))?;
    let signature = B64.decode(signature).map_err(|_| invalid("is malformed"))?;
    // `verify_slice` compares in constant time.
    mac(secret, data.as_bytes())
        .verify_slice(&signature)
        .map_err(|_| invalid("was not issued by this application"))?;
    let json = B64.decode(data).map_err(|_| invalid("is malformed"))?;
    let payload: StatePayload = serde_json::from_slice(&json).map_err(|_| invalid("is malformed"))?;
    let age = epoch_secs(now).saturating_sub(payload.iat);
    if age > STATE_LIFETIME.as_secs() || payload.iat > epoch_secs(now) + 60 {
        return Err(invalid("has expired; start the connection again"));
    }
    if payload.provider != expected.provider.as_str() || payload.tenant != expected.tenant {
        return Err(invalid("belongs to a different connection"));
    }
    Ok(())
}

/// A PKCE verifier and its S256 challenge.
pub(crate) fn pkce_pair() -> Result<(SecretString, String)> {
    let verifier = random_token(32)?;
    let challenge = B64.encode(Sha256::digest(verifier.as_bytes()));
    Ok((SecretString::new(verifier), challenge))
}

/// Builds the URL the person is sent to.
pub(crate) fn authorization_url(
    oauth: &OAuth2Spec,
    client: &OAuthClient,
    state: &str,
    scopes: &[String],
    pkce_challenge: Option<&str>,
) -> Url {
    let mut url = oauth.authorize_url.clone();
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("client_id", &client.client_id);
        query.append_pair("redirect_uri", client.redirect_uri.as_str());
        query.append_pair("response_type", "code");
        query.append_pair("state", state);
        for (name, value) in &oauth.extra_authorize_params {
            query.append_pair(name, value);
        }
        if !scopes.is_empty() {
            query.append_pair("scope", &scopes.join(&oauth.scope_separator));
        }
        if let Some(challenge) = pkce_challenge {
            query.append_pair("code_challenge", challenge);
            query.append_pair("code_challenge_method", "S256");
        }
    }
    url
}

/// The provider's refusal code when a token response is a refusal in disguise.
///
/// GitHub answers a bad code with HTTP 200 and an `error` field; Slack answers
/// with `"ok": false`. Both are the provider declining the grant.
pub(crate) fn grant_refusal(raw: &Value) -> Option<String> {
    if let Some(code) = raw.get("error").and_then(Value::as_str) {
        return Some(code.to_owned());
    }
    if raw.get("ok") == Some(&Value::Bool(false)) {
        return Some("refused".to_owned());
    }
    None
}

/// Reads a token response of the standard OAuth shape:
/// `{ access_token, refresh_token?, expires_in?, scope? }`.
///
/// Scopes are split on spaces and commas, since providers return either.
pub fn standard_token_response(provider: &ProviderId, raw: &Value, now: SystemTime) -> Result<TokenSet> {
    let access_token = raw["access_token"]
        .as_str()
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::Decode,
                format!("{provider} answered without an access token"),
            )
            .with_provider(provider.clone())
        })?;
    let expires_at = raw["expires_in"]
        .as_u64()
        .and_then(|secs| now.checked_add(Duration::from_secs(secs)));
    let scopes = raw["scope"]
        .as_str()
        .map(|scope| {
            scope
                .split([' ', ','])
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    Ok(TokenSet {
        access_token: SecretString::new(access_token),
        refresh_token: raw["refresh_token"]
            .as_str()
            .filter(|t| !t.is_empty())
            .map(SecretString::new),
        expires_at,
        scopes,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::provider::ClientAuth;

    const SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";

    fn key(provider: &str, tenant: &str) -> ConnectionKey {
        ConnectionKey::new(ProviderId::new(provider).unwrap(), tenant)
    }

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn a_signed_state_verifies_for_its_own_connection() {
        let state = sign_state(&key("slack", "acme"), SECRET, at(1_000)).unwrap();
        verify_state(&state, SECRET, at(1_000 + 599), &key("slack", "acme")).unwrap();
    }

    #[test]
    fn two_states_for_the_same_connection_differ() {
        let a = sign_state(&key("slack", "acme"), SECRET, at(1_000)).unwrap();
        let b = sign_state(&key("slack", "acme"), SECRET, at(1_000)).unwrap();
        assert_ne!(a, b, "a state cannot be predicted from an earlier one");
    }

    #[test]
    fn a_state_is_refused_when_tampered_expired_misdirected_or_signed_by_another_secret() {
        let ours = key("slack", "acme");
        let state = sign_state(&ours, SECRET, at(1_000)).unwrap();
        let message = |r: Result<()>| {
            let err = r.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput);
            err.message().to_owned()
        };

        let (data, signature) = state.rsplit_once('.').unwrap();
        let forged_payload = B64.encode(br#"{"provider":"slack","tenant":"victim","nonce":"n","iat":1000}"#);
        let forged = format!("{forged_payload}.{signature}");
        assert!(message(verify_state(&forged, SECRET, at(1_000), &key("slack", "victim"))).contains("not issued"));
        let cut = format!("{data}.{}", &signature[..signature.len() - 2]);
        assert!(verify_state(&cut, SECRET, at(1_000), &ours).is_err());

        assert!(
            message(verify_state(
                &state,
                b"another-secret-another-secret-00",
                at(1_000),
                &ours
            ))
            .contains("not issued")
        );
        assert!(message(verify_state(&state, SECRET, at(1_000 + 601), &ours)).contains("expired"));
        assert!(
            message(verify_state(&state, SECRET, at(100), &ours)).contains("expired"),
            "a state from the future"
        );
        assert!(message(verify_state(&state, SECRET, at(1_000), &key("slack", "globex"))).contains("different"));
        assert!(message(verify_state(&state, SECRET, at(1_000), &key("github", "acme"))).contains("different"));
        for junk in ["", "no-dot", ".", "a.b", "!!.!!"] {
            assert!(verify_state(junk, SECRET, at(1_000), &ours).is_err(), "{junk:?}");
        }
    }

    #[test]
    fn debug_output_of_the_oauth_types_never_shows_a_secret() {
        let client = OAuthClient {
            client_id: "public-id".into(),
            client_secret: SecretString::new("client-shh"),
            redirect_uri: Url::parse("https://app.example.test/cb").unwrap(),
        };
        let pending = PendingAuthorization {
            key: key("slack", "acme"),
            state: "state-value".into(),
            pkce_verifier: Some(SecretString::new("verifier-shh")),
        };
        let shown = format!("{client:?} {pending:?}");
        assert!(shown.contains("public-id") && shown.contains("state-value"));
        assert!(
            !shown.contains("client-shh") && !shown.contains("verifier-shh"),
            "{shown}"
        );
    }

    #[test]
    fn a_pkce_challenge_is_the_sha256_of_its_verifier() {
        let (verifier, challenge) = pkce_pair().unwrap();
        assert!(
            verifier.expose().len() >= 43,
            "RFC 7636 asks for at least 43 characters"
        );
        assert_eq!(challenge, B64.encode(Sha256::digest(verifier.expose().as_bytes())));
        assert_ne!(pkce_pair().unwrap().0, verifier);
    }

    #[test]
    fn the_authorization_url_carries_every_parameter_encoded() {
        let oauth = OAuth2Spec {
            authorize_url: Url::parse("https://accounts.example.test/authorize?prompt=login").unwrap(),
            token_url: Url::parse("https://accounts.example.test/token").unwrap(),
            default_scopes: Vec::new(),
            scope_separator: ",".into(),
            pkce: true,
            client_auth: ClientAuth::Body,
            extra_authorize_params: vec![("access_type".into(), "offline".into())],
        };
        let client = OAuthClient {
            client_id: "id 1".into(),
            client_secret: SecretString::new("shh"),
            redirect_uri: Url::parse("https://app.example.test/callback?x=1&y=2").unwrap(),
        };
        let url = authorization_url(
            &oauth,
            &client,
            "st.ate",
            &["chat:write".into(), "users:read".into()],
            Some("chal"),
        );
        let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        let get = |name: &str| pairs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
        assert_eq!(
            get("prompt"),
            Some("login"),
            "parameters already on the authorize URL survive"
        );
        assert_eq!(get("client_id"), Some("id 1"));
        assert_eq!(get("redirect_uri"), Some("https://app.example.test/callback?x=1&y=2"));
        assert_eq!(get("response_type"), Some("code"));
        assert_eq!(get("state"), Some("st.ate"));
        assert_eq!(get("access_type"), Some("offline"));
        assert_eq!(get("scope"), Some("chat:write,users:read"));
        assert_eq!(get("code_challenge"), Some("chal"));
        assert_eq!(get("code_challenge_method"), Some("S256"));
        assert!(!url.as_str().contains("shh"), "the client secret never goes in the URL");

        let bare = authorization_url(&oauth, &client, "s", &[], None);
        assert!(!bare.query_pairs().any(|(n, _)| n == "scope" || n == "code_challenge"));
    }

    #[test]
    fn a_standard_token_response_is_read_with_expiry_and_scopes() {
        let slack = ProviderId::new("slack").unwrap();
        let raw =
            json!({ "access_token": "a", "refresh_token": "r", "expires_in": 3600, "scope": "repo,read:user gist" });
        let tokens = standard_token_response(&slack, &raw, at(1_000)).unwrap();
        assert_eq!(tokens.access_token.expose(), "a");
        assert_eq!(tokens.refresh_token.as_ref().map(SecretString::expose), Some("r"));
        assert_eq!(tokens.expires_at, Some(at(4_600)));
        assert_eq!(tokens.scopes, ["repo", "read:user", "gist"]);

        let minimal = standard_token_response(&slack, &json!({ "access_token": "a" }), at(0)).unwrap();
        assert_eq!(
            (minimal.refresh_token, minimal.expires_at, minimal.scopes.len()),
            (None, None, 0)
        );
    }

    #[test]
    fn a_token_response_without_a_usable_access_token_is_a_decode_error() {
        let slack = ProviderId::new("slack").unwrap();
        for raw in [
            json!({}),
            json!({ "access_token": "" }),
            json!({ "access_token": 7 }),
            json!(null),
        ] {
            let err = standard_token_response(&slack, &raw, at(0)).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Decode, "{raw}");
        }
    }

    #[test]
    fn refusals_hidden_in_a_successful_response_are_recognised() {
        assert_eq!(
            grant_refusal(&json!({ "error": "bad_verification_code" })).as_deref(),
            Some("bad_verification_code")
        );
        assert_eq!(grant_refusal(&json!({ "ok": false })).as_deref(), Some("refused"));
        assert_eq!(
            grant_refusal(&json!({ "ok": false, "error": "invalid_code" })).as_deref(),
            Some("invalid_code")
        );
        assert_eq!(grant_refusal(&json!({ "access_token": "t", "ok": true })), None);
    }
}
