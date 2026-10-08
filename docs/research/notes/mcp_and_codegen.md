# MCP and specification-driven code generation versus a native Rust multi-integration library (state as of 2026-10-08)

Method notes for the report writer:

- Counts marked "measured" were taken directly on 2026-10-08 from the GitHub API, the crates.io API, or a published machine-readable file (the URL is given). They are my own counts, not vendor claims.
- Counts marked "claimed" are a vendor's or directory's own number. Directory counts overlap and use different units (unique servers, version records, GitHub repos), so they must not be added together.
- Items marked "secondary" came from a search-result summary or an aggregator and were not confirmed against a primary source.
- Many sources are integration vendors selling an alternative to raw MCP or to hand-built integrations. Their bias is noted per item.

## 1. MCP ecosystem size and maturity: server counts, vendor-official remote servers, and what the current spec revision supports

### Takeaway
MCP is large and vendor-adopted: the official registry held about 30,000 unique servers in September 2026, and Slack, GitHub, Linear, Notion, Atlassian, Stripe, Salesforce and Google all run official hosted remote servers with OAuth. The current spec revision (2026-07-28) is a breaking redesign (stateless, no handshake, tasks moved to an extension), so the protocol is still churning. Vendor-official servers are a small share of the catalog.

### Cited Findings

Spec revisions and features (dated):

- Revision 2025-06-18 (previous revision 2025-03-26) added elicitation, structured tool output and resource links. It classified MCP servers as OAuth Resource Servers with protected resource metadata, required clients to implement RFC 8707 Resource Indicators, and removed JSON-RPC batching. — [MCP changelog 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/changelog)
- Revision 2025-11-25 added experimental tasks (SEP-1686: "tracking durable requests with polling and deferred result retrieval"), URL-mode elicitation (SEP-1036), OAuth Client ID Metadata Documents as the recommended client registration mechanism (SEP-991), OpenID Connect Discovery support, incremental scope consent (SEP-835), tool calling in sampling (SEP-1577), and an SDK tiering system (SEP-1730). — [MCP changelog 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/changelog)
- Revision 2026-07-28 is the current one. Its major changes are:
  - Protocol-level sessions and the `Mcp-Session-Id` header are removed (SEP-2567).
  - The `initialize` handshake is removed; every request carries protocol version and client capabilities in `_meta`, and servers must implement a new `server/discover` RPC (SEP-2575).
  - The HTTP GET endpoint and `resources/subscribe` are replaced by `subscriptions/listen`.
  - `ping`, `logging/setLevel` and `notifications/roots/list_changed` are removed.
  - Tasks move out of core into an official extension `io.modelcontextprotocol/tasks`, redesigned around polling `tasks/get` plus `tasks/update` (SEP-2663).
  - Server-initiated requests (sampling, elicitation, roots) are replaced by the Multi Round-Trip Requests pattern, with a required `resultType` field on all results (SEP-2322).
  - SSE resumability (`Last-Event-ID`) is removed: "A broken response stream loses the in-flight request; clients MUST re-issue it".
  — [MCP changelog 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/changelog)
- The same revision deprecates Roots, Sampling and Logging (SEP-2577), reclassifies the old HTTP+SSE transport as Deprecated, and deprecates OAuth Dynamic Client Registration (RFC 7591) in favor of Client ID Metadata Documents. It adds an `extensions` capability field, `ttlMs`/`cacheScope` caching hints on list results (SEP-2549), required `Mcp-Method`/`Mcp-Name` HTTP headers (SEP-2243), and a feature lifecycle policy with a minimum twelve-month deprecation window (SEP-2596). — [MCP changelog 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/changelog)
- Secondary coverage describes the 2026-07-28 release as shipping MCP Apps (server-rendered UIs) and Tasks as extensions, and says the Enterprise-Managed Authorization extension is now stable. — [MCP blog tag page (search summary)](https://blog.modelcontextprotocol.io/tags/mcp/); [Vercel changelog](https://vercel.com/changelog/vercel-mcp-now-supports-the-2026-07-28-mcp-specification)
- GitHub's MCP server announced support for "the next MCP specification" on 2026-07-23, and Cloudflare and Vercel announced 2026-07-28 support on or around release day. — [GitHub changelog 2026-07-23](https://github.blog/changelog/2026-07-23-github-mcp-server-supports-the-next-mcp-specification/); [Cloudflare changelog 2026-07-28](https://developers.cloudflare.com/changelog/post/2026-07-28-cloudflare-mcp-servers-mcp-2026-07-28/)

Official registry (third-party analysis of the registry API, snapshot 2026-09-10):

- 30,375 unique servers and 99,114 total version records. This is roughly 3x the May 2026 figure of about 9,650, and August 2026 alone added 6,265 servers. — [dev.to: The MCP Registry by the numbers](https://dev.to/amareswer/the-mcp-registry-by-the-numbers-38nc)
- Deployment split: 54.8% remote only, 38.9% local packages only, 4.9% both, 1.4% neither declared. Transports: 17,584 `streamable-http` versus 1,073 on deprecated `sse`. Packages: npm 8,673, PyPI 3,684, mcpb 1,158, OCI 899. — [same source](https://dev.to/amareswer/the-mcp-registry-by-the-numbers-38nc)
- Publisher concentration: 67.2% sit under `io.github.*` namespaces and 32.8% under verified domains. 16,356 publishers have exactly one server, and the largest single publisher (`io.github.sadri-dridi`) has 1,505 servers. — [same source](https://dev.to/amareswer/the-mcp-registry-by-the-numbers-38nc)
- Maintenance signals: 62% have exactly one published version, the median is one version per server, and 22.9% link no source repository (37% of remote-only servers). The registry "records no security reviews, authentication methods, or uptime checks". — [same source](https://dev.to/amareswer/the-mcp-registry-by-the-numbers-38nc)
- Vendor participation in the official registry is thin: `com.microsoft` has 16 servers, `io.github.github` 1, `com.anthropic` 0. — [same source](https://dev.to/amareswer/the-mcp-registry-by-the-numbers-38nc)
- The registry repository describes itself as "A community driven registry service for Model Context Protocol (MCP) servers" (7,326 stars, measured 2026-10-08). — [modelcontextprotocol/registry](https://github.com/modelcontextprotocol/registry)

Community directories (claimed, overlapping, different units):

- Glama displayed 97,610 MCP servers on 2026-10-08 ("Updated 2026-10-08 04:00"): 40,169 remote-capable, 35,283 local-only, 17,331 hybrid, and 6,579 labeled "Official". — [Glama registry](https://glama.ai/mcp/servers)
- Other sources report Glama at 50,845 (as of 2026-07-03) and at 21,586, which shows how unstable these figures are. — [alatirok MCP server statistics 2026 (secondary)](https://alatirok.com/mcp-server-statistics-2026/)
- PulseMCP displayed 21,738 servers on 2026-10-08, with filters for Anthropic References, Official Providers and Community but no per-class counts. — [PulseMCP](https://www.pulsemcp.com/servers)
- Smithery is reported at "over 6,000 servers as of early 2026" and mcp.so at "over 20,000" (secondary; neither site was fetched). — [alatirok MCP server statistics 2026 (secondary)](https://alatirok.com/mcp-server-statistics-2026/)
- On why the counts diverge: "each source measures a different unit: deduped server names, every published version, GitHub repositories with a topic tag, or a multi-registry union without cross-source dedupe." — [alatirok MCP server statistics 2026 (secondary)](https://alatirok.com/mcp-server-statistics-2026/)

Vendor-official remote servers (dated):

- Linear: remote server at `https://mcp.linear.app/mcp`, shipped 2025-05-01. — [Linear changelog 2025-05-01](https://linear.app/changelog/2025-05-01-mcp); [Linear MCP docs](https://linear.app/docs/mcp)
- GitHub: remote server in public preview 2025-06-12, generally available September 2025. The server repo has 33,444 stars (measured 2026-10-08). — [GitHub changelog 2025-06-12](https://github.blog/changelog/2025-06-12-remote-github-mcp-server-is-now-available-in-public-preview/); [GA write-up (secondary)](https://dev.classmethod.jp/en/articles/github-remote-mcp-ga); [github/github-mcp-server](https://github.com/github/github-mcp-server)
- Notion: hosted server at `mcp.notion.com/mcp` with browser OAuth; a secondary source counts 18 tools. — [Notion MCP docs](https://developers.notion.com/docs/mcp); [StackOne deep dive (secondary)](https://stackone.com/blog/notion-mcp-deep-dive)
- Atlassian: "Official remote MCP server for Atlassian" covering Jira, Confluence, Jira Service Management, Bitbucket and Compass, "using OAuth 2.1 or API tokens". — [atlassian/atlassian-mcp-server](https://github.com/atlassian/atlassian-mcp-server)
- Stripe: official hosted server at `https://mcp.stripe.com` with OAuth, or local with a restricted API key (secondary). — [MCP Playground catalog (secondary)](https://mcpplaygroundonline.com/blog/awesome-mcp-servers.md); [stripe/ai repo](https://github.com/stripe/ai)
- Slack: MCP server and Real-time Search API generally available 2026-02-17, at `mcp.slack.com/mcp` with user-token OAuth. — [Slack developer changelog 2026-02-17](https://docs.slack.dev/changelog/2026/02/17/slack-mcp/)
- Salesforce: Hosted MCP Servers generally available 2026-04-29 for Enterprise Edition orgs and above (beta October 2025). — [Salesforce Developers blog](https://developer.salesforce.com/blogs/2026/04/salesforce-hosted-mcp-servers-are-now-generally-available)
- Google: managed remote MCP servers announced 2025-12-10 (BigQuery, Maps, GKE, GCE). At Cloud Next on 2026-04-28 Google said "more than 50 Google-managed MCP servers are generally available or in preview". — [TechCrunch 2025-12-10](https://techcrunch.com/2025/12/10/google-is-going-all-in-on-mcp-servers-agent-ready-by-design/); [Google Cloud blog](https://cloud.google.com/blog/products/ai-machine-learning/google-managed-mcp-servers-are-available-for-everyone)
- Nango's open-source provider catalog has 36 provider entries whose auth mode is MCP-based (33 `MCP_OAUTH2`, 3 `MCP_OAUTH2_GENERIC`) out of roughly 1,046 entries (measured 2026-10-08 by counting keys in the file). This is a rough lower bound on vendor remote MCP servers that one integration vendor has chosen to wire up. — [NangoHQ/nango providers.yaml](https://github.com/NangoHQ/nango/blob/master/packages/providers/providers.yaml)

### Inferences
- Order of magnitude: tens of thousands of registered servers, but vendor-official remote servers for mainstream SaaS number in the dozens to low hundreds. Nango's 36 MCP-auth providers and Google's "50+" point the same way. Glama's "6,579 Official" label almost certainly uses a looser definition.
- The long tail is shallow: a median of one version, 62% single-version, one publisher with 1,505 entries, and no security review. Registry size is weak evidence that "the integration already exists at production quality".
- Three spec revisions in about 13 months, the last one removing sessions, the handshake, resumability and three core features, mean that a library built on MCP as its substrate inherits protocol churn. The new twelve-month deprecation policy is the first formal stability commitment.
- For the top 10 to 50 SaaS tools, "connect once, reuse everywhere" is largely true for interactive, user-delegated, LLM-driven tool calls. Whether it is true for backend integration is covered in question 3.

### Gaps
- I could not fetch the official MCP blog post for the 2026-07-28 release; MCP Apps and Enterprise-Managed Authorization status rest on search summaries and vendor changelogs.
- No authoritative count of vendor-official remote servers exists. The registry does not flag "official vendor", and directory labels are self-defined.
- Smithery and mcp.so counts were not checked on the sites themselves.
- The Towards AI article "MCP Registries in Mid-2026: One Upstream Won" returned HTTP 403 and was not read.
- I did not verify the official registry's launch date or its current GA/preview status.

## 2. MCP in Rust: status of `rmcp` and other Rust MCP crates as a client for many remote servers

### Takeaway
The official Rust SDK (`rmcp`) is Tier 1, heavily downloaded, tracks the 2026-07-28 spec, and has a streamable-HTTP client plus OAuth features, so consuming remote MCP servers from Rust is practical. Its API has gone through three major versions in 2026 (1.x, 2.0, 3.0), so anything built on it should expect breaking upgrades.

### Cited Findings
- The Rust SDK is listed as Tier 1 alongside TypeScript, Python, C#, Go and Ruby; Java is Tier 2 and Swift, PHP and Kotlin are Tier 3. — [MCP SDKs page](https://modelcontextprotocol.io/docs/sdk)
- Release cadence (measured from GitHub releases, 2026-10-08): `rmcp-v1.1.0` on 2026-03-04; 1.2 through 1.8 between March and 2026-06-23; `v2.0.0` on 2026-06-29; `v3.0.0` on 2026-07-28 (the spec release day, after five betas from 2026-07-23); then 3.0.1 through 3.5.1, with the latest `v3.5.1` on 2026-10-05. That is about 13 releases in the 10 weeks after 3.0.0. — [rust-sdk releases](https://github.com/modelcontextprotocol/rust-sdk/releases)
- Repository stats (measured 2026-10-08): 3,986 stars, 653 forks, 57 open issues, created 2025-02-18, last push 2026-10-06. — [modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk)
- crates.io (measured 2026-10-08): `rmcp` 3.5.1, 33.3 million total downloads, 17.9 million recent downloads, first published 2025-03-16. — [crates.io: rmcp](https://crates.io/crates/rmcp)
- Client-relevant Cargo features in `rmcp` (measured from `Cargo.toml` on main): `client`, `transport-streamable-http-client`, `transport-streamable-http-client-reqwest`, `transport-streamable-http-client-unix-socket`, `transport-child-process` (stdio), `auth` (OAuth via the `oauth2` crate), `auth-client-credentials-jwt`, `auth-enterprise-managed`, `elicitation`, and `request-state` (SEP-2322 helper). Default features are `base64`, `macros`, `server`; the client is opt-in. A `transport-ws` feature is commented out. — [rmcp Cargo.toml](https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/Cargo.toml)
- Recent release notes mention `ServerConfig`/`ClientConfig` types replacing deprecated `ServerInfo`/`ClientInfo` aliases (3.4.0), a `LATEST_WITH_INITIALIZE` constant (3.5.0), refresh-token exchanges and reconnect backoff fixes (3.3.0), and transport fallback after rejections (3.4.1). — [rust-sdk releases](https://github.com/modelcontextprotocol/rust-sdk/releases)
- A docs summary describes rmcp as supporting the 2026-07-28 specification, OAuth 2.0 authentication and the SEP-2663 Tasks extension, on the tokio runtime. — [docs.rs: rmcp](https://docs.rs/crate/rmcp)
- Other Rust MCP crates (measured on crates.io 2026-10-08):

  | Crate | Version | Total downloads | Last updated |
  | --- | --- | --- | --- |
  | `rust-mcp-sdk` | 2.0.0 | 305,401 | 2026-08-27 |
  | `pmcp` | 2.22.7 | 126,435 | 2026-10-04 |
  | `turbomcp` | 4.0.0-alpha.5 | 33,074 | 2026-10-01 |
  | `mcp-protocol-sdk` | 0.5.1 | 9,317 | 2025-08-03 |
  | `mcpkit` | 0.7.0 | 4,375 | 2026-07-27 |

  — [rust-mcp-sdk](https://crates.io/crates/rust-mcp-sdk); [pmcp](https://crates.io/crates/pmcp); [turbomcp](https://crates.io/crates/turbomcp); [mcp-protocol-sdk](https://crates.io/crates/mcp-protocol-sdk); [mcpkit](https://crates.io/crates/mcpkit)

### Inferences
- `rmcp` has roughly 100x the downloads of the next Rust MCP crate, so it is the only realistic base for a Rust MCP client layer. The alternatives are niche.
- A Rust library could expose "any remote MCP server" behind a single `mcp` Cargo feature at low cost: `rmcp` with `client`, `transport-streamable-http-client-reqwest` and `auth`. That gives breadth without writing per-vendor code. It delivers LLM-shaped tools, not typed Rust APIs.
- Three majors in seven months, plus a 3.4.0 that renamed core config types, mean a public API that re-exports `rmcp` types will break downstream users often. The MCP layer should be wrapped, not re-exported.
- The `LATEST_WITH_INITIALIZE` constant and "transport fallback after rejections" suggest clients must still handle both pre-2026-07-28 servers (handshake, sessions) and stateless servers. A multi-server client faces a mixed-version fleet for at least the twelve-month deprecation window.

### Gaps
- I did not test `rmcp` as a client against real vendor servers. Interop quality with Slack, Atlassian, Salesforce and other vendor OAuth implementations is unverified.
- I did not find the `rmcp-v1.0.0` release date.
- No independent benchmark or production write-up of a Rust process holding connections to many remote MCP servers was found.
- I did not check whether `rmcp` implements the draft MCP Events extension.

## 3. What MCP does not cover for a backend integration library

### Takeaway
MCP standardizes LLM-facing tool calls with user-delegated OAuth. It does not standardize inbound events (still a draft extension), bulk sync, normalized data models, or multi-tenant credential storage, and the community server supply has documented quality and security problems. These gaps are the functional space a backend integration library occupies.

### Cited Findings

Inbound webhooks and events:

- The MCP Triggers and Events Working Group was chartered 2026-03-24 (leads: Clare Liguori of AWS and Peter Alexander of Anthropic). Its charter states: "Today, clients learn about server-side updates by polling or holding an SSE connection open. This WG will specify a standardized callback mechanism—webhooks or similar". The charter lists the events SEP as "Ideating" with an "End April" target, and an incubation repo `experimental-ext-triggers-events`. — [Triggers and Events WG charter](https://modelcontextprotocol.io/community/working-groups/triggers-events)
- The 2026-07-28 changelog contains no events or webhook primitive. The only server-to-client push is `subscriptions/listen` for list-changed and resource-subscription notifications. — [MCP changelog 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/changelog)
- OpenAI documents "MCP Events" support in ChatGPT built on "the draft MCP Events specification", with `events/list`, `events/subscribe` and `events/unsubscribe`, and Standard Webhooks HMAC signing. It is webhook delivery only: "Polling, streaming, and the draft's `gap` and `terminated` control notifications are not supported by this integration". Payloads are capped at 256 KiB and there is no replay or gap recovery. — [OpenAI developers: MCP Events](https://developers.openai.com/plugins/build/mcp-events)
- A search summary says OpenAI announced this at DevDay on 2026-09-29 (secondary; date not confirmed on the primary page). — [OpenAI developers: MCP Events](https://developers.openai.com/plugins/build/mcp-events)

Async and long-running work:

- Tasks are an extension, not core, as of 2026-07-28. They were redesigned to polling (`tasks/get`), and `tasks/list` was removed. — [MCP changelog 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/changelog)
- SSE resumability was removed in the same revision, so a dropped stream loses the in-flight request and the client must re-issue it. — [MCP changelog 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/changelog)

Sync, auth and rate limits (vendor position, commercially biased):

- Nango (sells integration infrastructure) says agent integrations require four jobs: "authenticate the user, expose tool calls, keep RAG data in sync, and react to upstream changes". It adds that platforms need "handling tool calls and data syncs, along with customer authentication, permissions, scopes, rate limits, and token refresh" (published 2026-08-20). — [Nango blog](https://nango.dev/blog/best-embedded-integrations-platform-for-ai-agents)

Credential handling in the wild:

- Astrix analyzed over 5,200 open-source MCP server implementations ("State of MCP Server Security 2025"). It found 88% require credentials, 53% rely on long-lived static secrets (API keys, personal access tokens), OAuth is used by 8.5%, and 79% of API keys are passed through environment variables. Astrix sells non-human-identity security. — [Astrix: State of MCP Server Security 2025](https://astrix.security/learn/blog/state-of-mcp-server-security-2025/)
- The official registry records no authentication method for listed servers. — [dev.to registry analysis](https://dev.to/amareswer/the-mcp-registry-by-the-numbers-38nc)

Quality and security studies:

- "MCP at First Glance" (Hasan et al., arXiv 2506.13538, v1 2025-06-16, v5 2026-04-13) studied 1,899 open-source MCP servers. It found 7.2% with general vulnerabilities, 5.5% with MCP-specific tool poisoning, 66% with code smells and 14.4% with known bug patterns. — [arXiv 2506.13538](https://arxiv.org/abs/2506.13538)
- MCPTox (Wang et al., arXiv 2508.14925, submitted 2025-08-19) tested 45 live real-world MCP servers, 353 tools and 20 LLM agents. The highest attack success rate was 72.8% (o1-mini), the highest refusal rate was below 3% (Claude 3.7 Sonnet), and "more capable models are often more susceptible". — [arXiv 2508.14925](https://arxiv.org/abs/2508.14925)

Named incidents and CVEs (collected by an aggregator, secondary unless noted):

- Invariant Labs published the first tool-poisoning advisory on 2025-04-01. — [mcp.directory security roundup (secondary)](https://mcp.directory/blog/mcp-security-200000-exposed-servers-owasp-mcp-top-10-cves)
- Koi Security disclosed a backdoored `postmark-mcp` package on 2025-09-29 that silently BCCed outbound email (1,643 downloads). — [same source](https://mcp.directory/blog/mcp-security-200000-exposed-servers-owasp-mcp-top-10-cves)
- CVE-2025-6514 in `mcp-remote` (CVSS 9.6, OS command injection via a crafted authorization endpoint URL) and CVE-2025-49596 in MCP Inspector (CVSS 9.4). — [same source](https://mcp.directory/blog/mcp-security-200000-exposed-servers-owasp-mcp-top-10-cves)
- OX Security disclosed a STDIO transport flaw on 2026-04-15 across the Python, TypeScript, Java and Rust SDKs: 7,000+ publicly exposed servers documented and "200,000+" vulnerable instances inferred. The larger number is an inference by the vendor. — [same source](https://mcp.directory/blog/mcp-security-200000-exposed-servers-owasp-mcp-top-10-cves)
- An OWASP MCP Top 10 exists: token mismanagement, scope creep, tool poisoning, supply chain, command injection, intent flow subversion, insufficient authentication, lack of audit, shadow MCP servers, context over-sharing. — [same source](https://mcp.directory/blog/mcp-security-200000-exposed-servers-owasp-mcp-top-10-cves)
- A secondary roundup says over 30 CVEs were filed against MCP implementations in the first months of 2026. — [heyuan110 MCP security 2026 (secondary)](https://www.heyuan110.com/posts/ai/2026-03-10-mcp-security-2026/)
- Composio, a hosted agent-integration platform, disclosed on 2026-05-21 that attackers exfiltrated about 5,241 API keys and 5,001 GitHub OAuth tokens. The initial vector was an employee's compromised Gmail OAuth token, and for API-key connections Composio could not revoke on customers' behalf. — [Composio incident post](https://composio.dev/blog/composio-may-2026-security-incident); [Material Security analysis](https://material.security/resources/the-composio-breach-one-token-10242-doors)

Context cost and non-LLM use:

- An independent engineer (Manveer Chawla, 2026-03-08) states that a typical GitHub MCP server exposes "roughly 90+ tools" costing "~55,000 tokens" at initialization. No external source is cited for the figure. — [Chawla, MCP vs CLI](https://manveerc.substack.com/p/mcp-vs-cli-ai-agents)
- The 2026-07-28 revision loosens `structuredContent` to "any JSON value" and `inputSchema`/`outputSchema` to any JSON Schema 2020-12 keywords. — [MCP changelog 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/changelog)

### Inferences
- Inbound events are the clearest hard gap. As of October 2026 MCP Events is a draft with one major client implementing a subset, with no replay, and with a 256 KiB cap. A library that needs reliable webhooks (signature verification per vendor, replay, ordering, dedupe) cannot get them from MCP today.
- MCP has no notion of "list all records since cursor X with a stable typed schema". Pagination exists for listing tools and resources, not as a data-sync contract, and tool outputs are whatever the vendor chose to return to a model. Bulk sync, incremental cursors and normalized models (one `Issue` type across GitHub, Linear and Jira) are outside the protocol.
- Deterministic non-LLM use is possible (a Rust program can call `tools/call` directly), but tool names, schemas and descriptions are designed for models and can change without a versioned contract. The spec only says servers "SHOULD" return tools in deterministic order. Compile-time typing against remote MCP tools is not available.
- Per-tenant credential storage is out of scope for MCP: the spec defines how a client gets a token, not where a multi-tenant backend stores, refreshes and isolates thousands of them. The Composio breach shows the concentration risk of outsourcing that vault, and is an argument for an embeddable library where the user owns the credential store.
- Remote MCP servers require network access to a vendor-hosted endpoint, and many vendor servers are user-OAuth only (Slack's GA server uses user-token OAuth). Service-account, offline and embedded use cases are poorly served.
- Community servers are a supply-chain risk for a library that would recommend or bundle them (5.5% tool-poisoning prevalence, 53% static secrets, at least one malicious package). Vendor-official remote servers avoid most of this, which narrows the usable MCP catalog to the vendor-official subset.

### Gaps
- I found no source measuring rate-limit behavior of vendor MCP servers, such as whether they surface `Retry-After` or quota headers to clients.
- I found no primary source confirming which vendor MCP servers support non-interactive (service account or client-credentials) auth. This is a key question for backend use.
- The Asana, Supabase and GitHub "toxic agent flow" incidents from 2025 are widely referenced but I did not retrieve primary sources this session, so they are not cited.
- The Eric Holmes post "MCP is dead. Long live the CLI" (2026-02-28) appeared only in a search summary and is unverified.
- The current status of the MCP Events SEP (accepted or draft, target revision) is unknown beyond "draft". The charter page still shows "Ideating" with an April target, which looks stale.

## 4. What practitioners and vendors say about MCP versus direct API integration versus unified API

### Takeaway
Integration vendors have converged on a "MCP is one interface, not the whole integration" position, and all of them now sell an MCP surface on top of their own catalogs. Independent engineers mostly criticize MCP on token cost and favor CLIs or code execution for local, developer-facing work. Every vendor position below is commercially motivated.

### Cited Findings
- Nango (open-source integration infrastructure with a paid cloud) argues that closed unified APIs have "Limited API coverage: Unified APIs only support certain categories of integrations, and only a limited number of APIs", and that embedded iPaaS means "Integrations need to be built one by one". It positions itself as code-first with pre-built pieces. — [Nango blog: how Nango differs](https://www.nango.dev/blog/how-is-nango-different-from-embedded-ipaas-or-unified-api)
- Nango claims "1,000+ API integrations and 7,000+ pre-built tools" (2026-08-20). Its repo description is "Connect your agents & product to 1,000 APIs" with 12,554 stars (measured 2026-10-08). Other 2026 pages say 800+ or 900+ APIs and 6,000+ tools, so the numbers moved during the year. — [Nango blog](https://nango.dev/blog/best-embedded-integrations-platform-for-ai-agents); [NangoHQ/nango](https://github.com/NangoHQ/nango)
- Nango runs a hosted MCP server that exposes its action functions as tools scoped per user connection, plus a separate "Management MCP". — [Nango blog: Management MCP](https://nango.dev/blog/how-to-build-ai-agent-integrations-using-the-nango-management-mcp)
- Apideck (a unified-API competitor) says of Nango that "developers write and maintain custom TypeScript functions to fetch and transform data for each API", and contrasts this with its own normalized models across 200+ connectors. — [Apideck: Nango alternative](https://www.apideck.com/alternatives/nango)
- Merge (unified API plus an "Agent Handler" product) says of competitors: Composio "supports 1,000+ apps via toolkits"; Arcade "claims to support more than 8,000 tools" but "only built 43 connectors" and "Many of Arcade's connectors are community-maintained, and their quality and reliability vary widely". Merge positions its own "production-ready, fully-maintained MCP connectors". This is a direct competitor comparison. — [Merge: Composio vs Arcade](https://www.merge.dev/blog/composio-vs-arcade)
- WorkOS (sells a competing product, Pipes) describes Composio and Arcade as "agent tool runtimes", with Arcade calling itself "the MCP runtime for production AI agents". — [WorkOS blog](https://workos.com/blog/pipes-vs-nango-composio-arcade-paragon-and-merge)
- Zapier markets Zapier MCP as access to "8,000+ apps and 30,000+ actions" (claimed). — [Zapier MCP](https://zapier.com/mcp/workday)
- Pipedream offered "3,000+ apps and 10,000+ pre-built tools via managed MCP servers" (claimed). Workday announced its acquisition of Pipedream on 2025-11-19. — [SiliconANGLE 2025-11-19](https://siliconangle.com/2025/11/19/workday-acquire-pipedream-extend-ai-agent-integrations-across-enterprise-apps/); [Ry Walker research note (secondary)](https://rywalker.com/research/pipedream)
- Truto (unified API vendor) states the requirement as a platform that "exposes provider-specific schema when the LLM needs it, and hands rate limits and errors back to the orchestrator instead of pretending they don't exist". — [Truto buyer's guide](https://truto.one/blog/best-unified-api-for-ai-agents-2026-buyers-guide-platform-comparison/)
- Independent engineer Manveer Chawla (2026-03-08): "Make the transport decision per tool integration, not per system". He says MCP suits services with "no CLI, requires OAuth/dynamic auth across multiple users, or the workflow is stateful and multi-step", and CLI wins when the "tool runs locally or has a mature vendor CLI...auth can be pre-configured". — [Chawla, MCP vs CLI](https://manveerc.substack.com/p/mcp-vs-cli-ai-agents)
- A consultancy summary says to use MCP when a tool "will be consumed by multiple AI clients", and to use direct API integration "when you have one AI application, one data source, and no plans to expand". — [RaftLabs: MCP vs API integration](https://www.raftlabs.com/blog/mcp-vs-api-integration)
- Token-cost claims circulating in 2026 (secondary; primary posts not fetched):
  - A Scalekit benchmark put MCP at 4x to 32x more tokens per call than the CLI equivalent.
  - Cloudflare's Code Mode collapses an API into two tools (search and execute) for a claimed 99%+ token reduction.
  - Anthropic's code-execution-with-MCP pattern claims up to 98.7% context reduction.
  — [blocks.ai: MCP vs CLI context window cost (secondary)](https://blocks.ai/blog/mcp-vs-cli-context-window-cost); [Checkly: CLIs vs MCP token efficiency](https://www.checklyhq.com/blog/mcp-vs-cli-token-efficiency/)

### Inferences
- The vendor consensus ("MCP for tool calls; you still need auth, syncs and webhooks") lines up exactly with what each vendor sells. It is also consistent with the protocol gaps documented independently in question 3, so it is not merely marketing.
- The tool-count claims are not comparable: Zapier 30,000+ actions, Pica 25,000+ actions, Arcade 8,000+ tools, Nango 7,000+ tools, Pipedream 10,000+ tools. Merge's "8,000 tools but 43 connectors built" jab at Arcade shows how counts are padded by generated or community tools. A new library advertising "thousands of integrations" would be judged with the same skepticism.
- Both the industry response to MCP's context cost (code mode, code execution, CLIs) and the vendors' "code-first" position point toward typed, programmatic APIs as the efficient substrate. That is favorable to a typed Rust library, provided it can also be exposed as MCP tools.
- No vendor argues "MCP replaces integrations". The live disagreement is over who hosts credentials and execution. An embeddable open-source library is a third option (self-hosted, in-process) that none of the hosted vendors is motivated to promote.

### Gaps
- I did not retrieve first-party position posts from Composio, Arcade, Zapier or Pipedream on "MCP versus direct API". Their positions above are as characterized by competitors or by their own marketing pages.
- I did not find conference talks with measured production data, such as failure rates of MCP versus direct API calls.
- The Scalekit, Cloudflare and Anthropic figures were not verified against the original posts.
- I found no source discussing these trade-offs specifically for Rust backends.

## 5. Specification-driven generation: how well does generating clients from OpenAPI work in practice?

### Takeaway
OpenAPI generation gives typed request and response code for the minority of major SaaS vendors that publish maintained specs (GitHub, Stripe, Twilio and Atlassian do; Slack's is archived, Linear is GraphQL). The public spec commons (APIs.guru) is stale and dominated by cloud providers. Rust generator options are thinner than for other languages, and none of them produce auth flows, webhook handling or sync logic.

### Cited Findings

Spec availability (measured 2026-10-08 unless noted):

- GitHub publishes "An OpenAPI description for GitHub's REST API" (1,640 stars, pushed 2026-10-08). The bundled OpenAPI 3.0.3 file is 13.0 MB with 816 paths and 1,232 operations, and it has no top-level `webhooks` key. — [github/rest-api-description](https://github.com/github/rest-api-description)
- Stripe publishes "An OpenAPI specification for the Stripe API" (504 stars, pushed 2026-10-08). `spec3.json` is 8.3 MB, OpenAPI 3.0.0, with 431 paths and 612 operations. — [stripe/openapi](https://github.com/stripe/openapi)
- Twilio publishes its OpenAPI specification (pushed 2026-10-06). — [twilio/twilio-oai](https://github.com/twilio/twilio-oai)
- Atlassian serves a Jira Cloud platform spec: OpenAPI 3.0.1, 423 paths, 620 operations. — [Jira Cloud swagger-v3 JSON](https://developer.atlassian.com/cloud/jira/platform/swagger-v3.v3.json)
- Slack's OpenAPI specs repository is archived, with its last push on 2021-09-07. — [slackapi/slack-api-specs](https://github.com/slackapi/slack-api-specs)
- Linear's developer repository is described as "Tools, SDK's and plugins for Linear". Linear's API is GraphQL, so OpenAPI generation does not apply. — [linear/linear](https://github.com/linear/linear)

APIs.guru OpenAPI Directory:

- The metrics endpoint reports 3,992 specs, 2,529 APIs and 108,837 endpoints, with 688 flagged invalid and 166 unreachable. — [APIs.guru metrics.json](https://api.apis.guru/v2/metrics.json)
- From `list.json` (measured 2026-10-08): 1,205 of the 2,529 entries belong to azure.com, googleapis.com or amazonaws.com, across 677 distinct providers. — [APIs.guru list.json](https://api.apis.guru/v2/list.json)
- By the `updated` field of each API's preferred version, none was updated after 2023 (808 in 2023, 15 in 2022, 660 in 2021, the remainder 2016 to 2020). — [APIs.guru list.json](https://api.apis.guru/v2/list.json)
- The directory has no entries for salesforce.com, linear.app or shopify.com, and one each for stripe.com, notion.com, atlassian.com and asana.com. The repository was last pushed 2026-04-20. — [APIs.guru list.json](https://api.apis.guru/v2/list.json); [APIs-guru/openapi-directory](https://github.com/APIs-guru/openapi-directory)

Rust generators:

- progenitor (Oxide Computer) is "a Rust crate for generating opinionated clients from API descriptions in the OpenAPI 3.0.x specification". It can be used as a macro, from `build.rs`, or to emit a static crate. It works best with Dropshot-generated specs, and its README warns that "As OpenAPI covers a wide range of APIs, Progenitor may fail for some OpenAPI documents". Paginated interfaces become Streams. Authentication is configured manually by setting headers on the `reqwest` client. — [progenitor README](https://github.com/oxidecomputer/progenitor/blob/main/README.md)
- progenitor stats (measured 2026-10-08): version 0.15.0, 5.69 million total downloads, 1,021 stars, 114 open issues, updated 2026-09-10. — [crates.io: progenitor](https://crates.io/crates/progenitor); [oxidecomputer/progenitor](https://github.com/oxidecomputer/progenitor)
- The `openapiv3` crate that Rust generators parse with is at 2.2.0, last updated 2025-06-02. — [crates.io: openapiv3](https://crates.io/crates/openapiv3)
- openapi-generator (26,778 stars) ships a `rust` client generator and `rust-server`, `rust-axum` and `rust-salvo` server generators; a `rust-server-deprecated` doc also exists. — [openapi-generator generator docs](https://github.com/OpenAPITools/openapi-generator/tree/master/docs/generators)
- Microsoft Kiota has no Rust generator. Issue #4436 "Rust generator support?" has been open since 2024-04-02, labeled `enhancement, new language`, with 18 thumbs-up and 20 comments, last updated 2026-06-23. — [microsoft/kiota issue 4436](https://github.com/microsoft/kiota/issues/4436)
- Stainless did not list Rust among its targets and is being wound down: after acquisition by Anthropic, Stainless announced on 2026-05-18 that "new signups, projects, and SDKs will not be available". Existing users own the SDKs they generated, but the regeneration pipeline stops. The source is a competitor. — [Scalar: Stainless wind-down](https://scalar.com/resources/stainless-wind-down)
- Speakeasy lists Rust as a roadmap language, not a shipped one, per a competitor comparison dated January 2026. — [Fern comparison post](https://buildwithfern.com/post/stainless-vs-gitbook-vs-speakeasy-api-docs-sdk-comparison)
- Fern released a Rust SDK generator on 2026-02-12 with async/await, streaming, serde models and wire tests. Fern contrasts itself with openapi-generator, which it says "produces basic API wrappers" without OAuth token refresh, pagination or retries (vendor claim). — [Fern: Rust SDK generator](https://buildwithfern.com/post/rust-sdk-generator); [Fern vs openapi-generator](https://buildwithfern.com/post/openapi-generator-cli-vs-fern)
- Popular hand-written Rust clients coexist with vendor specs: `octocrab` (GitHub) has 18.1 million total downloads and was updated 2026-09-14, even though GitHub publishes a full OpenAPI description. — [crates.io: octocrab](https://crates.io/crates/octocrab)

### Inferences
- Coverage is uneven. For the named target list, maintained first-party specs exist for GitHub, Stripe, Twilio and Jira. Slack's is dead, Linear is GraphQL, and Google uses discovery documents and protos. A purely spec-driven library cannot reach uniform coverage even across the top 10 targets.
- APIs.guru is not a usable upstream for a living catalog: nothing updated since 2023, about 48% cloud-provider entries, and 688 specs flagged invalid. Each vendor's spec has to be tracked at its source.
- For an open-source Rust library the realistic generator is progenitor (free, Rust-native, `build.rs` and macro friendly), with openapi-generator as fallback. progenitor supports only OpenAPI 3.0.x, its auth is manual and its robustness on arbitrary vendor specs is explicitly limited. The commercial tier is thin for Rust: only Fern ships it, Speakeasy has not, and Stainless is exiting.
- Generated code covers the request and response surface only. OAuth flows, token refresh, vendor-specific pagination, rate-limit handling, webhook signature verification and event typing sit outside what progenitor and openapi-generator emit, so they stay hand-written or need a second declarative layer.
- Spec size hurts compile time: a 13 MB GitHub spec with 1,232 operations generated into one crate is heavy. Per-integration Cargo features (or per-service crates) are a necessity, not a nicety.
- The survival of `octocrab` alongside GitHub's own spec suggests developers value ergonomic, curated APIs over raw generated ones. The likely shape is a generated low-level layer with a thin hand-written ergonomic layer on top.

### Gaps
- I found no quantitative study of spec drift (how often vendor specs diverge from live API behavior). The problem is commonly asserted but unquantified in what I retrieved.
- I did not test progenitor or openapi-generator against the GitHub, Stripe or Jira specs, so actual failure modes are unverified.
- GitHub also publishes OpenAPI 3.1 descriptions, which I did not measure; whether its webhook schemas are published separately was not confirmed.
- Whether Notion, Salesforce, HubSpot, Asana or Zoom publish maintained first-party OpenAPI specs was not checked at source. APIs.guru presence is not evidence of a maintained spec.
- I did not verify Speakeasy's current Rust status on Speakeasy's own site.

## 6. Declarative connector approaches: Airbyte, Nango, Pica, Arazzo and Overlays

### Takeaway
Airbyte is the strongest evidence that declarative definitions scale for read-side REST connectors: 505 of 589 source connectors (about 86%) in its OSS registry are manifest-only YAML. Zero of its 54 destinations are declarative, and writes, webhooks and non-REST APIs still need code. Nango and Pica make auth and provider metadata declarative but keep business logic in code or in a proprietary cloud.

### Cited Findings

Airbyte:

- Measured from Airbyte's OSS connector registry on 2026-10-08: 589 sources and 54 destinations. Sources by language tag: 505 manifest-only (85.7%), 60 Python (10.2%), 24 Java (4.1%). Destinations: 31 Java, 23 Python, none manifest-only. — [Airbyte OSS registry JSON](https://connectors.airbyte.com/files/registries/v0/oss_registry.json)
- By support level, among sources: community tier is 459 manifest-only, 50 Python, 14 Java; certified tier is 46 manifest-only, 10 Python, 10 Java. So 46 of 66 certified sources (about 70%) are manifest-only, versus 459 of 523 community sources (about 88%). — [Airbyte OSS registry JSON](https://connectors.airbyte.com/files/registries/v0/oss_registry.json)
- Airbyte's docs say the low-code framework builds "source connectors for REST APIs via a connector builder UI or by modifying boilerplate YAML files". It supports OAuth and API-key auth, four pagination styles (limit-offset, page-number, cursor, header link), incremental sync with checkpointing, JSON/XML/CSV decoding and backoff strategies, and "is a part of the Python CDK". A search summary of the docs adds "The Connector Builder should be enough for 90% connectors out there". — [Airbyte low-code CDK overview](https://docs.airbyte.com/platform/connector-development/config-based/low-code-cdk-overview)

Nango:

- Provider definitions live in one YAML file with about 1,046 provider entries (measured 2026-10-08). Of the 984 entries with an explicit auth mode: 348 `API_KEY`, 306 `OAUTH2`, 106 `OAUTH2_CC`, 101 `BASIC`, 68 `TWO_STEP`, 33 `MCP_OAUTH2`, 5 `OAUTH1`, 4 `JWT`, 3 `MCP_OAUTH2_GENERIC`, and a few others. — [NangoHQ/nango providers.yaml](https://github.com/NangoHQ/nango/blob/master/packages/providers/providers.yaml)
- Nango states that "Tool calls, durable syncs for RAG, and webhooks run as customizable code on the same runtime" and that pre-built integrations handle "95%+ of heavy lifting" (vendor claim). — [Nango blog](https://nango.dev/blog/best-embedded-integrations-platform-for-ai-agents); [Nango blog: how Nango differs](https://www.nango.dev/blog/how-is-nango-different-from-embedded-ipaas-or-unified-api)

Pica:

- The open-source repository (now under the `withoneai` organization, 1,485 stars, last pushed 2026-08-19) states: "This repository is the Community Edition of Pica and is no longer actively maintained", with development moved to a hosted platform. It claims "200+ integrations and 25,000+ actions" and offers an MCP server. — [withoneai/pica](https://github.com/withoneai/pica)
- The Pica repository is mostly Rust: about 895 KB of Rust versus 154 KB of TypeScript (measured from the GitHub languages API, 2026-10-08). — [withoneai/pica](https://github.com/withoneai/pica)

OpenAPI Initiative workflow specs:

- The OpenAPI Initiative released Arazzo v1.1.0, whose headline feature is AsyncAPI support, so workflows can span "both synchronous and asynchronous APIs". It also completed Overlay Specification v1.1.0, adding a `copy` property on the Action Object. The Initiative describes the trio as: OpenAPI describes individual HTTP APIs, Arazzo describes workflows across them, and Overlays "repeatably apply transformations to one or many API descriptions". — [OpenAPI Initiative newsletter, June 2026](https://www.openapis.org/blog/2026/06/09/openapi-initiative-newsletter-june-2026); [Arazzo Specification page](https://www.openapis.org/arazzo-specification)

### Inferences
- The realistic ceiling for declarative coverage is about 85 to 90% of read-only REST connectors (Airbyte measured 85.7%, docs claim 90%). The share falls for the connectors that matter most: 70% of certified sources versus 88% of community ones. High-value, complex APIs are disproportionately the ones that need code.
- Declarative coverage for writes and actions is unproven: Airbyte has zero declarative destinations. A general-purpose integration library needs actions and webhooks, which is exactly where the declarative evidence stops.
- Auth is the most declarable layer. Nango expresses about 1,000 providers' auth with roughly ten auth modes in one YAML file, and four modes (API key, OAuth2, OAuth2 client credentials, Basic) cover about 87% of entries with an explicit mode. A Rust library could ship a data-driven auth and provider registry covering a very large catalog cheaply, and reserve code for typed operations.
- Pica is a cautionary precedent closest to the proposed project: a Rust codebase for data-defined integrations whose open-source edition was abandoned in favor of a proprietary cloud. The integration definitions (the valuable asset) did not stay open.
- Overlays are a practical tool for the spec-drift problem in question 5: keep a small patch file per vendor that corrects the upstream spec before generation, instead of forking it. Arazzo could describe multi-step flows such as OAuth dance plus pagination. I found no evidence of Rust tooling consuming either.
- Airbyte's declarative runtime is Python and Nango's is TypeScript. Reusing either catalog from Rust means writing a Rust interpreter for someone else's manifest format, not importing a library.

### Gaps
- I could not confirm whether Airbyte connectors tagged `manifest-only` may still include custom Python components (a `components.py`). If they can, 85.7% overstates "pure declarative" coverage.
- Airbyte's Python-tagged sources may be hybrids (declarative manifest plus custom Python); the registry tag does not distinguish.
- How Pica stores integration definitions (database records, JSON or code) was not confirmed from its README; "data-defined" comes from the task framing, not from a verified source.
- The licenses of Nango's `providers.yaml` and Airbyte's manifests, and whether a third-party Rust project may reuse them, were not checked.
- Adoption data for Arazzo and Overlays (how many vendors publish them) was not found; neither appears to be published by the major SaaS vendors examined here.
- Nango's "95%+" figure has no published methodology.

## 7. Precedents for generated SDK families at scale: AWS, Google and Azure in Rust

### Takeaway
Every Rust SDK family in the hundreds-of-services range is machine-generated from a first-party interface definition and published as one crate per service. The one large family that is partly hand-shaped (Azure) reached general availability in 2026 with six service libraries. Generation works at that scale because one owner controls both the spec and the generator, which a cross-vendor SaaS library does not.

### Cited Findings
- AWS SDK for Rust: the repository's `sdk/` directory contains 467 entries, one directory per crate (measured 2026-10-08; includes a few non-service support crates). The repo has 3,343 stars and was pushed 2026-10-06. — [awslabs/aws-sdk-rust sdk directory](https://github.com/awslabs/aws-sdk-rust/tree/main/sdk)
- The generator is smithy-rs: "Code generation for the AWS SDK for Rust, as well as server and generic smithy client generation". — [smithy-lang/smithy-rs](https://github.com/smithy-lang/smithy-rs)
- `aws-sdk-s3` is at version 1.152.0 with 93.8 million total downloads, updated 2026-10-01 (measured). A minor version of 152 indicates continuous regeneration as the service model changes. — [crates.io: aws-sdk-s3](https://crates.io/crates/aws-sdk-s3)
- google-apis-rs (community project, generated from Google discovery documents): the `gen/` directory has 660 entries, of which 328 are `-cli` crates, leaving 332 API library crates (measured 2026-10-08). 1,135 stars, last pushed 2026-09-04. — [Byron/google-apis-rs gen directory](https://github.com/Byron/google-apis-rs/tree/main/gen)
- `google-drive3` from that project is at `7.0.0+20251218` (the suffix is the discovery document date), with 3.1 million total downloads, last updated 2026-01-01. — [crates.io: google-drive3](https://crates.io/crates/google-drive3)
- google-cloud-rust (Google's official SDK): the repository tree contains 346 `Cargo.toml` files (measured 2026-10-08; includes non-generated, tooling and test crates), with a `librarian.yaml` generation config at the root and a `src/generated` tree. `google-cloud-storage` is at 1.20.0 with 22.2 million total downloads, updated 2026-09-30. — [googleapis/google-cloud-rust](https://github.com/googleapis/google-cloud-rust); [crates.io: google-cloud-storage](https://crates.io/crates/google-cloud-storage)
- Azure SDK for Rust reached general availability in mid-2026 (sources date it to the May or June 2026 release) with 1.0.0 crates for Core, Identity, Key Vault (Secrets, Keys, Certificates) and Storage (Blobs, Queues): "Six service libraries and the core infrastructure". Event Hubs and Cosmos DB are slated for later. — [Azure SDK blog: Rust GA](https://devblogs.microsoft.com/azure-sdk/from-beta-to-stable-announcing-the-azure-sdk-for-rust-ga/); [Azure SDK release, May 2026](https://devblogs.microsoft.com/azure-sdk/azure-sdk-release-may-2026/)
- The Azure repo has 34 crate manifests at `sdk/<service>/<crate>/Cargo.toml` depth (measured 2026-10-08), and `azure_core` has 37.8 million total downloads. — [Azure/azure-sdk-for-rust](https://github.com/Azure/azure-sdk-for-rust); [crates.io: azure_core](https://crates.io/crates/azure_core)
- For contrast, Azure has 1,829 spec entries in APIs.guru, so its spec surface is far larger than its shipped Rust SDK surface. — [APIs.guru metrics.json](https://api.apis.guru/v2/metrics.json)

### Inferences
- No one hand-writes hundreds of clients. AWS (about 467 crates) and Google (about 332 community-generated, plus the official family) are fully generated. Azure, with heavier design review per library, shipped six service libraries at GA after years of beta. For a "thousands of integrations" ambition, hand-writing is not a credible primary strategy.
- All three precedents use one crate per service, not one crate with hundreds of feature flags. A single crate with a Cargo feature per integration does not match precedent at scale: feature unification, docs.rs build limits, compile time and semver coupling (one integration's breaking change forces a major bump for all) all get worse. A workspace of per-integration crates with an optional umbrella crate that re-exports behind features is the pattern that matches.
- These precedents work because one vendor owns a rigorous IDL (Smithy, protobuf or discovery, TypeSpec) with uniform auth (SigV4, Google ADC, Entra ID), uniform pagination and uniform error shapes. Cross-vendor SaaS has none of that uniformity, so the same generator investment yields less: each vendor needs its own auth, pagination and error adapters.
- google-apis-rs shows the likely quality ceiling of pure generation from third-party-style specs: broad coverage, moderate adoption (3.1 million downloads for Drive versus 22 million for the official storage crate), and version strings tied to spec dates. Breadth is cheap; ergonomics and trust come from the curated official family.
- Continuous regeneration is the real cost. AWS is at minor version 152 on S3. A cross-vendor library needs automated spec fetching, diffing, regeneration, semver classification and release for every integration, with no vendor obligation to notify of changes.

### Gaps
- The AWS SDK for Rust general availability date and exact service count were not verified this session; 467 is a directory count, not an official service count.
- The 346 figure for google-cloud-rust mixes generated and hand-written crates. I did not obtain an official count of generated Google Cloud Rust crates, nor confirm the generator's inputs from documentation.
- Whether Azure's Rust crates are generated from TypeSpec, and how much hand-written code sits on top, was not confirmed from a primary source.
- I found no precedent, successful or failed, for a generated cross-vendor SaaS SDK family in Rust at more than about 100 services.
