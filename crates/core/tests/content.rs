//! Fetching content a provider points to: a file, a recording, an export.
//! Against local servers that play the provider's API and the hosts it keeps content on.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::json;
use socketkit_core::{
    ApiKeySpec, AuthScheme, ClientAuth, ConnectionKey, Content, ContentHost, ContentRequest, ErrorKind, KeyPlacement,
    MemoryTokenStore, OAuth2Spec, OAuthClient, ProviderId, ProviderSpec, Retry, RetryPolicy, SecretString, Socket,
    TokenSet, TokenStore,
};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Bytes that are not text: reading them as UTF-8 would change them.
const BYTES: &[u8] = &[0x00, 0x9f, 0x92, 0x96, 0xff, 0xfe, 0x0d, 0x0a, 0x80, 0x00];

fn host_entry(server: &MockServer) -> String {
    let url = url::Url::parse(&server.uri()).unwrap();
    format!("{}:{}", url.host_str().unwrap(), url.port().unwrap())
}

/// A provider whose API is `api`, and which keeps content on `content`:
/// each host with whether it is to receive the credential.
fn spec(api: &MockServer, content: &[(&MockServer, bool)]) -> ProviderSpec {
    ProviderSpec {
        id: ProviderId::new("acme").unwrap(),
        display_name: "Acme".into(),
        api_base: format!("{}/api", api.uri()).parse().unwrap(),
        allowed_hosts: vec![host_entry(api)],
        content_hosts: content
            .iter()
            .map(|(server, credentials)| ContentHost {
                host: host_entry(server),
                credentials: *credentials,
            })
            .collect(),
        auth: AuthScheme::OAuth2(OAuth2Spec {
            authorize_url: format!("{}/authorize", api.uri()).parse().unwrap(),
            token_url: format!("{}/token", api.uri()).parse().unwrap(),
            default_scopes: vec!["read".into()],
            scope_separator: " ".into(),
            pkce: false,
            client_auth: ClientAuth::Body,
            extra_authorize_params: Vec::new(),
        }),
    }
}

fn key() -> ConnectionKey {
    ConnectionKey::new(ProviderId::new("acme").unwrap(), "tenant-1")
}

fn fast_retry() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 2,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(50),
    }
}

async fn connected_with(spec: ProviderSpec, tokens: TokenSet) -> (Socket, Arc<MemoryTokenStore>) {
    let store = Arc::new(MemoryTokenStore::new());
    store.save(key(), tokens).await.unwrap();
    let mut builder = Socket::builder(store.clone())
        .provider(spec.clone())
        .retry(fast_retry());
    if matches!(spec.auth, AuthScheme::OAuth2(_)) {
        builder = builder.oauth_client(
            spec.id.clone(),
            OAuthClient {
                client_id: "client-id".into(),
                client_secret: SecretString::new("client-secret"),
                redirect_uri: "https://app.example.test/callback".parse().unwrap(),
            },
        );
    }
    (builder.build().unwrap(), store)
}

async fn connected(spec: ProviderSpec) -> Socket {
    connected_with(spec, TokenSet::bearer("the-token")).await.0
}

fn file() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(BYTES, "application/octet-stream")
}

fn authorization(request: &wiremock::Request) -> Option<String> {
    request
        .headers
        .get("authorization")
        .map(|value| value.to_str().unwrap().to_owned())
}

// ── Where content may be fetched from, and what goes with the request ───────

#[tokio::test]
async fn a_file_on_the_api_host_comes_back_byte_for_byte() {
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/files/1/content"))
        .and(query_param("alt", "media"))
        .and(header("authorization", "Bearer the-token"))
        .respond_with(file())
        .expect(1)
        .mount(&api)
        .await;
    let socket = connected(spec(&api, &[])).await;
    let request = ContentRequest::get("files/1/content").with_query("alt", "media");
    let content: Content = socket.fetch(key(), request).await.unwrap();
    assert_eq!(content.bytes, BYTES, "nothing was read as text on the way");
    assert_eq!(content.content_type.as_deref(), Some("application/octet-stream"));
    assert_eq!(content.len(), BYTES.len());
}

#[tokio::test]
async fn a_declared_content_host_gets_the_credential_only_when_it_is_marked_to() {
    let api = MockServer::start().await;
    let files = MockServer::start().await;
    let signed = MockServer::start().await;
    for server in [&files, &signed] {
        Mock::given(path("/blob")).respond_with(file()).mount(server).await;
    }
    let socket = connected(spec(&api, &[(&files, true), (&signed, false)])).await;

    // A host that needs the token, as Slack's file host does.
    let content = socket
        .fetch(key(), ContentRequest::get(format!("{}/blob", files.uri())))
        .await
        .unwrap();
    assert_eq!(content.bytes, BYTES);
    let received = files.received_requests().await.unwrap();
    assert_eq!(authorization(&received[0]).as_deref(), Some("Bearer the-token"));

    // A host that is reached by a signed address, and must never see the token.
    let content = socket
        .fetch(key(), ContentRequest::get(format!("{}/blob?sig=abc", signed.uri())))
        .await
        .unwrap();
    assert_eq!(content.bytes, BYTES);
    let received = signed.received_requests().await.unwrap();
    assert_eq!(authorization(&received[0]), None);
    assert_eq!(
        received[0].url.query(),
        Some("sig=abc"),
        "the signed address is used as it is"
    );
    assert!(api.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_redirect_is_followed_to_a_declared_host_and_the_credential_stays_behind() {
    let api = MockServer::start().await;
    let signed = MockServer::start().await;
    Mock::given(path("/api/recordings/1/content"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", format!("{}/blob?sig=abc", signed.uri()).as_str()),
        )
        .expect(1)
        .mount(&api)
        .await;
    Mock::given(path("/blob"))
        .and(query_param("sig", "abc"))
        .respond_with(file())
        .expect(1)
        .mount(&signed)
        .await;
    let socket = connected(spec(&api, &[(&signed, false)])).await;
    let content = socket
        .fetch(key(), ContentRequest::get("recordings/1/content"))
        .await
        .unwrap();
    assert_eq!(content.bytes, BYTES);
    assert_eq!(
        authorization(&api.received_requests().await.unwrap()[0]).as_deref(),
        Some("Bearer the-token")
    );
    assert_eq!(authorization(&signed.received_requests().await.unwrap()[0]), None);
}

#[tokio::test]
async fn a_redirect_to_a_host_that_was_not_declared_is_refused_and_nothing_is_sent_there() {
    let api = MockServer::start().await;
    let elsewhere = MockServer::start().await;
    Mock::given(path("/api/files/1/content"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", format!("{}/blob", elsewhere.uri()).as_str()),
        )
        .mount(&api)
        .await;
    Mock::given(path("/blob")).respond_with(file()).mount(&elsewhere).await;
    let socket = connected(spec(&api, &[])).await;
    let err = socket
        .fetch(key(), ContentRequest::get("files/1/content"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unexpected, "{err}");
    assert!(err.message().contains("redirected"), "{}", err.message());
    assert!(!err.message().contains("the-token"));
    assert!(elsewhere.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn redirects_are_followed_three_times_and_no_more() {
    let api = MockServer::start().await;
    for (from, to) in [("/api/1", "/api/2"), ("/api/2", "/api/3"), ("/api/3", "/api/file")] {
        Mock::given(path(from))
            .respond_with(ResponseTemplate::new(302).insert_header("location", to))
            .mount(&api)
            .await;
    }
    Mock::given(path("/api/0"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/api/1"))
        .mount(&api)
        .await;
    Mock::given(path("/api/file")).respond_with(file()).mount(&api).await;
    let socket = connected(spec(&api, &[])).await;

    let content = socket.fetch(key(), ContentRequest::get("1")).await.unwrap();
    assert_eq!(content.bytes, BYTES, "three redirects away is reached");
    assert_eq!(api.received_requests().await.unwrap().len(), 4);

    api.reset().await;
    Mock::given(path("/api/loop"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/api/loop"))
        .mount(&api)
        .await;
    let err = socket.fetch(key(), ContentRequest::get("loop")).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unexpected);
    assert!(err.message().contains("too many times"), "{}", err.message());
    assert_eq!(
        api.received_requests().await.unwrap().len(),
        4,
        "the request and three redirects"
    );
}

#[tokio::test]
async fn an_address_on_a_host_that_was_not_declared_is_refused_before_anything_is_sent() {
    let api = MockServer::start().await;
    let elsewhere = MockServer::start().await;
    Mock::given(path("/blob")).respond_with(file()).mount(&elsewhere).await;
    let socket = connected(spec(&api, &[])).await;
    for address in [
        format!("{}/blob", elsewhere.uri()),
        "https://evil.example/blob".to_owned(),
        format!("{}/blob", api.uri()).replace("http://", "http://user:pw@"),
        "ftp://files.example/blob".to_owned(),
    ] {
        let err = socket
            .fetch(key(), ContentRequest::get(address.clone()))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{address}: {err}");
    }
    // An address that cannot be read is not repeated: one that is signed carries its own credential.
    for address in [
        "https://exa mple.test/blob?sig=SECRET-SIG",
        "http://[::bad/blob?sig=SECRET-SIG",
    ] {
        let err = socket.fetch(key(), ContentRequest::get(address)).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{address}: {err}");
        assert!(!format!("{err} {err:?}").contains("SECRET-SIG"), "{err:?}");
    }
    assert!(elsewhere.received_requests().await.unwrap().is_empty());
    assert!(api.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_api_key_never_reaches_a_host_marked_not_to_receive_it() {
    for placement in [
        KeyPlacement::Query { name: "key".into() },
        KeyPlacement::Header {
            name: "X-Api-Key".into(),
            prefix: None,
        },
        KeyPlacement::Basic {},
    ] {
        let api = MockServer::start().await;
        let signed = MockServer::start().await;
        Mock::given(path("/api/export"))
            .respond_with(
                ResponseTemplate::new(307).insert_header("location", format!("{}/blob", signed.uri()).as_str()),
            )
            .mount(&api)
            .await;
        Mock::given(path("/blob")).respond_with(file()).mount(&signed).await;
        let mut with_key = spec(&api, &[(&signed, false)]);
        with_key.auth = AuthScheme::ApiKey(ApiKeySpec {
            placement: placement.clone(),
        });
        let (socket, _) = connected_with(with_key, TokenSet::bearer("the-key")).await;
        let content = socket.fetch(key(), ContentRequest::get("export")).await.unwrap();
        assert_eq!(content.bytes, BYTES);

        let at_api = &api.received_requests().await.unwrap()[0];
        let sent_to_api = format!("{} {:?}", at_api.url, at_api.headers);
        assert!(
            sent_to_api.contains("the-key") || authorization(at_api).is_some(),
            "{placement:?}: the API is given the key"
        );
        let at_content = &signed.received_requests().await.unwrap()[0];
        let sent_to_content = format!("{} {:?}", at_content.url, at_content.headers);
        assert!(!sent_to_content.contains("the-key"), "{placement:?}: {sent_to_content}");
        assert_eq!(authorization(at_content), None, "{placement:?}");
        assert!(at_content.headers.get("x-api-key").is_none(), "{placement:?}");
    }
}

#[tokio::test]
async fn a_caller_cannot_set_the_headers_that_carry_credentials() {
    let api = MockServer::start().await;
    Mock::given(path("/api/files/1"))
        .and(header("accept", "text/vtt"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("WEBVTT", "text/vtt"))
        .mount(&api)
        .await;
    let socket = connected(spec(&api, &[])).await;
    // A header that chooses the format is the caller's to set.
    let content = socket
        .fetch(key(), ContentRequest::get("files/1").with_header("Accept", "text/vtt"))
        .await
        .unwrap();
    assert_eq!(content.bytes, b"WEBVTT");
    for name in ["Authorization", "cookie", "Host"] {
        let err = socket
            .fetch(key(), ContentRequest::get("files/1").with_header(name, "x"))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}");
    }
}

#[tokio::test]
async fn a_redirect_that_writes_the_credential_into_the_address_of_a_host_denied_it_is_not_followed() {
    let api = MockServer::start().await;
    let signed = MockServer::start().await;
    Mock::given(path("/api/echo"))
        .respond_with(ResponseTemplate::new(302).insert_header(
            "location",
            format!("{}/blob?access_token=the-token", signed.uri()).as_str(),
        ))
        .mount(&api)
        .await;
    // The same token under percent-encoding, in the path and inside a longer value.
    for (route, location) in [
        ("/api/in-path", "/files/the%2Dtoken/blob"),
        ("/api/in-value", "/blob?next=Bearer%20the%2dtoken%26more"),
        ("/api/twice", "/blob/the%252Dtoken"),
    ] {
        Mock::given(path(route))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", format!("{}{location}", signed.uri()).as_str()),
            )
            .mount(&api)
            .await;
    }
    Mock::given(path("/api/fine"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", format!("{}/blob?sig=1", signed.uri()).as_str()),
        )
        .mount(&api)
        .await;
    Mock::given(path("/blob")).respond_with(file()).mount(&signed).await;
    let socket = connected(spec(&api, &[(&signed, false)])).await;
    let err = socket.fetch(key(), ContentRequest::get("echo")).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unexpected, "{err}");
    assert!(!err.message().contains("the-token"), "{}", err.message());
    for route in ["in-path", "in-value", "twice"] {
        let err = socket.fetch(key(), ContentRequest::get(route)).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{route}: {err}");
    }
    assert!(signed.received_requests().await.unwrap().is_empty());
    // The same host is reached when the address does not carry it.
    socket.fetch(key(), ContentRequest::get("fine")).await.unwrap();
}

#[tokio::test]
async fn a_redirect_is_followed_only_when_it_says_where_to_and_names_no_user() {
    let api = MockServer::start().await;
    let host = host_entry(&api);
    for (route, location) in [
        ("/api/nowhere", None),
        ("/api/user", Some(format!("http://user:pw@{host}/api/blob"))),
        ("/api/scheme", Some("ftp://files.example/blob".to_owned())),
        ("/api/relative", Some("blob?part=1#page=2".to_owned())),
    ] {
        let mut response = ResponseTemplate::new(303);
        if let Some(location) = location {
            response = response.insert_header("location", location.as_str());
        }
        Mock::given(path(route)).respond_with(response).mount(&api).await;
    }
    Mock::given(path("/api/blob"))
        .and(query_param("part", "1"))
        .respond_with(file())
        .expect(1)
        .mount(&api)
        .await;
    let socket = connected(spec(&api, &[])).await;
    for route in ["nowhere", "user", "scheme"] {
        let err = socket.fetch(key(), ContentRequest::get(route)).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{route}: {err}");
        assert!(err.message().contains("HTTP 303"), "{route}: {}", err.message());
    }
    // An address relative to the one asked for is on the same host.
    let content = socket.fetch(key(), ContentRequest::get("relative")).await.unwrap();
    assert_eq!(content.bytes, BYTES);
}

#[tokio::test]
async fn a_key_in_the_query_is_added_for_the_api_and_a_signed_address_is_left_as_it_is() {
    let api = MockServer::start().await;
    let signed = MockServer::start().await;
    Mock::given(path("/api/export"))
        .and(query_param("key", "the-key"))
        .respond_with(file())
        .mount(&api)
        .await;
    // The API moves the reader along, with the key echoed into the new address.
    Mock::given(path("/api/moved"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/api/export?key=the-key&alt=media"))
        .mount(&api)
        .await;
    Mock::given(path("/blob")).respond_with(file()).mount(&signed).await;
    let mut with_key = spec(&api, &[(&signed, false)]);
    with_key.auth = AuthScheme::ApiKey(ApiKeySpec {
        placement: KeyPlacement::Query { name: "key".into() },
    });
    let (socket, _) = connected_with(with_key, TokenSet::bearer("the-key")).await;

    // On the API, a second value under the key's own name is refused.
    for request in [
        ContentRequest::get("export").with_query("key", "another"),
        ContentRequest::get("export?KEY=another"),
    ] {
        let err = socket.fetch(key(), request).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{err}");
    }
    assert!(api.received_requests().await.unwrap().is_empty());

    // A signed address may use the same name for its own purpose: it is not the key's host.
    socket
        .fetch(
            key(),
            ContentRequest::get(format!("{}/blob?key=signature", signed.uri())),
        )
        .await
        .unwrap();
    assert_eq!(
        signed.received_requests().await.unwrap()[0].url.query(),
        Some("key=signature")
    );

    // On the way to another address of the API, the key is written once.
    socket.fetch(key(), ContentRequest::get("moved")).await.unwrap();
    let at_api = api.received_requests().await.unwrap();
    let last = &at_api.last().unwrap().url;
    assert_eq!(last.path(), "/api/export");
    assert_eq!(
        last.query_pairs().filter(|(name, _)| name == "key").count(),
        1,
        "{last}"
    );
    assert!(
        last.query_pairs()
            .any(|(name, value)| name == "alt" && value == "media")
    );
}

#[tokio::test]
async fn a_host_that_is_not_given_the_credential_cannot_send_it_anywhere() {
    let api = MockServer::start().await;
    let signed = MockServer::start().await;
    let files = MockServer::start().await;
    // A host that is reached without the token points back at hosts that are given it.
    for (route, target) in [("/to-api", &api), ("/to-files", &files)] {
        Mock::given(path(route))
            .respond_with(
                ResponseTemplate::new(307)
                    .insert_header("location", format!("{}/api/delete?id=1", target.uri()).as_str()),
            )
            .mount(&signed)
            .await;
    }
    Mock::given(path("/api/start"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", format!("{}/to-api", signed.uri()).as_str()))
        .mount(&api)
        .await;
    Mock::given(path("/api/delete")).respond_with(file()).mount(&api).await;
    Mock::given(path("/api/delete"))
        .respond_with(file())
        .mount(&files)
        .await;
    let socket = connected(spec(&api, &[(&signed, false), (&files, true)])).await;

    for request in [
        ContentRequest::get(format!("{}/to-api", signed.uri())),
        ContentRequest::get(format!("{}/to-files", signed.uri())),
        // And it stays so however the reader came to that host.
        ContentRequest::get("start"),
    ] {
        let err = socket.fetch(key(), request.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{request:?}: {err}");
        assert!(err.message().contains("HTTP 307"), "{}", err.message());
    }
    let at_api = api.received_requests().await.unwrap();
    assert!(
        at_api.iter().all(|request| request.url.path() == "/api/start"),
        "nothing it pointed at was asked for"
    );
    assert!(files.received_requests().await.unwrap().is_empty());

    // A host that is given the token may send the reader on to the API.
    Mock::given(path("/on"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", format!("{}/api/delete", api.uri()).as_str()),
        )
        .mount(&files)
        .await;
    let content = socket
        .fetch(key(), ContentRequest::get(format!("{}/on", files.uri())))
        .await
        .unwrap();
    assert_eq!(content.bytes, BYTES);
}

#[tokio::test]
async fn a_signed_host_asked_again_after_a_failure_is_still_not_given_the_token() {
    let api = MockServer::start().await;
    let signed = MockServer::start().await;
    Mock::given(path("/blob"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&signed)
        .await;
    Mock::given(path("/blob")).respond_with(file()).mount(&signed).await;
    let socket = connected(spec(&api, &[(&signed, false)])).await;
    socket
        .fetch(key(), ContentRequest::get(format!("{}/blob?sig=1", signed.uri())))
        .await
        .unwrap();
    let received = signed.received_requests().await.unwrap();
    assert_eq!(received.len(), 2);
    assert!(received.iter().all(|request| authorization(request).is_none()));
}

// ── How much is read ─────────────────────────────────────────────────────────

#[tokio::test]
async fn content_over_the_limit_is_an_error_with_its_own_code_and_never_a_shorter_file() {
    let api = MockServer::start().await;
    Mock::given(path("/api/big"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(vec![7u8; 2_000], "video/mp4"))
        .mount(&api)
        .await;
    let socket = connected(spec(&api, &[])).await;

    let err = socket
        .fetch(key(), ContentRequest::get("big").with_max_bytes(1_999))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TooLarge, "{err}");
    assert_eq!(err.kind().code(), "too_large");
    assert_eq!(err.to_wire().code, "too_large");
    assert_eq!(err.retry(), Retry::Never, "asking again gives the same file");
    assert!(err.message().contains("1999"), "the limit is named: {}", err.message());

    // At the limit exactly, the whole file comes back.
    let whole = socket
        .fetch(key(), ContentRequest::get("big").with_max_bytes(2_000))
        .await
        .unwrap();
    assert_eq!(whole.bytes.len(), 2_000);
}

#[tokio::test]
async fn the_limit_is_ten_megabytes_unless_the_caller_sets_another() {
    let api = MockServer::start().await;
    let large = vec![1u8; 10 * 1024 * 1024 + 1];
    Mock::given(path("/api/large"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(large.clone(), "application/zip"))
        .mount(&api)
        .await;
    let socket = connected(spec(&api, &[])).await;
    let err = socket.fetch(key(), ContentRequest::get("large")).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TooLarge);

    // A recording is larger than that, and the caller who wants one says so.
    let content = socket
        .fetch(key(), ContentRequest::get("large").with_max_bytes(16 * 1024 * 1024))
        .await
        .unwrap();
    assert_eq!(content.bytes.len(), large.len());
}

/// A server that answers one request with a body it does not state the length of.
fn unsized_body(body: Vec<u8>) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut asked = Vec::new();
        let mut byte = [0u8; 1];
        while !asked.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
            asked.push(byte[0]);
        }
        let head = "HTTP/1.1 200 OK\r\ncontent-type: video/mp4\r\ntransfer-encoding: chunked\r\n\r\n";
        let mut answer = head.as_bytes().to_vec();
        for piece in body.chunks(700) {
            answer.extend_from_slice(format!("{:x}\r\n", piece.len()).as_bytes());
            answer.extend_from_slice(piece);
            answer.extend_from_slice(b"\r\n");
        }
        answer.extend_from_slice(b"0\r\n\r\n");
        // The reader may hang up once it has seen too much.
        let _ = stream.write_all(&answer);
    });
    address
}

#[tokio::test]
async fn a_body_that_does_not_state_its_length_is_counted_as_it_arrives() {
    let spec_at = |address: &str| ProviderSpec {
        id: ProviderId::new("acme").unwrap(),
        display_name: "Acme".into(),
        api_base: format!("http://{address}/api").parse().unwrap(),
        allowed_hosts: vec![address.to_owned()],
        content_hosts: Vec::new(),
        auth: AuthScheme::ApiKey(ApiKeySpec {
            placement: KeyPlacement::Basic {},
        }),
    };
    let body: Vec<u8> = (0..5_000u32).map(|n| (n % 251) as u8).collect();

    let address = unsized_body(body.clone());
    let (socket, _) = connected_with(spec_at(&address), TokenSet::bearer("the-key")).await;
    let err = socket
        .fetch(key(), ContentRequest::get("recording").with_max_bytes(4_999))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TooLarge, "{err}");

    let address = unsized_body(body.clone());
    let (socket, _) = connected_with(spec_at(&address), TokenSet::bearer("the-key")).await;
    let content = socket
        .fetch(key(), ContentRequest::get("recording").with_max_bytes(5_000))
        .await
        .unwrap();
    assert_eq!(content.bytes, body, "every piece, in order");
    assert_eq!(content.content_type.as_deref(), Some("video/mp4"));
}

/// A server that never finishes its answer: it sends `start`, which may be
/// nothing, and then waits. Says how many times it was asked.
fn stalling(start: &'static str) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = asked.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::spawn(move || {
                let mut request = Vec::new();
                let mut byte = [0u8; 1];
                while !request.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                    request.push(byte[0]);
                }
                let _ = stream.write_all(start.as_bytes());
                std::thread::sleep(Duration::from_secs(20));
            });
        }
    });
    (address, asked)
}

#[tokio::test]
async fn content_is_given_the_time_its_caller_allows_and_a_slow_answer_is_not_asked_for_again() {
    for (stalls, start) in [
        ("before the headers", ""),
        (
            "in the file",
            "HTTP/1.1 200 OK\r\ncontent-type: video/mp4\r\ncontent-length: 1000\r\n\r\nstart",
        ),
        // A refusal that never ends is waited for once as well, though a 503 that ends is asked again.
        (
            "in a refusal",
            "HTTP/1.1 503 Service Unavailable\r\ncontent-type: application/json\r\ncontent-length: 1000\r\n\r\n{",
        ),
    ] {
        let (address, asked) = stalling(start);
        let spec = ProviderSpec {
            id: ProviderId::new("acme").unwrap(),
            display_name: "Acme".into(),
            api_base: format!("http://{address}/api").parse().unwrap(),
            allowed_hosts: vec![address.clone()],
            content_hosts: Vec::new(),
            auth: AuthScheme::ApiKey(ApiKeySpec {
                placement: KeyPlacement::Basic {},
            }),
        };
        let (socket, _) = connected_with(spec, TokenSet::bearer("the-key")).await;
        let started = std::time::Instant::now();
        let request = ContentRequest::get("recording").with_timeout(Duration::from_millis(300));
        let err = socket.fetch(key(), request).await.unwrap_err();
        assert_eq!(
            (err.kind(), err.retry()),
            (ErrorKind::Transport, Retry::Never),
            "{stalls}: {err}"
        );
        assert!(
            err.message().contains("in the time allowed"),
            "{stalls}: {}",
            err.message()
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{stalls}: {:?}",
            started.elapsed()
        );
        assert_eq!(
            asked.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "{stalls}: the same wait would end the same way"
        );
    }
}

#[tokio::test]
async fn a_file_with_nothing_in_it_is_a_file() {
    let api = MockServer::start().await;
    Mock::given(path("/api/empty"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "text/plain"))
        .mount(&api)
        .await;
    Mock::given(path("/api/gone"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&api)
        .await;
    let socket = connected(spec(&api, &[])).await;
    let content = socket
        .fetch(key(), ContentRequest::get("empty").with_max_bytes(0))
        .await
        .unwrap();
    assert!(content.is_empty());
    assert_eq!(content.content_type.as_deref(), Some("text/plain"));
    assert_eq!(content.into_text(&ProviderId::new("acme").unwrap()).unwrap(), "");
    assert!(
        socket
            .fetch(key(), ContentRequest::get("gone"))
            .await
            .unwrap()
            .is_empty()
    );
}

// ── What a provider answers ──────────────────────────────────────────────────

#[tokio::test]
async fn a_refusal_of_content_is_the_error_a_caller_can_act_on() {
    for (response, kind, retry) in [
        (
            ResponseTemplate::new(404).set_body_json(json!({ "message": "no such file" })),
            ErrorKind::NotFound,
            Retry::Never,
        ),
        (
            ResponseTemplate::new(403).set_body_json(json!({ "message": "the link has expired" })),
            ErrorKind::AccessDenied,
            Retry::Never,
        ),
        (
            ResponseTemplate::new(429).insert_header("retry-after", "120"),
            ErrorKind::RateLimited,
            Retry::After(Duration::from_secs(120)),
        ),
        // An error page is not a file, whatever it is made of.
        (
            ResponseTemplate::new(500).set_body_raw(BYTES, "application/octet-stream"),
            ErrorKind::Unexpected,
            Retry::Later,
        ),
    ] {
        let api = MockServer::start().await;
        Mock::given(path("/api/files/1"))
            .respond_with(response)
            .mount(&api)
            .await;
        let socket = connected(spec(&api, &[])).await;
        let err = socket.fetch(key(), ContentRequest::get("files/1")).await.unwrap_err();
        assert_eq!((err.kind(), err.retry()), (kind, retry), "{err}");
    }
}

#[tokio::test]
async fn a_failing_server_is_asked_again_and_the_file_then_comes_back() {
    let api = MockServer::start().await;
    Mock::given(path("/api/files/1"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&api)
        .await;
    Mock::given(path("/api/files/1")).respond_with(file()).mount(&api).await;
    let socket = connected(spec(&api, &[])).await;
    let content = socket.fetch(key(), ContentRequest::get("files/1")).await.unwrap();
    assert_eq!(content.bytes, BYTES);
    assert_eq!(api.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_token_the_provider_rejects_is_renewed_once_for_content_as_for_any_request() {
    let api = MockServer::start().await;
    Mock::given(path("/api/files/1"))
        .and(header("authorization", "Bearer old-access"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&api)
        .await;
    Mock::given(path("/api/files/1"))
        .and(header("authorization", "Bearer fresh"))
        .respond_with(file())
        .expect(1)
        .mount(&api)
        .await;
    Mock::given(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token": "fresh", "expires_in": 3600 })))
        .expect(1)
        .mount(&api)
        .await;
    let tokens = TokenSet {
        access_token: SecretString::new("old-access"),
        refresh_token: Some(SecretString::new("the-refresh")),
        expires_at: Some(SystemTime::now() + Duration::from_secs(3000)),
        scopes: vec!["read".into()],
        api_base: None,
    };
    let (socket, store) = connected_with(spec(&api, &[]), tokens).await;
    let content = socket.fetch(key(), ContentRequest::get("files/1")).await.unwrap();
    assert_eq!(content.bytes, BYTES);
    assert_eq!(store.load(key()).await.unwrap().unwrap().access_token.expose(), "fresh");
}

#[tokio::test]
async fn a_host_that_was_not_given_the_token_cannot_have_it_renewed() {
    let api = MockServer::start().await;
    let signed = MockServer::start().await;
    Mock::given(path("/blob"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({ "message": "signature expired" })))
        .mount(&signed)
        .await;
    Mock::given(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token": "fresh", "expires_in": 3600 })))
        .expect(0)
        .mount(&api)
        .await;
    let tokens = TokenSet {
        access_token: SecretString::new("old-access"),
        refresh_token: Some(SecretString::new("the-refresh")),
        expires_at: Some(SystemTime::now() + Duration::from_secs(3000)),
        scopes: vec!["read".into()],
        api_base: None,
    };
    let (socket, store) = connected_with(spec(&api, &[(&signed, false)]), tokens).await;
    let err = socket
        .fetch(key(), ContentRequest::get(format!("{}/blob", signed.uri())))
        .await
        .unwrap_err();
    // The address is no longer good. The connection is, and is left alone.
    assert_eq!(err.kind(), ErrorKind::AccessDenied, "{err}");
    assert!(err.message().contains("HTTP 401"), "{}", err.message());
    assert_eq!(signed.received_requests().await.unwrap().len(), 1);
    assert_eq!(
        store.load(key()).await.unwrap().unwrap().access_token.expose(),
        "old-access"
    );
}

#[tokio::test]
async fn an_error_that_repeats_the_token_does_not_carry_it_to_the_caller() {
    let api = MockServer::start().await;
    Mock::given(path("/api/files/1"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({ "message": "the-token may not read this" })))
        .mount(&api)
        .await;
    let socket = connected(spec(&api, &[])).await;
    let err = socket.fetch(key(), ContentRequest::get("files/1")).await.unwrap_err();
    assert_eq!(err.message(), "acme denied the request: [redacted] may not read this");
}

// ── Text, for an operation called by name ────────────────────────────────────

#[test]
fn only_content_that_is_text_is_handed_over_as_text() {
    let provider = ProviderId::new("acme").unwrap();
    let content = |bytes: &[u8], content_type: Option<&str>| Content {
        bytes: bytes.to_vec(),
        content_type: content_type.map(str::to_owned),
    };
    for kind in [
        "text/plain",
        "text/plain; charset=utf-8",
        "TEXT/VTT",
        "text/csv",
        "text/html; charset=UTF-8",
        "text/calendar",
        "application/json",
        "application/xml",
        "application/problem+json",
        "application/atom+xml; charset=utf-8",
    ] {
        let text = content("caf\u{e9} ok".as_bytes(), Some(kind))
            .into_text(&provider)
            .unwrap_or_else(|e| panic!("{kind}: {e}"));
        assert_eq!(text, "caf\u{e9} ok", "{kind}");
    }
    // Anything else is bytes, which an agent calling by name is never handed.
    for kind in [
        Some("application/pdf"),
        Some("video/mp4"),
        Some("application/octet-stream"),
        Some("image/svg"),
        Some("text"),
        Some("textile/plain"),
        Some("application/jsonp"),
        Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        Some(""),
        None,
    ] {
        let err = content(b"%PDF-1.7 secret words", kind)
            .into_text(&provider)
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unsupported, "{kind:?}");
        assert!(err.message().contains("21 bytes"), "{}", err.message());
        assert!(!err.message().contains("secret"), "{}", err.message());
        assert_eq!(err.provider().map(ProviderId::as_str), Some("acme"));
    }
    // Text in another encoding can pass for UTF-8 and read as other words, so it is not read.
    for kind in [
        "text/csv; charset=utf-16le",
        "text/plain;charset=ISO-8859-1",
        "text/plain; format=flowed; Charset=\"windows-1252\"",
        "application/json; charset=",
    ] {
        let err = content(b"plain ascii", Some(kind)).into_text(&provider).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unsupported, "{kind}");
        assert!(err.message().contains("encoding"), "{}", err.message());
    }
    for kind in [
        "text/plain; charset=US-ASCII",
        "text/plain; charset=\"UTF-8\"; format=flowed",
        "text/csv;charset=utf8",
    ] {
        assert_eq!(
            content(b"plain ascii", Some(kind)).into_text(&provider).unwrap(),
            "plain ascii",
            "{kind}"
        );
    }
    // Text that is not text after all is refused, not mended.
    let err = content(BYTES, Some("text/plain")).into_text(&provider).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
}
