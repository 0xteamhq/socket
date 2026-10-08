# Socket — roadmap

**Status:** Draft for review
**Date:** 2026-10-08

The order of work. The [design spec](./superpowers/specs/2026-10-08-socket-project-design.md) says what each phase must achieve; this page says how each phase is cut into plans. Every plan ends with software that builds and passes its tests on its own.

## Phases

| Phase | Outcome | Language reach | Catalogue |
| --- | --- | --- | --- |
| 1. Core | OAuth, refresh, transport, generic request and call-by-name work in Rust for six providers | Rust | Wave 1 authorised |
| 2. Operations and first release | Typed operations for GitHub, Slack and Linear; 0.1.0 on crates.io | Rust | Wave 1 partly verified |
| 3. Webhooks | Verified events from GitHub, Slack and Linear | Rust | |
| 4. Local server and MCP | A program with no Rust in it authorises and calls operations | Every language, through the server | |
| 5. Registry and definitions | Providers and operations added as data and verified | | Waves 2 and 3 authorised |
| 6. Bindings | In-process packages for Node.js, then Python, then others | Node.js, Python | |

Phase 6 is new in this roadmap: the spec lists bindings under "later", and they are now a committed phase after the local server.

## Phase 1, cut into four plans

| Plan | Builds | Ends with | Status |
| --- | --- | --- | --- |
| 1A. Core foundation | Workspace, errors, secrets, providers, token store, operations, the `Socket` handle and `invoke` | An example that registers an integration and invokes an operation by name with JSON, with no network | [Written](./superpowers/plans/2026-10-08-phase-1a-core-foundation.md) |
| 1B. Transport | One HTTP client, host allowlist, retry with `Retry-After`, response classification, cursor pagination, the generic request, `socketkit-testkit` wire server | A generic request against the wire server that attaches the token only to allowed hosts and maps throttling and refusal to the right errors | To write after 1A lands |
| 1C. Authorisation | OAuth begin and complete with signed state and PKCE, token-response parsing hook, single-flight refresh, reconnect-required | A full connect and an expired-token refresh against the wire server, with two concurrent calls causing one refresh | To write after 1B lands |
| 1D. Providers and facade | Six provider crates with `Identity` and `Resolve`, the conformance suite, the facade, CI with `cargo hack` and `cargo deny` | Phase 1's "done when" list in the spec | To write after 1C lands |

Plans 1B to 1D are written one at a time, each after the one before it has landed, because each depends on the exact types the earlier one produced.

## Open before 1C

Decision 4 in the spec (seed the OAuth code from the existing private crate, or write it fresh) does not affect 1A or 1B. It must be settled before 1C is written.
