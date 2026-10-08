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

```rust
use std::sync::Arc;
use socketkit::{ConnectionKey, MemoryTokenStore, ProviderId, RawRequest, Socket, TokenSet, TokenStore};

// Your own store in production; this one keeps tokens in memory.
let store = Arc::new(MemoryTokenStore::new());
let key = ConnectionKey::new(ProviderId::new("github")?, "user-42");
store.save(key.clone(), TokenSet::bearer("a token you already hold")).await?;

let socket = Socket::builder(store)
    .integration(Arc::new(socketkit::github::GitHub::new()))
    .build()?;

// Whose token is this?
let account = socket.invoke(key.clone(), "github.identity.get".into(), serde_json::json!({})).await?;

// Any endpoint, with the token, retries and error mapping handled.
let issues = socket.request(key, RawRequest::get("repos/acme/api/issues")).await?;
```

To connect a user through OAuth, give the builder your OAuth app with `oauth_client` and a `state_secret`, then call `begin_authorization` and, at your callback route, `complete_authorization`. Keep the pending record tied to the session of the person who started the flow, and delete it after one use.

## Develop

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```
