# Socket

**Connection, authorisation and operations for external services — as a library.**

Socket gives a product the layers it needs to connect to SaaS APIs: a registry of providers, an OAuth and token-refresh engine, and typed operations for each service. The application owns its OAuth apps and its token storage. Nothing is hosted.

> **Status:** Early. The core, the OAuth flow, token refresh and six providers are built and tested against local servers (~13k lines of Rust, 225 tests). Not yet run against the real services. See [the roadmap](docs/roadmap.md) for what is and is not done.

## Why

Every product that needs integrations rebuilds the same layers: OAuth, token storage, refresh, retries, pagination, error classification, webhook verification, and the operations themselves. n8n did it for 308 nodes, Activepieces for 736 pieces, and none of that work is reusable — it is either proprietary, tied to a specific runtime, or bound to a hosted platform that holds your users' tokens.

Socket is the library that assembles these layers so the next team does not have to. Tokens never leave the application. There is no Socket cloud, no auth proxy, no account.

Read [the vision](docs/vision.md) for the full picture.

## Features

- **OAuth 2.0** — begin and complete the flow; signed, expiring state; the application owns the callback route. PKCE is supported by the core. It is on for Microsoft, whose documentation recommends it, and switched off for the other six providers until each is confirmed against the real service
- **Token refresh** — single-flight per connection so concurrent calls never race a refresh token
- **Host allowlist** — credentials are attached only to HTTPS requests on the provider's declared hosts
- **Retry with backoff** — honours `Retry-After`; non-idempotent requests are retried only when safe
- **Pagination** — one model for every provider: a cursor in, a page and the next cursor out
- **Error classification** — `ReconnectRequired`, `RateLimited`, `AccessDenied`, `NotFound` and more, each with a stable code
- **Call by name** — every typed method is also an operation callable with JSON, with input and output schemas, so one implementation serves backend code, agents, MCP and other languages
- **Generic authenticated request** — call any endpoint of a registered provider with auth, retries and error mapping, even without a typed integration
- **Per-provider hooks** — token-response parsing and response classification for services that break the standard (Slack, Notion, GitHub)

## Workspace

```
socket/
├── crates/
│   ├── core/                   # socketkit-core: providers, OAuth, transport, operations
│   ├── facade/                 # socketkit: one feature per provider
│   ├── integrations/
│   │   ├── github/             # socketkit-github
│   │   ├── slack/              # socketkit-slack (54 typed methods)
│   │   ├── linear/             # socketkit-linear
│   │   ├── notion/             # socketkit-notion
│   │   ├── microsoft/          # socketkit-microsoft (Teams meetings: 9 typed methods)
│   │   ├── google/             # socketkit-google
│   │   └── zoom/               # socketkit-zoom
│   └── testkit/                # socketkit-testkit: wire-test server, conformance suite
└── docs/
    ├── vision.md               # Why the project exists
    ├── catalogue.md            # The first 100 services and the order they are added
    ├── roadmap.md              # Phases and plans
    └── integrations/slack.md   # Slack: how to connect, every method and operation
```

| Crate | Purpose |
| --- | --- |
| `socketkit` | What an application depends on. One feature per provider: `github`, `google`, `linear`, `microsoft`, `notion`, `slack`, `zoom`. |
| `socketkit-core` | Providers, the token store interface, the HTTP transport, OAuth, refresh, and call-by-name. |
| `socketkit-<provider>` | The provider's definition, plus operations (`identity.get`, `resource.resolve`, and for Slack its full typed API). |
| `socketkit-testkit` | A local test server and the conformance checks every provider must pass. |

## Quick start

### Try the examples

Neither needs a network or credentials:

```sh
# Print the providers and their operations
cargo run -p socketkit --all-features --example catalogue

# Register an integration, invoke an operation by name with JSON
cargo run -p socketkit-core --example invoke_by_name
```

### Use it with a token you already hold

```rust
use std::sync::Arc;
use socketkit::{ConnectionKey, ProviderId, RawRequest, Socket};

let socket = Socket::in_memory()
    .integration(Arc::new(socketkit::slack::Slack::with_token("xoxb-your-token")))
    .build()?;
let slack = ConnectionKey::new(ProviderId::new("slack")?, "me");

// Whose token is this?
let account = socket.invoke(
    slack.clone(),
    "slack.identity.get".into(),
    serde_json::json!({}),
).await?;

// Any endpoint, with the token, retries and error mapping handled
let channels = socket.request(slack, RawRequest::get("conversations.list")).await?;
```

### Use the typed client

Slack has 54 typed methods covering messages, conversations, users, reactions, pins, files, search, reminders, bookmarks, user groups and the workspace. Identifiers are plain arguments; content and filters are structs.

```rust
use std::sync::Arc;
use socketkit::slack::models::{History, PostMessage};
use socketkit::{ConnectionKey, ProviderId, Socket};

let slack = socketkit::slack::Slack::with_token("xoxb-your-token");
let socket = Socket::in_memory()
    .integration(Arc::new(slack.clone()))
    .build()?;
let connection = socket
    .connection(ConnectionKey::new(ProviderId::new("slack")?, "me"))
    .await?;

let posted = slack.chat(&connection)
    .post_message("C0123ABCD", PostMessage::text("Deploy finished"))
    .await?;
slack.reactions(&connection)
    .add("C0123ABCD", &posted.ts, "tada")
    .await?;
let recent = slack.conversations(&connection)
    .history("C0123ABCD", History { limit: Some(20), ..History::default() })
    .await?;
```

Every typed method is also an operation an agent can call by name with JSON — for example `slack.chat.post_message` with `{ "channel": "C0123ABCD", "text": "Deploy finished" }`. `socket.operations()` lists all of them with input and output schemas.

### Connect your users with OAuth

```rust
use std::sync::Arc;
use socketkit::{ConnectionKey, OAuthClient, ProviderId, SecretString, Socket};

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
// Redirect the user to `authorization.url`
// Keep `authorization.pending` in that user's server-side session

// At your callback route:
socket.complete_authorization(pending, code, state).await?;
// The token is now in your store — all future calls refresh it automatically
```

Keep the pending record tied to the session of the person who started the flow, and delete it after one use. An application that runs more than one instance must also set `state_secret` on the builder, so that every instance signs with the same secret.

## Providers

| Provider | Auth | Operations | Status |
| --- | --- | --- | --- |
| GitHub | OAuth 2.0 | `identity.get`, `resource.resolve` | Wire-tested |
| Slack | OAuth 2.0 | 54 typed methods across 11 API groups | Wire-tested |
| Linear | OAuth 2.0 | `identity.get`, `resource.resolve` | Wire-tested |
| Notion | OAuth 2.0 | `identity.get`, `resource.resolve` | Wire-tested |
| Google | OAuth 2.0 | `identity.get`, `resource.resolve` | Wire-tested |
| Zoom | OAuth 2.0 | `identity.get`, `resource.resolve` | Wire-tested |
| Microsoft | OAuth 2.0 | `identity.get`, and 9 typed methods for Teams meetings: transcripts, recordings and attendance | Wire-tested only |

All seven support the generic authenticated request, so any endpoint of theirs can be called even without a typed operation. The [catalogue](docs/catalogue.md) lists the first 100 services and the order they will be added.

## How breadth grows

Socket does not promise "thousands of integrations". Instead it provides layers, each reaching further:

| Layer | What you can do | Scale |
| --- | --- | --- |
| **Registry** | Authorise a user and keep the token fresh | Hundreds — auth describes well as data |
| **Generic request** | Call any endpoint with auth, retries and error classification | Same hundreds, no code per service |
| **Verified integrations** | Typed operations, webhooks, a named owner, live tests | Dozens, growing as owners appear |
| **Operation definitions** | Add an operation as data, verify it against the real API | Open-ended |
| **MCP client** | Reach any vendor's remote MCP server through the same token store | Whatever vendors publish |

## Roadmap

| Phase | What it delivers | Status |
| --- | --- | --- |
| 1. Core | OAuth, refresh, transport, generic request, call-by-name, six providers | Built, not yet live-tested |
| 2. Operations | Typed operations for GitHub, Slack, Linear; 0.1.0 on crates.io | Slack built; GitHub and Linear next |
| 3. Webhooks | Signature verification and typed events | Planned |
| 4. Local server and MCP | JSON-RPC and MCP server so any language can use Socket | Planned |
| 5. Registry and definitions | Providers and operations as data files | Planned |
| 6. Bindings | In-process packages for Node.js, Python, then others | Planned |

See [the full roadmap](docs/roadmap.md) for details and the [design spec](docs/superpowers/specs/2026-10-08-socket-project-design.md) for architecture.

## Developing

### Requirements

- Rust 1.85+ (edition 2024)

### Build and test

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

All three must pass before committing.

### Project structure

Each integration follows the same layout:

```
crates/integrations/<provider>/src/
├── lib.rs          # provider definition, settings, the Integration impl
├── operations.rs   # named operations and dispatch
├── client/
│   ├── mod.rs      # shared access to the API, re-exports
│   ├── chat.rs     # one file per area of the provider's API
│   └── …
└── models/
    ├── mod.rs      # re-exports only
    ├── message.rs  # one file per area
    └── …
```

See [CLAUDE.md](CLAUDE.md) for the full conventions (models, clients, naming).

## Design principles

1. **The application owns its credentials.** Bring your own OAuth app, your own token store.
2. **Every layer is useful alone.** Take the registry and auth flow and write your own calls.
3. **One implementation, every caller.** An operation is written once and serves typed Rust code, agents, MCP and other languages.
4. **Depth before breadth.** Four operations that work beat forty that fail on page two.
5. **Say what is verified.** Every service has a tier and every verified integration has an owner.
6. **Fail loudly.** Missing credentials, unsupported operations and skipped tests are errors, never silent successes.

## Documentation

- [Vision](docs/vision.md) — why the project exists
- [Design spec](docs/superpowers/specs/2026-10-08-socket-project-design.md) — architecture, phases and acceptance criteria
- [Catalogue](docs/catalogue.md) — the first 100 services and the order they are added
- [Roadmap](docs/roadmap.md) — phases and implementation plans
- [Slack guide](docs/integrations/slack.md) — how to connect, and every method and operation
- [Microsoft guide](docs/integrations/microsoft.md) — how to connect, Teams meeting transcripts, recordings and attendance, and their limits

## License

MIT OR Apache-2.0
