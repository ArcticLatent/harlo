# Harlo’s actual OpenWiki integration and replacement investment

## 1. What is actually integrated, installed, and shipped?

### Takeaway

Harlo’s working integration is **the npm package `openwiki@0.9.0`, with a Harlo-specific patch, running on its own pinned Node 24.22.3**. It is not an invocation of upstream GitHub HEAD, not a standalone generation CLI launched for every request, and not a Rust wiki-generation implementation. Preserve the current successful host-agent/lifecycle combination as the baseline. [Packaging][H-fetch] [Runtime and transport][H-runtime] [Host lifecycle][O-manager]

### Cited Findings

#### Evidence scope and concurrent development

- Research date: **2026-11-13**. `git rev-parse HEAD` was checked twice and returned **`563992a39e12a546d13170faa8d118719becd2ad`**. Thus this checkout really is still at the historical review commit, but **the working tree is not that clean historical snapshot**. Initially four agent source files were modified; later changes also appeared in `crates/harlo-app/src/{chat_session,workspace}.rs` and `crates/harlo-core/src/{lib,session}.rs`. Generated wiki files and untracked research/review material were present throughout. No user changes were overwritten. The inspection snapshot records status and source hashes at **20:59:41 UTC**. [Inspection record][E-inspection] [Commit][H-commit]
- The later `workspace.rs` diff changes typed-message forwarding, not the wiki adapter/tool-selection implementation. It inserts two lines before the phone wiki handlers and a further line before orchestration; therefore the immutable commit links below deliberately give stable line ranges for the unchanged wiki logic. Live working-tree line numbers after these insertions can differ by two or three lines. The wiki prompt is still forwarded through the existing session machinery. [Workspace source][H-workspace] [Inspection record][E-inspection]
- Harlo’s real wiki metadata, inspected read-only, records `updatedAt: "2026-11-13T20:01:57.508Z"`, `gitHead: "563992a39e12a546d13170faa8d118719becd2ad"`, `model: "host-agent/harlo"`, `status: "complete"`, and `language: "en"`. `.run.json` was absent; `.page-manifest.json` contained eight pages. This corroborates the user’s report of a functioning integration; it is not evidence that newly arriving uncommitted changes have already been reconciled. [Inspection record][E-inspection] [Last-update metadata](file:///home/burce/Projects/harlo/openwiki/.last-update.json)
- Applicable `AGENTS.md` was read. Application analysis was read-only. Only these notes and isolated `/tmp/opencode` probe artifacts were written. The probes used synthetic repositories and isolated homes, made no generation calls, and used no model/embedding credentials. Existing application tests were **read, not executed**. [Repository instructions][H-agents] [Probe script][E-script] [Probe results][E-results]

#### Exact artifact identity

| Item | Observed identity / evidence |
|---|---|
| Harlo package pin | `packaging/openwiki/package.json`: `"openwiki": "0.9.0"`; same in the lockfile. [Manifest][H-package] |
| npm artifact | `https://registry.npmjs.org/openwiki/-/openwiki-0.9.0.tgz`; integrity `sha512-ARArQcKjvBWD56cAl0L5SGNmPfrE957zTPVvY+Hi1wVqBg7FqWWeqquPA4+fkA+iHAWclCScD9Snu7jUB9DCHw==`. [Registry metadata][N-package] [Lockfile][H-lock] |
| Published source provenance | The npm SLSA attestation declares source commit **`b7027e3299c666e426cf218b3412a05767f702b6`**, built by `.github/workflows/release.yml`, invocation `41995917083/attempts/1`. This is registry provenance metadata, not an independently reproduced build. [Attestation][N-attestation] |
| Prepared runtime | `/home/burce/Projects/harlo/target/openwiki/VERSION` says **`openwiki 0.9.0, node 24.22.3 x64, lock c8fc6a74922e82b4, patches 84d855bf42356cc3`**. [Inspection record][E-inspection] [Runtime VERSION](file:///home/burce/Projects/harlo/target/openwiki/VERSION) |
| Full lock SHA-256 | `c8fc6a74922e82b4fb519e9f5acb2304ab560720a4d1c7c0ca21335750967324`; prepared-runtime lock and repository lock matched. [Inspection record][E-inspection] |
| Patch | `packaging/openwiki/patches/ignored-evidence-unresolved.patch`; concatenated patch SHA-256 `84d855bf42356cc38aad6e4f24fcea663839dfa0368ba6ca19b22c9ed3ecd827`. It makes newly ignored evidence unresolved instead of aborting preflight. [Patch][H-patch] |
| Node pin | `24.22.3`; Linux x64 archive SHA-256 `4f2b175a6d53a5c617a80dba2676238a0688cef28c6da4eba35c7d844e66fcfb`; ARM64 `6e51a2ada00f0011cc266f65d98922b403d7ad88f809ca2aa615a5fe1ce6e86c`. [Node manifest][H-node] |
| Prepared executable hash | Stripped `target/openwiki/bin/node`: `1bcc162fbe60c20f30922c52e47d5d0d203fcd19b8a027803abe26d6ac269d4a`. It is different from the downloaded archive hash by definition. [Inspection record][E-inspection] |
| Upstream OpenWiki HEAD | **`41c92c4a00d6dadaa3769c6bf93023ed5a618076`**, Nov 12, 2026. Its `package.json` still says `0.9.0`, but this commit is **not** the npm provenance commit, and Harlo additionally patches the published package. Equal version strings do not establish equal source. [Upstream commit][O-head] [HEAD package manifest][O-head-package] [Attestation][N-attestation] |
| Candidate public HEAD | **`f42c5898877e2fefde54e29c4772ec3f4cde0feb`**, Nov 10, 2026; manifest package **`llm-wiki-engine` 1.0.1**, binary **`llm-wiki`**, Rust MSRV **1.95**, edition 2024. [Candidate commit][L-head] [Cargo manifest][L-cargo] |

#### Prepared developer runtime versus installed/release artifacts

- The usable pinned runtime exists in **`target/openwiki`**. The running Harlo process observed via `/proc` was a **`target/debug/harlo (deleted)`** executable, with no `HARLO_OPENWIKI` override. Harlo’s discovery rules locate `target/openwiki` beside that development build. No running OpenWiki Node process was observed at that instant; the isolated benchmark processes were separate. This establishes the available runtime and expected discovery, not a capture of the user’s real engine executing a call. [Discovery code, lines 213–231][H-runtime] [Inspection record][E-inspection]
- `/usr/bin/harlo` resolved to `/opt/harlo/bin/harlo`; read-only `pacman -Q harlo` returned **`harlo 1.0.2-1`** and install-kind was `arch`. **`/opt/harlo/lib/harlo/openwiki` was absent.** The existing `dist/version` was `1.1.1`, and both `dist/AppDir/usr/lib/harlo/openwiki` and `dist/opt/harlo/lib/harlo/openwiki` were absent. Consequently, existing system/staged releases cannot be used as measurements of the newly configured OpenWiki bundle. An attempted `dpkg-query` was unavailable; the successful pacman query resolved the installed-package question. [Inspection record][E-inspection]
- Current **source packaging** does ship the runtime: `packaging/linux/bundle.sh:92–98` runs `packaging/openwiki/fetch.sh`, copies the full runtime into `AppDir/usr/lib/harlo/openwiki`, and installs its licenses. `bundle.sh:105–114` makes the AppImage and `/opt/harlo` tree; `nfpm.yaml:26–32` packages that tree and the `/usr/bin` symlink. This is a verified packaging contract, not a claim that the existing older artifacts contain it. [Bundle script][H-bundle] [Package manifest][H-nfpm]

#### Download, update, lookup, and remote deployment

- `fetch.sh:23–44` caches the architecture-specific official Node `.tar.xz` in `${XDG_CACHE_HOME:-$HOME/.cache}/harlo-build`, verifies its pinned SHA-256, and extracts the full Node/npm distribution into scratch space. `:63–74` copies only Node into the runtime, runs **`npm ci --omit=dev --no-audit --no-fund`**, requires a `better-sqlite3` native binary, and applies Harlo’s patches. `:100–106` removes dependency source maps/type declarations/docs, SQLite build sources, and strips Node. npm itself is a build-time installer here, not a separately copied runtime requirement. [Fetch script][H-fetch]
- Reuse is keyed by the exact VERSION string incorporating OpenWiki version, Node version/arch, lock hash, and patch hash; there is no npm “latest” resolution in a normal packaged run. Updating the pin/lock/patches and shipping a new Harlo bundle changes the runtime. Runtime lookup checks `HARLO_OPENWIKI`, then executable-relative `../lib/harlo/openwiki`, `../openwiki`, and an extra test/deps layout. “Usable” checks only that Node and the CLI files exist. [Fetch script, 54–74][H-fetch] [Runtime, 178–231][H-runtime]
- Local invocation is **`<runtime>/bin/node <runtime>/node_modules/openwiki/dist/cli/cli.js mcp --host harlo`**. It does not depend on `node` or `npx` from PATH. Server creation is lazy and cached by machine. [Core, 395–435][H-server]
- For another machine, `openwiki.rs:537–585` tars/gzips the **entire local runtime**, keeps those bytes in a process-global `OnceLock`, and derives the cache ID from VERSION using FNV-1a. It copies over SSH to `~/.cache/harlo/openwiki-<id>`, first extracting to a `.part.<pid>` sibling. It verifies Node can load `better-sqlite3` with an in-memory database, atomically moves the tree, and deletes older `openwiki-*` caches. Subsequent calls reuse it if `bin/node` is executable. Remote creation allows 900 seconds. [Remote deployment][H-remote]
- **Architecture caveat grounded in code:** the runtime is built for the build machine’s architecture. `on_host` checks the remote architecture against that one runtime and rejects a mismatch; it does not download a second ARM64 runtime when an x64 Harlo connects to ARM. The Rust agent/helper packaging supports separate architecture binaries, but OpenWiki’s path here does not. Node/native-module libc compatibility is still required remotely. [Remote deployment][H-remote] [Bundle script, 79–95][H-bundle]

### Inferences

- A fair replacement baseline is the patched npm artifact plus Harlo’s host-agent workflow, not today’s whole OpenWiki product or an unpatched upstream checkout. [Artifact evidence][N-attestation] [Patch][H-patch] [Host lifecycle][O-manager]
- Removing a system Node requirement is not the main benefit: this wiki integration already brings its own Node. The available benefits are removal of the bundled runtime/tree, reduced warm-service memory, simpler native-module distribution, and possibly improved remote compatibility—each needing measurement. [Fetch script][H-fetch] [Remote deployment][H-remote]

### Gaps

- No current release package containing this runtime was rebuilt or extracted. Actual future AppImage/SquashFS, deb, rpm, or zstd package-size deltas remain unmeasured; the tar.gz figure below is for Harlo’s remote transfer format.
- No user remote machines were contacted, no user wiki engine was started, and no live production engine invocation was intercepted. Remote installed runtime versions remain unknown.
- The running debug executable was deleted/replaced on disk, so its exact compiled source revision cannot be proven from the checked-out source alone. The real wiki’s successful metadata and the available runtime are independently observed.

## 2. What exact behavior and interfaces must survive?

### Takeaway

The migration boundary is much larger than search: Harlo depends on a **deterministic, resumable, claim-aware repository lifecycle**, while its selected agent supplies research and prose. Most display functionality is already Rust/QML/Swift; replacing the engine should retain those interfaces and data rather than rebuild the UI or silently change evidence semantics. [Host workflow][H-start] [Lifecycle contract][O-skill] [Native browser][H-browser]

### Cited Findings

#### Integration map

| Surface | Actual contract and source locations |
|---|---|
| Project ownership | `crates/harlo-app/src/workspace.rs:3087–3098`: features derive from the session’s Harlo project; `wiki_<project-id> != "0"` enables wiki tools by default. `openwiki.rs:102–113` resolves Git top-level; remote sessions assume wiki availability until a search answers. Harlo project IDs are not OpenWiki workspace IDs. [Feature settings][H-features] [Root resolution][H-root] |
| Normal agent tools | `openwiki.rs:50–86`: only `openwiki_search` and `openwiki_read` are offered, with shortened schemas and no `root`. Search accepts `query`, optional `paths`, `limit` 1–20, optional `workspace`; read takes `page`, required `sections`, optional `wiki`. Listing tools exist in the backend but are not normally offered. [Read schemas][H-read-tools] |
| Writing-session tools | `openwiki.rs:115–133` dynamically forwards backend descriptions/schemas for the two offered reading tools plus six lifecycle tools, removing `root` from properties/required. `workspace.rs:6887–6939` only grants writes to wiki sessions Harlo starts. The adapter startup nonetheless requires all **ten** backend tools, including both listings. [Writing schemas][H-writing] [Dispatch gate][H-tools] [Handshake][H-transport] |
| Native agent RPC / other agents’ MCP | `crates/harlo-app/src/orchestration.rs:1–4,57–99,101–128`: external agents use `harlo --mcp-bridge` over a local socket; Harlo’s agent uses its pipe. Authorization/project lookup crosses the Qt thread; OpenWiki work completes off-thread. Native replies are `{"type":"harlo_result","call":...,"ok":true,"result":...}` or `ok:false,error`. [Bridge/RPC][H-orchestration] |
| Engine transport | `openwiki.rs:254–378`: persistent newline JSON-RPC 2.0 stdio, protocol `2025-06-18`, numeric request IDs, initialized notification, `tools/list`, `tools/call`. Initialization timeout 60s, list 30s, read tools 90s, writing tools 600s. Concurrent response routing uses an ID→channel map. Backend requests other than ping are declined. [Transport][H-transport] [Call wrapper][H-call] |
| Result normalization | `openwiki.rs:89–100`: `isError` becomes a host error; otherwise prefer `structuredContent`, then parse joined text as JSON, otherwise return the text. MCP backend produces both text JSON and structured JSON. [Answer normalization][H-answer] [Backend response formatter][O-mcp] |
| Provider, auth, budget | `workspace.rs:2409–2463`: create/update/continue selects the project’s agent, falling back to the default; project model overrides agent default, with remote default handling. It starts `start_session_budget`, titles the session, adds it to `wiki_sessions`, and sends the prompt through the existing agent. No provider key/model configuration is passed to the OpenWiki MCP service. The lifecycle records `host-agent/harlo`; it does not identify the underlying paid/local model. [Session start][H-start] [Host manager, 66–85,255–262][O-manager] |
| Generation prompt | `openwiki.rs:136–166`: create→`begin(mode:"init")`, update→`begin(mode:"update")`, continue→the interrupted mode. It appends the **bundled version’s skill**, tells the agent Harlo owns `root`, and propagates `.openwikiignore` to the agent’s own research tools. It tells the agent to change only wiki content. [Prompt construction][H-writing] |
| Markdown authoring vs finalization | The host agent uses ordinary file tools to write exactly the assigned page; `submit_page` handles sparse claim decisions. OpenWiki owns queue, claim versions/IDs, validation, indexes, provenance, deletion, source-drift checks, and finalization. Those are not supplied by the renderer or Rust fallback search. [Bundled skill, 48–121,140–210][O-skill] [Host manager][O-manager] |
| Desktop overview | `workspace.rs:2325–2387`: JSON includes `root,project,enabled,exists,pages,run,last,behind,running,session,runtime,loading,host`. Pages contain `path,title,description,type,verified,links`; run contains `mode,phase,done,total,started`; last contains `at,status,model`. Remote reads are cached for five seconds and refreshed off-thread. [Wiki state JSON][H-ui-state] |
| Desktop page view | `workspace.rs:864–875`; `openwiki.rs:975–1068`: native file read and Markdown rendering; view fields `path,title,type,description,verified,blocks,sources`. Blocks are `prose`, syntax-highlighted `code`, or `mermaid`, with heading anchors. Sources are `[{path,ranges:[[from,to]],claims}]`, combining frontmatter `repo://` resources with `.claims/<page>.json`. [Page wrapper][H-ui-read] [Sources/rendering][H-sources] |
| Graph and links | `openwiki.rs:723–751` extracts relative Markdown `.md` links; page enumeration drops missing targets. `WikiPanel.qml:225–245` builds a Mermaid page-link graph; above 40 pages it shows the selected page’s neighborhood. This graph is not an engine-owned typed knowledge graph. `WikiPanel.qml:118–133` opens `repo://...#L...` in the code editor and relative page links in the reader. [Native browser][H-browser] [QML][H-panel] |
| Diagram renderer | `crates/harlo-app/src/diagrams.rs:1–8,54–61`: Rust `merman` renders native diagrams. A bundled `mermaid.min.js` supplies the optional “Open in browser” fallback. Replacing the wiki does not remove this JavaScript, and it does not require Node to display the wiki. [Diagram implementation][H-diagrams] |
| UI search | `workspace.rs:880–897`: first calls real `openwiki_search` with `limit:12`, maps results, restores underscore identifiers from local page text; on any engine error it logs and returns Rust substring/section search with `plain:true`. **This fallback is in the UI path; normal agent tools do not automatically get it.** [UI search][H-ui-read] [Fallback search][H-search] |
| Freshness | `openwiki.rs:789–824,920–923`: reads `.last-update.json` and counts Git commits since its `gitHead`, excluding `openwiki`, `AGENTS.md`, `CLAUDE.md`. `verified` comes from page frontmatter. Commit lag is not a semantic guarantee or a detector for all dirty working-tree changes. [State][H-state] |
| Automatic update offers | `workspace.rs:1391–1400` checks after 20s, then every 60s. `:2466–2537` only checks local enabled projects and persists `wiki_seen_<id>`. `openwiki.rs:864–907` offers an update when default-branch movement included in this checkout changes files the wiki cites, and no durable run exists. Taking the offer starts a normal user-visible writing session; it does not automatically spend tokens. [Scheduling][H-scheduler] [Offer orchestration][H-offers] [Offer detection][H-offer-core] |
| Scheduled GitHub updates | `openwiki.rs:468–490`: after a finish call Harlo removes a newly generated, untracked `.github/workflows/openwiki-update.yml`, preserving a tracked one. This code is not conditioned on a successful finish result. The real checkout’s workflow was absent. Native OpenWiki cron/CI capabilities are not automatically the Harlo product’s scheduling behavior. [Finish cleanup][H-call] [Inspection record][E-inspection] |
| Progress and interruption | `openwiki.rs:776–824,909–917` parses `.run.json`; done means status other than `pending`. `WikiPanel.qml:57–81` polls visible running/interrupted state every 2.5s and refreshes on busy-state signals. Continue starts/resumes through `begin`; the backend durable queue is authoritative. [State][H-state] [QML][H-panel] [Run schema][O-run-state] |
| Cancellation and errors | No OpenWiki cancel tool is in Harlo’s ten-tool contract. The RPC timeout removes the waiter but sends no `notifications/cancelled`; dropping a `Server` kills/waits for its child. The adapter ignores unsolicited backend notifications. Thus session stop, request timeout, engine process exit, and durable run completion must not be conflated. Unknown backend failures become bounded generic errors; expected errors preserve codes in text. [Transport][H-transport] [Backend errors][O-mcp] |
| Phone/iOS | `workspace.rs:6326–6347` handles `wiki`, `wikiFolder`, `wikiRun` with modes create/update/continue. `phone_server.rs:1065–1074` routes `wikiPage` and `wikiSearch` off-thread to the same native wrappers. Swift `ProjectFeatures.swift` and `WikiViews.swift` consume that JSON and `openwiki/` page paths. Generation remains on the computer. [Phone dispatch][H-phone] [Phone server][H-phone-server] [Swift model][H-swift] |
| Telemetry | `openwiki.rs:235–248,411–425`: defaults off; `OPENWIKI_TELEMETRY_DISABLED=1` and `DO_NOT_TRACK=1` set locally/remotely. Changing the setting clears cached servers so replacements inherit it. `workspace.rs:1324,4030–4031` connects the persisted setting. [Telemetry][H-telemetry] [Server invocation][H-server] |

#### Real request/output examples

**Retrieval:** the following request and result were actually exercised against the pinned runtime, using a synthetic two-page Git repository under `/tmp/opencode`. Harlo fills `root`; the model-facing schema omits it. This is not an example inferred from an upstream README. [Probe transcript/results][E-results] [Root injection][H-call]

```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
  "name":"openwiki_search",
  "arguments":{"root":"<isolated fixture repository>","query":"how are failed jobs retried","limit":12}
}}
```

Relevant `structuredContent` result:

```json
{"results":[
  {"kind":"section","ref":["openwiki/architecture/jobs.md#retry-control"],
   "content":"Jobs\nSection: Retry control\nHow jobs run.\nA failed job is retried three times with backoff."},
  {"kind":"section","ref":["openwiki/quickstart.md#quickstart"],
   "content":"Quickstart\nWhere to start.\nSee jobs."}
]}
```

Read request arguments and result:

```json
{"root":"<isolated fixture repository>","page":"openwiki/architecture/jobs.md","sections":["retry-control"]}
```

```json
{"page":"openwiki/architecture/jobs.md","sections":[
  {"section":"retry-control","content":"## Retry control\nA failed job is retried three times with backoff."}
]}
```

`openwiki.rs:1426–1431` tests UI normalization into `page`, `section`, `heading`, `snippet`. `:1460–1461` tests `{"status":"workspace_required","workspaces":[{"id":"w1","name":"Team","wikiCount":2}]}`. These are existing tests, not newly asserted runtime coverage for linked workspaces. [Native tests][H-tests]

**Writing:** grounded schema examples, **not executed in this research**. All `runId`/`jobId` values must be actual UUIDs returned by the engine; placeholders below are explanatory. Source definitions are in the installed `dist/integrations/core/protocol.js:7–83`; sequence and output interpretation are in the skill and host manager. [Protocol][O-protocol] [Skill][O-skill] [Manager][O-manager]

```text
openwiki_begin({mode:"update"})             # root supplied by Harlo
→ status:"noop", or an active run with planning/generating phase

openwiki_submit_plan({runId:<uuid>, pages:[{
  path:"/openwiki/architecture/jobs.md", title:"Jobs",
  purpose:"Explain retries and failure handling", seedPaths:["src/jobs.rs"],
  relatedPages:["/openwiki/quickstart.md"]
}]})

openwiki_next_page({runId:<uuid>})
→ status:"pending", job with UUID/path/title and claim issues; later status:"complete"

# Agent researches current source and writes the assigned Markdown itself.
openwiki_submit_page({runId:<uuid>,jobId:<uuid>,claims:[{
  statement:"Failed jobs are retried three times.",
  evidence:[{resource:"repo://src/jobs.rs#L20-L48"}]
}],confirmedClaimIds:[...],retractedClaimIds:[...]})

openwiki_finish({runId:<uuid>})
→ completion only after deterministic finalization succeeds
```

#### Search engine, generator, and UI are distinct

- **Search is deterministic and currently model-free.** Installed `dist/retrieval/wiki.js:97–158` reads authored pages, creates section units, and handles workspace scoping. `:172–209` uses **Node’s built-in `node:sqlite`**, an in-memory FTS5 table with Porter/unicode61 tokenization, and BM25 field weights **8/4/6/1/3** for title/description/heading/prose/identifiers. Ranking prioritizes source-path matches, then term coverage, then BM25, then row order. It builds/closes the index **on each search call**, although the Node process remains warm. It is not a persistent vector index, and the native `better-sqlite3` module bundled for other dependencies is not the implementation used by this search function. [Installed retrieval source][O-retrieval]
- **Read is exact-section disclosure, not page retrieval.** It parses Markdown headings, checks every requested anchor, preserves requested order, and can return optional recorded claim/source provenance. The shortened normal Harlo read schema does not expose `includeProvenance`, although the underlying backend and dynamically forwarded writing-session tool do. [Retrieval, 239–300][O-retrieval] [Backend schemas][O-read-tools] [Harlo read schema][H-read-tools]
- **Generation intelligence is already outside OpenWiki’s MCP service.** The service calls deterministic `beginRepositoryRun`, `submitRepositoryPlan`, `nextRepositoryPage`, `submitRepositoryPage`, and `finishRepositoryRun`; the agent reads source and writes prose. Replacing OpenWiki does not eliminate the selected agent’s model calls or guarantee lower generation token cost. [Manager][O-manager] [Skill][O-skill] [Session start][H-start]
- **Native browser data is intentionally lighter.** It enumerates Markdown to depth eight / roughly 2,000 pages, ignores hidden directories and structural `index.md`, `log.md`, `INSTRUCTIONS.md`, parses a limited frontmatter subset, and provides a simpler substring-scored fallback. Remote enumeration caps each page at 256 KiB and returns at most 2,000 pages. This fallback should not be presented as equivalent to the real FTS5/heading implementation. [Browser, 607–721][H-browser] [Fallback search][H-search]
- **Single-writer nuance:** Harlo multiplexes calls into one server per machine, but the installed `HostSessionManager` has one `active` run and rejects overlapping lifecycle operations. `begin` replaces its active run; later requests must match that run ID. Concurrent retrieval is supported, but fully independent simultaneous writers are not a demonstrated existing guarantee. [Server cache][H-server] [Manager, 11–19,66–85,201–233][O-manager]

#### Existing tests worth retaining as contract fixtures

- `crates/harlo-core/src/openwiki.rs:1251–1272` provides a compact page/claims/run-state fixture; `:1275–1462` covers page ordering, traversal rejection, merge offers, state/progress, citations, render blocks, section search, and workspace-result projection. `:1494–1538` covers request correlation and real bundled search/read; `:1542–1580` covers writer tools/prompts, `.openwikiignore`, missing tools, and stopped servers. [Core tests][H-tests]
- `crates/harlo-app/qml/Main.qml:8437–8594` has end-to-end scenarios checking both Harlo and Claude Code receive exactly the intended tools, **one OpenWiki start serves search and read**, telemetry restart, per-project disablement, project model selection, successful page finalization, workflow removal, and Wiki-shelf behavior. These are especially useful parity gates because they exercise Harlo’s product contract, not just a backend function. [Application scenarios][H-e2e]

### Inferences

- A backend facade should preserve the existing tool names and host-owned root boundary initially. A raw command substitution to `llm-wiki serve` fails startup because the ten required `openwiki_*` names are missing; renaming those tools alone still leaves lifecycle/state/ranking differences. [Handshake][H-transport] [Candidate tool catalog][L-readme]
- Reusing Rust UI readers avoids unnecessary migration work **only if** page paths, Markdown/frontmatter, source links, and sidecars remain compatible. Replacing evidence-based freshness with a page timestamp would preserve the look while losing the semantics users rely on. [Native readers][H-sources] [State][H-state]

### Gaps

- Current linked-wiki UI behavior is narrower than the backend: search hits carry `wiki`, but desktop/phone page-reader wrappers accept only a page path and read the current root. Do not promise complete cross-wiki browsing parity without a focused fixture; preserve linked search/read behavior in the **agent API** first. [UI wrappers][H-ui-read] [Backend linked read][O-read-tools]
- Cancellation of a session while a lifecycle call is in flight has not been exercised. The visible adapter does not propagate MCP cancellation; the exact resulting interruption timing and file state should be captured with deterministic lifecycle fixtures before changing it.
- No end-to-end generation test was run against a paid model or the real repository. Successful retrieval fixtures and recorded last-update metadata do not prove all generation edge cases.

## 3. What dependency cost is actually removable?

### Takeaway

The prepared wiki runtime costs **210.261 MiB apparent / 256.414 MiB allocated**, including a **103.676 MiB Node executable**. The tiny 3,856-byte CLI is not the dependency. An isolated, no-model MCP service used roughly **292 MiB median RSS after one idle second**; startup/search measurements are baseline observations, not proof that a Rust replacement will be faster. [Raw results][E-results]

### Cited Findings

#### Disk and transfer measurements

Measured read-only with GNU `du -s -b` (apparent bytes, including directory entries) and `du -s -B1` (allocated blocks). MiB means 1,048,576 bytes. The nested module/package figures are **subsets**, not additional totals. Source: `target/openwiki`, already pruned by the current fetch script. [Measurement script][E-script] [Results][E-results] [Pruning][H-fetch]

| Component | Apparent bytes | Apparent MiB | Allocated bytes | Allocated MiB |
|---|---:|---:|---:|---:|
| Full prepared runtime | 220,474,699 | 210.261 | 268,869,632 | 256.414 |
| `bin/node` | 108,712,632 | 103.676 | 108,716,032 | 103.680 |
| Entire `node_modules` | 110,601,843 | 105.478 | 158,973,952 | 151.609 |
| `node_modules/openwiki` only | 2,303,963 | 2.197 | 3,280,896 | 3.129 |
| `dist/cli/cli.js` only | 3,856 | 0.00368 | 4,096 | 0.00391 |

- **Compressed remote payload:** executing the same read-only `tar -C target/openwiki -czf - .` operation used by Harlo yielded **69,069,951 bytes / 65.870 MiB**. This is gzip-tar, not the compression ratio of a future AppImage/deb/rpm. `PACKED` retains this byte vector in the main Harlo process after first remote deployment; that retained allocation is separate from remote Node RSS. [Raw results][E-results] [Remote packing][H-remote]
- Cached official Node `.tar.xz`: **32,064,292 bytes**, containing the official binary distribution’s Node/npm tree. It is a build cache outside the shipped runtime and must not be added to installed-runtime cost. [Fetch/cache contract][H-fetch] [Runtime inspection measurements][E-inspection]
- Lockfile contains **336 non-root package entries**. This is a lock-entry count, not a vulnerability count or an exact count of installed distinct dependency packages. Optional `mermaid` and `jsdom` peer directories were absent in the measured runtime. [Lockfile][H-lock] [Inspection/probe evidence][E-inspection]
- The source comment “about 440 MB to 250” describes pruning expectations; the measured apparent/allocated values above are more precise and distinguish filesystem overhead. [Fetch script, 100–106][H-fetch] [Measurements][E-results]

Largest apparent-size module groups (read-only `du`; scope directories combine packages): [Inspection record][E-inspection]

| Module/group | Apparent bytes | Relevance |
|---|---:|---|
| `js-tiktoken` | 22,426,769 | Tokenizer data/code; a large part of the shipped agent-oriented dependency tree. |
| `@azure` | 13,143,573 | Provider/auth SDK family. |
| `openai` | 10,123,891 | Provider SDK. |
| `@langchain` | 9,914,011 | Agent/provider/runtime library family. |
| `@anthropic-ai` | 8,786,275 | Provider SDK family. |
| `zod` | 5,405,751 | Schemas used by MCP/lifecycle; not simply removable with providers. |
| `langchain` | 3,929,177 | Agent framework. |
| `langsmith` | 2,647,606 | Tracing/framework dependency. |
| `better-sqlite3` | 2,149,335 | Includes 2,122,448-byte native `.node`; explicitly required by packaging/remote smoke check. |

These are potential optimization targets, **not a proven list of deletable packages**. The installed general CLI eagerly imports its Ink/React application and runners even on the MCP path; the lifecycle imports `agent/docs-only-backend.js` and other agent utility/finalization modules. A proper headless split needs import/dependency reachability analysis and lifecycle tests. [CLI imports, 1–25][O-cli] [Lifecycle imports, 1–18][O-lifecycle] [Package dependencies][N-package]

#### Safe runtime probe methodology and observations

Probe script: `/tmp/opencode/harlo-wiki-integration-probe.py`. Full results, requests/responses, fixture files, and empty stderr logs: `/tmp/opencode/harlo-wiki-probe-5elgy6h6/`. It used the existing pinned Node/package without installation or modification, isolated `HOME`, all XDG paths, TMPDIR, and Git config; it passed no inherited API keys, disabled telemetry, and issued only initialize/list/search/read. Each process was terminated after measurement. There were three process starts per mode, two authored pages, and five repeat searches per process. All search/read assertions passed. [Script][E-script] [Results][E-results]

Two modes:

1. **Actual Harlo command:** Node → full CLI → `mcp --host harlo`.
2. **Exploratory minimal entrypoint:** a scratch `.mjs` imports the existing installed `runOpenWikiMcp` directly and supplies `host:"harlo", producerActor:"harlo"`. This is not a production patch or a complete dependency-pruned build. [Scratch entrypoint](file:///tmp/opencode/harlo-wiki-probe-5elgy6h6/mcp-only.mjs) [MCP stdio source][O-stdio]

| Metric | Actual CLI, n=3 | Scratch MCP-only import, n=3 |
|---|---|---|
| Spawn→initialize+tools-list ready | 845.0 / 518.9 / 541.3 ms; median **541.3 ms** | 463.1 / 458.9 / 469.4 ms; median **463.1 ms** |
| RSS after ready + one idle second | 298,260 / 299,296 / 341,108 KiB; median **292.281 MiB** | 298,576 / 298,764 / 326,412 KiB; median **291.762 MiB** |
| First search | 27.7 / 27.4 / 26.8 ms | 31.2 / 31.7 / 30.0 ms |
| Repeated search, all 15 samples | **4.061 ms median**; 3.64–6.23 ms | **4.035 ms median**; 3.22–5.70 ms |
| Exact section read | 3.78 / 2.99 / 3.51 ms | 3.09 / 3.53 / 3.03 ms |
| RSS after retrieval | 310,608 / 311,340 / 350,360 KiB | 316,216 / 316,096 / 343,812 KiB |

All values above come from the same raw result file. Host was Linux x86_64 on AMD Ryzen 9 9900X; no page-cache flush, no exclusive-machine scheduling, no long steady-state wait, no PSS accounting. The first CLI start may include colder filesystem caches; all MCP-only trials ran after CLI trials. These are process-start observations under ordinary local load, not controlled cold-boot benchmarks. [Raw results][E-results] [Measurement code][E-script]

**Important result:** the simple direct-entrypoint experiment does **not** demonstrate meaningful RSS savings. It reduced sampled median startup by about 78 ms, with overlapping environmental uncertainty, while median idle RSS stayed near 292 MiB and warm lookup stayed near 4 ms. Deeper module isolation or another backend may help, but the evidence does not support promising an easy large memory win merely by bypassing the CLI. [Results][E-results]

#### Dependencies that remain elsewhere

- Harlo’s core already links bundled SQLite through `rusqlite`, Markdown parsing/rendering, highlighting, and other Rust libraries. Removing `node:sqlite`/`better-sqlite3` does not remove SQLite from Harlo. A native FTS facade may reuse an existing native dependency rather than introducing Tantivy solely for language alignment. [Core Cargo manifest, 13–30][H-core-cargo]
- Qt/QML, native agent/helper binaries, fonts, media/rendering dependencies, and optional dictation are independent of OpenWiki. Their footprint remains. Bundling the candidate cannot be compared against the **entire** Harlo install as if replacing the desktop stack. [Linux bundle][H-bundle] [Package dependencies][H-nfpm]
- Optional **context-mode MCP** still needs a suitable installed Node: `crates/harlo-core/src/mcp_catalog.rs:1–13,43–54` discovers system/version-managed Node and launches `server.mjs`. User-configured `npx` MCP servers and JavaScript/TypeScript language servers can also need Node. Removing bundled wiki Node therefore means “Node-free wiki backend,” not “every possible Harlo configuration is Node-free.” [MCP catalog][H-catalog] [TypeScript LSP setup][H-lsp]
- The browser Mermaid fallback and phone web client JavaScript are separate from Node runtime dependencies. Native wiki rendering already avoids Node; eliminating those JS assets is not a benefit of changing wiki engines. [Diagrams][H-diagrams]
- Candidate Cargo dependencies include Tantivy/Jieba, petgraph/petgraph-live, vendored libgit2 via `git2`, `notify`, rmcp, axum, Tokio, JSON Schema, and ACP. “One Rust binary” removes Node/module deployment but does not mean zero transitive/native dependencies, zero index storage, or verified static-link portability. [Candidate Cargo][L-cargo] [Candidate configuration/cache docs][L-config]

### Inferences

- The meaningful disk ceiling is roughly **210 MiB apparent** for complete replacement of this prepared runtime, minus the replacement binary/libraries and its own caches. Removing only the OpenWiki package saves about 2.2 MiB, not 210 MiB. [Measurements][E-results]
- A hybrid native read path can avoid starting a roughly 292 MiB sampled Node process for ordinary retrieval, while retaining Node for explicit generation. It does not remove Node from a bundle unless the writer runtime becomes optional/on-demand. This is an architectural opportunity, not a measured native implementation. [Server laziness][H-server] [UI fallback][H-ui-read] [Measurements][E-results]
- Remote gzip retention in the main process is a discrete optimization target independent of wiki backend replacement: streaming/releasing packaging bytes could remove that retained allocation after upload, while preserving the working engine. [Packing code][H-remote]

### Gaps

- No llm-wiki binary size/startup/RSS/latency was measured in this assignment; other researchers own its clone/deep audit. No language-based speed claim is justified.
- No large-wiki scaling, CPU-idle rate, PSS/private-dirty measurement, 30–60s GC-stable memory, concurrent-query load, or real remote transfer timing was measured. The two-page lookup is intentionally too small to justify a search-performance investment.
- No complete release-size delta, optional writer download UX, or headless transitive closure was built. Cached Node archive and old Harlo release files are not substitute measurements.

## 4. How compatible is llm-wiki, and what migration risks matter?

### Takeaway

llm-wiki is a plausible **optional native content/search engine**, but its documented interface is not a drop-in replacement for Harlo’s claim-aware repository workflow. Its external-agent model fits Harlo; page-level search, space registry, validation/ingest, typed graph, persistent indexes, and automatic-commit defaults need deliberate adapters. [README][L-readme] [Search contract][L-search] [Ingest][L-ingest]

### Cited Findings

- Candidate README describes 24 MCP tools, `llm-wiki serve`, optional HTTP/ACP, external LLM/skills, Git-backed Markdown, Tantivy search, and typed graph. No LLM/provider belongs inside the engine. This matches the **location of intelligence** in Harlo’s existing host integration, not its lifecycle protocol. [README][L-readme] [Harlo host manager][O-manager]
- Candidate `wiki_search` returns **page** refs such as `{slug,uri,title,score,confidence,excerpt}` plus facets; its documented score is BM25 × status multiplier × confidence. It lacks the documented OpenWiki source-path-priority/coverage/section ranking contract. `include_sections` refers to section index pages, not a guarantee of Markdown heading hits. [Candidate search][L-search] [Existing search implementation][O-retrieval]
- Candidate `wiki_content_read` returns a page, optionally with backlinks, rather than ordered requested heading sections. Its `wiki://<space>/<slug>` URI grammar differs from OpenWiki’s `page#anchor` plus separate `wiki` ID and `repo://source#Lx-Ly` evidence. [Content operations][L-content] [Content source][L-content-source] [OpenWiki schema][O-read-tools]
- Candidate supports adopting existing content through `wiki_root="openwiki"` / `spaces register --wiki-root openwiki`, so moving all pages to its default `wiki/` directory is not inherently necessary. Register is documented not to create repository files or commits; global registry/config remains a new machine-local responsibility. [Space management][L-spaces] [wiki.toml][L-wiki-toml]
- Configuration is global `~/.llm-wiki/config.toml` plus per-repository `wiki.toml`, with `--config`/`LLM_WIKI_CONFIG` overrides. Indexes and graph snapshots are persistent caches. OpenWiki’s linked registry instead resides at `~/.openwiki/wiki-workspaces.json` (or `OPENWIKI_CONFIG_DIR`) with stable wiki IDs, named overlapping workspaces, active selection, and shared-membership access rules. These are not the same as one global default candidate wiki. The default OpenWiki registry file was absent for this local user, but remote/overridden registries were not inspected. [Candidate configuration][L-config] [Existing home][O-home] [Existing workspace registry][O-workspaces]
- Candidate `wiki_ingest` validates, indexes, and **auto-commits by default**; its config can disable that. Harlo explicitly presents generated wiki files in Changes for user review/commit. A replacement must set `ingest.auto_commit=false`, avoid broad `content commit --all`, and test unrelated dirty files. Candidate normal ingest narrows validation to Git-changed pages; `index rebuild` is documented to validate none. [Candidate ingest][L-ingest] [Configuration][L-config] [Candidate content source][L-content-source] [Harlo UI, 363–364][H-panel]
- Candidate README says content write validates frontmatter; its detailed tool spec says write does not validate/index/commit, and inspected `src/ops/content.rs` forwards `content_write` to `markdown::write_page` without an explicit schema-validation step. Treat ingest, not the README shorthand, as the documented validation boundary; verify the complete write handler in the candidate audit. [README][L-readme] [Detailed contract][L-content] [Source][L-content-source]
- Candidate `serve --watch`/`watch` watches wiki Markdown and schema changes, debounces (500ms default), indexes without committing, and sends MCP resource notifications. It does **not** describe researching changed repository code or regenerating prose/claim evidence. Harlo’s current MCP client ignores unsolicited notifications; native visible-state polling and source-change offers need separate integration. [Watcher contract][L-watch] [Harlo transport][H-transport] [Harlo offers][H-offer-core]
- Candidate stale-page lint is age/confidence based by configuration; OpenWiki’s claim preflight and page manifest track source/evidence versions, source fingerprints, page hashes, completion run IDs, and retained/retracted claims. A native typed edge or a confidence value is not an equivalent replacement for those records. [Candidate configuration][L-config] [Manifest schema][O-manifest] [Run schema][O-run-state] [Claims workflow][O-skill]

#### Capability parity checklist and data risks

The “required” column below is an assessment of what is necessary to preserve existing Harlo behavior; candidate gaps mean **not established by the inspected public contract**, not a claim that every deeper implementation has been exhaustively audited.

| Required parity | Current evidence / candidate fit | Migration risk or acceptance check |
|---|---|---|
| Host-selected agent/model/auth | Already Harlo-owned; candidate external-agent architecture fits. [H-start][H-start] [L-readme][L-readme] | Do not introduce a second provider login or silently change models, budgets, remote defaults, or paid-call count. |
| Search exact sections | OpenWiki FTS5 heading units and ref/read protocol; candidate documented page BM25. [O-retrieval][O-retrieval] [L-search][L-search] | Adapter needs heading parser/anchors, snippet/ref generation, path hints, term coverage, limits, and quality comparison. Whole-page return increases context even if backend latency improves. |
| Read current source of truth | OpenWiki reparses Markdown on each search; candidate persistent indexes/watch. [O-retrieval][O-retrieval] [L-watch][L-watch] | Changed/deleted/renamed pages must not be stale after checkout, crash, or missed watch events; distinguish content bytes from index state. |
| Existing pages and metadata | `openwiki/`, arbitrary descriptive types, description/tags, producer/verification/source frontmatter, structural indexes. Candidate can change `wiki_root`, use custom types, and skip bare Markdown by default. [H-browser][H-browser] [O-skill][O-skill] [L-wiki-toml][L-wiki-toml] [L-ingest][L-ingest] | Preserve unknown frontmatter and content byte-for-byte during adoption; exclude structural/control files; no compulsory regeneration or semantic reformatting. |
| Claims and evidence | `.claims/<page>.json`, stable IDs, bounded `repo://` spans, evidence versions and sparse decisions. No matching claim-lifecycle API in candidate catalog. [O-skill][O-skill] [L-readme][L-readme] | Retain sidecars or implement an owned compatibility layer; schema-valid prose alone cannot preserve evidence/freshness. |
| Per-page manifest | `.page-manifest.json` contains `pageVersion`, `sourceFingerprint`, `gitHead`, `completedBy`, `completedRunId`. [O-manifest][O-manifest] | Losing it can discard partial coverage or turn focused updates into full regeneration. Never label imported evidence freshly verified merely because indexing succeeded. |
| Resumable planning/write queue | `.run.json`, UUID jobs, mode/phase, pending/skipped/complete, source snapshots, previous update, deferred claim pages. [O-run-state][O-run-state] | Existing interrupted runs need compatible continuation, explicit legacy completion, or a transparent deferred migration policy—not silent deletion. |
| Source drift and exclusions | Replan on source drift; `.openwikiignore` governs evidence and host-agent research; Harlo’s patch reconciles newly excluded evidence. [O-skill][O-skill] [H-patch][H-patch] | Candidate slug-index excludes are not equivalent to source-file evidence exclusions. Test new ignore rules, symlinks/traversal, source deletion/rename, and stale claims. |
| Graph/citations/editor links | Native Markdown-link graph and `repo://...#L...` opening; candidate typed graph uses frontmatter relations. [H-panel][H-panel] [H-sources][H-sources] [L-readme][L-readme] | Preserve old edges/anchors/citation ranges first; additional typed graph is optional functionality, not required UI replacement. |
| Workspace scoping | Many-to-many named workspaces, current/active/ambiguous choice, stable `wiki` ID; candidate named spaces/default/all. [O-workspaces][O-workspaces] [L-spaces][L-spaces] | Name collisions, directory moves, cross-repo authorization, and old refs require mapping. Do not replace a scoped workspace query with “search all registered spaces.” |
| User review and Git ownership | Harlo-generated pages remain in Changes; candidate ingest defaults auto-commit. [H-panel][H-panel] [L-config][L-config] | Zero unrelated staging/commits; no broad all-files commit; adoption must preserve dirty work and checkout structure. |
| Status/freshness/progress | Harlo JSON/phone format and native pollers read OpenWiki files. [H-ui-state][H-ui-state] [H-phone][H-phone] | Adapter should continue these DTOs or explicit version them; maintain distinction between completed, interrupted, behind, and unknown. |
| Cancellation/error reporting | Timeout is not cancellation; engine errors vs fallback/UI errors differ. [H-transport][H-transport] [H-ui-read][H-ui-read] | Test stop during write, crash/restart, delayed replies, malformed state, retry, and no false success; do not kill a shared engine to cancel one read. |
| Remote engines | SSH-root routing, versioned cache, same-arch check/native load smoke test. [H-remote][H-remote] | Candidate needs pinned release checksums, architecture selection, libc/target verification, atomic deployment, rollback/cache policy, and remote config/index placement. A local binary benchmark proves none of this. |
| File watching / scheduling | Current retrieval fresh per call; local merge offers, not automatic LLM generation; candidate watch only indexes content. [H-scheduler][H-scheduler] [O-retrieval][O-retrieval] [L-watch][L-watch] | Preserve no-unsolicited-token-spend behavior. File events and default-branch advancement are different inputs. |
| Normal agent context size | Two compact retrieval tools; writer gets eight tools. Candidate exposes 24 by default. [H-read-tools][H-read-tools] [H-writing][H-writing] [L-readme][L-readme] | Do not expose all candidate config/write/commit tools to normal project sessions; measure tool-schema token cost. |

### Inferences

- Lowest-risk adoption is a sidecar/read-only facade over existing Markdown with an isolated candidate config/index and no rewriting of OpenWiki-owned state. `wiki_root` helps this, but matching section search and stable IDs is still adapter work. [Candidate registration][L-spaces] [Existing retrieval][O-retrieval]
- Full native replacement can retain **Harlo’s current semantic generator** and replace deterministic machinery only. It need not adopt a new authoring skill merely because the engine changes language. But then Harlo must own or reproduce claim validation, checkpointing, finalization, and drift handling; llm-wiki’s generic primitives do not establish that these contracts are supplied. [Current lifecycle][O-skill] [Candidate tool catalog][L-readme]
- A hybrid has real deployment benefits only with a deliberate packaging policy. Shipping both full engines by default adds footprint and maintenance; making the native engine optional first is safer for validation but is not immediate Node removal. [Measured baseline][E-results] [Existing packaging][H-bundle]

### Gaps

- Candidate binary ABI/static-link properties, release artifact sizes, detailed schema preservation, slug edge cases, watcher recovery, and exact MCP cancellation/error behavior remain for the candidate deep audit and fixtures.
- No migration was performed. Preservation of claim sidecars, workspace IDs, source hashes, and interrupted runs remains unproven.
- Candidate documentation contains some README/detail drift (write validation is one concrete example). Prefer pinned source and fixture results over broad “single binary/no database” marketing shorthand.

## 5. What should Harlo invest in, and what would justify migration?

### Takeaway

**Keep the current working default. Fund a bounded 1–2 person-week fixture-based investigation, with packaging/native-read optimization as the first contenders. Do not commit to a full llm-wiki replacement yet.** The measured runtime cost is substantial enough to investigate, but removing Node does not by itself replace Harlo’s successful evidence lifecycle or prove faster/better generation. [Measurements][E-results] [Existing successful metadata][E-inspection] [Lifecycle][O-skill]

### Cited Findings

The implementation already separates host intelligence, deterministic lifecycle, and native UI. There is a large measurable runtime tree and a warm-service RSS cost, but the small lookup fixture is already about 4 ms warm and the simple MCP-entry shortcut gave no meaningful memory improvement. These observations favor a constrained architecture experiment over a language-led rewrite. [Session start][H-start] [Lifecycle][O-manager] [Native browser][H-browser] [Probe results][E-results]

### Inferences

#### Options and rough investment ranges

**Planning estimates, not measured development forecasts.** One experienced Harlo/Rust engineer, existing source/tests available, Linux first, two architecture targets for production remote support, no UI redesign, no new paid-model tuning program. Includes implementation, fixtures, packaging, and focused regression verification; excludes calendar delays waiting for upstream. Confidence decreases with lifecycle/migration scope. Ranges are alternative scopes, not additive commitments. Basis: the contracts and gaps mapped above. [Current adapter][H-call] [Existing lifecycle][O-skill] [Candidate scope][L-readme]

| Option | Rough effort | Expected benefit / scope | Confidence |
|---|---:|---|---|
| **Keep current behavior; improve packaging/service management** | **1–3 person-weeks** | Audit headless import closure, propose/upstream split or safe bundle pruning, reduce retained remote tar memory, consider idle service shutdown. Retains lifecycle and data; still Node-based. No promise that swapping the entrypoint alone helps RSS. | Medium for focused changes; low for exact savings until reachability/benchmark evidence. |
| **Native deterministic retrieval facade + existing writer** | **2–4 person-weeks** for local API parity; **4–6** including linked workspaces, remote transport, robust invalidation and product rollout | Keep Harlo generator plus OpenWiki write lifecycle; native search/read avoids Node for ordinary retrieval, reuses native readers/SQLite where appropriate. Node remains for generation. Optional writer delivery is a separate packaging decision. | Medium for local retrieval; medium-low for full edge-case parity. |
| **Optional llm-wiki backend for explicitly selected projects** | **3–6 person-weeks** for a constrained content/search backend and host tool adapter | Tests native engine operations and compatibility without displacing established wikis. Can target new knowledge spaces; evidence lifecycle remains legacy or explicitly limited. Shipping both by default is not a size win. | Medium-low; candidate ABI, index/watch semantics, and tooling still unmeasured here. |
| **Full native deterministic port retaining current host-generator behavior** | **8–14 person-weeks** | Reproduce queue, claims/evidence resolver/versioning, manifests, ignore/drift rules, finalization and remote distribution. Can remove Node without necessarily needing llm-wiki. Avoids assuming generic page operations replace evidence workflow. | Low-medium; requires differential lifecycle tests and maintenance commitment. |
| **Full llm-wiki replacement with current Harlo parity and migration** | **10–18 person-weeks** | Candidate search/storage engine plus Harlo-owned compatibility lifecycle, migration/rollback, UI/phone DTOs, workspaces, remote packaging. Largest risk of two semantics being maintained or silently dropped. | Low until candidate deep audit and POC; substantial unknowns remain. |

#### Recommended proof of concept: 1–2 person-weeks, no mandatory migration

1. **Freeze the baseline artifacts.** Use the exact VERSION/lock/patch above and recorded Harlo commit plus current source snapshots. Reuse the native fixture and UI contracts. Make isolated fixtures with 2, 50, 500, and 2,000 authored pages; include code citations, duplicate/non-ASCII headings, tables/code/diagrams, path hints, structural indexes, linked-workspace ambiguity, renamed/deleted files, stale/ignored evidence, and interrupted runs. Do not regenerate the real project. [Existing fixtures][H-tests] [Search contracts][O-retrieval] [Run schema][O-run-state]
2. **Compare alternatives at the same boundary.** Benchmark exact current MCP command, a genuinely dependency-isolated current engine if feasible, a native search/read facade, and pinned candidate binary. Use the same `openwiki_search/read` JSON facade and fixtures so result quality, tokens, and UI compatibility can be compared. Candidate internal page ranking alone is not an equivalent benchmark. [Current schema][H-read-tools] [Candidate schema][L-search]
3. **Use a deterministic scripted host to exercise generation lifecycle.** Supply prewritten fixture pages/claims, never a real LLM. Test begin/plan/next/submit/finish, source-drift replan, resume after failure at each persisted step, excluded evidence, and final metadata. Any candidate route without compatible claims/checkpoints should report the gap rather than fabricate a successful “equivalent” update. [Lifecycle sequence][O-skill] [Manifest/run persistence][O-manifest]
4. **Measure full costs.** Report stripped binary plus all runtime libraries/data, installed apparent and allocated bytes, remote gzip size, persistent cache/index size at each corpus size, startup-to-tools-ready, RSS and preferably PSS after 1s and 30s idle, cold/warm p50/p95 search/read, file-change visibility, and tokenized schema/result sizes. Distinguish disk cache from shipped payload and engine RSS from model/app memory. [Baseline measurement methodology][E-script]

Proposed **go/no-go thresholds** below are investment decision criteria, not results or external industry standards:

| Gate | Proposed threshold |
|---|---|
| Preservation | 100% pass for existing retained API/UI fixtures; no unrelated file writes, staging or commits; no silent loss/rewrite of pages, unknown frontmatter, claim IDs/evidence, manifests, citations, workspace identities, or interruption state. Legacy backend must reopen original data after abandoning the experiment. |
| Search usefulness | On at least 30–50 hand-curated repository questions across fixture corpora, relevant-section Recall@5/NDCG@5 at least 95% of baseline; exact targeted-section read correctness 100%. Result/schema token usage at most 110% of baseline unless explicitly justified by better retrieval. |
| Real Node-free deployment | For a full-replacement route, run create/update/resume/search/read with Node absent from PATH **and** no bundled Node; preserve existing host-agent model choice. A native read-only route should be labeled as such and demonstrate Node is not started for reading. |
| Size win | Full replacement payload target **≤55 MiB apparent and ≤20 MiB gzip**, including required runtime libraries/data, with index/cache costs reported separately. This is roughly a ≥74% apparent reduction from 210.261 MiB; do not count moving costs to an undeclared system dependency as removal. |
| Memory win | At matched corpus/idle conditions, aim for **≥60% lower measured engine PSS or consistently measured RSS** than the real baseline. Use 30s measurements/repeated runs before promising savings; the one-second 292 MiB sample is a starting point only. |
| Latency | No material warm-read regression (e.g. p95 within 20% at 50/500 pages), and cold tools-ready ≤300 ms on the same hardware if startup is claimed as a benefit. For large corpora, require a clear measured gain rather than a language-based assumption. |
| Freshness/reliability | Edited/deleted/renamed Markdown reflected within 1s under watch, and correct immediately after an explicit refresh/restart; no false “complete” or “up to date” when evidence is unresolved or a durable queue remains. |
| Remote | Successful isolated x64 and ARM64 deployment/round trip with pinned hashes and atomic failed-upload recovery; no user-wide config changes; bounded useful errors on incompatible binaries and disconnected hosts. |

**Decision rule:** pursue the cheapest option that passes the relevant preservation gates and demonstrates a meaningful measured benefit. If the native engine saves footprint but cannot preserve the evidence/update contract without a multi-month compatibility project, keep it optional or improve the current package. A large corpus-search win could justify a native read facade independently of any full writer migration. A simpler headless package may justify itself on supply-chain/distribution cost even if query latency stays similar. These are recommendations derived from the measured costs and source contracts, not claims that any option already passes. [Measurements][E-results] [Lifecycle requirements][O-skill]

### Gaps

High-priority questions for the coordinator/follow-on POC, in order:

1. **What is the primary desired outcome?** Smaller downloads/install, lower background memory, no Node on remote hosts, reduced release-maintenance burden, or faster large-wiki retrieval? Those goals select different options; this research establishes baseline cost, not their user priority.
2. **How much of the package closure is actually required by host-mode lifecycle?** The direct-entry experiment did not reduce RSS; deeper imports need profiling/pruning before estimating an achievable lean OpenWiki package.
3. **Does llm-wiki retain arbitrary OpenWiki frontmatter/sidecars under register/ingest/watch without mutation, and what are actual binary/cache/RSS costs on both target architectures?** Await the other researchers’ evidence, not their language choice.
4. **Can exact section/path/workspace retrieval and source-version claims be retained cheaply?** This is the decisive compatibility question; a faster page-search demo is insufficient.
5. **What are the required generation durability/error invariants under concurrent writers, cancellation, source changes, and crashes?** Capture current behavior with no-model scripted fixtures before redesigning single-active-run ownership.
6. **What installed/current release actually contains this feature?** Prepared development runtime and source packaging are verified; old installed/staged packages do not. A release build containing the pinned runtime is needed for true end-user download-size comparison.
7. **How should existing interrupted runs and linked workspace registries migrate?** No forced regeneration, no silent state deletion, explicit reversible adoption. The local default workspace registry was absent, but that does not establish absence on remote machines or other users’ installs.

[H-commit]: https://github.com/ArcticLatent/harlo/commit/563992a39e12a546d13170faa8d118719becd2ad
[H-agents]: file:///home/burce/Projects/harlo/AGENTS.md
[H-package]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/packaging/openwiki/package.json
[H-lock]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/packaging/openwiki/package-lock.json#L3356-L3410
[H-fetch]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/packaging/openwiki/fetch.sh#L23-L106
[H-node]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/packaging/openwiki/node.env
[H-patch]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/packaging/openwiki/patches/ignored-evidence-unresolved.patch
[H-bundle]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/packaging/linux/bundle.sh#L79-L114
[H-nfpm]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/packaging/linux/nfpm.yaml#L26-L112
[H-root]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L102-L113
[H-read-tools]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L50-L86
[H-answer]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L89-L100
[H-writing]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L115-L166
[H-runtime]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L171-L231
[H-telemetry]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L235-L248
[H-transport]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L254-L393
[H-server]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L395-L435
[H-call]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L443-L490
[H-remote]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L537-L585
[H-browser]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L590-L773
[H-state]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L776-L824
[H-offer-core]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L827-L923
[H-search]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L925-L967
[H-sources]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L975-L1103
[H-tests]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/openwiki.rs#L1251-L1581
[H-workspace]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/src/workspace.rs
[H-ui-read]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/src/workspace.rs#L864-L897
[H-scheduler]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/src/workspace.rs#L1391-L1400
[H-ui-state]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/src/workspace.rs#L2325-L2407
[H-start]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/src/workspace.rs#L2409-L2463
[H-offers]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/src/workspace.rs#L2466-L2537
[H-features]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/src/workspace.rs#L3087-L3098
[H-phone]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/src/workspace.rs#L6326-L6347
[H-tools]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/src/workspace.rs#L6870-L6939
[H-orchestration]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/src/orchestration.rs#L1-L128
[H-panel]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/qml/WikiPanel.qml#L1-L380
[H-diagrams]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/src/diagrams.rs#L1-L61
[H-e2e]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/qml/Main.qml#L8437-L8594
[H-phone-server]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-app/src/phone_server.rs#L1065-L1074
[H-swift]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/ios/HarloKit/Sources/HarloKit/ProjectFeatures.swift#L185-L490
[H-core-cargo]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/Cargo.toml#L13-L30
[H-catalog]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-core/src/mcp_catalog.rs#L1-L54
[H-lsp]: https://github.com/ArcticLatent/harlo/blob/563992a39e12a546d13170faa8d118719becd2ad/crates/harlo-agent/src/lsp.rs#L219
[N-package]: https://registry.npmjs.org/openwiki/0.9.0
[N-attestation]: https://registry.npmjs.org/-/npm/v1/attestations/openwiki@0.9.0
[O-head]: https://github.com/langchain-ai/openwiki/commit/41c92c4a00d6dadaa3769c6bf93023ed5a618076
[O-head-package]: https://github.com/langchain-ai/openwiki/blob/41c92c4a00d6dadaa3769c6bf93023ed5a618076/package.json
[O-cli]: file:///home/burce/Projects/harlo/target/openwiki/node_modules/openwiki/dist/cli/cli.js
[O-stdio]: file:///home/burce/Projects/harlo/target/openwiki/node_modules/openwiki/dist/integrations/mcp/stdio.js
[O-manager]: file:///home/burce/Projects/harlo/target/openwiki/node_modules/openwiki/dist/integrations/core/session-manager.js
[O-mcp]: file:///home/burce/Projects/harlo/target/openwiki/node_modules/openwiki/dist/integrations/mcp/server.js
[O-protocol]: file:///home/burce/Projects/harlo/target/openwiki/node_modules/openwiki/dist/integrations/core/protocol.js
[O-read-tools]: file:///home/burce/Projects/harlo/target/openwiki/node_modules/openwiki/dist/integrations/core/retrieval-tools.js
[O-retrieval]: file:///home/burce/Projects/harlo/target/openwiki/node_modules/openwiki/dist/retrieval/wiki.js
[O-lifecycle]: file:///home/burce/Projects/harlo/target/openwiki/node_modules/openwiki/dist/generation/repository-run.js
[O-manifest]: file:///home/burce/Projects/harlo/target/openwiki/node_modules/openwiki/dist/generation/page-manifest.js
[O-run-state]: file:///home/burce/Projects/harlo/target/openwiki/node_modules/openwiki/dist/generation/run-state.js
[O-workspaces]: file:///home/burce/Projects/harlo/target/openwiki/node_modules/openwiki/dist/linking/wiki-workspaces.js
[O-home]: file:///home/burce/Projects/harlo/target/openwiki/node_modules/openwiki/dist/config/openwiki-home.js
[O-skill]: file:///home/burce/Projects/harlo/target/openwiki/node_modules/openwiki/integrations/openwiki/SKILL.md
[L-head]: https://github.com/geronimo-iia/llm-wiki/commit/f42c5898877e2fefde54e29c4772ec3f4cde0feb
[L-cargo]: https://github.com/geronimo-iia/llm-wiki/blob/f42c5898877e2fefde54e29c4772ec3f4cde0feb/Cargo.toml
[L-readme]: https://github.com/geronimo-iia/llm-wiki/blob/f42c5898877e2fefde54e29c4772ec3f4cde0feb/README.md
[L-search]: https://github.com/geronimo-iia/llm-wiki/blob/f42c5898877e2fefde54e29c4772ec3f4cde0feb/docs/specifications/tools/search.md
[L-content]: https://github.com/geronimo-iia/llm-wiki/blob/f42c5898877e2fefde54e29c4772ec3f4cde0feb/docs/specifications/tools/content-operations.md
[L-content-source]: https://github.com/geronimo-iia/llm-wiki/blob/f42c5898877e2fefde54e29c4772ec3f4cde0feb/src/ops/content.rs
[L-ingest]: https://github.com/geronimo-iia/llm-wiki/blob/f42c5898877e2fefde54e29c4772ec3f4cde0feb/docs/specifications/tools/ingest.md
[L-watch]: https://github.com/geronimo-iia/llm-wiki/blob/f42c5898877e2fefde54e29c4772ec3f4cde0feb/docs/specifications/engine/watch.md
[L-spaces]: https://github.com/geronimo-iia/llm-wiki/blob/f42c5898877e2fefde54e29c4772ec3f4cde0feb/docs/specifications/tools/space-management.md
[L-config]: https://github.com/geronimo-iia/llm-wiki/blob/f42c5898877e2fefde54e29c4772ec3f4cde0feb/docs/guides/configuration.md
[L-wiki-toml]: https://github.com/geronimo-iia/llm-wiki/blob/f42c5898877e2fefde54e29c4772ec3f4cde0feb/docs/specifications/model/wiki-toml.md
[E-script]: file:///tmp/opencode/harlo-wiki-integration-probe.py
[E-results]: file:///tmp/opencode/harlo-wiki-probe-5elgy6h6/results.json
[E-inspection]: file:///tmp/opencode/harlo-wiki-probe-5elgy6h6/inspection.json
