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
├── client.rs       # typed methods, grouped by area
├── operations.rs   # each typed method registered as a named operation
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

## Before committing

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

All three must pass. Commit messages are plain, with no tool attribution.
