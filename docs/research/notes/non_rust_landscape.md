# Non-Rust landscape of unified third-party integration libraries, frameworks and platforms (state as of 2026-10-08)

Notes on method and reliability, for the report writer:

- "GitHub API, 2026-10-08" means I queried the repository directly on that date (stars, licence, language, directory counts, LICENSE text, README text). These are primary and exact for that day.
- Directory counts are counts of connector folders in the repo. They are not the same as the vendor's marketing count, and both are given where I have them.
- Counts obtained through GitHub code search or npm keyword search are approximate and are labelled as such.
- Anything I only saw in a search-engine summary of a third-party page, without opening a primary source, is listed under Gaps as unverified, not under Cited Findings.

## 1. Embeddable open-source integration libraries/frameworks: which can be consumed as a library rather than as a whole platform?

### Takeaway
Only Apache Camel (Java) is a true embeddable library with per-integration artifacts under a permissive licence, and it is transport/protocol-oriented rather than SaaS-auth-oriented. Almost everything else is either a platform whose connectors only run inside its own engine (n8n, Activepieces, Pipedream, Windmill, Node-RED), a subprocess/CLI protocol for data extraction (Airbyte, Singer/Meltano, Steampipe), or no longer ships integrations at all.

### Cited Findings

Comparison table (all GitHub figures: GitHub API, 2026-10-08):

| Project | Language | Stars | Licence (from LICENSE file) | Integration count | How an integration is defined | Library or platform |
|---|---|---|---|---|---|---|
| Apache Camel | Java | 6,360 | Apache-2.0 | 29 core + 387 non-core components (4 deprecated) | Hand-written Java component per JAR | Embeddable library |
| Airbyte | Python/Java | 22,188 | ELv2 (protocol MIT) | 699 connector dirs (612 source, 82 destination); marketing "600+/700+" | Mostly declarative YAML manifest; Python CDK; Java | Platform; connectors are containers |
| Meltano / Singer | Python | 2,647 (meltano) | MIT (meltano), Apache-2.0 (SDK) | Meltano Hub: 631 extractors, 50 loaders | Hand-written Python tap per repo | CLI + subprocess protocol |
| Steampipe | Go | 7,976 | AGPL-3.0 (plugin SDK Apache-2.0) | Hub: 158 plugins | Hand-written Go plugin, one repo each | CLI/Postgres-based engine |
| n8n | TypeScript | 206,845 | Sustainable Use License (source-available) | 308 node dirs in `nodes-base` | Hand-written TS node (declarative or programmatic style) | Platform |
| Activepieces | TypeScript | 24,941 | MIT except `packages/ee` | 736 community piece dirs | Hand-written TS "piece", one npm package each | Platform (pieces published to npm) |
| Pipedream | JavaScript | 11,720 | Pipedream Source Available License 1.0 | 3,401 component dirs; marketing "3,000+ APIs" | Hand-written JS components | Hosted platform only |
| Windmill | Rust backend | 18,128 | AGPLv3 + Apache-2.0 clients + proprietary EE | Not verified | Scripts shared on a hub | Platform |
| Trigger.dev | TypeScript | 16,496 | Apache-2.0 | None bundled (see Gaps) | n/a | Job platform |
| Node-RED | JavaScript | 23,720 | Apache-2.0 | ~7,655 npm packages tagged `node-red` (approximate) | Hand-written JS node + HTML editor file | Platform/runtime |

- Apache Camel's component index lists "29 in 26 JAR artifacts (0 deprecated)" core components and "387 in 314 JAR artifacts (4 deprecated)" non-core components — [Camel components index](https://camel.apache.org/components/next/index.html)
- Camel describes itself as "a small library with minimal dependencies for easy embedding in any Java application" and says "learn the API once and you can interact with all the Components provided out-of-box" — [What is Camel](https://camel.apache.org/manual/faq/what-is-camel.html)
- Camel is Apache-2.0, Java, 6,360 stars; `camel-slack` is published as its own Maven artifact (latest 4.20.0), confirming per-integration opt-in at build time — [apache/camel](https://github.com/apache/camel), [Maven Central search](https://search.maven.org/solrsearch/select?q=g:org.apache.camel+AND+a:camel-slack&rows=1&wt=json)
- Airbyte repo holds 699 connector directories: 612 `source-*`, 82 `destination-*` (GitHub API, 2026-10-08) — [airbytehq/airbyte connectors dir](https://github.com/airbytehq/airbyte/tree/master/airbyte-integrations/connectors)
- Airbyte connector implementation mix by `metadata.yaml` tag (GitHub code search, approximate, 2026-10-08): 518 `manifest-only` (declarative YAML), 106 Python, 71 Java. So roughly three quarters of connectors are pure declarative manifests — [airbytehq/airbyte](https://github.com/airbytehq/airbyte)
- Airbyte's Connector Builder is "an intuitive user interface on top of the low-code YAML format"; manifests cover auth, streams, pagination, incremental sync and error handling; the Builder is limited to source connectors, not destinations — [Connector Builder overview](https://docs.airbyte.com/platform/connector-development/connector-builder-ui/overview)
- Airbyte licensing as of 2026-10-08: "Airbyte Connectors and everything in our public repos excluding the airbytehq/airbyte-protocol are open sourced and available under the Elastic License 2.0 (ELv2)"; the protocol is MIT; Cloud, Enterprise and "Airbyte Agents" need a commercial licence — [Airbyte licenses doc](https://docs.airbyte.com/community/licenses)
- Connector `metadata.yaml` licence tally (code search, approximate): 654 ELv2, 10 MIT — [airbytehq/airbyte](https://github.com/airbytehq/airbyte)
- PyAirbyte exists as a way to run Airbyte connectors from Python ("brings the power of Airbyte to every Python developer"), 342 stars; the Python CDK is a separate MIT repo with 27 stars — [airbytehq/PyAirbyte](https://github.com/airbytehq/PyAirbyte), [airbytehq/airbyte-python-cdk](https://github.com/airbytehq/airbyte-python-cdk)
- Meltano Hub lists 631 extractor plugin names and 50 loader plugin names (directory count in the hub data repo, 2026-10-08) — [meltano/hub](https://github.com/meltano/hub)
- The `singer-io` GitHub org has 173 `tap-*` repositories; the community `MeltanoLabs` org has 89 repos, 62 of them `tap-*` — [singer-io org](https://github.com/singer-io), [MeltanoLabs org](https://github.com/MeltanoLabs)
- Steampipe Hub advertises "158 plugins"; the `turbot` org has 130 `steampipe-plugin-*` repositories; engine is AGPL-3.0, plugin SDK Apache-2.0 — [Steampipe Hub](https://hub.steampipe.io/plugins), [turbot/steampipe](https://github.com/turbot/steampipe), [turbot/steampipe-plugin-sdk](https://github.com/turbot/steampipe-plugin-sdk)
- n8n is under the "Sustainable Use License": "You may use or modify the software only for your own internal business purposes or for non-commercial or personal use"; `.ee` files need an Enterprise licence — [n8n LICENSE.md](https://github.com/n8n-io/n8n/blob/master/LICENSE.md)
- n8n core has 308 node directories under `packages/nodes-base/nodes`; npm has about 13,474 packages carrying the `n8n-community-node-package` keyword (npm search total, approximate, 2026-10-08) — [n8n nodes-base](https://github.com/n8n-io/n8n/tree/master/packages/nodes-base/nodes), [npm keyword search](https://www.npmjs.com/search?q=keywords:n8n-community-node-package)
- A secondary comparison site reports n8n's directory at 2,102 integrations and Activepieces at 761 pieces as of August 2026 (secondary source, not checked against vendor pages) — [automationatlas](https://automationatlas.io/answers/activepieces-vs-n8n-open-source-2026/)
- Activepieces is MIT outside `packages/ee`; 736 piece directories under `packages/pieces/community`; pieces are published individually to npm (e.g. `@activepieces/piece-slack` 0.21.0, 133,726 downloads in the week of 2026-09-28) — [Activepieces LICENSE](https://github.com/activepieces/activepieces/blob/main/LICENSE), [pieces dir](https://github.com/activepieces/activepieces/tree/main/packages/pieces/community), [npm piece-slack](https://www.npmjs.com/package/@activepieces/piece-slack)
- Activepieces describes itself as "extensible through a type-safe pieces framework written in TypeScript" and says contributed pieces "become automatically available as MCP servers"; repo tagline claims "~400 MCP servers for AI agents" — [activepieces/activepieces](https://github.com/activepieces/activepieces)
- Pipedream's component registry has 3,401 directories but is under the "Pipedream Source Available License", whose "Excluded Purpose" is "any commercial use of the software including, but not limited to, making available any software-as-a-service" — [Pipedream LICENSE](https://github.com/PipedreamHQ/pipedream/blob/master/LICENSE)
- Workday announced a definitive agreement to acquire Pipedream on 2025-11-19, citing 3,000+ pre-built connectors — [SiliconANGLE](https://siliconangle.com/2025/11/19/workday-acquire-pipedream-extend-ai-agent-integrations-across-enterprise-apps/)
- Windmill: backend AGPLv3, client libraries and OpenAPI/OpenFlow spec Apache-2.0, enterprise features proprietary; the backend is written in Rust — [Windmill LICENSE](https://github.com/windmill-labs/windmill/blob/main/LICENSE)
- Trigger.dev is Apache-2.0; v3 "has been retired and is end of life", v4 is current — [Trigger.dev changelog v4.5.4](https://trigger.dev/changelog/v4-5-4), [triggerdotdev/trigger.dev](https://github.com/triggerdotdev/trigger.dev)
- Node-RED is Apache-2.0 with 23,720 stars; about 7,655 npm packages carry the `node-red` keyword (npm search total, approximate) — [node-red/node-red](https://github.com/node-red/node-red), [npm keyword search](https://www.npmjs.com/search?q=keywords:node-red)
- Adjacent projects checked for scale only (GitHub API, 2026-10-08): CloudQuery 6,538 stars MPL-2.0 Go; dlt 5,939 stars Apache-2.0 Python; StackQL 1,066 stars MIT Go; Huginn 50,027 stars MIT Ruby; Home Assistant core 91,305 stars Apache-2.0 Python — [cloudquery](https://github.com/cloudquery/cloudquery), [dlt](https://github.com/dlt-hub/dlt), [stackql](https://github.com/stackql/stackql), [huginn](https://github.com/huginn/huginn), [home-assistant/core](https://github.com/home-assistant/core)

### Inferences
- Consumable as an in-process library: Apache Camel (clearly), dlt and PyAirbyte (Python, data-extraction only). Consumable as a subprocess with no server: Singer taps, Steampipe plugins. Not consumable outside their own engine in any documented way: n8n nodes, Pipedream components, Windmill hub scripts, Node-RED nodes.
- Activepieces is the odd one: its pieces are MIT and individually installable from npm with high download counts, so the packaging matches "opt in per integration", but they are written against the Activepieces engine context, and I found no documentation of standalone use.
- Licence is a real filter. Of the large catalogues, only Camel (Apache-2.0), Activepieces (MIT), Node-RED (Apache-2.0) and Meltano/Singer are permissively licensed. Airbyte (ELv2), n8n (SUL) and Pipedream (source-available, no commercial use) cannot be freely reused inside another product.
- Airbyte shows that declarative definitions scale catalogue size: about 518 of 699 connectors are YAML-only. That works because all its connectors do the same job (read records), which is not true of action-style SaaS integrations.

### Gaps
- I did not verify whether Activepieces pieces can be executed outside the Activepieces runtime; the publishing doc I opened did not address it.
- Windmill Hub integration count and Node-RED flow-library count were not verified from a primary page.
- Whether Trigger.dev's older per-service integration packages (from v2) still exist was not confirmed; search returned only the v3 retirement notice.
- Steampipe's alternative distributions (plugins as Postgres FDW / SQLite extensions / standalone exporters) were not checked in this session.
- Camel's GitHub star count understates its use; I did not gather Maven download figures.

## 2. Unified API vendors: how do they differ, what do they charge, which are open source?

### Takeaway
Unified API vendors are hosted, proprietary services that normalise data per category (HRIS, ATS, CRM, accounting) and charge either per connected customer account or per API call, typically from roughly $600 to $3,000 per month before enterprise contracts. Nango is the only one with a public source repository of note, it is ELv2 rather than open source, and its documented self-hosting paths now sit on the Enterprise plan.

### Cited Findings

| Vendor | Model | Claimed coverage | Public pricing (date checked 2026-10-08) | Source availability |
|---|---|---|---|---|
| Merge.dev | Normalised models per category; sync-and-store | Categories: accounting, ATS, CRM, file storage, HRIS, ticketing | 3 linked accounts free; $650/mo up to 10; $65 per extra linked account | Proprietary |
| Unified.to | Pass-through, normalised | "1120+ Integrations", "33+ Unified APIs" | $750 / $1,500 / $3,000 per month by API-call volume | Proprietary |
| Apideck | Pass-through, normalised | 12 selectable unified API categories | €599/mo (25-50 consumers), €1,299/mo (100-500) | Proprietary |
| Nango | Auth + proxy + user-written TypeScript functions; no forced common model | "1,000+ APIs", "7,000+ pre-built tools, triggers & syncs" | Free tier; $50/mo + usage ($0.29 per connection, $0.72 per compute hour, $0.50/GB) | ELv2, 12,554 stars |
| Pica (formerly IntegrationOS) | Agent tooling, passthrough actions | "200+ integrations and 25,000+ actions" | Not checked | Community edition GPL-3.0, unmaintained |
| Paragon | Embedded iPaaS + ActionKit | "1000+ integration actions" | Not public | Proprietary; cloud or on-prem |

- Merge pricing: Launch tier gives 3 production Linked Accounts free, then "$650/month for up to 10 total production Linked Accounts" and $65 per additional account; Professional and Enterprise are contract-priced — [Merge pricing](https://www.merge.dev/pricing)
- Merge uses a sync-and-store architecture while Unified.to and Apideck are pass-through and do not cache customer data (this is Unified.to's own characterisation of a competitor) — [Unified.to blog](https://unified.to/blog/apideck_vs_unified_which_pass_through_unified_api_is_right_for_your_product_in_2026)
- Unified.to pricing: Grow $750/mo for 750,000 API calls, Pro $1,500/mo for 2,000,000, Scale $3,000/mo for 6,000,000; "unlimited customer connections"; claims "1120+ Integrations" across "33+ Unified APIs"; customers can "switch to your app's OAuth 2 credentials" — [Unified.to pricing](https://unified.to/pricing)
- Apideck pricing: Launch from €599/mo with 1 unified API category, Scale from €1,299/mo with 3 categories, metered on "active consumers" (a customer whose connected account made at least one call or webhook event that month); per-consumer price falls from €23.96 to €8.50 with volume — [Apideck pricing](https://www.apideck.com/pricing)
- Nango pricing: Free ($0, 10 connections, 10 compute hours, 10 GB); Pay-as-you-go $50/mo base plus $0.29 per connection, $0.72 per compute hour, $0.50 per GB; Enterprise custom and includes "self-hosting & BYOC" — [Nango pricing](https://nango.dev/pricing)
- Nango is licensed under Elastic License 2.0: "You may not provide the software to third parties as a hosted or managed service" — [Nango LICENSE](https://github.com/NangoHQ/nango/blob/master/LICENSE)
- Nango's README: "open-source platform for building product integrations. It supports 1,000+ APIs"; three primitives (Auth, Proxy, Functions); "You write integration logic as TypeScript functions ... and deploy to Nango's production runtime"; "Used in production by Replit, Ramp, Mercor" — [NangoHQ/nango](https://github.com/NangoHQ/nango)
- Nango's provider catalogue is a single declarative `providers.yaml`; a grep for top-level keys returned 1,046 entries (approximate, 2026-10-08) — [providers.yaml](https://github.com/NangoHQ/nango/blob/master/packages/providers/providers.yaml)
- Nango self-hosting, per the docs in its repo: "Nango offers two ways to run outside of Nango Cloud, both on the Enterprise plan" (BYOC or Self-Managed via Helm). The stack is five Node services (Server, Orchestrator, Jobs, Runner, Persist) plus Postgres, object storage, ElasticSearch and Redis — [Nango self-hosting doc](https://github.com/NangoHQ/nango/blob/master/docs/guides/platform/self-hosting/self-hosting.mdx)
- Pica's repo (now `withoneai/pica`, Rust, GPL-3.0, 1,485 stars) states: "This repository is the Community Edition of Pica and is no longer actively maintained. Pica has transitioned to a private, cloud-hosted platform" — [withoneai/pica](https://github.com/withoneai/pica)
- Paragon ActionKit claims "1000+ integration actions", offers cloud hosting and on-premise deployment, and a "fully managed authentication flow for your users" through its Connect Portal — [Paragon ActionKit](https://www.useparagon.com/actionkit)
- Earlier open-source unified API attempts have stalled: Supaglue (MIT, 426 stars) is archived with its last push on 2024-03-07; Revert (AGPL-3.0, 1,014 stars) had its last push on 2025-04-07 — [supaglue-labs/supaglue](https://github.com/supaglue-labs/supaglue), [revertinc/revert](https://github.com/revertinc/revert)

### Inferences
- Two pricing philosophies exist: per connected customer (Merge, Apideck, Finch, Nango's connection fee) and per API call (Unified.to). Both scale with the buyer's customer count, which is the standard complaint that motivates open-source alternatives.
- The category-normalised vendors (Merge, Apideck, Unified.to, and the vertical specialists Finch, Kombo, Rutter, Codat) sell the common data model. Nango and Pica sell auth plus raw access and leave modelling to the customer. A library aimed at "stop rewriting integrations" is closer to the second group.
- Open-source unified APIs have a poor survival record: Supaglue archived, Revert stale, Pica closed its source. Nango survived by being a hosted product first with ELv2 source.
- Nango's free self-hosted tier appears to have been withdrawn: the older "free self-hosting" doc URLs return 404 and the current doc places both self-hosting options on the Enterprise plan. This is my reading of the docs, not an announcement I found.

### Gaps
- Rutter, Finch, Codat and Kombo: I did not open their own pricing or coverage pages. Unverified secondary claims seen only in search summaries: Kombo 250+ integrations and a $25M Series A in February 2026 led by Volition Capital (total $30M); Finch 250+ integrations of which about 30 are automated API integrations, with a $65/month-per-connection starter price; Rutter $28.5M raised; Codat custom pricing. Candidate pages: [Apideck alternatives](https://www.apideck.com/alternatives), [Tracxn Kombo](https://tracxn.com/d/companies/kombo/__9zWrTI1RX4QKvEn-gdCncJd90_AOwNsJL9Dmlz7jgV0), [Tracxn Rutter](https://tracxn.com/d/companies/rutter/__nd_20FZ6mMt-MC9PA51AV6K7A3SzsFvNFGf6OAT55f0).
- Merge's total integration count and Merge Agent Handler pricing were not on the pricing page I read.
- Conflict on Unified.to coverage: its pricing page says 1120+ integrations and 33+ unified APIs; a competitor-authored comparison surfaced in search said 446+ integrations across 26+ categories, probably an older figure — [Truto blog](https://www.truto.one/blog/top-5-unifiedto-alternatives-for-b2b-saas-integrations-2026/).
- Conflict on Nango pricing: a third-party review cites Starter $50 and Growth $500 plans, which does not match the pricing page I read on 2026-10-08 — [Knit review](https://getknit.dev/blog/nango-review-evaluation-integration-platform). Nango's funding (a search summary said $7.5M seed) was not verified.
- Paragon's connector count (a search summary said 130+) and pricing were not verified.
- Whether Apideck and Merge store customer data was not stated on the pages I read.

## 3. Agent/LLM tool integration platforms: counts, and library vs. hosted service

### Takeaway
The agent-tool vendors (Composio, Arcade, Klavis, Pipedream Connect, Zapier MCP, Metorial) claim the largest catalogues, from about 100 to 9,000 apps, but all execute tools on a hosted or self-hosted server; their open-source repos are client SDKs or MCP server collections. LangChain and LlamaIndex are the only true libraries here, and LangChain sunset its shared community integrations package in May 2026.

### Cited Findings

| Product | Claim (source, date) | Form | Open-source part |
|---|---|---|---|
| Composio | "1500+ toolkits" (pricing page), "1000+ toolkits" (repo) | Hosted execution, SDKs | SDK repo MIT, 30,465 stars |
| Arcade.dev | "7,500+ agent-optimized tools" | Hosted, or self-hosted via Helm | `arcade-mcp` MIT, 1,046 stars |
| Klavis | "100+ prebuilt integrations" (README) | Hosted MCP + self-hostable servers | Apache-2.0, 5,806 stars |
| Pipedream Connect | "3,000+ APIs", "10,000+ tools" | Hosted only | Source-available, no commercial use |
| Zapier MCP | "9,000 apps", "66,000+ triggers and actions" | Hosted only | None |
| Metorial | "1200+ integrations" | Hosted or self-hosted control plane | FSL-1.1-ALv2, 3,365 stars |
| Pica | "200+ integrations and 25,000+ actions" | Hosted | Community edition unmaintained |
| LangChain | "nearly 700 integrations" (Jan 2024) | Python/JS library | MIT, 147,551 stars |
| LlamaIndex | 101 LLM, 63 embedding, 78 vector store, 149 reader, 67 tool packages | Python library | MIT, 52,434 stars |

- Composio pricing: Hobby $0 with 100K tool calls/month; Pro $29/month plus $0.0003 per tool call and $0.003 per trigger event; "1500+ toolkits"; supports bring-your-own OAuth apps, and Composio-managed shared OAuth apps are limited to "20K free tool calls a month" versus 100K with your own apps; white-labelling is $0.30 per connection — [Composio pricing](https://composio.dev/pricing)
- Composio's SDK repo is MIT, TypeScript, 30,465 stars, described as powering "1000+ toolkits, tool search, context management, authentication, and a sandboxed workbench" — [ComposioHQ/composio](https://github.com/ComposioHQ/composio)
- Arcade describes itself as "the enterprise-ready actions runtime for AI agents", claims "7,500+ agent-optimized tools", documents 40+ built-in auth providers, and deploys as Arcade Cloud, via Azure/AWS marketplaces, or self-hosted with Helm; tools are written with a Python SDK — [Arcade docs](https://docs.arcade.dev/en/home)
- Arcade's own comparison says Composio "prioritizes breadth and fast onboarding with 500+ auto-generated integrations" while Arcade prioritises depth (vendor's claim about a competitor) — [Arcade vs Composio](https://arcade.dev/compare/arcade-vs-composio/)
- Klavis repo: Apache-2.0, 5,806 stars, 101 directories under `mcp_servers`, last push 2026-06-01; README advertises "100+ prebuilt integrations out-of-the-box, with OAuth support" and "Strata", a single MCP server that discloses tools progressively — [Klavis-AI/klavis](https://github.com/Klavis-AI/klavis), [Introducing Strata](https://www.klavis.ai/blog/introducing-strata-one-mcp-server-for-thousands-of-tools)
- Pipedream Connect: "3,000+ APIs", "10,000+ tools" via its MCP server; developers may use Pipedream's approved OAuth clients or "use your own"; credentials "encrypted at rest"; free in development mode — [Pipedream Connect docs](https://pipedream.com/docs/connect)
- Zapier MCP: "9,000 apps" and "66,000+ triggers and actions"; "Each tool call uses two tasks from your existing task quota"; credentials "stay in Zapier's managed connection layer instead of being passed into the model" — [Zapier MCP](https://zapier.com/mcp)
- Metorial (YC F25) calls itself an "open-source identity and access layer for AI agents" with "1200+ integrations"; licence is Functional Source License 1.1 with Apache-2.0 future licence, which is source-available, not open source — [metorial/metorial](https://github.com/metorial/metorial)
- LangChain claimed "nearly 700 integrations" at v0.1.0 in January 2024 — [LangChain v0.1.0 blog](https://www.langchain.com/blog/langchain-v0-1-0)
- The LangChain monorepo now carries 16 partner packages (anthropic, chroma, deepseek, exa, fireworks, groq, huggingface, mistralai, nomic, ollama, openai, openrouter, perplexity, qdrant, typesafe, xai); the `langchain-ai` org has 244 repositories with `langchain-` in the name (GitHub API, 2026-10-08) — [libs/partners](https://github.com/langchain-ai/langchain/tree/master/libs/partners), [langchain-ai org](https://github.com/langchain-ai)
- LlamaIndex integration package counts by folder (GitHub API, 2026-10-08): llms 101, embeddings 63, vector_stores 78, readers 149, tools 67 — [llama-index-integrations](https://github.com/run-llama/llama_index/tree/main/llama-index-integrations)
- The reference MCP servers repository has 91,071 stars and is moving from MIT to Apache-2.0 — [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers)

### Inferences
- Counts are not comparable across vendors. Composio counts toolkits (apps), Arcade and Pipedream count individual tools, Zapier counts apps and actions separately, Nango counts APIs for which it has an auth template. A catalogue of "7,500 tools" and one of "1,500 toolkits" may cover a similar number of services.
- Every agent-tool vendor's open-source artifact is a client for a server that holds the credentials. None lets you compile the integration code into your own binary and run without their runtime or a self-hosted equivalent.
- LangChain and LlamaIndex prove the per-integration-package library model works at the hundreds scale, but their integrations are mostly model providers, vector stores and document loaders with API-key auth. They do not manage end-user OAuth connections.
- MCP has become the default interop layer for this category; every vendor in the table exposes its catalogue as MCP.

### Gaps
- Toolhouse: no primary data found on tool counts, licence or pricing; search returned only third-party MCP directory listings.
- Funding figures were not verified against primary announcements. Seen only in search summaries: Composio $29M total ($4M seed plus $25M Series A led by Lightspeed; candidate page [rywalker.com](https://rywalker.com/research/composio)); Arcade "$60M Series A in June 2026, $72M total" (no primary source located); Klavis as YC Spring 2025 with "300+ services" (candidate page [rywalker.com](https://rywalker.com/research/klavis-ai)), which conflicts with the "100+" in its README.
- A Composio marketing article surfaced in search claims "50,000+ pre-built, LLM-optimized tools"; I did not open it and it conflicts in unit with the "1500+ toolkits" on the pricing page.
- Klavis pricing page returned no content. Arcade pricing was not checked. LangChain's and LlamaIndex's current official total integration counts were not checked.

## 4. Maintenance burden at scale and governance models

### Takeaway
Every large catalogue ends up tiered: a small set the core team guarantees (Airbyte certifies about 71 of about 700 connectors) and a long tail that is community-owned, unsupported and periodically archived. The models that held up put each integration in its own package with a named owner, ideally the vendor of the API (Terraform partner providers, LangChain partner packages); the monolithic shared package model failed explicitly at LangChain.

### Cited Findings
- Airbyte's support levels: "Airbyte Connectors" maintained by Airbyte; "Enterprise Connectors" maintained by Airbyte for paying customers at additional cost; "Marketplace Connectors" maintained by community members, "Not covered by Airbyte support SLAs" and which "Might not be feature complete and may experience backward-incompatible, breaking changes"; "Custom Connectors" are the user's responsibility — [Airbyte connector support levels](https://docs.airbyte.com/integrations/connector-support-levels)
- Airbyte `metadata.yaml` tally (GitHub code search, approximate, 2026-10-08): 71 connectors with `supportLevel: certified`, 556 with `supportLevel: community`. About 10% of the catalogue carries the vendor's maintenance commitment — [airbytehq/airbyte](https://github.com/airbytehq/airbyte)
- Airbyte archives connectors for "low use and/or lack of maintenance from the Community"; archived connectors get no updates and live in a separate `connector-archive` repo created 2024-02-16 — [Airbyte connector support levels](https://docs.airbyte.com/integrations/connector-support-levels), [airbytehq/connector-archive](https://github.com/airbytehq/connector-archive)
- Airbyte's 2021 licence post states that competitors "plateau at approximately 150 connectors" because of maintenance, and that Airbyte intended a "participative model" in which community maintainers share revenue for upholding SLAs; at that time "existing connector licenses won't change" (core moved to ELv2 from v0.30.0, 2021-09-27) — [Airbyte ELv2 blog](https://airbyte.com/blog/a-new-license-to-future-proof-the-commoditization-of-data-integration)
- On 2023-06-30 Airbyte extended ELv2 to its strategic, self-maintained connectors (Postgres, MySQL, MSSQL, MongoDB, Oracle, S3, GCS, Salesforce, HubSpot, Stripe, Shopify, Google Ads, Facebook Marketing, Zendesk Support, BigQuery, Snowflake, Redshift and others), saying community-maintained connectors were not affected — [Update on Airbyte's license](https://airbyte.com/blog/update-on-airbytes-license)
- By 2026-10-08 Airbyte's docs say all connectors in its public repos are ELv2, and the metadata tally shows 654 ELv2 versus 10 MIT — [Airbyte licenses doc](https://docs.airbyte.com/community/licenses)
- LangChain's reasons for the 2024 split: the monolith "became bloated and unstable as we took a 'maintain everything' approach"; "all dependencies were optional, leading to some headaches when trying to install specific versions"; third-party changes "require breaking changes. These can now be reflected on an individual integration basis with proper versioning" — [LangChain v0.1.0 blog](https://www.langchain.com/blog/langchain-v0-1-0)
- LangChain sunset `langchain-community` on 2026-05-22, effective immediately. Stated reasons: the package "grew to include a very large number of integrations with widely varying levels of usage, maintenance, and maturity", with "a large number of optional dependencies, and a shared release cycle, which makes it difficult to iterate on individual integrations, maintain consistent quality, and introduce breaking changes"; it had "been effectively deprecated for more than a year" with "no new integrations or features" accepted — [langchain-community issue #674](https://github.com/langchain-ai/langchain-community/issues/674)
- The same notice gives two demand-side reasons: "With coding agents, it is often simpler to implement tools directly in application code", and "broader adoption of MCP has made it more natural for some teams to consume tools through the MCP protocol instead". It endorses standalone packages "maintained by the people closest to them" — [langchain-community issue #674](https://github.com/langchain-ai/langchain-community/issues/674)
- The `langchain-community` repository is now archived (287 stars, last push 2026-06-19) — [langchain-ai/langchain-community](https://github.com/langchain-ai/langchain-community)
- n8n's own docs on community nodes: they "have full access to the machine that n8n runs on, and can do anything, including malicious actions", and "upgrading to a version with a breaking change could cause all workflows using the node to break"; n8n runs a verified community node programme and self-hosters can disable community nodes — [n8n community node risks](https://docs.n8n.io/integrations/community-nodes/risks/)
- January 2026 supply-chain attack on n8n community nodes: Endor Labs identified at least 10 malicious npm packages posing as n8n integrations; "Community nodes run with the same level of access as n8n itself. They can read environment variables, access the file system, make outbound network requests, and ... receive decrypted API keys and OAuth tokens during workflow execution" — [Endor Labs](https://www.endorlabs.com/learn/n8mare-on-auth-street-supply-chain-attack-targets-n8n-ecosystem)
- The Hacker News reported the same campaign as eight packages with over 27,000 downloads combined (figures differ from Endor Labs' count of at least 10; taken from a search summary of the article) — [The Hacker News](https://thehackernews.com/2026/01/n8n-supply-chain-attack-abuses.html)
- Terraform Registry tiers: Official providers "are owned and maintained by HashiCorp"; Partner providers "are written, maintained, validated and published by third-party companies against their own APIs"; Community providers are published by individuals or groups; Archived providers "are no longer maintained", typically due to API deprecation or low adoption. There is also a Partner Premier tier — [Terraform Registry providers](https://developer.hashicorp.com/terraform/registry/providers)
- Terraform core is under the Business Source License with IBM as licensor (GitHub API, 2026-10-08) — [Terraform LICENSE](https://github.com/hashicorp/terraform/blob/main/LICENSE)
- Singer org health (GitHub search, 2026-10-08): 173 `tap-*` repos, only 5 archived; 141 pushed within the last 12 months; 26 with no push since October 2024; but 723 open pull requests across the org, 401 of them opened before 2024 — [singer-io org](https://github.com/singer-io)
- A second-generation community home exists for Singer connectors: MeltanoLabs holds 62 `tap-*` repos built on the Meltano SDK, 4 archived — [MeltanoLabs org](https://github.com/MeltanoLabs)
- Activepieces' catalogue is majority community-built: about 60% of roughly 764 pieces as of September 2026 (secondary source) — [automationatlas](https://automationatlas.io/answers/n8n-vs-activepieces-2026/)
- Apache Camel, governed by the ASF with all components in one repo, lists only 4 deprecated components out of 416 — [Camel components index](https://camel.apache.org/components/next/index.html)

### Inferences
- The observed ceiling for what one core team can keep at a guaranteed quality level is in the tens to low hundreds: Airbyte's "150" claim about competitors, its own roughly 71 certified connectors, LangChain's 16 in-repo partner packages, n8n's 308 core nodes.
- Three governance patterns appear, with different outcomes:
  - Core-team-owned monorepo (Camel, n8n core, Airbyte certified): consistent quality, slow growth, bounded size.
  - Community-owned inside a shared package or repo (langchain-community, Airbyte marketplace, Singer org): fast growth, then uneven quality, stale PRs and eventual archiving or sunset.
  - Vendor- or owner-maintained separate packages with a registry and tiers (Terraform partner providers, LangChain partner packages, Steampipe one-repo-per-plugin): the most durable, because ownership and versioning are per integration.
- The Singer data is mixed rather than a clean abandonment story: repos still receive pushes, but 401 pull requests older than 2024 remain open, which fits a pattern of owner-driven updates with external contributions going unreviewed. This is my interpretation of the counts.
- Running third-party integration code in-process with access to decrypted credentials is a security liability at catalogue scale, as the n8n incident shows. A compiled library with per-integration opt-in narrows this (you only link what you chose) but does not remove the need for review of each integration.
- Airbyte's path from MIT connectors (2021) to selected ELv2 (2023) to all ELv2 (by 2026) shows that a company funding connector maintenance tends to restrict the licence over time. A community library needs a different funding or ownership answer.
- LangChain's stated view that coding agents make it "simpler to implement tools directly in application code" is a direct challenge to the premise of a shared integrations library and should be weighed in the report.

### Gaps
- I found no published figures on engineering hours or cost per connector per year from any vendor.
- I did not find a first-party n8n statement of how many community nodes are verified, nor Activepieces' own data on community piece quality.
- No count of Terraform providers per tier was on the page I read.
- I did not find engineering blog posts from Merge, Nango or Composio with quantitative maintenance data.
- Whether Airbyte's revenue-sharing "participative model" for community maintainers was ever implemented was not verified.

## 5. Bring-your-own OAuth app and credential storage

### Takeaway
Bring-your-own OAuth client is widely supported and is the norm for production use (Composio, Pipedream Connect, Unified.to, Nango, Corsair, and by design in Camel, n8n and Activepieces), but in the hosted products the resulting user tokens are stored on the vendor's servers. Corsair is the one project found that documents keeping user tokens in the developer's own database under the developer's own encryption key in every mode.

### Cited Findings
- Composio: supports "bring-your-own OAuth apps and API keys"; its managed shared OAuth apps carry a lower free quota (20K tool calls/month versus 100K); advanced white-labelling costs $0.30 per connection — [Composio pricing](https://composio.dev/pricing)
- Pipedream Connect: developers can use Pipedream's approved OAuth clients or "use your own"; credentials are "encrypted at rest" on Pipedream — [Pipedream Connect docs](https://pipedream.com/docs/connect)
- Unified.to: customers can "switch to your app's OAuth 2 credentials" for production — [Unified.to pricing](https://unified.to/pricing)
- Nango: "Managed OAuth, API keys, and token refresh for 1,000+ APIs ... Nango handles credentials, token storage, and multi-tenant connection management"; in a self-hosted deployment Postgres "Stores data for the control plane, API credentials, scheduled tasks, and synced records" — [NangoHQ/nango README](https://github.com/NangoHQ/nango), [Nango self-hosting doc](https://github.com/NangoHQ/nango/blob/master/docs/guides/platform/self-hosting/self-hosting.mdx)
- Arcade: "handles OAuth and manages user tokens, API keys, and secrets for tools" with 40+ built-in auth providers, and can be self-hosted — [Arcade docs](https://docs.arcade.dev/en/home)
- Zapier MCP: hosted auth only in what I read; credentials "stay in Zapier's managed connection layer" — [Zapier MCP](https://zapier.com/mcp)
- Paragon: "fully managed authentication flow for your users, directly in your product" via its Connect Portal — [Paragon ActionKit](https://www.useparagon.com/actionkit)
- Corsair: "Your users' API tokens live in your database under your KEK in both modes"; in Hub mode, "Hub delivers your tenants' tokens to you and keeps zero copies; it holds only the OAuth client id/secret" (managed or bring-your-own); Manual mode "is fully featured, with no external dependency", with the developer hosting the connect page and OAuth callback route — [Corsair manual-vs-hub doc](https://github.com/corsairdev/corsair/blob/main/docs/hub/manual-vs-hub.mdx)
- n8n stores credentials encrypted with an instance master key and passes them decrypted to node code at runtime, which is what the January 2026 malicious nodes exploited — [Endor Labs](https://www.endorlabs.com/learn/n8mare-on-auth-street-supply-chain-attack-targets-n8n-ecosystem)
- Metorial positions auth as its core: "Auth and token lifecycle management for OAuth, API keys, service accounts", with RBAC, SAML SSO and audit logs — [metorial/metorial](https://github.com/metorial/metorial)
- General-purpose OAuth libraries exist separately from integration catalogues, e.g. Grant (MIT, 4,163 stars, last push 2025-02-04), OmniAuth (MIT, 8,104 stars), better-auth (MIT, 30,209 stars); these solve sign-in and token acquisition but ship no API clients (GitHub API, 2026-10-08) — [simov/grant](https://github.com/simov/grant), [omniauth/omniauth](https://github.com/omniauth/omniauth), [better-auth/better-auth](https://github.com/better-auth/better-auth)

### Inferences
- "Vendor's shared OAuth app" is a development convenience everywhere and a production liability: consent screens show the vendor's name, rate limits are shared, and switching vendors forces every end user to re-authorise. Vendors price around this (Composio's quota difference and white-label fee).
- The harder part is not the OAuth client but the public surfaces: a callback URL, a connect page, token refresh scheduling and webhook receipt. Corsair's Manual/Hub split isolates exactly these as the only part that needs a server. That is the cleanest decomposition found and is directly relevant to a library design: a library can own provider metadata, token exchange, refresh and an encrypted storage trait, and leave the HTTP callback to the host application.
- A pluggable credential-store interface (developer-supplied database plus key) is rare. Most products assume their own store.

### Gaps
- I did not verify bring-your-own OAuth support or token storage location for Merge, Apideck, Klavis, Kombo, Finch, Rutter or Codat from primary pages.
- I did not read the credential-handling docs for Activepieces, Camel, Windmill or Steampipe; statements about them supplying credentials through configuration are from general knowledge and are not cited here.
- Nango's exact mechanism for supplying your own client id and secret was not read (doc pages returned 404), though the product is built around per-integration OAuth app configuration.

## 6. Does any project in any language match "single library, opt in per integration at build time, no server required"? What is the gap?

### Takeaway
Yes, partly. Apache Camel (Java) has matched the packaging model for years but for transports and protocols rather than SaaS auth, and Corsair (TypeScript, Apache-2.0, first commit October 2025, 13,422 stars) matches it closely for SaaS APIs with one plugin package per integration, though it requires a database and steers users to a hosted Hub. Nothing equivalent was found outside the JVM and TypeScript in this (non-Rust) sweep, and nothing found combines permissive licence, compile-time opt-in, no required runtime service, pluggable credential storage and a governance model built for a long tail.

### Cited Findings
- Corsair usage: `createCorsair({ plugins: [github(), slack()], database: db, kek: ... })`; each integration is a separate npm package such as `@corsair-dev/slack` (v0.1.8, Apache-2.0) with `corsair` as a peer dependency; calls follow `corsair.[integration].api.[resource].[action]()` — [Corsair core README](https://github.com/corsairdev/corsair/blob/main/packages/corsair/README.md), [slack package.json](https://github.com/corsairdev/corsair/blob/main/packages/slack/package.json), [Corsair integrations doc](https://github.com/corsairdev/corsair/blob/main/docs/concepts/integrations.mdx)
- Corsair traction and scale (GitHub API and npm, 2026-10-08): 13,422 stars, repo created 2025-10-14, about 273 contributors, 358 directories under `packages/`, core package `corsair` at v0.1.138 with 29,258 npm downloads in the week of 2026-09-28 — [corsairdev/corsair](https://github.com/corsairdev/corsair), [npm corsair](https://www.npmjs.com/package/corsair)
- Corsair requires a database: "Five tables"; "Every API call and webhook that flows through Corsair is stored in your database automatically"; SQLite and Postgres are supported; the core package depends on `kysely` and `kysely-postgres-js` — [Corsair database doc](https://github.com/corsairdev/corsair/blob/main/docs/concepts/database.mdx), [npm corsair](https://www.npmjs.com/package/corsair)
- Corsair has three operating modes: Manual (self-hosted, "no external dependency"), Hub (Corsair hosts OAuth connect/callback/approval surfaces; "the recommended path"), and Corsair Cloud ("a hosted runtime for your integrations ... every tool call is one HTTP request") — [manual-vs-hub doc](https://github.com/corsairdev/corsair/blob/main/docs/hub/manual-vs-hub.mdx), [cloud overview doc](https://github.com/corsairdev/corsair/blob/main/docs/cloud/overview.mdx)
- Corsair integrations are hand-written TypeScript plugins scaffolded by a generator into the monorepo (`client.ts`, `endpoints/`, `webhooks/`, `schema/`), with the docs advising contributors to "let Claude Code fill in the implementation"; non-TypeScript clients (Go, Python, Swift) exist in the repo alongside adapters for LangChain, LlamaIndex and Mastra — [create-your-own-plugin doc](https://github.com/corsairdev/corsair/blob/main/docs/guides/create-your-own-plugin.mdx), [corsairdev/corsair](https://github.com/corsairdev/corsair)
- Apache Camel: "a small library with minimal dependencies for easy embedding in any Java application"; 416 components shipped as 340 separate JAR artifacts under Apache-2.0 — [What is Camel](https://camel.apache.org/manual/faq/what-is-camel.html), [Camel components index](https://camel.apache.org/components/next/index.html)
- LangChain's direction is per-integration standalone packages "maintained by the people closest to them" — [langchain-community issue #674](https://github.com/langchain-ai/langchain-community/issues/674)
- LlamaIndex ships each integration as its own package in categorised folders (101 LLM, 149 reader, 67 tool packages) under MIT — [llama-index-integrations](https://github.com/run-llama/llama_index/tree/main/llama-index-integrations)
- Activepieces publishes each piece as an MIT npm package (`@activepieces/piece-slack`, 133,726 weekly downloads) — [npm piece-slack](https://www.npmjs.com/package/@activepieces/piece-slack), [Activepieces LICENSE](https://github.com/activepieces/activepieces/blob/main/LICENSE)
- Nango, the closest "open" unified-auth product, needs five services plus Postgres, object storage, ElasticSearch and Redis to self-host, on an Enterprise plan — [Nango self-hosting doc](https://github.com/NangoHQ/nango/blob/master/docs/guides/platform/self-hosting/self-hosting.mdx)
- Superface OneSDK, an earlier attempt at "One Node.js SDK for all the APIs you want to integrate with", has 52 stars and no push since 2025-01-31 — [superfaceai/one-sdk-js](https://github.com/superfaceai/one-sdk-js)
- Search for further TypeScript alternatives surfaced "OpenConnector", described in a search summary as an open-source auth gateway for 1,000+ SaaS providers (a gateway, i.e. a server; not opened or verified) — [Nango blog on Composio alternatives](https://nango.dev/blog/composio-alternatives/)

### Inferences

Closest matches, ranked by fit to "single library, per-integration opt-in at build time, no server":

1. Corsair (TypeScript). Fits on packaging, licence, typed uniform API, own-database token storage and bring-your-own OAuth. Departs on: mandatory database with automatic mirroring of API data, a hosted Hub as the recommended OAuth path, a commercial Cloud, and very young versions (0.1.x). Its growth to about 358 packages in twelve months, with docs that push AI-generated plugin code, raises the same long-tail quality question as Airbyte's marketplace; I found no tiering or certification scheme in the docs I read.
2. Apache Camel (Java). Fits on packaging, licence, embedding and governance (ASF, low deprecation). Departs on: it models message endpoints and routing, not typed SaaS resource APIs, and I found no built-in multi-tenant OAuth connection management in what I read.
3. LangChain partner packages and LlamaIndex integrations (Python). Fit on packaging and licence. Depart on scope: model, vector store and loader integrations with API-key auth.
4. Activepieces pieces (TypeScript). Fit on packaging and licence. Depart on runtime: built to execute inside the Activepieces engine.
5. Singer taps, PyAirbyte, dlt, Steampipe plugins. No server needed, per-integration install, but read-only data extraction and (for Airbyte) ELv2.

What none of the surveyed projects offer, taken together:

- A permissively licensed library in a compiled systems language where each integration is selected at build time and only that code is linked into the binary. The existing library-shaped options are JVM or interpreted (Java, TypeScript, Python).
- Operation with no required runtime service and no required database, where token storage is a small interface the host application implements. Corsair comes nearest but mandates its schema; every other SaaS-auth product is a server.
- A neutral project with no hosted upsell. Corsair, Nango, Composio, Arcade, Klavis, Metorial, Pipedream and Airbyte are all venture-backed companies whose open or source-available code feeds a paid cloud, and Airbyte, Nango, n8n, Pipedream, Metorial and Pica show the licence or openness narrowing over time.
- A stated governance model for the long tail that learns from the failures above: explicit tiers, per-integration ownership and versioning, and an archive policy from day one. Terraform has this for infrastructure providers; no SaaS integration library found has it.
- Typed, non-LLM-specific coverage of both actions and events (webhooks) with end-user OAuth, usable equally from a backend service and an agent. The unified API vendors cover data sync per category; the agent vendors cover tool calls; Corsair is the only one found claiming both from a library.
- A shared, reusable, openly licensed provider-metadata catalogue (auth URLs, scopes, token quirks, rate-limit headers). Nango's `providers.yaml` with about 1,046 entries is the largest found, and it is ELv2.

Counter-evidence the report should weigh:

- LangChain's maintainers concluded in May 2026 that a shared community integrations package is not maintainable and that coding agents plus MCP reduce the need for one.
- The n8n incident shows that a large community catalogue with credential access is an attack surface.
- Corsair's rapid adoption shows demand for exactly this shape in TypeScript, which both validates the idea and means a Rust version would be a port of a proven pattern rather than a new category.

### Gaps
- This sweep excluded Rust by assignment, so "nothing equivalent" applies only to non-Rust ecosystems. Windmill's backend and Pica's abandoned community edition are Rust, but neither is an integrations library.
- I did not search Go, .NET, Elixir or PHP ecosystems specifically for Corsair-like libraries; absence there is not established.
- Corsair's Hub and Cloud pricing, funding, company background and any quality-tier policy were not found in the files I read.
- Whether Corsair's plugins are generated from OpenAPI specs or written by hand per endpoint was not determined beyond the generator scaffold description.
- I did not verify how many of Corsair's 358 package directories are integrations versus core, adapter or tooling packages.
