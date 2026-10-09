# Socket

An open-source library that gives a product the connection, authorisation and operation layers for external services. The application keeps its own credentials; nothing is hosted.

Status: early, and not yet run against the real services. The core, the OAuth flow, token refresh and six providers are built and tested against local servers. See [the roadmap](docs/roadmap.md) for what is and is not done.

- [Vision](docs/vision.md)
- [Design](docs/superpowers/specs/2026-10-08-socket-project-design.md)
- [Catalogue](docs/catalogue.md)

## What is here

| Crate | What it is |
| --- | --- |
| `socketkit` | What an application depends on. One feature per provider: `github`, `google`, `linear`, `notion`, `slack`, `zoom`. |
| `socketkit-core` | Providers, the token store interface, the HTTP transport, OAuth, refresh, and call-by-name. |
| `socketkit-<provider>` | The provider's definition, plus two operations: `identity.get` and `resource.resolve`. |
| `socketkit-testkit` | A local test server and the checks every provider must pass. |

## Try it

```sh
cargo run -p socketkit --all-features --example catalogue
cargo run -p socketkit-core --example invoke_by_name
```

The first prints the six providers and their operations. The second registers an integration, invokes an operation by name with JSON, and prints an error in the form other languages will receive. Neither needs a network or credentials.

## Use it

Each integration is given its connection details when it is created.

With a token you already hold:

```rust
use std::sync::Arc;
use socketkit::{ConnectionKey, ProviderId, RawRequest, Socket};

let socket = Socket::in_memory()
    .integration(Arc::new(socketkit::slack::Slack::with_token("xoxb-your-token")))
    .build()?;
let slack = ConnectionKey::new(ProviderId::new("slack")?, "me");

// Whose token is this?
let account = socket.invoke(slack.clone(), "slack.identity.get".into(), serde_json::json!({})).await?;
// Any endpoint, with the token, retries and error mapping handled.
let channels = socket.request(slack, RawRequest::get("conversations.list")).await?;
```

Slack has typed methods for its whole everyday surface: messages, conversations, users, reactions, pins, files, search, reminders, bookmarks, user groups and the workspace. Identifiers are plain arguments; content and filters are structs.

```rust
use socketkit::slack::models::{History, PostMessage};

let slack = socketkit::slack::Slack::with_token("xoxb-your-token");
let socket = Socket::in_memory().integration(Arc::new(slack.clone())).build()?;
let connection = socket.connection(ConnectionKey::new(ProviderId::new("slack")?, "me")).await?;

let posted = slack.chat(&connection).post_message("C0123ABCD", PostMessage::text("Deploy finished")).await?;
slack.reactions(&connection).add("C0123ABCD", &posted.ts, "tada").await?;
let recent = slack.conversations(&connection).history("C0123ABCD", History { limit: Some(20), ..History::default() }).await?;
```

Each of those is also an operation an agent can call by name with JSON, for example `slack.chat.post_message` with `{ "channel": "C0123ABCD", "text": "Deploy finished" }`. `socket.operations()` lists all of them with input and output schemas.

With your own OAuth app, to connect your users:

```rust
let socket = Socket::builder(Arc::new(my_token_store))        // where your users' tokens are kept
    .integration(Arc::new(socketkit::github::GitHub::with_oauth(OAuthClient {
        client_id: config.github_client_id,
        client_secret: SecretString::new(config.github_client_secret),
        redirect_uri: "https://yourapp.example/oauth/callback".parse()?,
    })))
    .build()?;

// When a user clicks "Connect GitHub":
let key = ConnectionKey::new(ProviderId::new("github")?, user.id);
let authorization = socket.begin_authorization(key, None)?;
// keep `authorization.pending` in that user's session, then redirect them to `authorization.url`

// At your callback route:
socket.complete_authorization(pending, code, state).await?;
```

Keep the pending record tied to the session of the person who started the flow, and delete it after one use. An application that runs more than one instance must also set `state_secret` on the builder, so that every instance signs with the same secret.

## Develop

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```
