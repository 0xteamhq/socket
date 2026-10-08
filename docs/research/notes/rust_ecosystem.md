# Rust ecosystem: libraries that bundle many third-party service integrations behind one interface (state as of 2026-10-08)

Method note for the report writer: registry numbers (versions, dates, download counts, feature lists, owners) were read directly from the crates.io JSON API on 2026-10-08; "recent" downloads means crates.io's trailing-90-day figure. GitHub numbers (stars, license, last push, archived flag, directory listings) were read from the GitHub REST API on the same day. Feature counts for Vector were parsed from its `Cargo.toml` on `main`. Anything not checked this way is marked as inference or listed under Gaps.

## 1. Is there any Rust crate offering a unified connector interface across many SaaS/business tools with per-integration feature flags?

### Takeaway
No maintained, adopted direct equivalent exists. The only thing that matches the idea closely is the `swissknife-*-sdk` family (24 crates, per-provider Cargo features, one release on 2025-12-24, about 170 total downloads each, source repository now returns 404), which looks like an abandoned one-shot publication rather than a live project.

### Cited Findings

**Closest match: `swissknife-*-sdk` (Alex Choi)**
- A crates.io search for "swissknife" returns 24 crates named `swissknife-<category>-sdk`, all at version 0.1.1 and all last updated 2025-12-24: banking, ai, queue, auth, automation, cloud, communication, crm, devtools, hr, pm, productivity, scraping, search, social, vectordb, file, markets, memory, observability, payments, research, database, ecommerce — [crates.io search](https://crates.io/search?q=swissknife)
- Integrations are selected by per-provider Cargo features inside each category crate. `swissknife-pm-sdk` features: `asana, clickup, jira, linear, trello, full, default` — [swissknife-pm-sdk](https://crates.io/crates/swissknife-pm-sdk)
- `swissknife-productivity-sdk` features: `airtable, calendly, confluence, excel, google, google-calendar, google-docs, google-drive, google-sheets, microsoft, notion, onedrive, planner, sharepoint, typeform, full, default` — [swissknife-productivity-sdk](https://crates.io/crates/swissknife-productivity-sdk)
- `swissknife-crm-sdk` features: `apollo, clay, hubspot, hunter, pipedrive, salesforce, wealthbox, zendesk, zoho, full, default` — [swissknife-crm-sdk](https://crates.io/crates/swissknife-crm-sdk)
- `swissknife-communication-sdk` features include `slack, teams, discord, telegram, twilio, whatsapp, gmail, outlook, sendgrid, mailgun, mailchimp, resend, intercom, smtp, apns, fcm` — [swissknife-communication-sdk](https://crates.io/crates/swissknife-communication-sdk)
- `swissknife-devtools-sdk` features: `cursor, github, gitlab, stagehand, full, default` — [swissknife-devtools-sdk](https://crates.io/crates/swissknife-devtools-sdk)
- `swissknife-ai-sdk` is an umbrella "AI agent tools SDK wrapping swissknife service integrations" with roughly 130 feature flags (one per provider plus one per category) and exposes `Tool` and `ToolBuilder` traits plus `ToolRegistry`, `ToolDefinition`, `ToolResponse`; docs.rs reports 0% documentation coverage — [docs.rs swissknife-ai-sdk](https://docs.rs/swissknife-ai-sdk/0.1.1/swissknife_ai_sdk/), [crates.io](https://crates.io/crates/swissknife-ai-sdk)
- Adoption is negligible: e.g. `swissknife-pm-sdk` 168 total downloads / 41 recent, `swissknife-crm-sdk` 167 / 41, `swissknife-ai-sdk` 25 / 3. Each crate has only 1-2 published versions — [crates.io search](https://crates.io/search?q=swissknife)
- License is `MIT OR Apache-2.0`; the crates.io owner is listed as login `ghost_376345` (name "Alex Choi") — [swissknife-banking-sdk](https://crates.io/crates/swissknife-banking-sdk)
- The declared repository `https://github.com/alexchoi0/swissknife-sdk` returned HTTP 404 from the GitHub API on 2026-10-08 — [declared repo URL](https://github.com/alexchoi0/swissknife-sdk)
- The published `swissknife-pm-sdk` crate tarball is 12,908 bytes for five project-management providers; `swissknife-crm-sdk` is 36,514 bytes for nine providers — [swissknife-pm-sdk](https://crates.io/crates/swissknife-pm-sdk)
- Conflict: the docs.rs page summary gave a publish date of 2026-09-06 for `swissknife-ai-sdk` 0.1.1, while the crates.io API reports 2025-12-24. The crates.io API value is the primary source — [docs.rs](https://docs.rs/swissknife-ai-sdk/0.1.1/swissknife_ai_sdk/); contradicted by [crates.io](https://crates.io/crates/swissknife-ai-sdk)

**Other multi-SaaS Rust collections (none provides a unified interface with feature flags)**
- `oxidecomputer/third-party-api-clients`: "A place for keeping all our generated third party API clients." 21 generated clients in one repo (DocuSign, Giphy, GitHub, Google Admin/Calendar/Cloud Resource Manager/Drive/Groups Settings/Sheets, Gusto, MailChimp, Okta, Ramp, Rev.ai, SendGrid, ShipBob, Shopify, Slack, Stripe, TripActions, Zoom). MIT, 150 stars, last push 2026-09-30 ("Bump to 0.11.0-rc.2"). Each service is a separate crate generated from an OpenAPI spec held in `specs/` by an in-repo `generator` — [GitHub](https://github.com/oxidecomputer/third-party-api-clients)
- Those Oxide crates are published separately under unrelated names: `octorust` (GitHub; 0.10.0, 2025-03-12; 607,104 downloads), `slack-chat-api` (0.7.0, 2023-07-19), `google-drive` (0.10.0, 2025-12-30), `google-calendar` (0.10.0, 2025-12-30), `sheets` (0.10.0, 2025-12-30), `gsuite-api`, `dolladollabills` (Stripe; 0.7.0, 2023-07-19), `okta` (0.7.1, 2023-09-01), `docusign` (0.1.17, 2022-11-18), `mailchimp-api`, `gusto-api`, `ramp-api`, `zoom-api` (0.10.0, 2025-04-08) — [octorust](https://crates.io/crates/octorust), [slack-chat-api](https://crates.io/crates/slack-chat-api), [google-drive](https://crates.io/crates/google-drive), [dolladollabills](https://crates.io/crates/dolladollabills), [zoom-api](https://crates.io/crates/zoom-api)
- Oxide's earlier hand-written collection `oxidecomputer/cio` ("Rust libraries for APIs needed by our automated CIO") is archived; last push 2024-11-01, 263 stars, Apache-2.0 — [GitHub](https://github.com/oxidecomputer/cio)
- `flows-network/flows-connector`: "The SaaS connectors that flows.network supported", all written in Rust; 17 directories (cloudinary, discord, dropbox, github, gitlab, gmail, jira, monday, notion, sendgrid, slack, telegram, twilio, twitter, plus helpers). 5 stars, no license file detected, last push 2022-12-01 — [GitHub](https://github.com/flows-network/flows-connector)
- flows.network also published per-service crates tied to its WASM platform: `slack-flows` 0.3.4 (2023-07-10), `github-flows` 0.8.1 (2024-08-21), `airtable-flows` 0.1.9 (2023-03-03) — [slack-flows](https://crates.io/crates/slack-flows), [github-flows](https://crates.io/crates/github-flows), [airtable-flows](https://crates.io/crates/airtable-flows)
- `tinyhumansai/tinyconnectors`: "OAuth integrations & connectors for agents", GPL-3.0, 2 stars, created 2026-08-30, last push 2026-10-02. Its README states "Composio is the connector backend today" and that it ships as a `cdylib` module loaded by the project's own bus, so it proxies to Composio rather than implementing integrations — [GitHub](https://github.com/tinyhumansai/tinyconnectors)
- `tapfs` 0.1.0 (2026-04-25, Apache-2.0, 26 downloads): "FUSE filesystem that mounts enterprise REST APIs as agent-readable files"; its declared repo `tapfs/tap` returned 404 — [crates.io](https://crates.io/crates/tapfs)

**Narrow-domain "many services, one API" crates (notifications only)**
- `pling` 0.6.0 (2026-01-05, MIT, 31,454 downloads, 3 GitHub stars): "Send notifications via Slack, Telegram, ..." Its current feature list is `clap, reqwest, ureq` (HTTP-client selection, not per-service) — [crates.io](https://crates.io/crates/pling), [GitHub](https://github.com/EdJoPaTo/pling)
- `chatterbox` 2.9.1 (2026-06-13, MIT, 7,316 downloads, 0 stars): "streamlined text notifications via telegram, email, slack, discord, teams, gotify, resend, ntfy, pushover, matrix, mattermost..."; no Cargo features — [crates.io](https://crates.io/crates/chatterbox)
- `omnihook` 0.1.2 (2026-06-05, MIT, 382 downloads): outbound webhook payload builders for Slack, Discord, Telegram — [crates.io](https://crates.io/crates/omnihook)

**Agent runtimes that bundle integrations behind feature flags (applications, not libraries)**
- `zeroclaw-labs/zeroclaw` (Apache-2.0, 32,943 stars, created 2026-02-13, pushed 2026-10-08) has 66 Cargo features of which about 38 are `channel-*` (e.g. `channel-slack`, `channel-discord`, `channel-telegram`, `channel-notion`, `channel-email`, `channel-matrix`, `channel-whatsapp-cloud`) plus `provider-github`, `provider-gitea` — [Cargo.toml](https://github.com/zeroclaw-labs/zeroclaw/blob/main/Cargo.toml)

**Search coverage that came back empty**
- crates.io keyword searches returned 0 results for "nango", "unified.to", and "merge api hris"; "apideck" returned one unrelated crate; "connector slack github" returned 20 crates, none a unified SaaS connector library — [crates.io search: nango](https://crates.io/search?q=nango), [crates.io search: connector slack github](https://crates.io/search?q=connector%20slack%20github)
- GitHub repository search restricted to Rust for "unified api integrations" returned nothing, "saas connectors" returned only `flows-network/flows-connector`, and the `integrations` topic in Rust is led by Pica (1,485 stars) followed by repos with 7 stars or fewer — [GitHub topic search](https://github.com/search?q=topic%3Aintegrations+language%3ARust&type=repositories&s=stars)

### Inferences
- A direct equivalent of "one repository, many SaaS integrations, opt in per integration via Cargo features, one consistent interface" does not exist in a maintained, adopted form. The slot is effectively open.
- `swissknife-*-sdk` proves the idea has been attempted and that the feature-flag layout is workable, but its profile (single publish day, tiny tarballs, zero docs, deleted repo, owner login prefixed `ghost_`) suggests a generated or abandoned project; it should not be treated as a competitor with users. It does occupy the idea's "prior art" position and is worth naming in any announcement.
- The tarball sizes imply shallow coverage per provider (a few kilobytes of source per service), so depth of coverage, not breadth of the feature list, is where a new library could differentiate.
- Oxide's repo is the most credible "many SaaS clients in one Rust repo" precedent, but it is the opposite design: independent generated crates with no shared trait, no shared auth, and unrelated crate names.
- The fastest-growing place integrations are being bundled in Rust is inside agent applications (zeroclaw's `channel-*` features), which indicates demand for the feature-flag pattern but leaves no reusable library behind.

### Gaps
- Could not read the swissknife source (repo 404), so the actual trait design, auth handling and API depth are unverified beyond docs.rs item names.
- lib.rs category pages (api-bindings, web-programming) were not browsed page by page; coverage relied on crates.io keyword search and GitHub search, so a low-profile crate with unusual naming could have been missed.
- `pling`'s docs.rs page for 0.3.0 describes services as depending on compile-time features, but the 0.6.0 feature list shows only HTTP-client features; I did not confirm which services 0.6.0 supports or how they are gated.

## 2. Nearest Rust analogs in adjacent domains using "one crate, many backends via feature flags"

### Takeaway
The pattern is well established and successful in storage, observability pipelines and LLM tooling: Apache OpenDAL exposes 68 `services-*` feature flags in one crate, and Vector gates about 48 sources and 55 sinks behind features. OpenDAL is the cleanest structural template for the proposed library; several other well-known projects (Rig, genai) do not actually use per-backend features.

### Cited Findings

| Project | What it unifies | Backend count and how selected | Status (2026-10-08) | License |
|---|---|---|---|---|
| Apache OpenDAL (`opendal`) | Storage services behind one `Operator` API | 104 features total, 68 named `services-*` (s3, gcs, azblob, gdrive, onedrive, dropbox, github, huggingface, postgresql, redis, sftp, webdav, ...); Cargo feature per service | 0.59.4 released 2026-10-05; 17.0M downloads, 5.35M recent; 5,404 stars | Apache-2.0 |
| `object_store` | Object stores | 16 features; backends `aws`, `azure`, `gcp`, `http`, `fs`; Cargo feature per backend | 0.14.2 on 2026-09-15; 93.7M downloads, 22.2M recent; repo 325 stars (split out 2025-03-20) | MIT/Apache-2.0 |
| Vector (Datadog) | Observability sources, transforms, sinks | 268 features on `main`; 49 `sources-*` (one is a test flag), 55 `sinks-*`, 21 `transforms-*`; Cargo feature per component, chosen at build, wired by runtime config | v0.59.0 on 2026-10-06; 22,678 stars | MPL-2.0 |
| Arroyo | Stream connectors | 19 connector modules in `crates/arroyo-connectors/src` (kafka, confluent, kinesis, fluvio, mqtt, nats, rabbitmq, redis, filesystem, webhook, websocket, sse, polling_http, ...) | v0.15.0 on 2025-12-01; pushed 2026-10-07; 5,048 stars | Apache-2.0 |
| Rig (`rig-core`) | LLM providers and vector stores | 26 provider modules inside `rig-core` (not feature-gated; `rig-core` has 12 features, none per provider); vector stores are 12 separate companion crates (`rig-lancedb`, `rig-qdrant`, `rig-postgres`, `rig-mongodb`, `rig-neo4j`, `rig-sqlite`, `rig-surrealdb`, `rig-milvus`, `rig-scylladb`, `rig-helixdb`, `rig-s3vectors`, `rig-vectorize`); 28 crates in workspace | 0.44.0 on 2026-10-07; 3.35M downloads, 1.81M recent; 8,824 stars | MIT |
| `genai` | LLM providers | 17 adapter modules (anthropic, openai, gemini, bedrock, vertex, cohere, ollama, fireworks, zai, ...); only 5 features, none per provider; provider chosen at runtime | 0.7.0-rc.4 on 2026-10-07; 455,341 downloads; 898 stars | MIT OR Apache-2.0 |
| `llm` (graniet) | LLM and voice backends | 25 features, about 15 per-provider (`openai`, `anthropic`, `google`, `ollama`, `groq`, `mistral`, `bedrock`, `xai`, ...) plus `full`; Cargo feature per backend | 1.3.8 on 2026-04-19; 365 stars | MIT |
| `sqlx` | SQL databases | 47 features; database features `postgres`, `mysql`, `sqlite`, plus `any`, `all-databases` | 0.9.0 on 2026-05-21; 160M downloads; 17,551 stars | MIT OR Apache-2.0 |
| `sea-orm` | SQL databases (over sqlx) | 44 features; `sqlx-postgres`, `sqlx-mysql`, `sqlx-sqlite`, `rusqlite`, `sqlx-all` | 2.0.4 on 2026-09-27; 26.3M downloads; 9,915 stars | MIT OR Apache-2.0 |

- Sources for the table rows: [opendal](https://crates.io/crates/opendal), [apache/opendal](https://github.com/apache/opendal); [object_store](https://crates.io/crates/object_store), [apache/arrow-rs-object-store](https://github.com/apache/arrow-rs-object-store); [vector Cargo.toml](https://github.com/vectordotdev/vector/blob/master/Cargo.toml), [vector releases](https://github.com/vectordotdev/vector/releases); [arroyo connectors](https://github.com/ArroyoSystems/arroyo/tree/master/crates/arroyo-connectors/src); [rig crates](https://github.com/0xPlaygrounds/rig/tree/main/crates), [rig-core](https://crates.io/crates/rig-core); [rust-genai adapters](https://github.com/jeremychone/rust-genai/tree/main/src/adapter/adapters), [genai](https://crates.io/crates/genai); [llm](https://crates.io/crates/llm), [graniet/llm](https://github.com/graniet/llm); [sqlx](https://crates.io/crates/sqlx); [sea-orm](https://crates.io/crates/sea-orm)
- The crate named `vector` on crates.io is an unrelated nearest-neighbour search package (0.4.1, 2024-05-09); Datadog's Vector is not consumable as a crates.io library — [crates.io vector](https://crates.io/crates/vector)
- The crate named `arroyo` on crates.io is stale (0.7.0, 2023-10-17, 3,342 downloads) while the GitHub project is at v0.15.0 — [crates.io arroyo](https://crates.io/crates/arroyo), [GitHub releases](https://github.com/ArroyoSystems/arroyo/releases)
- Other Rust projects with connector catalogues, for context: Fluvio (5,259 stars, Apache-2.0, pushed 2026-08-30), RisingWave (9,360 stars, Apache-2.0), ConnectorX (2,656 stars, MIT), Tremor (932 stars, last push 2025-07-27), Estuary Flow (982 stars) — [fluvio](https://github.com/fluvio-community/fluvio), [risingwave](https://github.com/risingwavelabs/risingwave), [connector-x](https://github.com/sfu-db/connector-x), [tremor-runtime](https://github.com/tremor-rs/tremor-runtime), [estuary/flow](https://github.com/estuary/flow)

### Inferences
- OpenDAL is the closest structural model: one crate, one operator abstraction, `services-<name>` features, layered middleware. It reached 68 service flags and about 5.3M downloads per quarter, which shows the model scales in both maintenance and adoption when the abstraction is narrow (read/write/list/stat).
- The successful analogs all unify a narrow, uniform verb set (bytes in/out, SQL, chat completion, events). SaaS business tools have no such common verb set, which is the likely reason no SaaS equivalent has emerged; a SaaS library would have to choose between a thin "shared plumbing plus per-service typed clients" model and a lossy "common model" (as Pica and the unified-API vendors do).
- Rig and genai are often cited as feature-flag examples but are not: Rig compiles all providers into `rig-core` and puts heavier backends in companion crates; genai selects at runtime. Both are evidence that separate crates in one workspace is an equally accepted answer to compile-time and dependency weight, and that feature flags matter most when a backend drags in a heavy or conflicting dependency.
- Vector shows the scale ceiling of a single crate with hundreds of features (268), but as an application; it is not a reusable library.

### Gaps
- Did not verify whether Arroyo's connectors are individually feature-gated or always compiled.
- The count of distinct OpenDAL services is slightly below 68 because some flags are variants or aliases (e.g. `services-redis-native-tls`, `services-gcs-grpc`, `services-hf`/`services-huggingface`); I did not de-duplicate against the OpenDAL docs.
- Vector's source/sink counts are from feature names on `main` (0.60.0-dev); a few flags are group flags (e.g. `sinks-gcp`), so the documented component count on vector.dev may differ.

## 3. Rust-based integration platforms (Pica / IntegrationOS) and Rust SDKs for unified-API vendors

### Takeaway
Pica (formerly IntegrationOS) is the only substantial integration platform written in Rust, but it is a GPL-3.0 service stack (MongoDB, Redis, JS sandbox) whose integrations are stored as data records rather than Rust code, and its open-source "Community Edition" was declared no longer actively maintained in February 2026 after the company moved to a private cloud product. None of the unified-API or agent-tooling vendors checked ships an official Rust SDK on crates.io; the only Composio crate is an unofficial one with 87 downloads.

### Cited Findings

**Pica (formerly IntegrationOS)**
- `picahq/pica` now redirects to `withoneai/pica`, described as "The community edition of Pica, the agentic tooling platform." Primary language Rust (895,188 bytes Rust, 154,080 bytes TypeScript), GPL-3.0, 1,485 stars, 92 forks, not archived — [GitHub](https://github.com/withoneai/pica)
- The README states: "This repository is the Community Edition of Pica and is no longer actively maintained. Pica has transitioned to a private, cloud-hosted platform with the latest features and updates." — [README](https://github.com/withoneai/pica/blob/main/README.md)
- Activity: last tagged release 1.59.0 on 2025-07-16; last feature commit 2025-07-16 ("Puzzle.io OAuth Configuration"); a 2026-02-20 commit added the community notice; the two 2026-08 commits are CI-only — [commits](https://github.com/withoneai/pica/commits/main), [releases](https://github.com/withoneai/pica/releases)
- The Rust code is a Cargo workspace under `core/` with members `api`, `archiver`, `cache`, `cli`, `osentities`, `database`, `unified`, `watchdog`; workspace dependencies include `axum`, `mongodb`, `redis`, `kube`, `handlebars`, `jsonpath_lib` and `js-sandbox-ios` — [core/Cargo.toml](https://github.com/withoneai/pica/blob/main/core/Cargo.toml)
- Integrations are declarative data, not per-service Rust modules: the domain crate defines `ConnectionDefinition`, `ConnectionModelDefinition`, `ConnectionModelSchema` and `ConnectionOAuthDefinition` record types (fields such as `connection_platform`, `platform_version`, `action: http::Method`, `action_name: CrudAction`, `mapping: Option<CrudMapping>`, `knowledge`) serialized with a Mongo `_id` — [connection_model_definition.rs](https://github.com/withoneai/pica/blob/main/core/osentities/src/domain/connection/connection_model_definition.rs)
- The definitions ship as MongoDB BSON seed dumps in `core/resources/seed/` (`connection-definitions.bson` 19 KB, `connection-model-definitions.bson` 1.78 MB, `connection-model-schema.bson` 2.59 MB, `common-models.bson` 390 KB, `connection-oauth-definitions.bson` 26 KB) — [seed directory](https://github.com/withoneai/pica/tree/main/core/resources/seed)
- Parsing the community seed gives 15 connection definitions (activecampaign, anthropic, bigcommerce, close, freshdesk, front, hubspot, openai, pipedrive, quickbooks, shopify, woocommerce, workable, xero, zendesk) and 5 OAuth definitions, far fewer than the hosted product claims — [connection-definitions.bson](https://github.com/withoneai/pica/blob/main/core/resources/seed/connection-definitions.bson)
- The `unified` crate ("Unified service library for Pica") depends on `mongodb`, `js-sandbox-ios`, `handlebars`, `jsonpath_lib` and the `osentities` and `cache` path crates — [core/unified/Cargo.toml](https://github.com/withoneai/pica/blob/main/core/unified/Cargo.toml)
- Library availability: `osentities` ("Shared library for Pica") 2.0.0, 2025-05-06, GPL-3.0, 2,354 downloads; its predecessor `integrationos-domain` ("Shared library for IntegrationOS") 8.0.0, 2024-10-02, GPL-3.0, 42,440 downloads; `picahq` 0.1.1, 2025-03-04, GPL-3.0, 1,406 downloads. The crate named `pica` is an unrelated Google UWB controller — [osentities](https://crates.io/crates/osentities), [integrationos-domain](https://crates.io/crates/integrationos-domain), [picahq](https://crates.io/crates/picahq), [pica](https://crates.io/crates/pica)
- The developer-facing SDKs are JavaScript: the core README's install step is `npm install @picahq/ai`, and the top-level README installs `@picahq/cli` via npm — [core/README.md](https://github.com/withoneai/pica/blob/main/core/README.md)
- The organisation has rebranded to "One" (`withoneai`); its active repos are TypeScript (`cli` 414 stars, `mcp`, `connect`, dozens of `template-*` repos pushed Aug-Oct 2026). Marketing counts are inconsistent across its own repos: "200+ integrations and 25,000+ actions" (pica README), "500+ platforms" (`hermes-agent`), "700+ apps" (`one-agent-plugin`) — [withoneai org](https://github.com/withoneai), [pica README](https://github.com/withoneai/pica/blob/main/README.md)

**Vendor SDKs in Rust**
- Composio: the official `ComposioHQ/composio` repo (30,465 stars, MIT) contains only `ts/` and `python/` SDK directories; language breakdown is TypeScript, Python, Shell, JavaScript with no Rust — [GitHub](https://github.com/ComposioHQ/composio)
- The only Composio client crate is community-made: `composio-sdk` 0.3.0 (2026-03-12), "Minimal Rust SDK for Composio Tool Router REST API", MIT OR Apache-2.0, 87 total downloads, repo `DotViegas/composio-sdk-rust` with 0 stars and last push 2026-03-12 — [crates.io](https://crates.io/crates/composio-sdk), [GitHub](https://github.com/DotViegas/composio-sdk-rust)
- `ryu-composio` 0.1.14 (2026-08-15, 276 downloads) is an app-internal Composio seam for the "Ryu" project, not a general SDK — [crates.io search: composio](https://crates.io/search?q=composio)
- No crate exists under the names `composio`, `composio-rs`, `composio-client`, `nango`, `nango-sdk`, `arcadeai`, `arcade-ai`, `arcade-sdk`, `merge-api`, `merge-hris`, `unified-to`, `unified_to`, `apideck`, `apideck-unify`, `pipedream-sdk`, `zapier`, `stackone`, `kombo`, `klavis`, `metorial`, `smithery`; the crates `arcade`, `paragon` and `pipedream` exist but are unrelated (a terminal game, a placeholder, an asset pipeline) — [crates.io search: nango](https://crates.io/search?q=nango), [arcade](https://crates.io/crates/arcade), [paragon](https://crates.io/crates/paragon), [pipedream](https://crates.io/crates/pipedream)
- Nango is TypeScript (12,554 stars) and describes integrations as functions living in the customer's codebase run on Nango's runtime — [GitHub](https://github.com/NangoHQ/nango), [Nango blog](https://nango.dev/blog/composio-vs-nango)
- Windmill (18,128 stars) has a Rust backend but is a script/workflow platform, not an integration library — [GitHub](https://github.com/windmill-labs/windmill)
- The Model Context Protocol has an official Rust SDK, `rmcp` 3.5.1 (2026-10-05, Apache-2.0), with 33.3M total and 17.9M recent downloads, and Rig ships a `rig-rmcp` bridge crate — [rmcp](https://crates.io/crates/rmcp), [rust-sdk](https://github.com/modelcontextprotocol/rust-sdk), [rig crates](https://github.com/0xPlaygrounds/rig/tree/main/crates)

### Inferences
- Pica cannot serve as a drop-in library for the proposed project: it is GPL-3.0 (incompatible with a permissive crate that others embed), requires MongoDB and Redis, executes mappings in an embedded JS sandbox, and is unmaintained in the open. Its useful lesson is architectural: at scale (hundreds of platforms) the vendor chose data-defined integrations over hand-written code.
- The published community seed (15 platforms) versus the hosted claims (200 to 700+) suggests the valuable integration definitions were never open-sourced, so there is no reusable open dataset of connector definitions to import from Pica.
- Every unified-API vendor treats Rust as out of scope; Rust users today reach these services by hand-rolled REST calls or via MCP. `rmcp`'s download volume (17.9M in 90 days) suggests MCP is the de facto way Rust agent code reaches SaaS tools, which is the main indirect competitor to a native integrations crate.
- A permissively licensed (MIT/Apache-2.0) Rust library would have no licensing-compatible Rust competitor in this category.

### Gaps
- Vendor documentation pages for Nango, Arcade, Merge, Unified.to and Apideck were not individually fetched; "no Rust SDK" is established by absence on crates.io (and, for Composio, by the repo contents), not by each vendor's SDK list.
- Did not confirm from primary company sources why or when exactly Pica closed its open development, beyond the README notice and commit dates.
- Did not test whether `osentities` can be used standalone without the MongoDB-backed services.

## 4. State of individual per-service Rust SDK crates

### Takeaway
Coverage is uneven: GitHub, Slack, Stripe, Google, AWS/Azure, Discord and Datadog have strong, current crates, while Notion, Linear, Jira, Salesforce, HubSpot, Asana, Airtable, Zendesk and Confluence have only small community crates that are fragmented, low-download or stale. Almost none are vendor-official, and they share no conventions for auth, HTTP client, errors, pagination or TLS features, so a unified library would wrap a handful and build most business-tool clients itself.

### Cited Findings

**Well-maintained (candidates to wrap)**

| Service | Crate | Latest (date) | Downloads total / recent | Provenance | License |
|---|---|---|---|---|---|
| GitHub | `octocrab` | 0.54.2 (2026-09-14) | 18.1M / 2.45M | Community (XAMPPRocky), 1,450 stars | MIT OR Apache-2.0 |
| GitHub (generated) | `octorust` | 0.10.0 (2025-03-12) | 607K / 73K | Oxide, generated from OpenAPI | MIT |
| Slack | `slack-morphism` | 2.29.0 (2026-09-19) | 5.52M / 1.77M | Community (abdolence), 232 stars; features `axum`, `hyper`, `signature-verifier` | Apache-2.0 |
| Stripe | `async-stripe` | 1.0.0-rc.9 (2026-09-11) | 5.55M / 1.05M | Community (arlyon), 753 stars; "generated directly from Stripe's official OpenAPI", regenerated weekly | MIT OR Apache-2.0 |
| Google Workspace + all Google APIs | `google-drive3`, `google-calendar3`, `google-gmail1`, `google-sheets4` (+ `google-apis-common` 8.0.0) | 7.0.0 (2026-01-01) | drive3 3.11M / 729K; sheets4 1.13M; calendar3 197K; gmail1 205K | Community (Byron/google-apis-rs, 1,135 stars), generated from discovery docs | MIT |
| Google Cloud | `google-cloud-storage`, `google-cloud-auth` | 1.20.0 (2026-09-30), 1.17.0 (2026-09-24) | 22.2M / 7.05M; 38.1M / 12.6M | `googleapis/google-cloud-rust` (963 stars) | Apache-2.0 |
| Google Cloud (alt) | `gcloud-sdk` | 0.32.4 (2026-10-04) | 6.82M / 1.72M | Community (abdolence); 366 features, one per API | MIT OR Apache-2.0 |
| Microsoft Graph | `graph-rs-sdk` | 3.0.1 (2025-04-20) | 128K / 32K | Community (sreeise), 154 stars, last push 2025-08-28 | MIT |
| Dropbox | `dropbox-sdk` | 0.21.0 (2026-10-03) | 137K / 31K | `dropbox` org; README: "This SDK is not yet official" | Apache-2.0 |
| Discord | `serenity`; `twilight-http` | 0.12.5 (2025-12-20); 0.17.1 (2025-12-13) | 7.38M; 2.77M | Community | ISC |
| Telegram | `teloxide` | 0.17.0 (2025-07-11) | 2.07M / 460K | Community | MIT |
| GitLab | `gitlab` | 0.1904.0 (2026-09-25) | 5.06M / 1.61M | Community (Kitware) | MIT/Apache-2.0 |
| Datadog | `datadog-api-client` | 0.37.0 (2026-09-24) | 2.28M / 620K | `DataDog` org | Apache-2.0 |
| Sentry | `sentry` | 0.49.3 (2026-09-21) | 56.7M | `getsentry` org | MIT |
| SendGrid | `sendgrid` | 0.27.2 (2026-09-24) | 2.63M / 142K | "An unofficial client library" | MIT |
| AWS / Azure | `aws-sdk-s3`; `azure_core` | 1.152.0 (2026-10-01); 1.2.0-beta.1 (2026-09-05) | 93.8M; 37.8M | `awslabs`, `azure` orgs | Apache-2.0; MIT |

- Sources: [octocrab](https://crates.io/crates/octocrab), [octorust](https://crates.io/crates/octorust), [slack-morphism](https://crates.io/crates/slack-morphism), [async-stripe](https://crates.io/crates/async-stripe), [async-stripe README](https://github.com/arlyon/async-stripe), [google-drive3](https://crates.io/crates/google-drive3), [google-apis-rs](https://github.com/Byron/google-apis-rs), [google-cloud-storage](https://crates.io/crates/google-cloud-storage), [google-cloud-rust](https://github.com/googleapis/google-cloud-rust), [gcloud-sdk](https://crates.io/crates/gcloud-sdk), [graph-rs-sdk](https://crates.io/crates/graph-rs-sdk), [dropbox-sdk-rust README](https://github.com/dropbox/dropbox-sdk-rust), [serenity](https://crates.io/crates/serenity), [twilight-http](https://crates.io/crates/twilight-http), [teloxide](https://crates.io/crates/teloxide), [gitlab](https://crates.io/crates/gitlab), [datadog-api-client](https://crates.io/crates/datadog-api-client), [sentry](https://crates.io/crates/sentry), [sendgrid](https://crates.io/crates/sendgrid), [aws-sdk-s3](https://crates.io/crates/aws-sdk-s3), [azure_core](https://crates.io/crates/azure_core)

**Weak, fragmented or stale (would have to be built)**
- Notion: five competing community crates. `notion-client` 1.1.1 (2026-04-20; 84,712 downloads, 25,351 recent; 41 stars); `notionrs` 0.32.0 (2026-08-14) whose repo is now archived; `notion` 0.6.0 (last release 2024-08-13; 141 stars); `rusticnotion` 0.5.2 (2024-02-08, a "maintained fork" with 2 recent downloads); `notion-sdk` 0.0.0 placeholder — [notion-client](https://crates.io/crates/notion-client), [notionrs](https://crates.io/crates/notionrs), [46ki75/notionrs](https://github.com/46ki75/notionrs), [notion](https://crates.io/crates/notion), [rusticnotion](https://crates.io/crates/rusticnotion)
- Linear: `linear-sdk` / `linear_sdk` 0.0.1 (2022-10-29, 1,644 downloads); `linear-api` 0.1.0 (2026-07-06, "Unofficial async Rust client for the Linear GraphQL API (API-key auth)", 521 downloads, 0 stars); `lineark` 3.1.0 (2026-08-01) is a CLI — [linear-sdk](https://crates.io/crates/linear-sdk), [linear-api](https://crates.io/crates/linear-api), [lineark](https://crates.io/crates/lineark)
- Jira: `gouqi` 0.20.0 (2025-10-21; 222,522 downloads, 156,004 recent; 34 stars; features include `oauth`, `async`, `cache`); `jira_v3_openapi` 1.6.1 (2026-02-09, generated, 53,042 downloads); `jira` 0.1.1 (updated 2026-10-01, 3,373 downloads, 1 star); `jira_query` 1.7.4 (2026-07-09, read-only); `jira-api-v2` 1.0.1 (2025-02-20, GPL-3.0-or-later); `goji` 0.2.4 (2018, dead) — [gouqi](https://crates.io/crates/gouqi), [jira_v3_openapi](https://crates.io/crates/jira_v3_openapi), [jira](https://crates.io/crates/jira), [jira_query](https://crates.io/crates/jira_query), [jira-api-v2](https://crates.io/crates/jira-api-v2), [goji](https://crates.io/crates/goji)
- Salesforce: `rustforce` 0.2.2 (last release 2022-05-21; 22,498 downloads; 47 stars); `salesforce-client` 0.2.0 (2026-01-08; 49 downloads); no crate named `salesforce` or `salesforce-rs` — [rustforce](https://crates.io/crates/rustforce), [salesforce-client](https://crates.io/crates/salesforce-client)
- HubSpot: `hubspot` 0.2.5 (2025-03-31; "An unofficial hupspot api client library"; 23,313 downloads; 3 stars, repo pushed 2026-09-27); `hubspot-rust-sdk` 0.4.4 (2025-03-24; 25 recent downloads) — [hubspot](https://crates.io/crates/hubspot), [hubspot-rust-sdk](https://crates.io/crates/hubspot-rust-sdk)
- Stale or squatted names: `asana` 0.1.1 (2022-09-26, 3,288 downloads); `airtable` 0.1.0 (2019); `airtable-api` 0.1.36 (2022-06-03, from archived Oxide `cio`); `zendesk` 0.1.0 (2022-08-10); `confluence` 0.4.1 (2019); `shopify` 0.1.6 (2018); `twilio` 1.1.0 (2024-03-25); `pagerduty-rs` 0.1.6 (2022-03-03); `slack_api` 0.23.1 (2020); `stripe-rust` 0.12.3 (2020); `intercom` is an unrelated COM-interop crate — [asana](https://crates.io/crates/asana), [airtable](https://crates.io/crates/airtable), [airtable-api](https://crates.io/crates/airtable-api), [zendesk](https://crates.io/crates/zendesk), [confluence](https://crates.io/crates/confluence), [shopify](https://crates.io/crates/shopify), [twilio](https://crates.io/crates/twilio), [pagerduty-rs](https://crates.io/crates/pagerduty-rs), [slack_api](https://crates.io/crates/slack_api), [stripe-rust](https://crates.io/crates/stripe-rust), [intercom](https://crates.io/crates/intercom)
- Figma and Zoom have generated clients: `figma-api` 0.31.4 (2025-12-03, 9,376 downloads), `zoom-api` 0.10.0 (2025-04-08, Oxide) — [figma-api](https://crates.io/crates/figma-api), [zoom-api](https://crates.io/crates/zoom-api)

**Inconsistency between crates (evidence from feature lists)**
- TLS/HTTP feature naming differs per crate: `octocrab` uses `rustls`, `rustls-ring`, `rustls-aws-lc-rs`, `opentls`, `default-client`; `async-stripe` uses `default-tls`, `native-tls`, `rustls-tls-native`, `rustls-tls-webpki-roots`, `async-std-surf`, `blocking`; `slack-morphism` uses `hyper`, `axum`, `rustls-native-certs`; `graph-rs-sdk` uses `native-tls`, `rustls-tls`, `openssl` — [octocrab](https://crates.io/crates/octocrab), [async-stripe](https://crates.io/crates/async-stripe), [slack-morphism](https://crates.io/crates/slack-morphism), [graph-rs-sdk](https://crates.io/crates/graph-rs-sdk)
- Licences vary across the set: MIT, Apache-2.0, dual MIT/Apache-2.0, ISC (serenity, twilight), GPL-3.0-or-later (`jira-api-v2`), LGPL-3.0-or-later (`oauth-axum`) — [serenity](https://crates.io/crates/serenity), [jira-api-v2](https://crates.io/crates/jira-api-v2), [oauth-axum](https://crates.io/crates/oauth-axum)

### Inferences
- Vendor-official Rust SDKs effectively exist only for cloud infrastructure (AWS, Azure, Google Cloud) and a few developer tools (Datadog, Sentry). For the business SaaS tools the proposed library targets (Slack, GitHub, Linear, Notion, Jira, Salesforce, HubSpot), everything is community-maintained, and Dropbox's own-org SDK explicitly disclaims official support. "Official" for the Google Cloud, AWS, Azure, Datadog and Sentry rows is inferred from the publishing GitHub organisation.
- Roughly a wrap-versus-build split: wrap candidates are `octocrab`, `slack-morphism`, `async-stripe`, Byron's `google-*` crates, `serenity`/`teloxide`, `gitlab`; build-from-scratch (or generate from OpenAPI/GraphQL schema) candidates are Linear, Notion, Jira, Salesforce, HubSpot, Asana, Airtable, Zendesk, Confluence, Intercom.
- Wrapping has a real cost: the strong crates disagree on HTTP stack (hyper vs reqwest vs surf), TLS feature names, error types and async style, so a unified crate that re-exports them would inherit several HTTP/TLS dependency trees and could not offer one auth or retry layer. This favours owning a shared HTTP/auth core and generating or hand-writing thin clients on top, in the OpenDAL style.
- The weak spots are exactly the categories where the proposed library's users feel the pain (issue trackers, CRMs, docs), which supports the premise that people are rewriting these integrations.

### Gaps
- API coverage depth of each crate (what fraction of each vendor API is implemented) was not measured.
- Did not verify whether Stripe, Slack, GitHub, Linear, Notion, Atlassian, Salesforce or HubSpot have announced any official Rust SDK plans; the "community" classification rests on repository ownership and crate descriptions.
- Did not check webhook/event support or OAuth support per crate beyond what feature names reveal.

## 5. Shared plumbing crates a unified integration library would need

### Takeaway
Every plumbing layer already has a mature, permissively licensed crate: `oauth2` for flows and refresh, `reqwest-middleware`/`reqwest-retry`/`tower`/`governor` for retry and rate limiting, `progenitor` for OpenAPI client generation, `cynic`/`graphql_client` for GraphQL. The missing pieces are the integration-specific glue: a catalogue of per-provider OAuth configurations, multi-provider webhook verification with real adoption, and a common pagination abstraction.

### Cited Findings

**OAuth2 / OIDC**
- `oauth2` 5.0.0 (2025-01-21), "An extensible, strongly-typed implementation of OAuth2", 55.9M downloads / 15.7M recent, MIT OR Apache-2.0, 1,210 stars, repo pushed 2026-02-22; features include `reqwest-blocking`, `rustls-tls`, `native-tls`, `pkce-plain` — [crates.io](https://crates.io/crates/oauth2), [GitHub](https://github.com/ramosbugs/oauth2-rs)
- `oauth2-reqwest` 0.1.0-alpha.3 (2026-02-22), "reqwest HTTP client for oauth2", from the same repo, already at 958K downloads — [crates.io](https://crates.io/crates/oauth2-reqwest)
- `openidconnect` 4.0.1 (2025-07-06), 14.7M downloads — [crates.io](https://crates.io/crates/openidconnect)
- `yup-oauth2` 12.1.2 (2026-01-07), 26.3M downloads, "device, service account and installed authorization flows" (used by the Google crates) — [crates.io](https://crates.io/crates/yup-oauth2)
- `oauth-axum` 0.1.4 (2024-12-18), "OAuth2 authorization code flow with Axum", 15,637 downloads, LGPL-3.0-or-later — [crates.io](https://crates.io/crates/oauth-axum)

**HTTP middleware, retry, rate limiting, caching**
- `reqwest-middleware` 0.5.2 (2026-05-19), 86.7M downloads / 19.4M recent, MIT OR Apache-2.0 — [crates.io](https://crates.io/crates/reqwest-middleware)
- `reqwest-retry` 0.9.1 (2026-02-05), 51.1M downloads; `reqwest-tracing` 0.7.1 (2026-05-19), 32.6M downloads — [reqwest-retry](https://crates.io/crates/reqwest-retry), [reqwest-tracing](https://crates.io/crates/reqwest-tracing)
- `tower` 0.5.3 (2026-01-12), 717M downloads; `tower-http` 0.7.1 (2026-08-31), 506M downloads — [tower](https://crates.io/crates/tower), [tower-http](https://crates.io/crates/tower-http)
- `governor` 0.10.4 (2025-12-16), 82.7M downloads; `tower-governor` 0.8.0 (2025-08-14), 5.1M; `leaky-bucket` 1.1.2 (2024-05-22), 2.8M — [governor](https://crates.io/crates/governor), [tower-governor](https://crates.io/crates/tower-governor), [leaky-bucket](https://crates.io/crates/leaky-bucket)
- `backon` 1.6.0 (2025-10-18), 92.6M downloads / 33.0M recent; `backoff` 0.4.0 has had no release since 2021-12-14 (107M downloads) — [backon](https://crates.io/crates/backon), [backoff](https://crates.io/crates/backoff)
- `http-cache-reqwest` 1.0.0-alpha.9 (2026-09-09), 3.8M downloads — [crates.io](https://crates.io/crates/http-cache-reqwest)

**Pagination**
- `page-turner` 1.0.0 (2024-01-15), "A generic abstraction of APIs with pagination", 130,919 downloads / 6,601 recent, MIT OR Apache-2.0 — [crates.io](https://crates.io/crates/page-turner)

**Webhook signature verification**
- `svix` 2.7.0 (2026-10-06), "Svix webhooks API client and webhook verification library", 1.12M downloads / 276K recent, MIT; repo 3,439 stars — [crates.io](https://crates.io/crates/svix), [GitHub](https://github.com/svix/svix-webhooks)
- `standardwebhooks` 1.0.1 (2024-03-04), 76,181 downloads / 43,522 recent, MIT — [crates.io](https://crates.io/crates/standardwebhooks)
- `webhook-verify` 0.1.0 (2026-09-08), "One function to verify inbound webhook signatures from major providers", features `paypal`, `sendgrid`, `actix`, `tower`, `http`; 652 downloads, 0 stars — [crates.io](https://crates.io/crates/webhook-verify)
- `webhookkit` 2.2.1 (2026-10-06), "provider-specific parsers for Stripe, GoCardless, ...", 247 downloads, 0 stars — [crates.io](https://crates.io/crates/webhookkit)
- `slack-morphism` exposes a `signature-verifier` feature for Slack request signing — [crates.io](https://crates.io/crates/slack-morphism)
- `hmac` 0.13.0 (2026-03-29), 633M downloads, is the primitive underneath — [crates.io](https://crates.io/crates/hmac)

**API client generation**
- `progenitor` 0.15.0 (2026-09-10), "An OpenAPI client generator" from Oxide, MPL-2.0, 5.69M downloads / 1.55M recent, 1,021 stars; runtime `progenitor-client` 0.15.0 has 6.7M downloads — [progenitor](https://crates.io/crates/progenitor), [GitHub](https://github.com/oxidecomputer/progenitor)
- `typify` 0.10.0-alpha.2 (2026-10-05), JSON Schema to Rust types, 31.2M downloads — [crates.io](https://crates.io/crates/typify)
- `openapiv3` 2.2.0 (2025-06-02), 13.8M downloads (OpenAPI 3.0.x); `oas3` 0.22.0 (2026-05-06), 2.5M downloads (OpenAPI 3.1.x) — [openapiv3](https://crates.io/crates/openapiv3), [oas3](https://crates.io/crates/oas3)
- `paperclip` 0.9.7 (2026-04-20), 959K downloads; the repo describes itself as "WIP OpenAPI tooling for Rust" — [crates.io](https://crates.io/crates/paperclip), [GitHub](https://github.com/paperclip-rs/paperclip)
- OpenAPI Generator (Java tool, 26,778 stars, Apache-2.0, pushed 2026-10-08) is the multi-language generator; the crates.io crate named `openapi-generator` is an unrelated 2019 package — [GitHub](https://github.com/OpenAPITools/openapi-generator), [crates.io](https://crates.io/crates/openapi-generator)
- `openapitor` 0.0.5 (2022-08-16) is Oxide's older generator associated with the third-party-api-clients repo — [crates.io](https://crates.io/crates/openapitor)
- Fern (commercial) advertises a Rust SDK generator from OpenAPI/AsyncAPI — [Fern blog](https://buildwithfern.com/post/rust-sdk-generator)
- A Fern marketing page claims Stainless "was acquired by Anthropic in May 2026 and is winding down its hosted SDK generator" and that Speakeasy pivoted; this is a competitor's claim and was not confirmed against a primary source — [Fern landing page](https://buildwithfern.com/lp/leave-stainless)
- GraphQL clients (needed for Linear, GitHub v4, Shopify): `cynic` 3.14.0 (2026-07-12, MPL-2.0, 5.05M downloads), `graphql_client` 0.16.0 (2026-01-15, Apache-2.0 OR MIT, 33.2M downloads) — [cynic](https://crates.io/crates/cynic), [graphql_client](https://crates.io/crates/graphql_client)

### Inferences
- The generic layers (OAuth protocol, retry, rate limit, tracing, codegen) do not need to be written; the library's own value would be the provider-specific knowledge on top: authorize/token URLs, scopes, refresh quirks, pagination style, rate-limit headers and webhook signing scheme per service. No crate found supplies that catalogue.
- Multi-provider webhook verification is an emerging niche (two crates appeared in September 2026) with no adoption yet, so shipping it as part of a unified library would not duplicate an established dependency.
- `progenitor` (MPL-2.0) generated code and runtime carry weak-copyleft terms at file level; a library choosing MIT/Apache-2.0 should check how generated output and `progenitor-client` are licensed before depending on them. This is a licensing point to verify, not a confirmed obstacle.
- `backoff` is effectively unmaintained; `backon` is the current choice.

### Gaps
- The exact names and maturity of OpenAPI Generator's Rust targets were not checked in this session.
- Did not find or verify a crate offering ready-made OAuth provider presets across many SaaS vendors; none surfaced in the searches run, but a dedicated search for "oauth providers" was not exhaustive.
- Did not verify whether `async-stripe` or `octocrab` ship webhook signature verification helpers.
- The Stainless and Speakeasy status claims rest on a single competitor-authored page.

## 6. Is the crate name `link` available, and what about alternatives?

### Takeaway
`link` is taken on crates.io by a deprecated 2016 crate that has been dormant since 2020; `linkkit`, `connectors`, `integrations` and `cognis-link` all returned "crate does not exist" on 2026-10-08. `cognis` itself is already owned by the same team (repository `0xvasanth/cognis`), which makes `cognis-link` the lowest-friction name.

### Cited Findings
- `link`: version 0.1.1, created 2016-07-10, last updated 2020-07-19, description "Now deprecated.", MIT, 5,346 total downloads (185 recent), repository `crlf0710/link-rs`, sole owner `crlf0710` (Charles Lew) — [crates.io](https://crates.io/crates/link)
- Not found on crates.io (API returned "crate does not exist") on 2026-10-08: `linkkit`, `connectors`, `integrations`, `integration`, `cognis-link`, `link-core`, `link-sdk`, `linkrs`, `linkhub`, `omnilink`, `unilink`, `onelink`, `connectkit`, `connectorkit`, `hookup`, `saas` — [linkkit](https://crates.io/crates/linkkit), [connectors](https://crates.io/crates/connectors), [integrations](https://crates.io/crates/integrations), [cognis-link](https://crates.io/crates/cognis-link)
- `cognis`: 0.3.2, created 2026-03-13, updated 2026-05-21, 256 downloads, "Cognis umbrella crate: agent builder, multi-agent orchestration, memory, middleware...", repository `0xvasanth/cognis` — [crates.io](https://crates.io/crates/cognis)
- `connector`: 0.1.1, created 2026-04-26, updated 2026-04-30, "connector lib for solana", MIT, 103 downloads, owner `ax-x2` — [crates.io](https://crates.io/crates/connector)
- `links`: 0.1.0 (2024-03-14), "find links from html and javascript", 2,803 downloads, owner `priv2024` — [crates.io](https://crates.io/crates/links)
- `link-rs`: 0.1.2 (2022-06-16), URL-shortening hash-id helper, 3,979 downloads — [crates.io](https://crates.io/crates/link-rs)
- Other taken names checked: `linkd` (actor framework, 2024-05-25), `linker` (2023-12-22), `linkage` (typing tutor, 2026-03-07), `uplink` (Storj binding, 0.11.0, 2025-05-30), `interlink` (2023-06-11), `integrate` (numerical integration, 0.3.1, 2026-05-10, 22,684 downloads), `integrator` (math, 2025-08-10), `plug` (IPC, 2026-04-27), `plugs` (0.0.1, 2026-03-31), `tether` (2019), `bridge` (2026-02-23), `bridges` (2019), `nexus` (0.0.1, 2016), `conduit` (0.10.0, 2021-11-06, 187K downloads), `junction` (2.1.0, 2026-09-24, 5.96M downloads), `unified` (Unifi controller client, yanked 0.1.0, 2021) — [linkd](https://crates.io/crates/linkd), [linker](https://crates.io/crates/linker), [linkage](https://crates.io/crates/linkage), [uplink](https://crates.io/crates/uplink), [interlink](https://crates.io/crates/interlink), [integrate](https://crates.io/crates/integrate), [integrator](https://crates.io/crates/integrator), [plug](https://crates.io/crates/plug), [plugs](https://crates.io/crates/plugs), [tether](https://crates.io/crates/tether), [bridge](https://crates.io/crates/bridge), [bridges](https://crates.io/crates/bridges), [nexus](https://crates.io/crates/nexus), [conduit](https://crates.io/crates/conduit), [junction](https://crates.io/crates/junction), [unified](https://crates.io/crates/unified)

### Inferences
- `link` cannot simply be registered. Obtaining it would require the current owner (crlf0710) to transfer it voluntarily; the "Now deprecated." description and six years of inactivity make a polite request plausible but not guaranteed.
- `link` is also a poor search term (collides with linker/linking concepts in Rust); `connectors` or `integrations` are free, descriptive, and currently unclaimed, which is unusual for such generic names and may not last.
- A "does not exist" API response means no crate is published under that name; it does not rule out crates.io reserving or rejecting a name at publish time, so availability should be confirmed by an actual `cargo publish` of a placeholder.
- Because per-integration feature flags want short names like `slack`, `github`, `linear`, the umbrella crate name matters less than keeping companion crate prefixes consistent (compare `rig-*`, `swissknife-*-sdk`); `cognis-link` fits the existing `cognis` namespace.

### Gaps
- Did not contact or check the activity of the `link` owner, and did not review crates.io's current name-transfer or squatting policy.
- Did not check trademark or GitHub organisation/repository name availability for any candidate name.
- Did not check lib.rs for name conflicts beyond crates.io (lib.rs mirrors crates.io, so none are expected).
