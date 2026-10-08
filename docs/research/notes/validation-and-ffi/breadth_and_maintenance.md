# Reaching and sustaining hundreds to thousands of SaaS integrations: mechanisms, measured quality and maintenance economics (state as of 2026-10-08)

Method notes for the report writer:

- "Measured" means I counted it myself on 2026-10-08 from the GitHub API, a published registry file, npm or crates.io. The URL given is the thing counted. File counts approximate operation counts (one action per file is the convention in these repositories, but helper files can inflate the totals slightly).
- "Claim" means a vendor's own number or statement. Almost every non-academic source here sells an integration product, so the bias is noted per item.
- "Search summary" means the fact came from a search-result summary of a page I did not fetch in full. Treat these as unconfirmed.
- I did not repeat what `mcp_and_codegen.md` and the 2026-10-08 landscape report already establish (progenitor, APIs.guru staleness, Airbyte manifest-only counts, Nango provider counts, MCP registry statistics).

## 1. Declarative operation formats: how each describes an operation, what they cover, and whether a Rust runtime could interpret them as data

### Takeaway
Runtime-interpreted definitions that cover writes exist and are in production (n8n declarative nodes, Zapier request objects, and the closed engines of Truto and StackOne), but no open catalogue uses them for more than a small minority of its integrations: about 28 of n8n's 308 node directories are declarative, and triggers are excluded by design. Every large open catalogue of actions and triggers (Activepieces, Pipedream, Nango, most of n8n) is ordinary code, and webhooks are code everywhere I looked.

### Cited Findings

Coverage summary (sources follow in the bullets below; "code" means the capability exists but is hand-written per integration):

| Format | Operation is described as | Writes | Pagination | Rate limits | Webhooks / triggers | Interpreted as data at runtime |
| --- | --- | --- | --- | --- | --- | --- |
| n8n declarative node | TypeScript object: `requestDefaults` plus per-field `routing` | Yes | Declared (`operations.pagination`) | Not found | No, by design | Yes, by n8n's routing engine |
| Zapier platform schema | JSON-schema-validated app definition; `perform` is a function or a request object | Yes (`creates`) | Flag only (`canPaginate`), logic in code | Declared (`throttle`, `lock`) | Declared lifecycle (`performSubscribe` / `performUnsubscribe`), bodies usually code | Partly (request-object form) |
| Activepieces piece | TypeScript `createAction` / `createTrigger` | Code | Code | Code | Code, with three declared strategies | No |
| Pipedream component | Node.js module with `props`, `hooks`, `run()` | Code | Code | Code | Code (`$.interface.http`, hooks) | No |
| Airbyte low-code manifest | YAML interpreted by the Python CDK | No (sources only) | Declared | Declared backoff | No | Yes |
| Singer tap | Executable emitting JSON messages | No (taps read) | Code | Code | No | No |
| OpenAPI + Arazzo + Overlays | Spec documents | Yes (any HTTP method) | Not in the standard; vendor `x-` extensions | Retry only (Arazzo `retryAfter` / `retryLimit`) | Declared shape only (`webhooks`, AsyncAPI channels) | Needs a custom engine |
| Truto (closed) | JSON config plus JSONata | Not stated | Declared | Metadata normalised, no retry | Declared verification and mapping | Yes (vendor claim) |
| StackOne (closed) | YAML connector | Not confirmed | Not confirmed | Not confirmed | Not confirmed | Yes (vendor claim) |

n8n:

- n8n's docs say "Build your node in the declarative style. It's the default for new nodes", and list the exceptions: "Trigger nodes must use the programmatic style. The declarative style doesn't support trigger nodes", any node "that isn't REST-based. This includes nodes that need to call a GraphQL API and nodes that use external dependencies", and any node "that needs to transform data beyond what routing handles". — [n8n docs: choose a node building style](https://docs.n8n.io/integrations/creating-nodes/plan/choose-node-method)
- The declarative keys are `requestDefaults` (base URL, headers), `routing.request` (method and URL per operation) and `routing.output.postReceive` (root-property extraction, key mapping, sorting). — [n8n docs: declarative-style parameters](https://docs.n8n.io/integrations/creating-nodes/build/reference/node-base-files/declarative-style-parameters)
- A pagination type for declarative routing (`IN8nRequestOperationPaginationOffset`) is defined in n8n's workflow package (measured: identifier present in `interfaces.ts` and `schemas.ts`). — [n8n `packages/workflow/src/interfaces.ts`](https://github.com/n8n-io/n8n/blob/HEAD/packages/workflow/src/interfaces.ts)
- Measured adoption inside n8n's own catalogue: `packages/nodes-base/nodes` has 308 top-level directories and 561 `*.node.ts` files, of which 105 are trigger nodes (in 97 directories). `requestDefaults` appears in 30 files, two of them tests, so about 28 node definitions are declarative. They include Okta, Brevo, Gong, Asana, WhatsApp, Google Ads and several Microsoft and AWS nodes. — [n8n nodes-base](https://github.com/n8n-io/n8n/tree/HEAD/packages/nodes-base/nodes)
- Declarative nodes do writes: 51 files under `nodes-base/nodes` contain both `routing` and `method: 'POST'` (measured by GitHub code search). — [n8n nodes-base](https://github.com/n8n-io/n8n/tree/HEAD/packages/nodes-base/nodes)

Zapier:

- The top-level `AppSchema` keys are `version`, `platformVersion`, `authentication`, `requestTemplate`, `beforeRequest`, `afterResponse`, `hydrators`, `resources`, `triggers`, `searches`, `creates`, `searchOrCreates`, `bulkReads`, `flags` and `throttle`. — [zapier-platform schema.md](https://github.com/zapier/zapier-platform/blob/HEAD/packages/schema/docs/build/schema.md)
- `perform` accepts either form: "This can be a function like `(z) => [{id: 123}]` or a request like `{url: 'http...'}`". — [zapier-platform schema.md](https://github.com/zapier/zapier-platform/blob/HEAD/packages/schema/docs/build/schema.md)
- Webhook triggers (`BasicHookOperationSchema`) require `perform`, `performList`, `performSubscribe` and `performUnsubscribe`; polling triggers and searches carry a `canPaginate` boolean that enables "pagination via temporary cursor storage". — [zapier-platform schema.md](https://github.com/zapier/zapier-platform/blob/HEAD/packages/schema/docs/build/schema.md)
- Rate control is declared: `ThrottleObjectSchema` applies "throttling when the limit for the window is exceeded" at app or action level, and `LockObjectSchema` makes actions run "one at a time per scope" (`user`, `auth` or `account`). Bulk creates use `performBuffer` with a `buffer` config. — [zapier-platform schema.md](https://github.com/zapier/zapier-platform/blob/HEAD/packages/schema/docs/build/schema.md)

Activepieces:

- Triggers are TypeScript objects with `onEnable`, `onDisable`, `run`, optional `test` and `onHandshake` functions, under one of three strategies: `TriggerStrategy.POLLING`, `WEBHOOK` or `APP_WEBHOOK`, plus a `WebhookRenewStrategy` enum (measured from source). — [Activepieces `trigger.ts`](https://github.com/activepieces/activepieces/blob/HEAD/packages/pieces/framework/src/lib/trigger/trigger.ts)
- Measured catalogue shape: 736 community piece directories, 8,491 action files in 689 pieces, and 1,470 trigger files in 394 pieces (54%). The median piece has 5 action files, the 90th percentile 33, the largest 376; 213 pieces have three or fewer action files and 47 have no `actions` directory. — [Activepieces community pieces](https://github.com/activepieces/activepieces/tree/HEAD/packages/pieces/community)

Pipedream:

- A component is a Node.js module exporting `name`, `key`, `type`, `version`, `description`, `props`, `methods`, `hooks`, `dedupe` and an async `run()`. Sources have `deploy()`, `activate()` and `deactivate()` hooks and are driven by `$.interface.http` (webhook endpoint) or `$.interface.timer` (interval or cron). Dedupe strategies are `unique` ("Pipedream maintains a cache of 100 emitted `id` values"), `greatest` and `last`. The documentation contains no declarative pagination or rate-limit construct. — [Pipedream component API](https://pipedream.com/docs/components/contributing/api)
- Measured catalogue shape: 3,401 app directories, 12,099 actions across 2,028 apps and 3,505 sources across 1,191 apps. 1,266 app directories (37%) contain neither an action nor a source, only the app/auth definition. — [Pipedream components](https://github.com/PipedreamHQ/pipedream/tree/HEAD/components)

Airbyte low-code (new facts only):

- 53 of the 505 manifest-only source connectors ship a custom `components.py` next to the manifest (measured: registry language tag joined to the repository tree; 501 of the 505 were found in the tree). That resolves the gap in the earlier notes: about 10% of "manifest-only" connectors still need Python. — [Airbyte OSS registry](https://connectors.airbyte.com/files/registries/v0/oss_registry.json); [Airbyte connectors directory](https://github.com/airbytehq/airbyte/tree/HEAD/airbyte-integrations/connectors)
- Only 3 of 54 destinations are flagged `supportsDataActivation` (reverse-ETL style writes into SaaS), and all destinations are Java or Python. — [Airbyte OSS registry](https://connectors.airbyte.com/files/registries/v0/oss_registry.json)

Singer:

- "A *Tap* is an application that takes a *configuration* file and an optional *state* file as input and produces an ordered stream of *record*, *state* and *schema* messages as output." Taps "may be implemented in any programming language". The spec is version 0.3.0. — [Singer SPEC.md](https://github.com/singer-io/getting-started/blob/HEAD/docs/SPEC.md)
- The `singer-io` organisation has 207 public repositories; 158 were pushed in the 12 months to 2026-10-08 (measured). — [singer-io on GitHub](https://github.com/singer-io)

OpenAPI, Arazzo and Overlays:

- Arazzo 1.1.0 (dated 17 May 2026) defines steps that call an OpenAPI operation, an AsyncAPI channel or another workflow, with success criteria, outputs and `dependsOn`. Failure actions are `end`, `goto` or `retry`, with `retryAfter` ("seconds to delay after the step failure") and `retryLimit`. The first 100,000 characters of the specification contain no construct for pagination, loops, rate limiting or token acquisition. — [Arazzo Specification](https://spec.openapis.org/arazzo/latest.html)
- "Arazzo adoption is zero in every market measured", across fourteen API Evangelist market reports (2026-08-17). In the same reports, Standard Webhooks was named by 2 of 951 developer-tools organisations. — [API Evangelist: standards layer](https://apievangelist.com/2026/08/17/what-the-trend-reports-found-in-the-standards-layer/)
- Pagination is not part of OpenAPI itself; SDK vendors add it through extensions. `x-speakeasy-pagination` supports `offsetLimit`, `cursor` and `url`; `x-fern-pagination` supports offset, cursor, `next_uri` and `next_path` (search summary of both docs). — [Speakeasy pagination docs](https://www.speakeasy.com/docs/sdks/customize/runtime/pagination); [Fern pagination extension](https://buildwithfern.com/learn/api-definitions/openapi/extensions/pagination)

Closed engines that claim fully declarative integrations (vendor claims):

- Truto (2026-07-02): "The behavior of an integration is defined by a declarative configuration blob and a set of JSONata expressions", and "Adding support for a new CRM does not require writing new code or deploying the platform. It simply requires adding a new JSON configuration record to the database." Pagination is declared per provider. Rate-limit metadata is normalised into IETF headers but 429s are passed to the caller, not retried. Inbound webhooks are signature-verified and normalised with JSONata. The article does not address writes. — [Truto vs Nango](https://truto.one/blog/truto-vs-nango-code-first-vs-declarative-saas-integrations/)
- StackOne: "A connector is a YAML definition of how to interact with a provider's API." Its AI Builder is described as generating and validating YAML and testing actions "using the Falcon execution engine" (second quote from a search summary). — [StackOne connector engine](https://docs.stackone.com/guides/connector-engine/introduction.md); [StackOne AI Builder](https://docs.stackone.com/guides/connector-engine/ai-builder)

Superface:

- `superfaceai/one-sdk-js` was last pushed 2025-01-31 (52 stars). The organisation's most recently pushed repositories are `arcpay` projects (2026-06-15), not the Comlink profile/map tooling (measured). — [superfaceai on GitHub](https://github.com/superfaceai); [one-sdk-js](https://github.com/superfaceai/one-sdk-js)

Rust building blocks for an interpreter (measured on crates.io, 2026-10-08):

| Crate | Role | Version | Total downloads | Last updated |
| --- | --- | --- | --- | --- |
| `jsonpath-rust` | JSONPath | 1.0.11 | 97.6 million | 2026-09-09 |
| `minijinja` | Templating | 3.0.0-alpha.3 | 37.5 million | 2026-10-04 |
| `jaq-core` | jq expressions | 3.1.1 | 4.0 million | 2026-08-28 |
| `cel` | CEL expressions | 0.15.0 | 3.8 million | 2026-10-06 |
| `jmespath` | JMESPath | 0.5.0 | 3.2 million | 2026-01-19 |
| `jsonata-rs` | JSONata | 0.3.4 | 208,144 | 2025-02-03 |
| `rhai` | Embedded scripting | 1.26.1 | 12.6 million | 2026-09-10 |
| `rquickjs` | Embedded JavaScript | 0.14.0 | 5.0 million | 2026-09-18 |
| `wasmtime` | WebAssembly runtime | 50.0.0-rc.1 | 39.1 million | 2026-10-05 |
| `extism` | WebAssembly plugins | 1.30.0 | 742,551 | 2026-06-04 |

— [jsonpath-rust](https://crates.io/crates/jsonpath-rust); [minijinja](https://crates.io/crates/minijinja); [jaq-core](https://crates.io/crates/jaq-core); [cel](https://crates.io/crates/cel); [jmespath](https://crates.io/crates/jmespath); [jsonata-rs](https://crates.io/crates/jsonata-rs); [rhai](https://crates.io/crates/rhai); [rquickjs](https://crates.io/crates/rquickjs); [wasmtime](https://crates.io/crates/wasmtime); [extism](https://crates.io/crates/extism)

### Inferences
- A Rust runtime can interpret operation definitions as data. The pattern is proven in other languages for request/response operations including writes (n8n routing, Zapier request objects) and is claimed for the whole surface by Truto and StackOne. The needed parts exist as maintained Rust crates. JSONata, Truto's choice, is the weak one in Rust (`jsonata-rs`, last release February 2025); jq, CEL or JSONPath are better-maintained substitutes.
- The declarative ceiling is lower for actions than Airbyte's read-side figure suggests. n8n recommends declarative as the default and still has only about 28 declarative definitions against 308 node directories, and after joining the tree data about 10% of Airbyte's "manifest-only" connectors still carry Python. A realistic design is a data-interpreted core with an escape hatch, not data only.
- Webhooks split into two parts with different declarability. The lifecycle (subscribe, unsubscribe, renew, handshake) and signature scheme can be declared, as Zapier's schema and Activepieces' three strategies show. The payload handling is code in every open catalogue examined. No open format declares webhook signature verification across vendors.
- The escape hatch is the real design decision for "ship without recompiling". The options are an embedded expression language (jq or CEL: safe, limited), an embedded script engine (`rhai`, `rquickjs`: flexible, sandboxable) or WebAssembly components (`wasmtime`: any language, heaviest). Truto's claim of zero integration-specific code rests on JSONata being expressive enough to act as that hatch.
- Pagination and retry are the easiest behaviours to declare (Airbyte, Truto, Speakeasy and Fern each reduce pagination to three to six named styles). Rate limiting is rarely declared: only Zapier's schema has first-class throttle and lock objects.
- Reusing someone else's format is separable from reusing their catalogue. Zapier's and n8n's formats are documented well enough to borrow ideas from; the licences in question 2 decide whether their definitions can be taken.

### Gaps
- I did not read the Superface Comlink profile and map format, so how its maps describe an operation is not covered. The project appears dormant for this purpose.
- I did not confirm whether n8n declarative `preSend` and `postReceive` hooks may be arbitrary functions (which would make "declarative" nodes non-serialisable). n8n node definitions are TypeScript modules, not JSON files, either way.
- Truto's and StackOne's engines are closed. Their claims about declarative pagination, webhooks and "zero integration-specific code" could not be verified, and neither source states that writes are fully declarative.
- The last 10,000 characters of the Arazzo specification were not read; the absence of pagination and loop constructs is based on the first 100,000.
- No Rust project that interprets a cross-vendor operation catalogue at runtime was found beyond Pica (covered in the earlier notes).
- Whether Airbyte's CDK has any declarative destination support was not established; a code search for a guessed identifier returned nothing, which is weak evidence.

## 2. Licences: which existing catalogues a permissively licensed project could reuse or translate

### Takeaway
Of the large catalogues, only Activepieces' community pieces (MIT) are clearly reusable by an MIT/Apache-2.0 project, and they are TypeScript code, not data. n8n, Pipedream, Nango and, by its own metadata, 92% of Airbyte's source connectors are under source-available licences that block relicensing; Singer taps are almost all AGPL-3.0. Vendor-published OpenAPI specs are the other reusable input: 19 of 26 sampled spec repositories carry MIT, Apache-2.0 or BSD licences.

### Cited Findings
- n8n: everything outside `.ee` files is under the Sustainable Use License 1.0. "You may use or modify the software only for your own internal business purposes or for non-commercial or personal use. You may distribute the software or provide it to others only if you do so free of charge for non-commercial purposes." The grant is "non-sublicensable, non-transferable". — [n8n LICENSE.md](https://github.com/n8n-io/n8n/blob/HEAD/LICENSE.md)
- n8n community nodes are a separate case: verification requires "Make sure your package license is MIT", no external dependencies, and code that "must not interact with environment variables or attempt to read/write files". — [n8n verification guidelines](https://docs.n8n.io/connect/create-nodes/build-your-node/reference/verification-guidelines)
- 13,485 npm packages carry the `n8n-community-node-package` keyword (measured 2026-10-08; includes unverified and junk packages, each with its own licence). — [npm registry search](https://registry.npmjs.org/-/v1/search?text=keywords:n8n-community-node-package&size=1)
- Activepieces: content under `packages/ee/` and `packages/server/api/src/app/ee` is under a separate enterprise licence; "Content outside of the above mentioned directories or restrictions above is available under the "MIT Expat" license". The 736 community pieces sit in `packages/pieces/community`, outside those directories. — [Activepieces LICENSE](https://github.com/activepieces/activepieces/blob/HEAD/LICENSE)
- Pipedream: the repository is under the Pipedream Source Available License 1.0. The licensee "shall not, exercise the License for an Excluded Purpose", defined as "any commercial use of the software including, but not limited to, making available any software-as-a-service, platform-as-a-service, infrastructure-as-a-service or other online service that competes with the Software or any other Pipedream products or services". — [Pipedream LICENSE](https://github.com/PipedreamHQ/pipedream/blob/HEAD/LICENSE)
- Airbyte: the root `LICENSE` of `airbytehq/airbyte` is the Elastic License 2.0 text with no MIT carve-out (measured). In the OSS registry, 544 of 589 sources declare ELv2 (including 4 spelled "Elv2"), 39 declare MIT and 6 "Airbyte Enterprise"; 38 of 54 destinations declare ELv2 and 16 MIT. Of the 505 manifest-only sources, 473 are ELv2 and 31 MIT. `source-stripe`'s `metadata.yaml` reads `license: ELv2`. — [Airbyte LICENSE](https://github.com/airbytehq/airbyte/blob/HEAD/LICENSE); [Airbyte OSS registry](https://connectors.airbyte.com/files/registries/v0/oss_registry.json)
- Airbyte's licence FAQ says the opposite for connectors: connectors and the CDK are MIT and the platform is ELv2, and "Our own connectors remain open-source". — [Airbyte licence FAQ](https://docs.airbyte.com/community/licenses/license-faq); contradicted by [Airbyte OSS registry](https://connectors.airbyte.com/files/registries/v0/oss_registry.json)
- The Airbyte Python CDK, which contains the declarative manifest schema and interpreter, is MIT (measured from the GitHub API). — [airbytehq/airbyte-python-cdk](https://github.com/airbytehq/airbyte-python-cdk)
- ELv2's limits: "You may not provide the software to third parties as a hosted or managed service, where the service provides users with access to any substantial set of the features or functionality of the software", and the grant is "non-sublicensable". — [Airbyte LICENSE](https://github.com/airbytehq/airbyte/blob/HEAD/LICENSE)
- Nango: the main repository (including `providers.yaml`) and the separate `integration-templates` repository are both ELv2 (measured). — [Nango LICENSE](https://github.com/NangoHQ/nango/blob/HEAD/LICENSE); [Nango integration-templates LICENSE](https://github.com/NangoHQ/integration-templates/blob/HEAD/LICENSE)
- Zapier: the platform SDK repository is not open source. Its licence file says use is governed by "the Zapier Platform Agreement, which can be found at: https://zapier.com/platform/tos". Partner integration definitions are not published in it. — [zapier-platform LICENSE](https://github.com/zapier/zapier-platform/blob/HEAD/LICENSE)
- Singer: 194 of the 207 public repositories in `singer-io` are AGPL-3.0, 7 Apache-2.0 and 6 have no detected licence (measured). — [singer-io on GitHub](https://github.com/singer-io)
- Composio's SDK repository is MIT (30,469 stars, measured); its top-level directories are `python`, `ts`, `docs`, `skills`, `harness` and similar, with no visible catalogue of tool definitions. — [ComposioHQ/composio](https://github.com/ComposioHQ/composio)
- Vendor OpenAPI spec repositories (26 sampled, listed in question 3): 15 MIT, 3 Apache-2.0, 1 BSD-3-Clause, 6 with no detected licence, 1 unclassified (measured from the GitHub API licence field). — [github/rest-api-description](https://github.com/github/rest-api-description); [stripe/openapi](https://github.com/stripe/openapi)

### Inferences
- Translating n8n nodes, Pipedream components, Nango templates or ELv2-tagged Airbyte manifests into another format would be preparing a derivative work. None of those licences allows the result to be relicensed as MIT or Apache-2.0, and n8n's and Pipedream's also bar commercial distribution. Socket should treat all four as read-for-understanding only. This is a reading of licence text, not legal advice, and the facts-versus-expression question (endpoint paths and field names are facts about a vendor's API) needs a lawyer before anyone relies on it.
- Activepieces is the one large permissive catalogue: 736 pieces, about 8,500 action files and about 1,470 trigger files under MIT. It can be ported or mined with attribution. It is TypeScript code with hand-written pagination, so it is a porting source for an agent-assisted pipeline, not a dataset to load.
- Airbyte's connector licence is ambiguous in public. The FAQ says MIT; the root licence file and 92% of source metadata say ELv2. The safe reading is the per-connector metadata. The 39 MIT-tagged sources are mostly long-tail services (Workday, Plaid, PostHog, Auth0, BambooHR and similar).
- The reusable Airbyte asset is the format, not the catalogue. The MIT CDK defines the declarative schema, so a Rust project could implement a compatible reader and write its own manifests.
- First-party OpenAPI specs are the cleanest reusable input at scale: mostly MIT or Apache-2.0, vendor-maintained and pushed weekly. Six of 26 sampled repositories have no detected licence, so each needs checking before redistribution.
- Community n8n nodes that follow the verification rules are MIT, dependency-free and one-service-per-package, which makes them a second permissive porting source. Quality and safety are uneven across 13,485 packages (the earlier report records malicious ones).

### Gaps
- I did not resolve the conflict between Airbyte's FAQ and its registry metadata, nor find when connector metadata moved to ELv2.
- Whether Composio publishes its tool definitions under MIT anywhere was not established.
- I did not read the Activepieces enterprise licence or check whether any community piece imports enterprise code.
- Windmill's hub scripts, Meltano Hub definitions and Trigger.dev integrations were not examined.
- The licences of individual n8n community packages were not sampled.
- No legal analysis of whether API facts in a source-available connector (paths, parameter names, scopes) are protectable was found or attempted.

## 3. OpenAPI coverage among the most-integrated SaaS products

### Takeaway
No ranked survey of spec availability for the top 50 to 100 SaaS products was found. In a convenience sample of 45 well-known products I checked on GitHub, 28 publish a maintained first-party OpenAPI description; the rest are GraphQL-first, use another IDL, have an archived spec or could not be established. Published specs are also unreliable as ground truth: the one large measurement found 75% of production APIs deviating from their own spec.

### Cited Findings

Measured sample (repository existence and last push, GitHub API, 2026-10-08):

- Maintained first-party OpenAPI repositories, all pushed between 2026-07-16 and 2026-10-08: GitHub, Stripe, Twilio, HubSpot, Asana, Box, DigitalOcean, Intercom, PagerDuty, OpenAI, Discord, Cloudflare, Sentry, Square, Adyen, Plaid, Xero, DocuSign, Okta, Microsoft Graph, Figma, Netlify, Klaviyo, Webflow, Zoho CRM, Resend and Mailchimp (27). Jira's hosted spec, measured in the earlier notes, makes 28. — [HubSpot spec collection](https://github.com/HubSpot/HubSpot-public-api-spec-collection); [Asana/openapi](https://github.com/Asana/openapi); [intercom/Intercom-OpenAPI](https://github.com/intercom/Intercom-OpenAPI); [discord/discord-api-spec](https://github.com/discord/discord-api-spec); [figma/rest-api-spec](https://github.com/figma/rest-api-spec); [okta/okta-management-openapi-spec](https://github.com/okta/okta-management-openapi-spec); [microsoftgraph/msgraph-metadata](https://github.com/microsoftgraph/msgraph-metadata); [klaviyo/openapi](https://github.com/klaviyo/openapi)
- Archived: Slack's spec repository (last push 2021-09-07) and `zoom/api` (archived, last push 2021-06-03). — [slackapi/slack-api-specs](https://github.com/slackapi/slack-api-specs); [zoom/api](https://github.com/zoom/api)
- Other IDL: Dropbox publishes "The Official API Spec for Dropbox API V2 SDKs", which is not OpenAPI (pushed 2026-10-07). — [dropbox/dropbox-api-spec](https://github.com/dropbox/dropbox-api-spec)
- GraphQL-first: monday.com (`monday-graphql-api`, pushed 2026-10-06) and Linear (earlier notes). — [mondaycom/monday-graphql-api](https://github.com/mondaycom/monday-graphql-api)
- Not established (no first-party spec repository at the names I tried; this is not evidence of absence): Notion, Airtable, Calendly, ClickUp, Freshdesk, QuickBooks, Salesforce, Zendesk, Typeform, Pipedrive, Datadog, Shopify.

Surveys and datasets:

- API Evangelist's fourteen 2026 market trend reports (published 2026-08-17): "OpenAPI runs between 89 and 94 percent. Government is the high mark at 94 percent". Cohorts include 951 developer-tools organisations and 1,062 AI organisations. MCP "ranges from 3 to 36 percent", AsyncAPI is "adopted nowhere near OpenAPI levels", and GraphQL is cited at 13.2% in developer tools. — [API Evangelist: standards layer](https://apievangelist.com/2026/08/17/what-the-trend-reports-found-in-the-standards-layer/)
- A separate API Evangelist paper is summarised as finding 14,257 OpenAPI documents from 6,750 organisations, equal to 26.2% of providers tracked in its catalogue (search summary; page not fetched). — [API Evangelist: the OpenAPI standard](https://papers.apievangelist.com/papers/the-openapi-standard/); in tension with [API Evangelist: standards layer](https://apievangelist.com/2026/08/17/what-the-trend-reports-found-in-the-standards-layer/)
- APIContext's white paper "OpenAPI Specifications in the Real World" (reported 2024-09-16) analysed "650 million API calls to more than 10,000 different API endpoints": "75% of production APIs tested had variances to their published OpenAPI Specifications", "Only 57% of APIs have public API specifications", and "Over half of API specifications hadn't been updated in over six months". Version mix: 30% OAS 2.0, 54% OAS 3.0, 7% OAS 3.1.0. APIContext sells API monitoring. — [Nordic APIs: most APIs suffer from specification drift](https://nordicapis.com/most-apis-suffer-from-specification-drift/)
- Postman's 2025 State of the API report (published 2025-10-08, more than 5,700 respondents): 83.2% report some level of API-first approach, 25% are fully API-first, 89% use generative AI tools and 24.3% design APIs with AI agents in mind (search summary). The summary contained no figure for OpenAPI publication. — [Business Wire: Postman 2025 State of the API](https://secure.businesswire.com/news/home/20251008162423/en/One-in-Four-Developers-Now-Design-APIs-for-AI-Agents-According-to-Postmans-2025-State-of-the-API-Report)

### Inferences
- For popular developer-facing SaaS, a maintained first-party OpenAPI spec is now the common case: 28 of 45 in my sample (62%), and actively pushed. That is more favourable than the earlier notes' picture, which rested on six vendors. It makes spec-driven generation of the request layer a credible route to a few hundred services.
- The sample is biased upward. I checked vendors I expected to publish specs, and the "not established" group contains many of the most-integrated products (Salesforce, Notion, Airtable, Zendesk, Shopify, Slack). The head of the demand curve is where specs are missing, archived or not REST.
- The 89 to 94% and 26.2% figures cannot both describe first-party publication. The likeliest reading is that the higher number counts providers for which the catalogue holds an OpenAPI document from any origin, so it should not be quoted as "share of vendors that publish a spec".
- Spec drift is large enough to change the design. If three in four APIs deviate from their published spec, generated operations need live verification and a per-vendor patch layer (Overlays are the standard tool), and "generate from spec" cannot be the whole quality story.
- Specs give request and response shapes, not behaviour. Pagination style, rate limits and webhook verification are outside OpenAPI, so even a perfect spec leaves the same hand-declared layer described in question 1.

### Gaps
- No 2025 or 2026 report from Speakeasy, Stainless, Fern or APIMatic quantifying spec availability or quality across SaaS vendors was found.
- I found no published ranking of "most-integrated SaaS products" to sample against, so the 28-of-45 figure is not a share of any defined top-N list.
- Spec quality (validity, completeness against the live API, presence of webhook schemas) was not measured for the sampled repositories.
- The APIContext figures are from 2024 and reached me through a secondary article; the white paper itself was not read.
- GraphQL schema publication was not measured beyond monday.com and Linear.
- The Postman report's own figures on OpenAPI use were not retrieved.

## 4. AI-generated connectors as of 2026: what vendors report and what can be measured

### Takeaway
Vendors report speed and volume but no acceptance or failure rates. The best independent evidence is Airbyte's public repository: an AI agent (Devin) authored 1,192 of 12,263 pull requests merged in 2026 (9.7%), with 1,461 merged against 1,116 closed unmerged overall, and its merged work is mostly documentation, fixes and added streams inside a catalogue that already has CI and credentials. The only academic benchmark found shows models hallucinating endpoints when working from memory.

### Cited Findings

Airbyte:

- Airbyte announced an AI Assistant for the Connector Builder on 2024-09-24 that creates "data connectors from an API documentation link in seconds", alongside a Marketplace with "more than 300" connectors (search summary; the release returned HTTP 403 when fetched). — [Business Wire 2024-09-24](https://www.businesswire.com/news/home/20240924140740/en)
- In September 2023 Airbyte said its "community has built more than 1,500 connectors for their own needs in just three months" with the no-code builder, and that 40 connectors had been migrated to low-code with over 100 more planned. These were private connectors, not catalogue additions. — [MarTech Series 2023-09-20](https://martechseries.com/technology/airbyte-users-create-more-than-1500-data-integration-connectors-with-no-code-builder/)
- In August 2024 Airbyte engineers described a catalogue of "over 350 open-source connectors" and a goal of more than 1,000 by the end of that year (the goal is the fetch tool's paraphrase), adding "We can't realistically maintain this sheer number of connectors in house". — [Airbyte blog 2024-08-14](https://airbyte.com/blog/how-we-test-airbyte-and-marketplace-connectors)
- Measured outcome: the OSS registry holds 643 connectors on 2026-10-08 (589 sources, 54 destinations). Of the 401 entries with a `releaseDate`, 222 are dated 2024, 72 are dated 2025 and 20 are dated 2026. — [Airbyte OSS registry](https://connectors.airbyte.com/files/registries/v0/oss_registry.json)
- Measured agent activity in `airbytehq/airbyte` (GitHub search API, 2026-10-08): pull requests authored by `devin-ai-integration`: 1,461 merged and 1,116 closed without merging, all time. By year merged: 0 of 10,831 in 2024, 269 of 17,121 in 2025 (1.6%), 1,192 of 12,263 in 2026 to date (9.7%). — [Airbyte PRs by Devin, merged](https://github.com/airbytehq/airbyte/pulls?q=is%3Apr+is%3Amerged+author%3Aapp%2Fdevin-ai-integration); [closed unmerged](https://github.com/airbytehq/airbyte/pulls?q=is%3Apr+is%3Aclosed+is%3Aunmerged+author%3Aapp%2Fdevin-ai-integration)
- The 25 most recent merged Devin pull requests (2026-10-02 to 2026-10-07) are 18 documentation changes, 4 bug fixes (one in a destination), 1 feature (incremental sync for three GitLab streams), 1 test change and 1 promotion of a connector to certified. Recent unmerged ones include repeated attempts at the same `source-faker` change and titles marked "[EVAL - DO NOT MERGE]". — [Airbyte PRs by Devin, merged](https://github.com/airbytehq/airbyte/pulls?q=is%3Apr+is%3Amerged+author%3Aapp%2Fdevin-ai-integration)
- A non-AI bot does most of the routine upkeep: `octavia-bot-hoard` authored 9,304 of the 12,263 pull requests merged in 2026 (75.9%), with titles of the form "deps(source-x): update dependencies". In the registry, the latest release of 425 connectors is attributed to a bot and 43 to a maintainer (175 have no attribution), and 467 of the 468 connectors with release data were re-released in September or October 2026. — [Airbyte PRs by octavia-bot-hoard](https://github.com/airbytehq/airbyte/pulls?q=is%3Apr+is%3Amerged+author%3Aapp%2Foctavia-bot-hoard); [Airbyte OSS registry](https://connectors.airbyte.com/files/registries/v0/oss_registry.json)
- Airbyte job postings describe "an AI-powered Integration Factory that automates the lifecycle of their connectors: generating code, managing dependency updates, diagnosing production issues, proposing fixes" (search summary). — [Airbyte job posting](https://jobs.thrivecap.com/companies/airbyte/jobs/81564533-senior-integrations-engineer-api-sources-automation)
- Airbyte's separate agent-facing SDK has 51 connector directories (measured); its PyPI page claims "50+ third-party APIs". — [airbytehq/airbyte-agent-sdk](https://github.com/airbytehq/airbyte-agent-sdk); [PyPI: airbyte-agent-sdk](https://pypi.org/project/airbyte-agent-sdk/0.1.285/)

Nango:

- Nango (2026-04-17) reports that its agent pipeline "generated approximately 200 integrations in 15 minutes for under $20 in token costs", across five APIs (Google Calendar, Drive, Sheets, HubSpot, Slack). Here "integration" means an individual action or sync function. No success or failure rate is given. — [Nango blog: using AI coding agents](https://nango.dev/blog/using-ai-coding-agents-for-building-api-integrations)
- Its stated test method is live execution: the agent uses `nango dryrun` to "execute its code against a real connection and iterate until tests pass", because "An agent that generates code without testing it against the real API will produce code that looks correct but fails in production." — [Nango blog: using AI coding agents](https://nango.dev/blog/using-ai-coding-agents-for-building-api-integrations)
- Measured size of Nango's template catalogue: 259 integration directories, 6,465 action files, 1,280 sync files and 7,053 test files (181 directories have tests). The repository root contains agent configuration (`.agents`, `.claude`, `.aider-desk`, `skills-lock.json`). — [NangoHQ/integration-templates](https://github.com/NangoHQ/integration-templates)
- A report checked into that repository (verification date 2026-08-24) documents one bulk change across 20 integrations: 66 changed sync files, 66 successful real `dryrun --validate` calls and 66 `dryrun --save` calls "against working provider connections", 55 generated test files changed, 4 snapshot files "manually sanitized after generation" and 1,371 tests passed. It notes that two integrations were dropped from the branch because newer implementations landed first. — [Nango checkpoint backfill report](https://github.com/NangoHQ/integration-templates/blob/HEAD/CHECKPOINT_BACKFILL_REPORT.md)
- Devin authored 24 merged and 3 unmerged pull requests in `NangoHQ/nango` (measured). — [Nango PRs by Devin](https://github.com/NangoHQ/nango/pulls?q=is%3Apr+author%3Aapp%2Fdevin-ai-integration)

Others:

- StackOne's AI Builder "leverages 20+ specialised MCP tools to research APIs, generate and validate YAML, and exhaustively test actions", follows an 8-step workflow, and "is in early access" (search summary of the docs). Its CLI runs an action locally with a credentials file. No volumes or rates are published. — [StackOne AI Builder](https://docs.stackone.com/guides/connector-engine/ai-builder)
- Membrane (2025-11-21, vendor) lists three failure modes for coding agents: "AI can build simple integrations that work in perfect scenarios, but it can't reliably handle the complexity needed for production use"; agents lack "real-world experience with how integrations actually behave in production"; and "AI doesn't have robust tools to test integrations properly." No measurements are given. — [Membrane article](https://getmembrane.com/articles/all/why-ai-coding-agents-fail-at-building-integrations)
- Agent-authored pull requests are rare in the other open catalogues (measured, all time): n8n has 3 merged and 7 unmerged from GitHub Copilot's agent and 0 merged and 4 unmerged from Cursor's; Activepieces has 0 merged and 1 unmerged from Copilot's agent; Pipedream has none from Devin. — [n8n PRs by Copilot agent](https://github.com/n8n-io/n8n/pulls?q=is%3Apr+author%3Aapp%2Fcopilot-swe-agent); [Activepieces PRs by Copilot agent](https://github.com/activepieces/activepieces/pulls?q=is%3Apr+author%3Aapp%2Fcopilot-swe-agent)

Independent benchmark:

- WAPIIBench (Maninger et al., arXiv 2509.20172, submitted 2025-09-24, version 7 dated 2026-07-03, AIware 2025) tests generation of web API invocation code on 395 endpoints across Asana (167), Slack Web (174), Google Calendar (37) and Google Sheets (17); 182 are POST and 173 GET. "None of the evaluated open-source models was able to solve more than 40% of the tasks." — [arXiv 2509.20172](https://arxiv.org/abs/2509.20172)
- In the paper's body, GPT-4o reaches 60% correct in full completion and 77% in argument completion; the best open-source model (Code Llama 70B) reaches 30% and 40%. Models "often hallucinate endpoint URLs (up to 39%) and parameter names (up to 31%)", and up to 13% of calls use an illegal HTTP method. No retrieval or agentic setup was evaluated: models worked without the API specification. — [arXiv 2509.20172 (HTML)](https://arxiv.org/html/2509.20172v7)

### Inferences
- Agents are demonstrably useful for upkeep inside an existing catalogue. At Airbyte, agent-authored pull requests went from none in 2024 to about one in ten merged in 2026, and deterministic bots do three quarters of the rest. The human share of merged pull requests is therefore about 14% (12,263 minus 9,304 minus 1,192 leaves 1,767). That is the strongest quantitative support for the user's larger ambition.
- The same data argue against "agents will grow the catalogue for free". Airbyte's catalogue additions fell from 222 dated 2024 to 20 dated 2026 while agent activity rose, and it sits at 643 two years after a 1,000 target. Agent effort went into documentation and fixes for existing connectors, not into breadth.
- A merge ratio of 1,461 to 1,116 is not a clean acceptance rate. The unmerged set visibly includes evaluation runs and duplicate attempts, and merged pull requests passed human review and CI. A fair summary is "somewhat over half of opened agent pull requests merge, with a human in the loop for every one".
- Every credible pipeline verifies against a live API with real credentials (Nango's `dryrun`, StackOne's local action runs, Airbyte's CI). The scarce input for agent-generated connectors is a working authenticated connection per provider, not model capability. Nango's report covers only integrations with "working connections", which is the same constraint showing up in practice.
- WAPIIBench measures recall without documentation on 2024-era models, so it is a lower bound that says little about a 2026 agent reading the spec and iterating. It does establish that unverified generation from memory fails often on exactly the APIs a library would target (Slack, Asana, Google), so verification cannot be skipped.
- Nango's 200 functions for under $20 puts generation cost at roughly ten cents per operation. If that holds, generation is not the cost that matters; verification, credentials and review are.

### Gaps
- No vendor publishes an acceptance rate, defect rate or human-review time for AI-generated connectors. Airbyte's pull-request counts are my measurement, not a statement by Airbyte, and mix connector work with documentation and release notes.
- I found nothing from Composio, Zapier, Pipedream, n8n or Activepieces on generating connectors with LLMs (volumes, tests or review). Their catalogue counts are in the earlier notes.
- I found no benchmark of coding agents writing API integrations with documentation access and live testing, which is the setting that matters in 2026.
- The original Airbyte AI Assist press release could not be fetched, and no usage or success figures for it were found.
- I did not sample enough Devin pull requests to estimate what share adds connector functionality as opposed to documentation.
- Agent contributions made through human accounts (a developer using a coding agent locally) are invisible to this measurement, so agent involvement is understated everywhere.

## 5. Maintenance economics with numbers

### Takeaway
Upkeep is concentrated and largely mechanical at the tail, but real at the head: 68% of Airbyte's certified connectors have shipped at least one breaking release against 10% of community ones, the most-broken are the big ad, commerce and CRM APIs, and more than half of rated community sources are in the lowest sync-success band. Usage is long-tailed (23% of Airbyte sources are high-usage, and every certified source is among them). Per-connector cost figures exist only as vendor claims of roughly 300 to 430 hours a year.

### Cited Findings

Breakage and upstream change:

- Measured from the Airbyte registry's `releases.breakingChanges` field: 109 of 643 connectors have at least one breaking-change entry, 208 entries in total. Among the 81 certified connectors, 55 (68%) have at least one and they account for 134 entries; among the 562 community connectors, 54 (10%) have at least one, with 74 entries. By upgrade-deadline year: 46 in 2023, 76 in 2024, 36 in 2025, 50 in 2026. — [Airbyte OSS registry](https://connectors.airbyte.com/files/registries/v0/oss_registry.json)
- Connectors with the most breaking releases: Shopify 7, Amazon Ads 7, LinkedIn Ads 6, Jira 6, Google Ads 6, Facebook Marketing 6, Zendesk Support 5, HubSpot 5, Amazon Seller Partner 5, Stripe 4, Slack 3, Notion 3, Klaviyo 3. — [Airbyte OSS registry](https://connectors.airbyte.com/files/registries/v0/oss_registry.json)
- Vendor version policies (search summaries of the primary pages): Shopify releases API versions quarterly and supports each stable version for at least 12 months; Stripe ships twice-yearly major releases that include breaking changes plus monthly non-breaking releases; Salesforce supports each API version for at least three years and gives at least one year's notice of retirement. — [Shopify API versioning](https://shopify.dev/docs/api/usage/versioning); [Stripe: new API release process](https://stripe.com/blog/introducing-stripes-new-api-release-process); [Salesforce API end-of-life policy](https://developer.salesforce.com/docs/platform/connect-rest-api/guide/intro_api_eol.html)
- Di Lauro, Pautasso and Serbout (ECSA 2022) studied "1,192,664 operations and their histories distributed across 407,028 commits contained in 149,704 unique APIs" and found "only 5.2% of the explicit-deprecated operations and 8.0% of the deprecated-in-description operations end with a removal"; when removal happens it is typically within two years. — [ECSA 2022 paper page](https://conf.researchr.org/details/ecsa-2022/ecsa-2022-research-papers/9/To-deprecate-or-to-simply-drop-operations-An-empirical-study-on-the-evolution-of-a-l)
- A search summary of related work from the same group states that of 41,627 APIs, 263 (0.6%) deprecated operations before removing them while 10,242 (24.6%) removed operations without notice (not confirmed against the paper). — [USI ECSA 2022 PDF](https://oas-search.inf.usi.ch/pdf/ecsa-2022.pdf)

Quality across a large catalogue:

- Airbyte's registry rates each connector's sync success as low, medium or high. Sources overall: 117 high, 141 medium, 225 low, 106 unrated. Certified sources: 66 high, 1 low. Community sources: 51 high, 141 medium, 224 low, 106 unrated, so 54% of the 416 rated community sources are "low". Manifest-only sources: 93 high, 118 medium, 195 low, 99 unrated. — [Airbyte OSS registry](https://connectors.airbyte.com/files/registries/v0/oss_registry.json)
- Airbyte claimed in September 2023 that "Our average overall sync success rate across all connectors is now over 99%", a usage-weighted figure. — [MarTech Series 2023-09-20](https://martechseries.com/technology/airbyte-users-create-more-than-1500-data-integration-connectors-with-no-code-builder/)
- Release stage in the same registry: 507 of 589 sources are `alpha`, 23 `beta` and 59 `generally_available`. — [Airbyte OSS registry](https://connectors.airbyte.com/files/registries/v0/oss_registry.json)

Certified versus community:

- Measured 2026-10-08: 81 certified connectors (67 sources, 14 destinations) and 562 community (522 sources, 40 destinations), so 12.6% certified. — [Airbyte OSS registry](https://connectors.airbyte.com/files/registries/v0/oss_registry.json)
- Activepieces states "60% of the pieces are contributed by the community" (claim, README). — [Activepieces README](https://github.com/activepieces/activepieces/blob/HEAD/README.md)
- Zapier says its first version "integrated with just 34 apps" and that "we've scaled our partner ecosystem to 5,000 apps" about a decade later (undated post); it now markets 8,000+ apps, and a search summary says most are built and maintained by the vendors themselves. — [Zapier blog: 5,000 partner apps](https://zapier.com/blog/5000-partner-apps/)

Usage distribution:

- Airbyte's registry rates usage as low, medium or high. Sources: 133 high (23%), 79 medium, 271 low, 106 unrated. All 67 certified sources are "high"; of 522 community sources, 66 are high, 79 medium, 271 low and 106 unrated. By implementation, 84 of 505 manifest-only sources are high-usage against 30 of 60 Python and 19 of 24 Java. — [Airbyte OSS registry](https://connectors.airbyte.com/files/registries/v0/oss_registry.json)
- Activepieces pieces on npm (measured: `@activepieces/piece-*` monthly downloads for 735 of the 736 community pieces, period 2026-09-05 to 2026-10-04, 4.83 million downloads in total). The top 10 packages take 22.1% of downloads, the top 20 take 30.1%, the top 50 take 40.1% and the top 100 take 49.1%. Slack leads with 429,845, then Google Sheets (119,831), Google Drive (84,738), Gmail (70,190), Microsoft Outlook (69,748), Salesforce (59,284), Notion (49,317), OpenAI (44,543), HubSpot (37,971) and Linear (34,644), with utility pieces in between. — [npm downloads API: piece-slack](https://api.npmjs.org/downloads/point/last-month/@activepieces/piece-slack); [Activepieces community pieces](https://github.com/activepieces/activepieces/tree/HEAD/packages/pieces/community)
- The same data have a floor: the median piece gets 4,160 downloads a month, the interquartile range is 3,604 to 5,177, and only 4 pieces fall below 1,000. Only 78 pieces exceed twice the median and 59 exceed 10,000. Slack is 103 times the median. — [npm downloads API: piece-slack](https://api.npmjs.org/downloads/point/last-month/@activepieces/piece-slack)
- A third-party dashboard built on the n8n templates API (7.8K public templates, as of 2026-02-23) ranks generic nodes first: Sticky Note, HTTP Request, Edit Fields, Code, OpenAI Chat Model, AI Agent, Google Sheets, If, Telegram, Wait. HTTP Request appears in 3.8K of the 7.8K templates; the first SaaS integrations are Google Sheets (2.3K templates) and Telegram (984). — [n8n Pulse node statistics](https://n8n-stats.gui.do/nodes)

Catalogue depth behind headline counts:

- Pipedream: 1,266 of 3,401 app directories (37%) have no action and no source (measured). — [Pipedream components](https://github.com/PipedreamHQ/pipedream/tree/HEAD/components)
- Activepieces: 213 of 736 pieces have three or fewer action files and 342 have no trigger (measured). — [Activepieces community pieces](https://github.com/activepieces/activepieces/tree/HEAD/packages/pieces/community)

Cost per connector (all vendor claims; no independent figure found):

- Truto: a production integration "typically takes 150+ engineering hours to build and 300+ hours per year to maintain", about $50,000 per integration per year including support; it also cites a 2023 Workato survey finding an average of 430 hours per year maintaining each custom integration (search summary). — [Truto: true cost of maintaining 20 integrations](https://truto.one/blog/the-true-cost-of-maintaining-20-saas-integrations-a-worked-tco-example/)
- Apideck's calculator assumes annual upkeep of 25% of build cost and labels its figures an "Illustrative estimate". — [Apideck: build vs buy in the AI era](https://www.apideck.com/why-not-vibecode-integrations)
- Airbyte's engineers on scale (2024-08-14): "We can't realistically maintain this sheer number of connectors in house, this is why we originally decided to go open source." — [Airbyte blog 2024-08-14](https://airbyte.com/blog/how-we-test-airbyte-and-marketplace-connectors)

### Inferences
- Breaking upkeep follows usage and API complexity, not catalogue size. Certified connectors average about 1.7 recorded breaking releases each over roughly four years (134 over 81), or around 0.4 per connector per year; community connectors average 0.13 in total. A long tail of small REST connectors costs little in breaking changes, partly because nobody notices when they break.
- The low community figure is under-detection as much as stability. More than half of rated community sources sit in the lowest success band, and 106 have too little use to be rated at all. A catalogue of thousands will be mostly in this state unless every entry has a live test.
- Headline counts overstate usable breadth by roughly a third to a half wherever I could count: 37% of Pipedream's apps have no operations, 29% of Activepieces pieces have three or fewer actions, and 86% of Airbyte sources are alpha.
- The Activepieces npm floor of about 4,000 downloads a month per piece, almost uniform across 600-odd packages, is far more likely automated traffic (mirrors, CI, the platform's own sync) than human demand. Subtracting the median as a baseline, the top 10 pieces hold 47% of the excess downloads, the top 20 hold 63%, the top 50 hold 79% and the top 100 hold 90%. This adjustment is mine and rough; the raw shares above are the measured figures.
- Demand is concentrated enough that depth on the head matters more than tail breadth. Airbyte's high-usage set is 133 of 589 sources, made up of all 67 certified sources plus 66 community ones. On the adjusted Activepieces figures, about 50 of 736 pieces account for roughly four fifths of real demand. In n8n templates the generic HTTP Request node outranks every SaaS integration, which shows users reach for a generic authenticated request when a specific operation is missing.
- That points to a two-speed design for Socket: an authenticated generic request primitive available for every provider in the auth registry (cheap, covers the tail the way n8n's HTTP Request node does) and curated, tested operations for the head.
- The 300 to 430 hours per connector per year claims describe in-house enterprise integrations with customer support attached and come from sellers. Airbyte's measured activity suggests routine upkeep of a mature catalogue is mostly automatable (about 86% of merged pull requests in 2026 came from a bot or an agent), so those figures are an upper bound for a library, not a planning number.
- The certified share is the honest size of a catalogue. For Airbyte that is 81 after six years, which is in line with the earlier report's "dozens of deep integrations" conclusion, even though the surrounding community tier is seven times larger.

### Gaps
- Airbyte does not document the thresholds behind its low, medium and high usage and success bands that I could find, so the bands cannot be converted to percentages or shares of sync volume.
- Breaking-change entries record connector releases, which also break for internal reasons (schema clean-ups, low-code migrations). I could not separate upstream-caused breaks from self-inflicted ones.
- How Airbyte's certified-to-community ratio moved over time was not established; I have only the 2026-10-08 snapshot and the earlier notes' figure.
- No independent measurement of hours or cost per connector per year was found. The Workato survey was not located at source.
- No vendor publishes the share of usage held by its top N integrations. Zapier's, Pipedream's and Composio's distributions are unknown.
- npm downloads are a weak proxy for Activepieces usage: they count installs by self-hosted instances and automation, not flow executions, and cloud usage is invisible. The n8n dashboard gave no totals, so top-N shares could not be computed from it.
- The partner-built share of Zapier's catalogue rests on a search summary.
- I found no dataset of how often the top SaaS APIs ship breaking changes per year beyond the three stated version policies.

## 6. The counter-argument: do coding agents and MCP make a shared integration library unnecessary?

### Takeaway
The evidence supports half of the claim. Agents have made writing a first version of an integration nearly free, and every vendor now concedes this. Nothing found shows agent-written bespoke integrations holding up in production without shared infrastructure for auth, live verification and drift detection, and the vendors' own agent pipelines all run on top of such a layer. No independent practitioner report with measurements was found on either side.

### Cited Findings

Evidence for the claim:

- Nango reports about 200 action and sync functions generated "in 15 minutes for under $20 in token costs" (2026-04-17). — [Nango blog: using AI coding agents](https://nango.dev/blog/using-ai-coding-agents-for-building-api-integrations)
- Apideck, a unified-API vendor, concedes "AI can vibecode a working integration in an afternoon". — [Apideck: build vs buy in the AI era](https://www.apideck.com/why-not-vibecode-integrations)
- Nango describes coding agents as able to "autonomously build integrations by reading API docs, writing integration code, and iterating on failing requests until tests pass" (search summary). — [Nango blog: best API integration platforms for coding agents](https://www.nango.dev/blog/best-api-integration-platforms-claude-code-cursor-codex)
- Agents already carry a measurable share of upkeep in one large catalogue: 9.7% of Airbyte's merged pull requests in 2026 (measured; see question 4). — [Airbyte PRs by Devin, merged](https://github.com/airbytehq/airbyte/pulls?q=is%3Apr+is%3Amerged+author%3Aapp%2Fdevin-ai-integration)

Evidence against the claim (vendor sources unless marked):

- Nango: "An agent that generates code without testing it against the real API will produce code that looks correct but fails in production", and "OAuth alone has dozens of provider-specific quirks, token refresh race conditions, and undocumented behaviors". It also notes prompting is hard "if you're not an integrations expert, and requirements change from API to API" (last quote from a search summary). — [Nango blog: using AI coding agents](https://nango.dev/blog/using-ai-coding-agents-for-building-api-integrations)
- Apideck: "The cost of an integration is owning it". — [Apideck: build vs buy in the AI era](https://www.apideck.com/why-not-vibecode-integrations)
- Membrane: agent-built integrations "work in perfect scenarios" but agents lack production knowledge and "robust tools to test integrations properly" (2025-11-21). — [Membrane article](https://getmembrane.com/articles/all/why-ai-coding-agents-fail-at-building-integrations)
- Independent, academic: without the spec in context, GPT-4o produced correct API calls 60% of the time on Slack, Asana and Google endpoints, and models hallucinated up to 39% of endpoint URLs. — [arXiv 2509.20172 (HTML)](https://arxiv.org/html/2509.20172v7)
- Independent of integration vendors: 75% of production APIs deviated from their published OpenAPI spec in APIContext's 2024 measurement, so an agent reading the spec is reading something that is often wrong. — [Nordic APIs: specification drift](https://nordicapis.com/most-apis-suffer-from-specification-drift/)
- Standards meant to make APIs self-describing to agents are barely adopted: Arazzo adoption is "zero in every market measured" and MCP ranges "from 3 to 36 percent" by market (2026-08-17). — [API Evangelist: standards layer](https://apievangelist.com/2026/08/17/what-the-trend-reports-found-in-the-standards-layer/)

### Inferences
- The claim confuses authoring with owning. Authoring cost has collapsed (cents per operation by Nango's figure). The remaining costs are the ones a shared library amortises: an OAuth and token-refresh implementation that has met each provider's quirks, a credential store, a harness that runs operations against live accounts, and a signal when upstream changes.
- Agents raise the value of a shared core while lowering the value of a shared catalogue of hand-written operations. An agent given a provider registry, an authenticated HTTP client with pagination and retry built in, and a `dryrun`-style verifier can produce a working operation quickly. Without those it reproduces the WAPIIBench failure modes. Nango, StackOne and Airbyte have all built exactly this: an agent-facing toolchain on top of their runtime.
- For Socket this reframes the ambition in a way the data support. "Thousands of integrations with all their features, hand-maintained" is not supported by any catalogue measured. "A core plus a format and verifier that lets a user's agent add or repair an operation in minutes, with a tested head catalogue" is consistent with what the three vendors above are doing and with Airbyte's pull-request data.
- MCP does not change this for backend use; the earlier notes cover its gaps (events, sync, credential custody). The new point is adoption: by API Evangelist's count MCP servers exist for a minority of providers in every market measured.
- Drift is the unanswered problem for bespoke, agent-written integrations. A team that generates an integration on demand has no fleet-wide signal when the vendor changes behaviour; a shared library with live conformance tests does. No source measured how often agent-written integrations break in production, so this remains an argument from structure.

### Gaps
- I found no independent practitioner account (engineering blog, conference talk or forum thread) with production data on agent-written integrations: failure rates, time to first incident or maintenance effort. Two targeted searches returned only vendor content. This is the largest hole in the evidence.
- Every qualitative source arguing against the claim sells integration infrastructure.
- I found no case study of a team replacing an integration vendor or library with agent-generated code, in either direction.
- The API Evangelist post "Build or buy in the age of vibe coding" (2026-04-29, by a SaaS vendor) could not be read beyond its summary.
- No benchmark compares agents working with a shared integration core against agents working from raw documentation.

## 7. Testing at scale without live credentials

### Takeaway
Nobody tests a large catalogue without live credentials for the part they vouch for; they shrink the vouched-for part. Airbyte says outright that seeded sandboxes for every connector do not scale and relaxes test requirements by usage, Nango regenerates recorded fixtures from real connections, and unit-test coverage in the open catalogues ranges from 12% to 70% of integrations. No source gives the cost of sandbox accounts or test infrastructure.

### Cited Findings
- Airbyte's test types: QA checks ("Static asset checks that validate that a connector is correctly packaged"), unit tests (no source access needed), integration tests (may need access) and Connector Acceptance Tests, for which "Credentials to a source/destination sandbox account are required", supplied as a `config.json` in a `.secrets` folder. Regression tests are listed as deprecated. — [Airbyte docs: testing connectors](https://docs.airbyte.com/platform/connector-development/testing-connectors)
- For community pull requests: "if Airbyte's CI can successfully fetch connector secrets and finds no sandbox or test credentials, integration tests and non-Java container tests are allowed to be non-blocking. Unit tests remain blocking." — [Airbyte docs: testing connectors](https://docs.airbyte.com/platform/connector-development/testing-connectors)
- Airbyte engineers (2024-08-14): "It does not scale for Airbyte to maintain a pool of sandboxes seeded with test data for all our connectors." The "acceptance test suite is not required to be configured in CI for marketplace connectors with low usage", and the stated policy is to "grow our test requirements based on the level of usage of a connector". Regression tests compare a release candidate against the current version, and the team was "working toward an automated gradual rollout strategy" with rollback. — [Airbyte blog 2024-08-14](https://airbyte.com/blog/how-we-test-airbyte-and-marketplace-connectors)
- Measured in Airbyte's repository: 591 of 694 connector directories have an `acceptance-test-config.yml`, and 138 have unit-test files. Among the 505 manifest-only sources, 496 have an acceptance-test config and 80 (16%) have unit tests. — [Airbyte connectors directory](https://github.com/airbytehq/airbyte/tree/HEAD/airbyte-integrations/connectors)
- n8n: 179 of 308 top-level node directories (58%) contain tests, 1,195 test files in total (measured). — [n8n nodes-base](https://github.com/n8n-io/n8n/tree/HEAD/packages/nodes-base/nodes)
- Activepieces: 88 of 736 community pieces (12%) contain any test file (measured). — [Activepieces community pieces](https://github.com/activepieces/activepieces/tree/HEAD/packages/pieces/community)
- Nango: 7,053 test files across 181 of 259 integration directories (70%) (measured). Tests are generated from snapshots recorded by `dryrun --save` against real provider connections; one documented run needed 20 working connections for 20 integrations, and 4 snapshot files were "manually sanitized after generation". — [NangoHQ/integration-templates](https://github.com/NangoHQ/integration-templates); [Nango checkpoint backfill report](https://github.com/NangoHQ/integration-templates/blob/HEAD/CHECKPOINT_BACKFILL_REPORT.md)
- StackOne's CLI validates connector YAML statically and runs an action "with local credentials" from a JSON file, or against a linked account's stored credentials after upload. — [StackOne AI Builder](https://docs.stackone.com/guides/connector-engine/ai-builder)
- Zapier describes "automation checks" and a launch checklist for partner integrations, without detail on criteria. — [Zapier blog: 5,000 partner apps](https://zapier.com/blog/5000-partner-apps/)
- Contract testing against the spec is weakened by drift: 75% of production APIs tested deviated from their published OpenAPI description (2024). — [Nordic APIs: specification drift](https://nordicapis.com/most-apis-suffer-from-specification-drift/)

### Inferences
- There are four layers in use, in rising cost: static validation of the definition (every catalogue), replay of recorded fixtures (Nango, n8n), live sandbox runs (Airbyte certified, Nango at generation time) and production telemetry (Airbyte's success-rate bands). Only the last two detect upstream drift.
- Recorded fixtures are the workable default for a credential-free CI in an open-source library: record once against a real account, sanitise, replay on every pull request. Nango shows this scaling to about 7,000 tests. The weakness is that fixtures prove the code matches the API as it was on recording day, and Nango's four manually sanitised files show that recorded payloads leak data unless scrubbed.
- A library has no production telemetry, which is the layer Airbyte leans on for its long tail. Socket would need a substitute: scheduled live runs for the integrations where someone sponsors credentials (the OpenDAL pattern in the earlier report) and an explicit "fixtures only, last recorded on date X" label for the rest.
- Declarative definitions are cheaper to test, because one interpreter test suite covers pagination, retry and auth for every definition. Airbyte's manifest-only connectors mostly have no unit tests (16%) and rely on the shared acceptance suite. This is an argument for keeping as much behaviour as possible in the interpreted core.
- Tiering by evidence is what every catalogue converges on. The measurable tier boundary is the kind of test an integration has, which supports the earlier report's recommendation to make "has live conformance tests" the top-tier criterion.
- Contract tests against vendor OpenAPI specs are useful for detecting that a spec changed, and weak for proving correctness, given the measured drift.

### Gaps
- No source gives the cost of maintaining sandbox accounts, paid test tenants or CI for a large catalogue, in money or hours.
- I did not find how Airbyte, Nango or Pipedream obtain credentials for paid-only APIs, or whether vendors sponsor test accounts for them.
- How many Airbyte connectors actually run acceptance tests against live sandboxes in CI, as opposed to merely having a config file, was not measured.
- Pipedream's and Zapier's internal test practices for catalogue components were not found.
- I did not examine how n8n's node tests are built (recorded HTTP mocks or hand-written), nor the mock format in Nango's repository beyond counting test files.
- No evaluation of vendor-provided mock servers or sandbox environments (for example Stripe test mode) across the top SaaS products was found.
