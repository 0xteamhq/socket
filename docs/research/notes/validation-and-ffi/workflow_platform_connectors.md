# Workflow-automation platforms as consumers of a shared connector library (state as of 2026-10-08)

Notes on method and reliability, for the report writer:

- "GitHub API, 2026-10-08" means I queried the repository directly that day (directory counts, package.json contents, label and issue counts, file contents). These are primary and exact for that day.
- "npm registry, 2026-10-08" means I read the package manifest from `registry.npmjs.org` and the download count from `api.npmjs.org` for the week 2026-09-28 to 2026-10-04.
- GitHub code-search and issue-search totals are approximate. Label counts from the issue search include pull requests unless I say "issues only".
- "Search summary" means I saw the claim only in a search-engine summary of the linked page and could not open the page itself. Treat those as unconfirmed.
- Facts already established in `non_rust_landscape.md` and the 2026-10-08 landscape report (licences of n8n, Pipedream, Nango, Airbyte; Corsair; the Composio breach; hosted agent-tool vendors) are reused, not re-researched.

## 1. Per-platform connector model: count, language, definition style, auth location, licence, maintainer

### Takeaway
Every platform defines a connector as code or configuration written against its own runtime context, and all but Zapier, Make and Node-RED keep the long tail in a vendor-run monorepo. Auth is split in the same way everywhere that has OAuth: the connector declares the auth shape (URLs, scopes, fields) and a shared platform layer performs the flow, stores the tokens and refreshes them. Counts are not comparable across platforms because the units differ (node directories, node types, apps, tasks, scripts, npm packages).

### Cited Findings

Summary table (GitHub API and npm registry, 2026-10-08, unless a vendor claim is marked):

| Platform | Independently countable | Vendor claim | Language | Definition style | Auth lives | Connector licence | Maintainer |
|---|---|---|---|---|---|---|---|
| n8n | 308 node dirs; 445 node entries and 412 credential entries in `n8n-nodes-base/package.json`; 1,792 entries in n8n's community-node API | "1500+ integrations" | TypeScript | Imperative `execute()` in most nodes; a declarative `requestDefaults`/routing style in a minority | Separate credential classes per service; shared helpers in core do OAuth and inject tokens | Sustainable Use License (core nodes); MIT required for verified community nodes | Core team for `nodes-base`; partners and community for npm community nodes |
| Activepieces | 736 community + 27 core + 1 custom piece dirs | README: "280+" as MCP, "60% ... contributed by the community" | TypeScript | Imperative `createPiece`/`createAction` with declarative props | `PieceAuth.OAuth2({authUrl, tokenUrl, ...})` declared in the piece; flow, storage and refresh in the platform | MIT | Core team plus paid-bounty community |
| Pipedream | 3,401 component dirs | "3,000+ APIs" | JavaScript (`.mjs`) | Imperative object with `propDefinitions` and `methods` | Injected `this.$auth`; no OAuth config in the component | Pipedream Source Available License | Core team plus community; now part of Workday |
| Windmill | Not countable from a repo; Hub page shows "100+ integrations" | "200+ resource types" | Rust backend; scripts in TS, Python, Go, Bash, SQL, PHP, Rust and others | A resource type (JSON schema) plus free-standing scripts | Resource holds credentials; instance-level OAuth for a fixed list of providers | Not established for Hub scripts | Windmill team plus community submissions |
| Node-RED | 6,283 modules in the official catalogue | n/a | JavaScript | Imperative JS node plus HTML editor file | Per node type; no shared OAuth | Each package its own; runtime Apache-2.0 | Community |
| Zapier | Not countable (private) | "9,000 apps" | JavaScript/TypeScript on Node.js 22 | JS object validated against a schema; also a web UI builder | `authentication` property per app; Zapier runs the flow and stores tokens | Each partner's own code; SDK under the Zapier Platform Agreement | Partners |
| Make | Not countable (private) | "more than 3000 apps" | JSON plus JavaScript IML functions | Declarative JSON config | `connections` component per app; platform executes | Not established | Make, plus users for "verified" apps |
| Kestra | 201 `plugin-*` repos in the org (198 not archived) | "2,000 plugins" (tasks and triggers), about 230 Maven artifacts | Java | Imperative Java task classes with annotated properties | Plain task properties per plugin (e.g. `oauthToken`); no shared connection object seen | Apache-2.0 | Core plugin team plus community |
| Automatisch | 92 app dirs | n/a | JavaScript | Imperative JS per app | `auth/` folder per app | AGPL-3.0 (`.ee.` files Enterprise) | Core team; repo quiet since January 2026 |
| Workato | Not countable | "1,200+ pre-built connectors" | Ruby DSL (Connector SDK) | Declarative-ish Ruby hash with lambdas | In the connector definition; platform executes | Proprietary | Vendor plus community connectors |
| Tray.ai | Not countable | "700+ connectors" (search summary) | Not established | Not established | Not established | Proprietary | Vendor |

n8n:
- `packages/nodes-base/nodes` has 308 directories and `packages/nodes-base/credentials` has 414 files; the package manifest (version 2.43.0 on master) registers 445 node files and 412 credential files, and is licensed `LicenseRef-n8n-sustainable-use` (GitHub API, 2026-10-08) — [nodes-base/nodes](https://github.com/n8n-io/n8n/tree/master/packages/nodes-base/nodes), [nodes-base/package.json](https://github.com/n8n-io/n8n/blob/master/packages/nodes-base/package.json)
- n8n's README claims "1500+ integrations and 9,000+ workflow templates" (vendor claim, 2026-10-08) — [n8n README](https://github.com/n8n-io/n8n/blob/master/README.md)
- n8n's own community-node API returned a total of 1,792 entries on 2026-10-08, of which 1,263 have `isOfficialNode: true`, 529 `false`, and 690 carry a company name. I could not find documentation of what `isOfficialNode` means — [api.n8n.io community-nodes](https://api.n8n.io/api/community-nodes?pagination%5BpageSize%5D=1)
- A third-party index (the `n8n-mcp` project) reports 2,175 n8n nodes: 827 core plus 1,348 community, 1,195 of them verified; older versions of the same README report 1,851 total. Unit is node types, not services (search summary of a third-party README) — [n8n-mcp on glama.ai](https://glama.ai/mcp/servers/@czlonkowski/n8n-mcp/blob/5825a85ccc504e36e16bad3437546bfdc1dd4fb5/README.md)
- Style split inside `nodes-base` (GitHub code search, approximate, 2026-10-08): about 30 files contain `requestDefaults` (the declarative style) against about 363 files containing `async execute(this: IExecuteFunctions)` (the programmatic style) — [nodes-base/nodes](https://github.com/n8n-io/n8n/tree/master/packages/nodes-base/nodes)
- Credentials are separate classes typed against `n8n-workflow` (e.g. `SlackOAuth2Api.credentials.ts` holds the scope list), and nodes call `this.helpers.requestWithAuthentication.call(this, credentialType, options, ...)` on an `IExecuteFunctions` context supplied by the runtime — [SlackOAuth2Api.credentials.ts](https://github.com/n8n-io/n8n/blob/master/packages/nodes-base/credentials/SlackOAuth2Api.credentials.ts), [Slack V2 GenericFunctions.ts](https://github.com/n8n-io/n8n/blob/master/packages/nodes-base/nodes/Slack/V2/GenericFunctions.ts)
- The monorepo has a shared OAuth package, `@n8n/client-oauth2`, which `n8n-core` depends on — [packages/@n8n](https://github.com/n8n-io/n8n/tree/master/packages/%40n8n), [npm n8n-core](https://www.npmjs.com/package/n8n-core)
- Verified community nodes must be MIT, must have no external dependencies ("Ensure that your package does not include any external dependencies"), "must not interact with environment variables or attempt to read/write files", and from 2026-05-01 must be published through a GitHub Action with a provenance statement — [n8n verification guidelines](https://docs.n8n.io/integrations/creating-nodes/build/reference/verification-guidelines/)

Activepieces:
- 736 directories in `packages/pieces/community`, 27 in `packages/pieces/core`, 1 in `packages/pieces/custom` (GitHub API, 2026-10-08) — [packages/pieces](https://github.com/activepieces/activepieces/tree/main/packages/pieces)
- README (vendor claims): "All pieces are open source and available on npmjs.com, 60% of the pieces are contributed by the community"; "All our pieces (280+) are available as MCP"; a roadmap line still reads "200+ Pieces". The README numbers lag the 736 directories — [Activepieces README](https://github.com/activepieces/activepieces/blob/main/README.md)
- A piece is built with `createPiece` and `createAction` from `@activepieces/pieces-framework`; the Slack piece declares `PieceAuth.OAuth2({ authUrl, tokenUrl, required, getConnectionIdentifier })`, and an action's `run(context)` reads `context.auth` — [slack auth.ts](https://github.com/activepieces/activepieces/blob/main/packages/pieces/community/slack/src/lib/auth.ts), [send-message-action.ts](https://github.com/activepieces/activepieces/blob/main/packages/pieces/community/slack/src/lib/actions/send-message-action.ts)

Pipedream:
- 3,401 directories under `components` (git tree API, 2026-10-08) — [PipedreamHQ/pipedream components](https://github.com/PipedreamHQ/pipedream/tree/master/components)
- A component app file exports `{ type: "app", app: "github", propDefinitions, methods }`, imports helpers from `@pipedream/platform`, and reads credentials from an injected object (`this.$auth.oauth_access_token`, `this.$auth.bot_token`); no authorize or token URL appears in the component — [github.app.mjs](https://github.com/PipedreamHQ/pipedream/blob/master/components/github/github.app.mjs), [slack_v2.app.mjs](https://github.com/PipedreamHQ/pipedream/blob/master/components/slack_v2/slack_v2.app.mjs)

Windmill:
- "integrations are referred to as resources and resource types"; a resource type "defines the JSON schema (host, credentials, options) for that system"; docs claim "200+ resource types" on the Hub; OAuth is offered for a named list (Google Calendar, Drive, Gmail, Sheets, Workspace, GitHub, GitLab, LinkedIn, Slack, Microsoft Teams) and self-hosters must configure OAuth themselves — [Integrations on Windmill](https://www.windmill.dev/docs/integrations/integrations_on_windmill)
- The Hub front page shows "100+ integrations" with per-integration script counts that are very uneven: Jira 529, Windmill 341, OpenAI 58, Slack 39, Nextcloud 24, Google Sheets 16, Telegram 11, Gmail 8, S3 8 (read 2026-10-08) — [Windmill Hub](https://hub.windmill.dev/)
- The backend is Rust and contains parsers for Bash, C#, Go, GraphQL, Java, Nu, PHP, Python, R, Ruby, Rust, SQL, TypeScript and YAML scripts (GitHub API, 2026-10-08) — [windmill backend/parsers](https://github.com/windmill-labs/windmill/tree/main/backend/parsers)

Node-RED:
- The official catalogue JSON listed 6,283 modules, updated 2026-10-08 — [catalogue.nodered.org](https://catalogue.nodered.org/catalogue.json)
- "A node may define a number of properties as `credentials`"; they "are stored separately to the main flow file"; the page describes no shared OAuth mechanism — [Node-RED credentials doc](https://nodered.org/docs/creating-nodes/credentials)

Zapier:
- A Platform CLI integration is a JavaScript object exported from `index.js` and validated against the Zapier Platform Schema; auth schemes are Basic, Digest, Custom, Session, OAuth1 and OAuth2 (with PKCE), set in the app's `authentication` property; "All Zapier CLI integrations are run using Node.js `v22`" on AWS Lambda — [Zapier Platform CLI docs](https://docs.zapier.com/platform/build-cli/overview)
- The SDK repo (`zapier/zapier-platform`, 557 stars, packages `cli`, `core`, `schema`, `legacy-scripting-runner`) is not open source: its LICENSE says use is governed by the Zapier Platform Agreement. `zapier-platform-core` had 158,723 weekly npm downloads — [zapier-platform LICENSE](https://github.com/zapier/zapier-platform/blob/main/LICENSE), [npm zapier-platform-core](https://www.npmjs.com/package/zapier-platform-core)
- Analyst view (Sacra, secondary): "Zapier grew cheaply because it turned integrations into partner supplied inventory instead of an internal engineering backlog", whereas Make "requires people to add and maintain those integrations directly" — [Sacra](https://sacra.com/chat/h/5ea37191-7fc3-4312-914a-de39cd2172e9/)

Make:
- "Make currently supports more than 3000 apps"; verified apps are "developed by Make or by other Make users", pass an app review and "must be actively maintained" (search summary of Make's page) — [Introduction to Make apps](https://apps.make.com/introduction-to-make-apps?x=1)
- In the apps builder "you write down a JSON configuration which is then used by the Make platform to generate all connections and app modules"; complex logic goes in custom IML functions written in JavaScript; components are base, connections (basic, JWT, OAuth 1.0, OAuth 2.0), webhooks, modules and RPCs (search summary of Make developer docs) — [Make custom apps docs](https://developers.make.com/custom-apps-documentation/custom-apps-documentation), [Make OAuth2 connection doc](https://developers.make.com/custom-apps-documentation/app-structure/connections/oauth2)

Kestra:
- "Kestra 2.0 just crossed 2,000 plugins" where a plugin is a task or trigger; about 230 Maven artifacts are 2.0-compatible; the post refers to "~8,800 properties across more than 150 plugin repositories" (published 2026-09-14, vendor) — [Kestra 2.0 plugins blog](https://kestra.io/blogs/kestra-2-0-plugins)
- The `kestra-io` org has 201 repositories with `plugin-` in the name, 3 archived; Kestra and sampled plugin repos are Apache-2.0 and Java (GitHub search, 2026-10-08) — [kestra-io org](https://github.com/kestra-io), [plugin-notifications](https://github.com/kestra-io/plugin-notifications)
- Auth is ordinary task properties: `AbstractGithubTask` declares `login`, `oauthToken`, `jwtToken`, `appInstallationToken` and `endpoint` as `Property<String>` — [AbstractGithubTask.java](https://github.com/kestra-io/plugin-github/blob/main/src/main/java/io/kestra/plugin/github/AbstractGithubTask.java)

Automatisch:
- 92 directories under `packages/backend/src/apps`; the Slack app has `actions`, `auth`, `common`, `dynamic-data`, `dynamic-fields`, `index.js`, and its `auth` folder holds `generate-auth-url.js`, `verify-credentials.js`, `is-still-verified.js` (GitHub API, 2026-10-08) — [automatisch apps](https://github.com/automatisch/automatisch/tree/main/packages/backend/src/apps)
- AGPL-3.0 for the Community Edition; files with `.ee.` in the name are under an Enterprise licence; contributors sign a CLA — [Automatisch README](https://github.com/automatisch/automatisch/blob/main/README.md)
- 13,995 stars; last push 2026-02-11 and the most recent commits are dated 2026-01-15 and 2025-12-03 (GitHub API, 2026-10-08) — [automatisch/automatisch](https://github.com/automatisch/automatisch)

Workato and Tray (brief):
- Workato advertises 1,200+ pre-built connectors in three groups (pre-built, universal HTTP/OpenAPI/GraphQL/SOAP, community) and a Connector SDK in Ruby (search summary plus docs) — [Workato connectors docs](https://docs.workato.com/connectors), [Workato SDK getting started](https://docs.workato.com/developing-connectors/sdk/cli/guides/getting-started.html)
- Tray.ai "turns 700+ connectors into governed MCP Tools" through its Agent Gateway (search summary) — [Tray MCP server configuration](https://tray.ai/documentation/platform/artificial-intelligence/agent-gateway/mcp-server-configuration)

### Inferences
- The split "connector declares auth shape, platform runs the flow" is universal among the OAuth-capable platforms (n8n, Activepieces, Pipedream, Zapier, Make, Windmill, Automatisch). The part a shared library would replace is therefore the platform's credential service, which each already has, plus the per-service declarations, which each has already written.
- Three platforms have no shared end-user OAuth at all in what I read: Kestra (tokens are task properties), Node-RED (per node type) and, for most services, Windmill (OAuth for ten named providers). A library that supplies OAuth, refresh and a provider catalogue fills a real hole there, more than it does for n8n or Activepieces.
- n8n's independently countable first-party catalogue is about 300 services. The "1500+" headline is reached by adding community and partner npm packages. That matches the earlier finding that one core team sustains a few hundred connectors.
- Declarative definitions dominate only where the vendor hosts execution (Make JSON, Workato Ruby DSL, Zapier's UI builder). The open-source, self-hostable platforms are overwhelmingly imperative code, n8n by roughly 12 to 1 on the code-search numbers.

### Gaps
- Zapier's split between partner-built and Zapier-built apps, and Make's split between Make-built and user-built verified apps: no primary figure found.
- Windmill Hub totals (scripts, resource types) could not be read from an API; the Hub licence for contributed scripts was not found.
- Tray's connector SDK language and definition style, and Workato's community connector count, were not verified.
- The Make findings come from search summaries; both direct fetches of Make's developer docs returned only a landing page.
- I did not confirm what n8n's `isOfficialNode` flag means, so I cannot state a verified-node count with confidence. The plausible reading is 1,263 verified of 1,792 listed.
- Kestra's secret handling (how a token property is populated from a secret store) was not read.

## 2. Can any platform's connector packages be used standalone, outside the platform runtime?

### Takeaway
Activepieces pieces can, and people are doing it: since mid-2026 each `@activepieces/piece-*` npm package is a zero-dependency single-file bundle, and at least three independent projects load them outside Activepieces, one of them a Rust engine that calls pieces through a Node sidecar. What you get is the action code only. The OAuth flow, token refresh, trigger lifecycle and types stay in the Activepieces platform, so every reuser rebuilds those. n8n and Pipedream packages are blocked by licence before coupling matters, and Windmill Hub scripts are reusable only as source snippets.

### Cited Findings

Activepieces:
- Published manifest of `@activepieces/piece-slack` 0.21.1 (published 2026-10-08): zero dependencies, no `license` field, 1,281,585 bytes unpacked, 133,726 weekly downloads. `@activepieces/piece-github` 0.9.3: zero dependencies, 7,203 weekly downloads (npm registry, 2026-10-08) — [npm piece-slack](https://www.npmjs.com/package/@activepieces/piece-slack), [npm piece-github](https://www.npmjs.com/package/@activepieces/piece-github)
- The published tarball of `piece-slack` 0.21.1 contains 15 files: `src/index.js`, `package.json` and 13 i18n JSON files. It has no `.d.ts` type files and no LICENSE file (tarball listing, 2026-10-08) — [npm piece-slack](https://www.npmjs.com/package/@activepieces/piece-slack)
- Activepieces docs: "Activepieces builds every piece into a self-contained bundle. Instead of shipping a piece that depends on `@activepieces/shared`, `@activepieces/pieces-framework`, `@activepieces/pieces-common`, and the `@activepieces/core-*` packages at install time, the build inlines all of that code into a single artifact." And: "These libraries are never published to npm; they only exist as part of each piece's bundle." The page says nothing about use outside Activepieces — [Bundling pieces](https://www.activepieces.com/docs/build-pieces/misc/bundling-pieces)
- Conflict with the registry: `@activepieces/pieces-framework` does exist on npm with 146 versions from 2023-04-09; its latest, 0.32.0, was published 2026-06-17 and nothing since, yet it still had 461,262 weekly downloads. `@activepieces/pieces-common` 0.12.5 had 296,638 and `@activepieces/shared` 0.96.2 had 822,692. None of the three manifests has a `license` field (npm registry, 2026-10-08) — [npm pieces-framework](https://www.npmjs.com/package/@activepieces/pieces-framework), [npm pieces-common](https://www.npmjs.com/package/@activepieces/pieces-common)
- In the repository the piece source still depends on five workspace packages (`pieces-common`, `pieces-framework`, `core-piece-types`, `core-utils`, plus the vendor SDK `@slack/web-api`) — [slack package.json](https://github.com/activepieces/activepieces/blob/main/packages/pieces/community/slack/package.json)
- Standalone use, case 1: `orch8-io/engine` (Rust, 75 stars, created 2026-04-14, pushed 2026-10-07) ships `@orch8/activepieces-worker`, an "HTTP sidecar that lets Orch8 workflows execute ActivePieces community pieces out of the box — Slack, Gmail, Stripe, GitHub, HubSpot, Notion, and ~280 others." The Rust engine dispatches any step whose handler starts with `ap://` to a Node process that "loads @activepieces/piece-<name> and invokes action.run()". The caller passes the token in the step parameters (`"auth": { "access_token": "xoxb-..." }`) — [orch8 activepieces README](https://github.com/orch8-io/engine/blob/main/activepieces/README.md)
- Standalone use, case 2: `borgius/freepieces` (0 stars) offers "a lightweight MIT framework and compatibility shim" to run "all 700+ MIT-licensed community pieces" on Cloudflare Workers, adding its own "OAuth2 + API-key auth — CSRF-protected OAuth flow, AES-256-GCM encrypted token storage in Cloudflare KV" and "drop-in `createAction`, `PieceAuth`, and `Property` wrappers" — [borgius/freepieces](https://github.com/borgius/freepieces)
- Standalone use, case 3: `Kastarter/eyeball` (9 stars), "One typed, authenticated tool API for AI agents ... TypeScript SDK, MCP gateway, 37 toolkits", has a `packages/bridge` with a `shim.ts` and an RFC titled "bridge-spike-findings" that reference Activepieces (75 code-search hits; I did not read the RFC) — [Kastarter/eyeball](https://github.com/Kastarter/eyeball)
- A GitHub code search for `"@activepieces/piece-slack"` in `package.json` files returned about 25 repositories outside the Activepieces org; most are forks or rebranded copies of the whole platform, not library use (approximate, 2026-10-08) — [GitHub code search](https://github.com/search?q=%22%40activepieces%2Fpiece-slack%22+filename%3Apackage.json&type=code)

n8n:
- `n8n-nodes-base` on npm (latest tag 2.15.1): 71,707,305 bytes unpacked, 75 dependencies including `n8n-workflow`, `@n8n/config`, `@n8n/di`, `@n8n/errors`, `@n8n/imap`; licence field "SEE LICENSE IN LICENSE.md"; 101,811 weekly downloads. All nodes ship in this one package (npm registry, 2026-10-08) — [npm n8n-nodes-base](https://www.npmjs.com/package/n8n-nodes-base)
- Node code runs as a method on a runtime-provided `IExecuteFunctions` object and obtains authenticated HTTP through `this.helpers.requestWithAuthentication`, which lives in `n8n-core` — [Slack V2 GenericFunctions.ts](https://github.com/n8n-io/n8n/blob/master/packages/nodes-base/nodes/Slack/V2/GenericFunctions.ts)
- The Sustainable Use License limits use to "your own internal business purposes or for non-commercial or personal use" (established in prior notes) — [n8n LICENSE.md](https://github.com/n8n-io/n8n/blob/master/LICENSE.md)
- A web search for standalone use of `n8n-nodes-base` found only documentation on extending n8n; no project or guide for running nodes without the n8n runtime — [n8n integrations docs](https://docs.n8n.io/integrations/)

Pipedream:
- `@pipedream/slack` 0.12.0 on npm depends on `@pipedream/platform` and has no `license` field; 9,884 weekly downloads. `@pipedream/platform` 3.4.0 had 189,044 (npm registry, 2026-10-08) — [npm @pipedream/slack](https://www.npmjs.com/package/@pipedream/slack), [npm @pipedream/platform](https://www.npmjs.com/package/@pipedream/platform)
- Components read credentials from `this.$auth`, which only the Pipedream runtime populates — [slack_v2.app.mjs](https://github.com/PipedreamHQ/pipedream/blob/master/components/slack_v2/slack_v2.app.mjs)
- The registry licence excludes "any commercial use of the software" (established in prior notes) — [Pipedream LICENSE](https://github.com/PipedreamHQ/pipedream/blob/master/LICENSE)

Windmill:
- Hub scripts are ordinary functions in a supported language that take a resource as a typed argument, and the docs point users to "standard client libraries in Python, Bash, and other supported languages" for custom integrations — [Integrations on Windmill](https://www.windmill.dev/docs/integrations/integrations_on_windmill)

### Inferences
- What stops reuse, by platform:
  - n8n: licence first (no commercial embedding), then packaging (one 72 MB package for every node), then runtime coupling (`this` context, credential helpers in `n8n-core`).
  - Pipedream: licence first, then the injected `$auth` and `@pipedream/platform` helpers, then the fact that OAuth client configuration is not in the open repository at all.
  - Activepieces: neither licence nor packaging. The blockers are that a piece carries only the auth declaration, not the flow or refresh; that triggers need the platform's webhook and polling lifecycle and `context.store`; and that the bundles now ship without types or a licence file.
  - Windmill: nothing to reuse as a package. Scripts are snippets, and the value is in the vendor SDK each one imports.
- The orch8 design is the most relevant single data point for Socket. A Rust workflow engine that needed connectors chose to shell out to Activepieces pieces over HTTP instead of writing Rust connectors or using a Rust library, and it passes raw access tokens in, which means it still has no OAuth or refresh layer. That is both evidence of the need and evidence of the incumbent a Rust library has to beat: about 700 MIT pieces available today through a sidecar.
- Activepieces' move to inlined bundles makes pieces easier to run standalone (no framework version matching) but harder to build on (no types, no stable framework package). It reads as an optimisation for their own runtime, not as support for outside reuse. No Activepieces document invites or supports standalone use.
- The MIT status of the published piece bundles rests on the repository LICENSE; the npm artefacts themselves declare no licence. A reuser relying on them should point at the repository, as freepieces does.

### Gaps
- I did not execute a piece outside Activepieces myself; the standalone claim rests on the orch8 and freepieces READMEs and the tarball contents.
- I did not determine how much of the `context` object a typical action needs beyond `auth` and `propsValue`, so the share of pieces that run cleanly in a minimal shim is unknown. orch8's README says "~280 others", well below 736, which may or may not reflect this.
- Whether Activepieces has stated a position on third-party runtimes for its pieces was not found.
- I did not read the eyeball RFC that records its findings from bridging Activepieces pieces; it may contain exactly the practical obstacles this question asks about.
- Windmill Hub script licensing and any export format were not established.

## 3. Evidence on the cost of building and maintaining connectors

### Takeaway
The few hard numbers point to low thousands of dollars per integration when built in-house and about $100 per integration when crowdsourced, with the real cost showing up as backlog: Pipedream has 2,633 open integration requests, and n8n has 816 open pull requests. No platform publishes maintenance hours, breakage rates or connector team size.

### Cited Findings
- Lindy (AI agent product), quoted in a Pipedream case study: "We've spent over $1M building about 250 integrations over the past 2 years. And now, with Pipedream, we're immediately offering 2,500." Also: "Even just a one-week delay kills the deal 90% of the time." This is a vendor case study, and I saw the quotes in a search summary; my direct fetch of the page returned unrelated content — [Pipedream blog: Lindy](https://pipedream.com/blog/lindy/)
- Activepieces pays cash bounties per piece. Its repo carries dollar-amount labels from $15 to $200. Items carrying each label (issues and PRs, GitHub search, 2026-10-08): $100: 69; $50: 23; $30: 16; $40: 11; $200: 9; $15: 8; $60: 4; $150: 4; all others 1 or 2 each; about 154 in total. 174 issues carry the "Bounty" label and 150 the "Rewarded" label — [Activepieces labels](https://github.com/activepieces/activepieces/labels), [bounty issues](https://github.com/activepieces/activepieces/issues?q=label%3A%22%F0%9F%92%8E+Bounty%22)
- The most recent $100-labelled items are single-app requests titled "[MCP] MeisterTask", "[MCP] Zendesk Sell", "[MCP] Front", "[MCP] Zoho Campaigns", "[MCP] Lemlist" and similar, all closed, dated September and October 2025; $30 items include "[MCP] Shippo" and "[MCP] Uscreen"; $200 items include "[MCP] Salesforce" and "[MCP] Gmail". No bounty-labelled issue is currently open — [$100 label](https://github.com/activepieces/activepieces/issues?q=label%3A%22%24100%22)
- Activepieces also ran a non-cash scheme: contributing a piece earned 1,400 free tasks per month, and community members objected that the reward was low (search summary of the forum thread) — [Activepieces community: Introducing Rewards](https://community.activepieces.com/t/introducing-rewards/3870)
- Activepieces backlog (GitHub search, 2026-10-08): 440 open issues, 230 open PRs, 112 open items labelled "area/third-party-pieces"; about 357 contributors — [activepieces issues](https://github.com/activepieces/activepieces/issues)
- Pipedream backlog (GitHub search, 2026-10-08): 4,321 open issues, of which 2,633 carry the `app` label (requests for a new integration), 363 `bug`, 208 `trigger / source`, 173 `action`. 583 of the open `app` requests were filed before 2024. 2,189 `app` issues have been closed. 1,304 issues were created in 2026 so far; about 322 contributors — [Pipedream issues](https://github.com/PipedreamHQ/pipedream/issues)
- Pipedream's label set records why integrations stall: `blocked-on-they-don't-have-an-api`, `blocked-on-limited-api`, `api-access-not-granted`, `paid-account-needed`, `phone-call-required`, `no-response-from-developer`, `app-developer-co-said-will-revert`, `missing scopes`, `blocked: app not registered`, `OAuth 1 integration`, `SOAP`, `Session Auth` — [Pipedream labels](https://github.com/PipedreamHQ/pipedream/labels)
- n8n backlog (GitHub search, 2026-10-08): 361 open issues but 816 open PRs; 233 open PRs labelled `community`, 132 labelled `node/improvement`, 12 labelled `node/new`; about 429 contributors. n8n triages issues into Linear (labels `in linear`, `status:in-linear`), so the public issue count understates the backlog — [n8n pull requests](https://github.com/n8n-io/n8n/pulls), [n8n labels](https://github.com/n8n-io/n8n/labels)
- n8n pushes new integrations out of the core repo: verified community nodes must have no external dependencies "to keep it lightweight and easy to maintain", and the guidelines assign no maintenance duty to n8n after verification — [n8n verification guidelines](https://docs.n8n.io/integrations/creating-nodes/build/reference/verification-guidelines/)
- Zapier's answer to the cost was to move it to partners (analyst view): it could "scale to thousands of apps without hiring a matching integrations team" — [Sacra](https://sacra.com/chat/h/5ea37191-7fc3-4312-914a-de39cd2172e9/)
- Kestra on per-plugin upkeep across a major version: no breaking API changes ("your 1.x plugin runs on 2.0, on purpose"), but the team still had to annotate "~8,800 properties across more than 150 plugin repositories" for one release — [Kestra 2.0 plugins blog](https://kestra.io/blogs/kestra-2-0-plugins)
- Automatisch, the smallest catalogue here (92 apps), shows no commits after 2026-01-15 — [automatisch/automatisch](https://github.com/automatisch/automatisch)

### Inferences
- Lindy's figures imply about $4,000 per integration ($1M over 250) for an in-house team at a funded startup. Activepieces' modal bounty implies about $100 for a community-built piece, excluding core-team review time. The 40x gap is partly scope (a bounty piece is a handful of actions; Lindy's number includes auth, UI and upkeep) and partly labour arbitrage. Neither figure covers ongoing maintenance.
- Pipedream's labels show that a large part of connector cost is not code: getting API access, paying for a vendor account, registering an OAuth app, waiting on a vendor. A shared library does not remove these costs for whoever maintains the connector, and bring-your-own-credentials pushes the OAuth app registration to each consumer.
- Supply does not keep up with demand even at the largest open catalogue: Pipedream has more open integration requests (2,633) than it has closed (2,189), with 3,401 integrations already built.
- n8n's ratio of open PRs to open issues suggests review capacity, not contributor supply, is the constraint on a vendor-run catalogue. That matches the Singer finding in the earlier notes.
- Activepieces' bounty programme appears to have paused: every sampled bounty item is closed, none is open, and the newest are from late 2025. I cannot tell whether this is a deliberate change.

### Gaps
- No platform publishes: hours per connector, share of connectors broken or outdated at a point in time, or headcount on connectors. I found no founder statement with such figures other than Lindy's.
- I did not open individual bounty issues to confirm that each dollar label corresponds to one complete piece, or what the acceptance criteria were.
- The Lindy quotes need confirmation from the page itself.
- n8n's internal Linear backlog is not visible. Node-level bug counts could not be derived because n8n no longer applies `node/issue` to open issues (0 open, 77 all-time).
- I found no data on how often upstream API changes break connectors (breakage rate per connector per year).

## 4. Have platforms adopted shared or third-party connector sources, and who treats MCP as the extension mechanism?

### Takeaway
No workflow platform has adopted another vendor's connector catalogue as its own source; each still writes or crowdsources connectors in its own format. What all of them have adopted, in both directions, is MCP: every platform checked can now call external MCP servers as tools, and most expose their own catalogue as MCP servers. The adopters of third-party connector sources are downstream products (agent builders, new engines), not the platforms.

### Cited Findings
- n8n ships four MCP node directories in `@n8n/nodes-langchain`: `McpClient`, `McpClientTool`, `McpRegistryClientTool` and `McpTrigger`; the monorepo also has `mcp-apps`, `mcp-browser` and `mcp-browser-extension` packages (GitHub API, 2026-10-08) — [nodes-langchain/nodes/mcp](https://github.com/n8n-io/n8n/tree/master/packages/%40n8n/nodes-langchain/nodes/mcp), [packages/@n8n](https://github.com/n8n-io/n8n/tree/master/packages/%40n8n)
- Activepieces: "When you contribute pieces to Activepieces they become automatically available as MCP servers"; the community pieces directory also contains `mcp` and `mcp-client` pieces; its bounty requests in 2025 were titled "[MCP] <App>", so new connectors were commissioned as MCP tools first — [Activepieces README](https://github.com/activepieces/activepieces/blob/main/README.md), [pieces/community](https://github.com/activepieces/activepieces/tree/main/packages/pieces/community)
- Windmill's Rust backend has a `windmill-mcp` crate built on `rmcp` with a `server` feature (streamable HTTP transport) and an `auth` feature (`rmcp/auth` plus the `oauth2` crate) — [windmill-mcp Cargo.toml](https://github.com/windmill-labs/windmill/blob/main/backend/windmill-mcp/Cargo.toml)
- Kestra 2.0's AI plugin added "MCP client tasks, input/output guardrails" — [Kestra 2.0 plugins blog](https://kestra.io/blogs/kestra-2-0-plugins)
- Zapier exposes its whole catalogue as a hosted MCP server ("9,000 apps", "66,000+ triggers and actions"; established in prior notes) — [Zapier MCP](https://zapier.com/mcp)
- Make's TypeScript SDK has "Model Context Protocol (MCP) support. All SDK endpoints are automatically exposed as MCP tools", and Make runs a cloud MCP server that supersedes a legacy one (search summaries) — [npm @makehq/sdk](https://www.npmjs.com/package/@makehq/sdk), [Make MCP server (legacy) README](https://glama.ai/mcp/servers/@TigerTeamCompany/MAKE.COM---Serwer-MCP/blob/d43b191f4c9134585e1853da8744ff8984eeabee/README.md)
- Workato publishes a registry of prebuilt MCP servers and, per a third-party report, plans 100+ in 2026; Tray's Agent Gateway exposes workflows and connector operations as MCP tools (search summaries) — [Workato prebuilt MCP servers](https://docs.workato.com/en/mcp/mcp-server-registry), [Tray MCP server configuration](https://tray.ai/documentation/platform/artificial-intelligence/agent-gateway/mcp-server-configuration)
- Universal fallbacks exist beside the catalogues: Workato's "universal connectors" cover "HTTP, OpenAPI, GraphQL, and SOAP", and Activepieces generates a custom API call action per piece (`createCustomApiCallAction`) — [Workato connectors docs](https://docs.workato.com/connectors), [slack index.ts](https://github.com/activepieces/activepieces/blob/main/packages/pieces/community/slack/src/index.ts)
- Downstream adopters of someone else's connectors:
  - Lindy moved from about 250 in-house integrations to Pipedream Connect; a later secondary source says Lindy "connects to 6,000+ integrations through partnerships with Apify and Pipedream" as of December 2025 (search summaries) — [Pipedream blog: Lindy](https://pipedream.com/blog/lindy/), [firstaimovers](https://www.firstaimovers.com/p/lindy-ai-agents-automation-guide-2026)
  - Pipedream published further Connect case studies in 2025 for Runbear and Alter (Hacker News submissions of Pipedream blog posts, 2 points and 1 point) — [HN 44000088](https://news.ycombinator.com/item?id=44000088), [HN 44246266](https://news.ycombinator.com/item?id=44246266)
  - orch8 (Rust engine) consumes Activepieces pieces through a Node sidecar — [orch8 activepieces README](https://github.com/orch8-io/engine/blob/main/activepieces/README.md)
  - Nango's README names Replit, Ramp and Mercor as production users (established in prior notes) — [NangoHQ/nango](https://github.com/NangoHQ/nango)

### Inferences
- MCP is now the common extension mechanism, in the sense that a platform user who lacks a native connector can attach an MCP server. It has not replaced native connectors for the deterministic, non-agent path: n8n's MCP nodes live in the LangChain (AI) package, and Kestra's are in its AI plugin. Triggers, typed fields in a workflow editor and credential UIs still come from native connectors.
- Platforms have a commercial reason not to adopt a shared connector source: the catalogue is their moat and their headline number. The absence of any platform building on Nango, Composio or Pipedream Connect fits this. A neutral library would face the same reluctance from the incumbents this research was asked about.
- The real third-party-source adopters are products for which integrations are a cost centre and not the product: agent builders (Lindy, Runbear, Alter), developer tools (Replit) and new engines (orch8). That is the more plausible consumer segment for an embeddable library.
- Pipedream Connect's position changed with the Workday acquisition (announced 2025-11-19, per prior notes); products that built on it now depend on an enterprise HR vendor's roadmap. I found no public statement from those customers about it.

### Gaps
- I searched for, and did not find, any workflow platform that imports connectors from Nango, Composio, Pipedream, Airbyte or Singer. Absence of evidence only; I did not read each platform's full changelog.
- Whether any platform auto-generates connectors from OpenAPI at scale (as opposed to offering a generic HTTP/OpenAPI node) was not established.
- The Make, Workato and Tray MCP findings are from search summaries, not opened pages.
- I did not verify what Gumloop, Relevance AI, Dust or Manus use for integrations; a search returned nothing citable.
- Node-RED's MCP position (community nodes exist, but no core mechanism that I verified) is not covered.

## 5. Which platforms could technically consume a Rust-core library, and which need bindings?

### Takeaway
Only Windmill could link a Rust crate natively, and even there integrations are user scripts, not backend code, so the practical route is Windmill's Rust script language or a TypeScript/Python binding. Seven of the ten platforms run connectors on Node.js and would need an N-API binding; Kestra needs a JVM binding; Make and Workato cannot load native code at all. n8n's rule that verified community nodes have no external dependencies rules out a native binding on n8n Cloud.

### Cited Findings

| Platform | Connector runtime | What a Rust core would need | Hard constraint found |
|---|---|---|---|
| n8n | Node.js (TypeScript) | Node binding (native addon or WASM) | Verified community nodes: MIT, no external dependencies, no env or file access |
| Activepieces | Node.js (TypeScript) | Node binding | Pieces are bundled to one file; only deps that "genuinely cannot be inlined: native addons or packages that use dynamic `require`" stay external |
| Pipedream | Node.js (hosted) | Node binding | Hosted only; source-available licence |
| Node-RED | Node.js | Node binding | None found |
| Automatisch | Node.js | Node binding | Project appears dormant |
| Zapier | Node.js 22 on AWS Lambda | Node binding built for Lambda's Linux target | Code runs on Zapier's infrastructure |
| Windmill | Rust backend; scripts in many languages including Rust | Native crate in Rust scripts, or bindings for TS/Python scripts | None found |
| Kestra | JVM (Java 21/25) | Java binding (JNI or FFM) | None found |
| Make | Hosted JSON plus JavaScript IML functions | Not possible in-process; HTTP service only | No native code |
| Workato | Hosted Ruby DSL | Not possible in-process; HTTP service only | No native code |

- n8n verified community nodes: "Ensure that your package does not include any external dependencies"; "The code must not interact with environment variables or attempt to read/write files" — [n8n verification guidelines](https://docs.n8n.io/integrations/creating-nodes/build/reference/verification-guidelines/)
- n8n Cloud exposes only a vetted set of community nodes: "Nodes that appear in the editor have been manually vetted for quality and security" (search summary of n8n's announcement) — [n8n blog: community nodes on Cloud](https://blog.n8n.io/community-nodes-available-on-n8n-cloud/)
- Activepieces bundling keeps "a small allow-list of deps external only when they genuinely cannot be inlined: native addons or packages that use dynamic `require`" — [Bundling pieces](https://www.activepieces.com/docs/build-pieces/misc/bundling-pieces)
- Zapier: "All Zapier CLI integrations are run using Node.js `v22`", deployed with `zapier-platform push` to Zapier's AWS Lambda environment — [Zapier Platform CLI docs](https://docs.zapier.com/platform/build-cli/overview)
- Windmill supports Rust as a script language; crates are declared in an inline partial Cargo manifest at the top of the script, a dependency job produces a lockfile, and "Every bundle on Rust is cached on disk by default" — [Windmill Rust quickstart](https://www.windmill.dev/docs/getting_started/scripts_quickstart/rust)
- Windmill's backend already depends on the Rust ecosystem a library like Socket would sit in: `rmcp`, `oauth2` 5.0, `reqwest`, `sqlx` — [windmill-mcp Cargo.toml](https://github.com/windmill-labs/windmill/blob/main/backend/windmill-mcp/Cargo.toml)
- Kestra plugins are Java; the 2.0 post discusses Java 21 versus 25 compatibility and plugin templates targeting Java 21 — [Kestra 2.0 plugins blog](https://kestra.io/blogs/kestra-2-0-plugins)
- A sidecar is the pattern already used to cross the language boundary in the other direction: orch8's Rust engine calls a Node worker over HTTP/1 JSON to run Activepieces pieces — [orch8 activepieces README](https://github.com/orch8-io/engine/blob/main/activepieces/README.md)

### Inferences
- A Node.js binding is the single highest-leverage binding: it reaches n8n, Activepieces, Node-RED, Automatisch, Zapier CLI apps and Pipedream-style runtimes. Python is second (Windmill scripts, n8n's Python task runner, agent frameworks). Java reaches only Kestra among these.
- For the TypeScript platforms, a Rust core is a disadvantage, not an advantage. Their connector authors write TypeScript, their packaging favours pure-JS bundles, and n8n's verification rules exclude native dependencies. Corsair (TypeScript, per-integration npm packages) fits these consumers better than a Rust library with a Node binding would.
- Windmill is the best technical fit and the weakest commercial fit. It could use a Rust crate in the backend for the piece it lacks (OAuth and refresh for more than ten providers), but its integration model is "write a script using the vendor's SDK", which does not need unit functions from a library.
- Kestra is the platform with the largest gap a library could fill (no shared OAuth, tokens as task properties) but needs a JVM binding, and Java already has Apache Camel for the embeddable-component role.
- For hosted, declarative platforms (Make, Workato, Zapier's UI builder) the only possible relationship is over HTTP, which makes Socket a service and not a library.

### Gaps
- I did not test whether a native Node addon loads inside n8n's task runner, an Activepieces sandbox or Zapier's Lambda. The table reflects documented constraints only.
- Whether WASM (which avoids native-addon restrictions and would count as a bundled, dependency-free artefact) is acceptable under n8n's verification rules was not established.
- Activepieces' sandboxing mode for piece execution and any limits it puts on native code were not read.
- Tray's runtime was not established.

## 6. What did 2025-2026 products that needed many integrations choose, and is anyone asking for an embeddable, self-hosted library?

### Takeaway
The documented choices are hosted catalogues (Pipedream Connect, Composio, Nango) and MCP; the one public cost figure is Lindy abandoning in-house building after about $1M for 250 integrations. Demand for a self-hostable, embeddable alternative shows up as projects people built, not as requests people posted: Corsair, OpenConnector (5,968 stars), freepieces, eyeball and orch8's sidecar. I found no explicit public request for an embeddable integration library in the forums searched, and none of the projects found is a compiled in-process library.

### Cited Findings
- Lindy: built about 250 integrations in-house over two years for over $1M, then adopted Pipedream Connect for 2,500 (vendor case study, seen via search summary); the Hacker News submission of that post dates it to 2025-04-02 — [Pipedream blog: Lindy](https://pipedream.com/blog/lindy/), [HN 43559318](https://news.ycombinator.com/item?id=43559318)
- Vendor framing of the buyer's decision in 2026 (Composio and Nango marketing, search summaries): Composio "when a custom agent application needs runtime tool discovery and managed connections", Nango "when integrations are product infrastructure and the team wants code-level control", Pipedream Connect "when broad action coverage and workflow composition need to live inside the product" — [Nango: Pipedream Connect vs Nango](https://nango.dev/blog/pipedream-connect-vs-nango), [Composio: Nango alternatives](https://composio.dev/content/nango-alternatives-ai-agents)
- OpenConnector (`oomol-lab/open-connector`): 5,968 stars, TypeScript, Apache-2.0, pushed 2026-10-08, described as an "Open-source auth gateway connecting 1500+ SaaS providers to AI agents through SDK, CLI, MCP, HTTP, and OpenAPI." It was submitted to Hacker News on 2026-08-07 as "OpenConnector, an open source alternative to Pipedream/Composio" and received 2 points (GitHub API and HN Algolia, 2026-10-08) — [oomol-lab/open-connector](https://github.com/oomol-lab/open-connector), [HN 49204987](https://news.ycombinator.com/item?id=49204987)
- A second, unrelated project uses the same name: `openconnector-dev/openconnector`, AGPL-3.0, 15 stars, "Source code is coming soon" — [openconnector-dev/openconnector](https://github.com/openconnector-dev/openconnector)
- AnythingMCP is marketed as a self-hosted AGPL-3.0 gateway alternative to Composio (vendor page, search summary) — [AnythingMCP](https://anythingmcp.com/vs/alternatives-to-composio)
- Projects built to reuse open connectors outside their home platform: freepieces ("All 700+ Activepieces community pieces without licensing blockers", Cloudflare Workers), eyeball ("One typed, authenticated tool API for AI agents ... 37 toolkits"), orch8 (Rust engine with an Activepieces sidecar) — [borgius/freepieces](https://github.com/borgius/freepieces), [Kastarter/eyeball](https://github.com/Kastarter/eyeball), [orch8 activepieces README](https://github.com/orch8-io/engine/blob/main/activepieces/README.md)
- Hacker News interest in open integration infrastructure, by points (HN Algolia, 2026-10-08; anecdotal): "Show HN: Open-source OAuth service for 40+ APIs" (Nango, 2023-02-07) 206 points, 56 comments; "Launch HN: Activepieces" (2023-02-09) 231 points; "Show HN: Nango – Open unified API" (2023-11-09) 106 points; "Show HN: Klavis AI – Open-source MCP integration" (2025-05-05) 79 points, 50 comments. Later submissions in the same space got almost none: Nango's 2024-11 and 2025-03 Show HNs got 3 and 2 points, and OpenConnector got 2 — [HN 34693233](https://news.ycombinator.com/item?id=34693233), [HN 34723989](https://news.ycombinator.com/item?id=34723989), [HN 38206973](https://news.ycombinator.com/item?id=38206973), [HN 43896410](https://news.ycombinator.com/item?id=43896410)
- Hacker News searches for explicit requests returned nothing relevant (HN Algolia, 2026-10-08): "build integrations ourselves instead of Nango" 0 hits; "n8n embed license integrations our product" 0 hits; "activepieces pieces npm use directly" 0 hits; "wish there was a library integrations OAuth self-hosted" 1 irrelevant hit; a story search for "Corsair integrations" 0 hits — [HN Algolia search](https://hn.algolia.com/?q=activepieces+pieces+npm+use+directly)
- Products that want n8n's integrations inside their own product must buy n8n Embed; n8n publishes no price, and community sources put it at around $50,000 per year, while another guide says a few thousand euros per year at the low end (anecdotal and conflicting; search summaries) — [nordflux n8n Embed guide](https://nordflux.de/en/guides/n8n-embed-oem-embedding-n8n-into-your-own-product), [n8n community thread on commercial licence cost](https://community.n8n.io/t/does-anyone-have-a-rough-idea-of-how-much-an-n8n-commercial-license-would-cost-to-integrate-n8n-into-a-b2b-saas-solution-that-we-sell-to-businesses/316047?tl=es)
- Counter-signal from the earlier notes, still the strongest stated position against a shared library: LangChain's maintainers wrote in May 2026 that "With coding agents, it is often simpler to implement tools directly in application code" and that MCP now carries much of the demand — [langchain-community issue #674](https://github.com/langchain-ai/langchain-community/issues/674)

### Inferences
- Revealed preference is clearer than stated preference. Nobody posts "I want an embeddable integrations library", but several teams independently built one layer of it: an auth gateway (OpenConnector), a typed tool API (eyeball), a compat shim for someone else's connectors (freepieces), a sidecar (orch8), a TypeScript library (Corsair). Each solved it for itself, mostly in TypeScript, mostly as a server.
- The gap these projects leave is the same one the earlier report identified: all of them are a gateway or a Node process. orch8 is the concrete case of a Rust product that had no in-process option and paid for it with a second runtime and unmanaged tokens.
- Buyers with money choose hosted. Lindy's reasoning was time to "yes" in a sales conversation, which a hosted catalogue of 2,500 answers and a library of a few dozen deep integrations does not. A library competes for teams that cannot accept vendor custody of tokens (the Composio breach in the earlier notes is the argument) or that ship self-hosted software.
- Attention on Hacker News for "open-source integrations" launches fell sharply between 2023 and 2026 while GitHub stars for the same category stayed high (OpenConnector 5,968, Corsair 13,422). Stars in this category in 2026 are a weak signal of either real demand or forum interest, and I would not treat either as validation without usage data.
- For Socket, the nearest substitute a Rust team would reach for today is not another Rust crate. It is "run Activepieces pieces in a Node sidecar" or "call an MCP server". The library has to be better than both for a narrow set of services, on token custody, refresh, typed operations and webhooks.

### Gaps
- I found no explicit request (GitHub issue, Reddit thread, HN comment) asking for an embeddable, self-hosted integration library. Reddit was not searchable with the tools I had, and GitHub issues were not searched across repositories for this phrasing. This is absence of evidence from a limited search.
- What Gumloop, Relevance AI, Dust, Manus, Glean and similar 2025-2026 agent products use for integrations was not established from primary sources.
- OpenConnector's architecture (how much is a library versus a gateway, where tokens are stored, how its 1,500+ provider count is reached) was not read beyond the repository description. Given its star count and Apache-2.0 licence it deserves a closer look before any positioning decision.
- Corsair had no Hacker News story I could find, so its adoption evidence remains the npm and GitHub figures in the earlier notes.
- I could not confirm the n8n Embed price, or how many companies hold an Embed licence.
- No data was found on how products that built on Pipedream Connect responded to the Workday acquisition.
