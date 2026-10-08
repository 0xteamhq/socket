# Socket — vision

**Status:** Draft for review
**Date:** 2026-10-08
**Owner:** Vasanth

## In one sentence

Socket is an open-source library that gives any product the connection, authorisation and operation layers for external services, so a team that needs integrations embeds it and runs it on its own machines instead of building those layers again.

## The problem

Take n8n. To be useful it has to talk to hundreds of other systems, and it built every one of those connectors itself: 308 first-party node directories, 412 credential definitions, and a shared layer that runs OAuth, stores tokens and refreshes them. Activepieces did the same work again for 736 pieces. So did Pipedream, Zapier and Make.

Every product that needs integrations goes through the same layers:

- describing each service: its endpoints, how it authenticates, which scopes it needs, where it breaks the standard;
- the OAuth flow, token storage and refresh;
- pagination, retries, and telling "you are rate limited" apart from "you are not allowed";
- webhook signature checks;
- the operations themselves: send a message, create an issue, list the files;
- and, more recently, the same operations again as tools for an AI agent.

None of that work can be picked up and reused. n8n's and Pipedream's connectors are under licences that forbid it, and they only run inside their own platforms. The open ones are tied to a runtime: one Rust workflow engine that needed connectors ended up running Activepieces pieces in a separate Node.js process and handing it raw access tokens. That is the state of the art for a team that wants integrations inside its own product today.

The alternatives each take something away:

| Alternative | What you give up |
| --- | --- |
| Hosted integration platforms | Your users' tokens are held by a vendor, and you depend on its pricing and licence. One such vendor disclosed in May 2026 that attackers took about 5,200 API keys and 5,000 GitHub OAuth tokens. |
| Another platform's connectors | Most licences forbid it. Where it is allowed, you run a second runtime and still write auth, refresh and storage yourself. |
| MCP servers | Good for an agent calling a tool. No webhooks, no typed operations, no answer for where a backend stores thousands of tokens. |
| Writing it yourself | The weeks it takes, per service, forever. |

## What Socket is

A library, compiled into your program. Nothing to deploy, nothing hosted.

```sh
cargo add socketkit --features slack,github
```

It provides three layers, and a product can use any of them without the ones above it.

| Layer | What it does |
| --- | --- |
| **Connection** | A registry that describes each service as data: where it lives, how it authenticates, its scopes and its quirks. A connection is one service authorised for one of your users or tenants. |
| **Authorisation** | Runs the OAuth flow or takes an API key, refreshes tokens before they expire, and serialises refreshes so two calls never race. Tokens are stored wherever the application decides; Socket never holds them. |
| **Operations** | The unit functions for a service: post a message, create an issue, fetch a page. Each is a typed Rust method and is also callable by name with JSON, with a schema that says what it takes, what it returns and whether it reads or writes. |

Under all three sits one transport: one HTTP client, one retry policy, one pagination model, one error model. Beside them sit webhook verification, for services that send events, and a generic authenticated request, for calling any endpoint Socket has no operation for yet.

Because every operation can be called by name and describes itself, one implementation serves backend code, an AI agent's tool list, an MCP server and programs written in other languages.

## Who it is for

Products for which integrations are a cost and not the product itself:

- **New workflow and automation engines** that would otherwise have to rebuild what n8n built.
- **Agent products** that need real tools with real auth, where the customer's tokens must not sit with a third party.
- **Self-hosted and on-premise software**, which cannot depend on a hosted integration vendor at all.
- **Rust backends** that connect to their customers' tools.

n8n is the example of the problem, not the expected adopter. Its catalogue is its product, its connectors run on Node.js under its own rules, and no workflow platform has ever adopted another project's connectors. Socket is for the next team that would otherwise have to do what n8n did.

## How it reaches breadth

The ambition is that nobody writes an integration twice. The evidence on large catalogues is blunt, and the design follows it.

No catalogue that was measured keeps thousands of integrations working. Airbyte certifies 81 of its 643 connectors after six years. In Pipedream's repository 37% of the listed apps have an auth definition and no operations at all. The projects that promised a long tail of hand-written connectors became hosted services, restricted their licences to fund the upkeep, or abandoned the code.

So Socket puts no ceiling on the catalogue and makes no promise about its size. It may reach hundreds of services or far more; nobody knows yet. It starts from [the hundred services products integrate with most](./catalogue.md), reaches breadth in layers, and says which layer a service is in:

| Layer | What you can do with a service in it | Size it can honestly reach |
| --- | --- | --- |
| **Registry** | Authorise a user and keep the token fresh | Hundreds. Auth describes well as data and changes rarely. |
| **Generic request** | Call any endpoint of that service with auth, retry and pagination handled | The same hundreds, with no code written per service |
| **Verified integrations** | Use typed operations and webhooks that have a named owner and tests against the real service | Dozens, growing as owners appear |
| **Definitions and a verifier** | Add or repair an operation yourself, or have a coding agent do it, and check it against the real API | Open-ended. This is how the long tail gets covered. |
| **MCP** | Reach any service whose vendor publishes a server, using the same token storage | Whatever vendors publish |

The fourth layer is the answer to "thousands". Writing an operation has become cheap: a coding agent produces one in minutes. Knowing that it works has not. So the project does not promise to maintain every operation for every service. It provides the registry, the authenticated client, a format for describing operations and a verifier, so the team that needs an operation can add it and trust it.

## Other languages

Socket supports other languages; that is a commitment, not an option. The library is written in Rust and the first phase is Rust only, because it has to work there before it is carried anywhere else. Other languages follow in this order:

1. **Rust**, as a crate.
2. **A local process** that speaks a simple protocol and MCP, so a Node.js, Python or Go program on the same machine can use every operation without Rust.
3. **In-process bindings**, starting with Node.js and Python, then further languages one at a time.

To keep steps 2 and 3 possible, the core has one rule from the first day: everything it offers can be reached through a narrow surface of plain data. Operations are invoked by name with JSON, errors are a flat list of codes, and the pieces the application supplies, such as the token store, are simple interfaces another language can implement.

Two limits are stated here so nobody is surprised later. Choosing integrations at compile time is a Rust convenience; packages for other languages ship with a fixed set. And every in-process binding is a separate build and release pipeline, which is why they are added one language at a time.

## What Socket is not

- **Not a hosted service.** There is no Socket cloud, no auth proxy, no account.
- **Not a workflow engine or an agent framework.** Those sit above it.
- **Not a unified data model.** A Linear issue and a Jira issue stay different types. Socket unifies the plumbing, not the vendors.
- **Not a promise of thousands of hand-maintained integrations.** See the section on breadth.

## Principles

1. **The application owns its credentials.** Bring your own OAuth app, your own token store. This is the reason to choose a library over a platform, so nothing may weaken it.
2. **Every layer is useful alone.** A team can take the registry and the auth flow and write its own calls.
3. **One implementation, every caller.** An operation is written once and serves typed Rust code, agents, MCP and other languages.
4. **Depth before breadth.** An integration that covers four operations well is worth more than one that lists forty and fails on the second page of results.
5. **Say what is verified.** Every service has a tier and every verified integration has an owner. Nothing is listed as supported on the strength of code that nobody runs.
6. **Fail loudly.** A missing credential, an unsupported operation, or a test that could not run is an error, never a silent success.
7. **Nothing in the core that cannot cross a language boundary.**
8. **No dependency on any framework or product.** Adapters depend on Socket, never the reverse.

## What success looks like

Twelve months after the first release:

- A product replaces its home-grown or second-runtime integration layer with Socket for ten services, and gains token refresh, custody of its own tokens, and webhooks in the exchange.
- A Rust application goes from `cargo add` to an authenticated call in about thirty lines when it already holds a token or API key, with no database and no Socket-operated server.
- A Node.js or Python program completes an OAuth connection and calls an operation through the local process, with no Rust written.
- A team adds an operation Socket did not ship, using the definition format and the verifier, without changing Socket's core.
- At least one verified integration is owned by someone outside the founding team.
- Every verified integration has passed its tests against the real service within the last 30 days, or has been moved down a tier.

## Why this, why now

- **Nobody has assembled it.** Every part exists somewhere, and no maintained project combines them. The two closest are Corsair, in TypeScript, which requires its own database tables and steers users to a hosted hub, and Ampersand's `connectors`, in Go, which leaves the OAuth flow to its hosted platform and offers generic reads and writes instead of unit operations. In Rust there is nothing maintained.
- **Teams are improvising it.** Beyond the engine running pieces in a second process, other projects have each built one layer for themselves: an auth gateway, a token-storage wrapper, a compatibility shim.
- **Custody matters more each year.** A library where tokens never leave the application is a direct answer to the May 2026 breach.
- **Coding agents changed what is scarce.** The operation is now the cheapest part of an integration and the verified, authorised connection is the most expensive. That favours a shared core over a shared pile of hand-written connectors.

## The risks, stated plainly

- **Operations are the part others walked away from.** Spring Social, Trigger.dev and LangChain each dropped shared per-service bindings and said so in writing. Socket keeps its verified set small and puts the rest behind definitions and a verifier for this reason.
- **A thin core does not justify bindings.** One project replaced its cross-language Rust SDK with plain per-language ones because the shared part was only HTTP calls. Socket's shared part has to be the hard part: the auth lifecycle, refresh, pagination, retries and webhooks.
- **Demand is revealed, not stated.** Teams build this for themselves, but a search found nobody asking for it by name, and buyers with budgets choose hosted catalogues.
- **Verification needs real accounts.** Testing against a live service needs a working account for it, and nobody has published what that costs to keep up.
- **No sponsor.** Every earlier attempt was the open edge of a company's hosted product and ended when the company moved on. A neutral project has not been tried.

## Starting point

Socket is its own project. It does not start from a blank page: OAuth and resource-checking code for six services, with 45 tests, already exists in a private product by the same author and can seed the core. That product is one early user of Socket, not its definition.

`socketkit` is a working name for the crate. The name is not settled; see the design spec.

Figures in this document come from the two research reports below, both dated 2026-10-08.

## Related documents

- [Catalogue](./catalogue.md) — the first hundred services and the order they are added.
- [Roadmap](./roadmap.md) — the phases and the plans under each.
- [Project design spec](./superpowers/specs/2026-10-08-socket-project-design.md) — architecture, phases and acceptance criteria.
- [Validation and FFI research](./research/2026-10-08-integration-library-validation-and-ffi.md) — whether this already exists, who would use it, the Rust-core strategy, and how breadth is reached.
- [Landscape research](./research/2026-10-08-integrations-library-landscape.md) — the earlier survey of the Rust ecosystem, hosted platforms and MCP.
