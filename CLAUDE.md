# Socket — project rules

Rules for anyone, person or agent, changing this repository. The design is in
`docs/superpowers/specs/2026-10-08-socket-project-design.md`; these are the
conventions that are easy to break without noticing.

## Models live in their own module, one file per area

Every integration crate keeps its data types in a `models` module that is a
directory, not a single file.

```
crates/integrations/<provider>/src/
├── lib.rs          # provider definition, settings, the Integration impl
├── operations.rs   # each typed method registered as a named operation
├── client/
│   ├── mod.rs      # shared access to the API, and re-exports
│   ├── chat.rs     # one file per area of the provider's API
│   ├── users.rs
│   └── …
└── models/
    ├── mod.rs      # `mod` lines and `pub use` re-exports only
    ├── message.rs  # one file per area of the provider's API
    ├── user.rs
    └── …
```

- **What counts as a model:** any struct, enum or type alias that describes
  what the provider returns (`Message`, `Channel`) or what a caller supplies as
  content or options (`PostMessage`, `History`, `Paging`). They never go in
  `lib.rs`, `client.rs` or `operations.rs`.
- **Group by area, not by direction.** A file holds one area's types, both
  what comes back and what goes in: `message.rs` has `Message` and
  `PostMessage`. Do not make `inputs.rs` and `outputs.rs`.
- **Name the file after the thing**, singular: `message.rs`, `conversation.rs`,
  `user.rs`, `file.rs`. A type used by several areas, such as `Paging`, gets
  its own small file.
- **`models/mod.rs` holds no types.** It declares the files and re-exports
  every public type, so callers write `socketkit::slack::models::Message` and
  never name the file.
- **A type that refers to one in another file** imports it with `use super::…`.
- **When a file passes about 250 lines, or gains a second area, split it.**
- **Start this way.** A new integration creates `models/` with its first type,
  even if that is one file. Do not begin with `models.rs` and split later.

What is not a model and stays where it is: the input structs in
`operations.rs` (they only pair a method's plain arguments with its options for
a named operation), the provider's settings types such as `SlackOAuth` (they
live in `lib.rs` beside the constructors that read them), and classifiers.

The Slack crate is the reference: `crates/integrations/slack/src/models/`.

## The client is a module too, one file per area of the API

An integration's typed methods live in a `client` module that is a directory,
split the way the provider splits its own API.

- **One file per group of methods**, named as the provider names the group:
  Slack's `chat.*` methods are in `client/chat.rs`, `conversations.*` in
  `client/conversations.rs`. The file holds the group's struct (`Chat`) and
  every method on it, with the private helpers only that group needs.
- **`client/mod.rs` holds only what every group shares:** the access to the
  API (how a read and a write are sent, how a response field is decoded) and
  the re-exports of each group's struct. No group's methods go there.
- **A group never calls another group's file.** If two groups need the same
  helper, it moves to `client/mod.rs`.
- **Identifiers are plain arguments; content and options are structs** from
  `models`. A channel id, user id or message timestamp is a `&str` argument.
  Message content and optional filters are a struct, so unset fields are not
  sent.
- **Reads go out as GET, writes as POST**, so the transport never repeats a
  write that may have happened.
- **Every public method is also a named operation** in `operations.rs`, and
  has a row in that integration's operations test.
- **When a file passes about 250 lines, split the group** along the
  provider's own sub-groups.
- **Start this way.** A new integration creates `client/` with its first
  group.

`operations.rs` stays one file: it is a single table that lists every
operation, and reading it top to bottom is how one checks that nothing is
missing or mislabelled.

The Slack crate is the reference: `crates/integrations/slack/src/client/`.

## Before committing

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

All three must pass. Commit messages are plain, with no tool attribution.
