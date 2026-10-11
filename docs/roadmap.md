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

Slack now has 54 typed methods, each also a named operation with generated schemas: chat, conversations, users, reactions, pins, files, search, reminders, bookmarks, user groups, and the workspace (team, emoji, Do Not Disturb). Not covered: uploading a file (it sends raw bytes to a different host; the transport fetches bytes now, and does not yet send them), modals and views, admin and SCIM methods, incoming events, and Socket Mode. None of it has been run against the real Slack; the request and response shapes follow Slack's documentation.

## Microsoft, the first provider of wave 2

`socketkit-microsoft` is one provider for everything behind Microsoft Graph: sign-in through the identity platform with a tenant setting, refresh with rotation, identity, and lookup of a OneDrive or SharePoint sharing link. It is the first provider with PKCE switched on, because Microsoft documents it for web applications. Not covered: application-only access (client credentials) and the national clouds.

The Outlook calendar is the first product on it: 12 typed methods in two groups, `calendars` and `events`, each also a named operation. Events are read between two times with repeating events expanded, with every time in UTC and the event's own zone kept beside it; free and busy times and suggested meeting times are read; events are created (with a Teams link when asked), changed, answered, cancelled and deleted. A list is paged with the query of the address Graph gives for the next page, once that address is seen to be on the API's own host; the request itself always goes to the address the method built. Two things wait on the shared typed-operation work on the `github-full-client` branch: the crate carries its own copy of the operation machinery, as Slack does, and the transport still repeats a DELETE after a server error.

Outlook mail is the second: 20 methods in `mail` and `mail_folders`. Messages are listed by folder, filter, search and sort, read with their body as plain text, and gathered by conversation; attachments are listed and described without their content, and fetched one at a time as bytes or, by name, as text; drafts, replies and forwards are written without sending; mail is sent, marked, moved and deleted. Sending, changing a draft, moving and deleting are marked destructive. Not covered: shared and delegated mailboxes, and adding attachments.

Teams is the third: 18 methods in `teams`, `channels`, `channel_messages` and `chats`. Teams, channels and their members are listed; a channel's messages and replies and a chat's messages are read, each also as plain text with mentions written as names; a message is posted to a channel or a chat, and a chat is created.

Teams meetings are the fourth: 10 methods in `online_meetings`, `transcripts`, `recordings` and `attendance`. A meeting is found from the join link on a calendar event; its transcripts are read as text and as entries with speaker, start and end; its recordings and its attendance are listed. This needed one change to the core, a request that says its answer is text. A recording's video is fetched as bytes by a typed method, up to a size its caller sets.

Nothing has been run against the real service; [the guide](./integrations/microsoft.md) lists what was confirmed against Microsoft's documentation and what was not.

## Content: files, recordings and exports

The core fetches content as well as JSON. A provider's definition may declare content hosts, each marked to receive the credential or not; `Connection::fetch` returns the bytes unchanged with the type the host stated, follows a redirect only to a declared host, decides for each host whether the credential goes, lets no host that was not given the credential send the reader back to one that is, and refuses content over the caller's limit (ten megabytes by default) with the error `too_large` instead of cutting it short. A file fetched by an operation called by name comes back as text or not at all: bytes are never handed over by name, and text is limited to a megabyte unless the caller asks for more. Microsoft is the first to use it, for a recording and for a mail attachment.

Not done: sending bytes to an address a provider issues, which Slack's upload needs; a stream, so that a file need not be held in memory; and a content host named by a pattern, which a provider that serves each tenant from its own host name needs (SharePoint, for OneDrive files). No provider declares a content host yet: Slack's and Zoom's file hosts are to be added with the methods that read from them.

## Google, built out on the provider that was there

`socketkit-google` was one file with identity and Drive lookup. It now has 60 typed methods, each also a named operation, on five of Google's products. The crate carries its own copy of the operation machinery, as Slack and Microsoft do.

Gmail is 19 methods in `gmail_messages`, `gmail_threads`, `gmail_labels`, `gmail_drafts` and `gmail_profile`. A list returns ids only; a message is read decoded, with its headers, its text, its HTML and what is attached; mail is written from structured content and sent, drafted, and answered in its thread; labels are changed and a message is binned and brought back. A reply goes to the people its caller names and to nobody else: nothing in the message it answers decides who receives it. Not covered: attachments on what is sent, a draft that is a reply, reply-all and forward, creating labels, history and push.

Calendar is 10 methods in `calendar_list`, `calendar_events` and `calendar_freebusy`: events in a window with recurring ones expanded, free and busy times, creating an event with a Meet link, changing, answering and deleting one. An answer changes only the calendar owner's own entry, and names the version it read so that a change made in between is not lost.

Meet is 12 methods, all of them reads: conference records, participants and their sessions, transcripts and their entries, recordings and spaces. `meet_transcripts.read` returns a whole transcript with each speaker named, as entries of speaker, start, end and text: the four fields a Teams transcript has in the Microsoft crate. The two crates agree on those by convention; there is no shared type in the core yet, and the tracking issue asks for that to be settled before a third transcript is built.

Drive is 10 methods in `drive_files` and `drive_shared_drives`: finding files with Drive's own query language, metadata, who can see a file, a Google document exported as text, and making a folder, copying, moving, renaming and binning. Docs and Sheets are 9 more: a document read as plain text with its tabs, created and added to; a spreadsheet's sheets and sizes, and its cells read, written and appended by range. Downloading a file that is not a Google document returns bytes and waits for the content request.

Sending mail, answering an invitation, changing an event, overwriting a draft or cells, moving a file, binning a message or a file and deleting are marked destructive. A PUT and a DELETE are still repeated by the transport after a server error, which the guide spells out for the four operations that use them.

Nothing has been run against the real service; [the guide](./integrations/google.md) lists what was confirmed against Google's documentation and what was not.

## Carried forward from reviews

- Decide before the first release whether public data structs (`ProviderSpec`, `OAuth2Spec`, `OperationInfo`, `TokenSet`) become `#[non_exhaustive]` with constructors. Today adding a field breaks every integration crate.
- Retrying is decided by HTTP method. The spec says the provider's classifier should decide; Slack accepts GET for some writes, so this matters before Slack gets write operations in phase 2.
- Notion treats any 400 on a lookup as "not found", including a 400 that means something else. (Zoom now reads its own error code.)
- Slack channel lookup by name reports "not found" after 20 pages even if the workspace has more.
- A request path may climb out of `api_base` with `..` to another path on the same allowed host.
- An application-supplied HTTP client builder is trusted apart from redirects: a shared cookie store or default headers would apply to every tenant.
- Single-flight refresh is per process. Two instances of an application can still refresh the same connection at once.
- Enforce the dependency rules in CI; add `cargo hack --each-feature` and `cargo deny`.
