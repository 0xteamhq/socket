# Socket

An open-source library that gives a product the connection, authorisation and operation layers for external services. The application keeps its own credentials; nothing is hosted.

Status: early. The core types and call-by-name exist. HTTP, OAuth and real integrations are next; see [the roadmap](docs/roadmap.md).

- [Vision](docs/vision.md)
- [Design](docs/superpowers/specs/2026-10-08-socket-project-design.md)
- [Catalogue](docs/catalogue.md)

## Try it

```sh
cargo run -p socketkit-core --example invoke_by_name
```

It registers one integration, lists its operations, invokes one by name with JSON, and prints an error in the form other languages will receive.

## Develop

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
