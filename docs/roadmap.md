# Socket — roadmap

**Status:** Draft for review
**Date:** 2026-10-08

The order of work. The [design spec](./superpowers/specs/2026-10-08-socket-project-design.md) says what each phase must achieve; this page says how each phase is cut into plans. Every plan ends with software that builds and passes its tests on its own.

## Phases

| Phase | Outcome | Language reach | Catalogue |
| --- | --- | --- | --- |
| 1. Core | OAuth, refresh, transport, generic request and call-by-name work in Rust for six providers. Built; not yet run against the real services. | Rust | Wave 1 authorised |
| 2. Operations and first release | Typed operations for GitHub, Slack and Linear; 0.1.0 on crates.io | Rust | Wave 1 partly verified |
| 3. Webhooks | Verified events from GitHub, Slack and Linear | Rust | |
| 4. Local server and MCP | A program with no Rust in it authorises and calls operations | Every language, through the server | |
| 5. Registry and definitions | Providers and operations added as data and verified | | Waves 2 and 3 authorised |
| 6. Bindings | In-process packages for Node.js, then Python, then others | Node.js, Python | |

Phase 6 is new in this roadmap: the spec lists bindings under "later", and they are now a committed phase after the local server.

## Phase 1, cut into four plans

| Plan | Builds | Ends with | Status |
| --- | --- | --- | --- |
| 1A. Core foundation | Workspace, errors, secrets, providers, token store, operations, the `Socket` handle and `invoke` | An example that registers an integration and invokes an operation by name with JSON, with no network | [Done](./superpowers/plans/2026-10-08-phase-1a-core-foundation.md), on branch `phase-1a-core-foundation` |
| 1B. Transport | One HTTP client, host allowlist, retry with `Retry-After`, response classification, the generic request, `socketkit-testkit` | A generic request that attaches the token only to allowed hosts and maps throttling and refusal to the right errors | Done |
| 1C. Authorisation | OAuth begin and complete with signed state and PKCE, token-response parsing hook, single-flight refresh, reconnect-required | A full connect and an expired-token refresh against a local server, with concurrent calls never spending a refresh token twice | Done |
| 1D. Providers and facade | Six provider crates with identity and resource lookup, the conformance suite, the facade | Every provider passes the conformance suite against a local server | Done, except live tests |

1B to 1D were built directly, test first, without a written plan each; the code and its tests are the record.

## What phase 1 has not done

- **Nothing has been run against a real service.** Every test uses a local server that answers the way the provider's documentation and the earlier private code say it does. The spec's phase 1 criterion, a real consumer running connect, refresh and lookup against the real providers, is still open. It needs an OAuth app and a test account for each of the six.
- **PKCE is switched off for all six providers**, matching the code they were ported from. Each should be switched on once it is confirmed against the real provider.
- **Zoom's `user:read:user` scope is new.** The earlier code did not ask for it; `zoom.identity.get` needs it.

## Slack, built out ahead of phase 2

Slack now has 54 typed methods, each also a named operation with generated schemas: chat, conversations, users, reactions, pins, files, search, reminders, bookmarks, user groups, and the workspace (team, emoji, Do Not Disturb). Not covered: uploading a file (it sends raw bytes to a different host, which the transport does not do yet), modals and views, admin and SCIM methods, incoming events, and Socket Mode. None of it has been run against the real Slack; the request and response shapes follow Slack's documentation.

## Microsoft, the first provider of wave 2

`socketkit-microsoft` is one provider for everything behind Microsoft Graph: sign-in through the identity platform with a tenant setting, refresh with rotation, identity, and lookup of a OneDrive or SharePoint sharing link. It is the first provider with PKCE switched on, because Microsoft documents it for web applications. Not covered: application-only access (client credentials) and the national clouds.

The Outlook calendar is the first product on it: 12 typed methods in two groups, `calendars` and `events`, each also a named operation. Events are read between two times with repeating events expanded, with every time in UTC and the event's own zone kept beside it; free and busy times and suggested meeting times are read; events are created (with a Teams link when asked), changed, answered, cancelled and deleted. A list is paged with the query of the address Graph gives for the next page, once that address is seen to be on the API's own host; the request itself always goes to the address the method built. Two things wait on the shared typed-operation work on the `github-full-client` branch: the crate carries its own copy of the operation machinery, as Slack does, and the transport still repeats a DELETE after a server error.

Outlook mail is the second: 18 methods in `mail` and `mail_folders`. Messages are listed by folder, filter, search and sort, read with their body as plain text, and gathered by conversation; attachments are listed without their content and fetched one at a time; drafts, replies and forwards are written without sending; mail is sent, marked, moved and deleted. Sending, changing a draft, moving and deleting are marked destructive. Not covered: shared and delegated mailboxes, adding attachments, and files too large for one answer.

Teams is the third: 18 methods in `teams`, `channels`, `channel_messages` and `chats`. Teams, channels and their members are listed; a channel's messages and replies and a chat's messages are read, each also as plain text with mentions written as names; a message is posted to a channel or a chat, and a chat is created.

Teams meetings are the fourth: 9 methods in `online_meetings`, `transcripts`, `recordings` and `attendance`. A meeting is found from the join link on a calendar event; its transcripts are read as text and as entries with speaker, start and end; its recordings and its attendance are listed. This needed one change to the core, a request that says its answer is text. Downloading a recording waits for the transport to carry bytes.

Nothing has been run against the real service; [the guide](./integrations/microsoft.md) lists what was confirmed against Microsoft's documentation and what was not.

## HubSpot and Attio, the first two CRMs

Both are new providers, and both needed nothing new from the core. A CRM's shape is its customer's to define, so each client is generic, with the kind of record as an argument, and each has operations that list the fields a record can have.

`socketkit-hubspot` signs in through OAuth, with scopes an account's plan may lack asked for as optional ones, or uses a private app's token. It has 18 typed methods in five groups: `objects` (list, get, batch read, search, create, update, batch create and update, archive, for contacts, companies, deals, tickets, the activities logged on them and custom objects), `associations`, `properties`, `pipelines` and `owners`. It is written against HubSpot's dated API version `2026-09`, which HubSpot names as the one for new integrations; the version is one constant. Its 429 says which of an account's limits was met, and search reports its own limit and its cap of 10,000 results as their own errors.

`socketkit-attio` signs in through OAuth with PKCE, or uses a workspace's API key. Attio takes no scopes at sign-in and its tokens do not expire. It has 38 typed methods in eleven groups: `objects` and `attributes` for the schema, `records`, `lists` and `entries` for the data, `notes`, `tasks` and `threads`, `workspace_members`, and `meetings` and `call_recordings`, which carry transcripts. A record keeps Attio's shape, a list of dated values for each attribute, and gives beside it what each attribute holds now.

Each crate carries its own copy of the operation machinery, as Microsoft and Slack do. Nothing has been run against either real service; the guides for [HubSpot](./integrations/hubspot.md) and [Attio](./integrations/attio.md) list what was confirmed against each vendor's documentation and what was not.

## Carried forward from reviews

- Decide before the first release whether public data structs (`ProviderSpec`, `OAuth2Spec`, `OperationInfo`, `TokenSet`) become `#[non_exhaustive]` with constructors. Today adding a field breaks every integration crate.
- Retrying is decided by HTTP method. The spec says the provider's classifier should decide; Slack accepts GET for some writes, so this matters before Slack gets write operations in phase 2.
- Notion treats any 400 on a lookup as "not found", including a 400 that means something else. (Zoom now reads its own error code.)
- Slack channel lookup by name reports "not found" after 20 pages even if the workspace has more.
- A request path may climb out of `api_base` with `..` to another path on the same allowed host.
- An application-supplied HTTP client builder is trusted apart from redirects: a shared cookie store or default headers would apply to every tenant.
- Single-flight refresh is per process. Two instances of an application can still refresh the same connection at once.
- Enforce the dependency rules in CI; add `cargo hack --each-feature` and `cargo deny`.
