# Socket — vision

**Status:** Draft for review
**Date:** 2026-10-08
**Owner:** Vasanth

## In one sentence

Socket is a Rust library that lets any application plug into external services (Slack, GitHub, Linear, Notion, Google and more) by turning on a Cargo feature, while the application keeps its own credentials.

## The problem

Every team that builds a product on top of other tools writes the same code again:

- the OAuth dance for each provider, with each provider's quirks;
- token storage and refresh;
- pagination, retries, and telling "you are rate limited" apart from "you are not allowed";
- webhook signature checks;
- and, since 2024, a second copy of all of it shaped as tools for an AI agent.

In Rust the situation is worse than elsewhere. GitHub, Slack and Stripe have good community clients. Notion, Linear, Jira and Salesforce do not. The good ones disagree on HTTP stack, error shape and auth, so an application that needs five services carries five conventions. No maintained Rust library offers them behind one interface (see [the landscape research](./research/2026-10-08-integrations-library-landscape.md)).

The alternatives all take something away:

| Alternative | What you give up |
| --- | --- |
| Hosted integration platforms (Composio, Nango, Merge and others) | Your users' tokens live on someone else's servers, and you depend on their pricing and licence. |
| MCP servers | Good for an agent calling a tool. No webhooks, no typed operations, no answer for where a backend stores thousands of tokens. |
| Writing it yourself | The weeks it takes, per service, forever. |

## What Socket is

A library, compiled into your program. Nothing to deploy.

```toml
socketkit = { version = "0.1", features = ["slack", "github"] }
```

With that line an application gets:

1. **Typed clients** for the services it enabled, with one error model, one retry policy and one pagination model across all of them.
2. **Auth it controls.** The application brings its own OAuth app and decides where tokens are stored. Socket runs the flow and refreshes tokens; it never holds them.
3. **The same operations as agent tools.** Every operation can describe itself (name, input schema, whether it reads or writes), so one implementation serves backend code, an in-process agent, and a tool registry.
4. **Webhook verification** for services that send events.
5. **MCP as one more feature**, for reaching any service that publishes a server without Socket writing a client for it.

## What Socket is not

- **Not a hosted service.** There is no Socket cloud, no auth proxy, no account.
- **Not a workflow engine or an agent framework.** Those sit above it. Cognis and stev are two such consumers.
- **Not a unified data model.** A Linear issue and a Jira issue stay different types. Socket unifies the plumbing, not the vendors.
- **Not a catalogue of thousands.** See the next section.

## How it reaches breadth honestly

The ambition is that nobody rewrites an integration twice. The research is blunt about what happens to projects that promise a long tail: they become hosted services, restrict their licence to fund the upkeep, or leave hundreds of connectors unmaintained. Socket reaches breadth in three layers instead, and says which layer a service is in.

| Layer | What it is | Realistic size |
| --- | --- | --- |
| **Deep integrations** | Hand-written typed clients, webhooks and tools, each with a named owner and live tests | Dozens |
| **Provider catalogue** | Auth definitions only (endpoints, scopes, quirks), so an application can authenticate and make its own calls with Socket's plumbing | Hundreds |
| **MCP** | A client for any vendor's MCP server, using the same token storage | Whatever vendors publish |

The architecture puts no ceiling on the first layer: each integration is its own crate, so the catalogue grows as fast as people show up to own integrations, and no faster.

## Principles

1. **The application owns its credentials.** Bring your own OAuth app, your own token store. This is the reason to choose a library over a platform, so nothing may weaken it.
2. **Opt in at compile time.** An integration you did not enable is not in your binary.
3. **Depth before breadth.** An integration that covers four operations well is worth more than one that lists forty and fails on the second page of results.
4. **Every integration has an owner and a tier.** Unowned code is archived, not left to rot.
5. **Fail loudly.** A missing credential, an unsupported operation, or a skipped live test is an error, never a silent success.
6. **No dependency on any framework.** Socket does not depend on Cognis, stev, or any agent library. Bridges depend on Socket, never the reverse.

## Who it is for

- **Rust backend teams** building products that connect to their customers' tools.
- **Agent builders in Rust** who need real tools with real auth, not demo stubs.
- **stev and Cognis**, the first two consumers. stev needs it to route agent tool calls to real services; Cognis gains a tool catalogue through a bridge crate.

## What success looks like

Twelve months after the first release:

- stev runs entirely on Socket for external services, with no integration code of its own.
- An application goes from `cargo add` to an authenticated API call in about thirty lines, with no server and no database.
- At least one integration is owned by someone outside the founding team.
- A new integration can be added without changing the core crate.
- Every published integration either passes live tests weekly or is marked archived.

## Why this, why now

- **The slot is empty.** The one Rust attempt at this shape was published once in December 2025 and abandoned.
- **Demand is proven elsewhere.** Corsair, the closest equivalent in TypeScript, reached 13,422 GitHub stars in its first year.
- **We already have a seed.** stev's private `connector` crate has working OAuth for six providers and 45 tests. Socket starts as an extraction of that, not a blank page.
- **Custody matters more each year.** One hosted integration platform disclosed a breach of about 5,000 OAuth tokens in May 2026. A library where tokens never leave the application is a direct answer.

Figures in this section come from the landscape research dated 2026-10-08.

## Related documents

- [Project design spec](./superpowers/specs/2026-10-08-socket-project-design.md) — architecture, phases and acceptance criteria.
- [Landscape research](./research/2026-10-08-integrations-library-landscape.md) — what exists, with sources.
