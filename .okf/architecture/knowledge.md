---
type: Architecture
title: Code indexing and source-backed knowledge
description: How the code index supplies declarations and call evidence, and how the OKF wiki tracks ownership, validates citations, stamps pages, and detects changes.
covers:
  - crates/harlo-okf/
  - crates/harlo-index/
generated: { by: harlo/gpt-6-luna, at: "2026-11-19T18:56:17Z" }
sources:
  - id: index-lib
    resource: "repo://crates/harlo-index/src/lib.rs"
  - id: okf-lib
    resource: "repo://crates/harlo-okf/src/lib.rs"
  - id: scan
    resource: "repo://crates/harlo-index/src/scan.rs"
  - id: defs
    resource: "repo://crates/harlo-index/src/defs.rs"
  - id: rules
    resource: "repo://crates/harlo-index/src/rules.rs"
  - id: text
    resource: "repo://crates/harlo-index/src/text.rs"
  - id: index
    resource: "repo://crates/harlo-index/src/index.rs"
  - id: named
    resource: "repo://crates/harlo-index/src/lang.rs"
    title: fn named
  - id: structural
    resource: "repo://crates/harlo-index/src/structural.rs"
    title: "fn run::search"
  - id: okf-structural
    resource: "repo://crates/harlo-okf/src/indexed.rs"
    title: fn structural
  - id: search-type
    resource: "repo://crates/harlo-index/src/structural.rs"
    title: struct Search
  - id: found-type
    resource: "repo://crates/harlo-index/src/structural.rs"
    title: struct Found
  - id: rewritten-type
    resource: "repo://crates/harlo-index/src/structural.rs"
    title: struct Rewritten
  - id: structural-type
    resource: "repo://crates/harlo-index/src/structural.rs"
    title: struct Structural
  - id: manifests
    resource: "repo://crates/harlo-index/src/manifests.rs"
  - id: packages
    resource: "repo://crates/harlo-index/src/packages.rs"
  - id: graph
    resource: "repo://crates/harlo-index/src/graph.rs"
  - id: lookup
    resource: "repo://crates/harlo-index/src/lookup.rs"
  - id: index-brief
    resource: "repo://crates/harlo-index/src/brief.rs"
  - id: main
    resource: "repo://crates/harlo-index/src/main.rs"
  - id: index-cargo
    resource: "repo://crates/harlo-index/Cargo.toml"
  - id: page
    resource: "repo://crates/harlo-okf/src/page.rs"
  - id: cite
    resource: "repo://crates/harlo-okf/src/cite.rs"
  - id: indexed
    resource: "repo://crates/harlo-okf/src/indexed.rs"
  - id: units
    resource: "repo://crates/harlo-okf/src/units.rs"
  - id: lex
    resource: "repo://crates/harlo-okf/src/lex.rs"
  - id: scope
    resource: "repo://crates/harlo-okf/src/scope.rs"
  - id: ledger
    resource: "repo://crates/harlo-okf/src/ledger.rs"
  - id: work
    resource: "repo://crates/harlo-okf/src/work.rs"
  - id: check
    resource: "repo://crates/harlo-okf/src/check.rs"
  - id: stamp
    resource: "repo://crates/harlo-okf/src/stamp.rs"
  - id: bundle
    resource: "repo://crates/harlo-okf/src/bundle.rs"
  - id: view
    resource: "repo://crates/harlo-okf/src/view.rs"
  - id: map
    resource: "repo://crates/harlo-okf/src/map.rs"
  - id: brief
    resource: "repo://crates/harlo-okf/src/brief.rs"
  - id: split
    resource: "repo://crates/harlo-okf/src/split.rs"
  - id: validate
    resource: "repo://crates/harlo-okf/src/validate.rs"
  - resource: "repo://crates/harlo-okf/"
  - resource: "repo://crates/harlo-index/"
---
# Code indexing and source-backed knowledge

`harlo-index` extracts declarations, imports, calls, and package dependencies; `harlo-okf` turns repository-backed Markdown into a wiki whose evidence can be checked against the working tree. The boundary is deliberately mechanical: hashes identify changed evidence, locators find declarations, and coverage identifies documentation work; these mechanisms do not establish that prose is true. The separate index helper carries the language grammars so wiki consumers need not embed them.[^index-lib][^okf-lib][^indexed][^work]

Navigation: [project overview](/overview.md), [agent architecture](/architecture/agent.md), [agent tools](/architecture/agent-tools.md), and [desktop architecture](/architecture/desktop.md).

## Ownership and data flow

The two crates divide responsibilities as follows.[^index-lib][^okf-lib]

| Layer | Responsibility | Source |
| --- | --- | --- |
| Index extraction | `lang`, `rules`, `defs`, and `scan` recognize files, grammar nodes, declarations, imports, calls, comments, and test code; `text` handles YAML and SQL. | [^scan][^defs][^rules][^text] |
| Repository index | `index` walks the repository, caches per-file results, and reads manifests; `manifests` and `packages` model dependencies and package reachability. | [^index][^manifests][^packages] |
| Code navigation | `graph`, `lookup`, and index `brief` provide heuristic call edges, compact lookup answers, repository maps, and declaration cards. | [^graph][^lookup][^index-brief] |
| Wiki evidence | `page` parses front matter and citations; `cite` resolves locators; `indexed` adapts helper symbols to `units`, with the `units`/`lex` scanner as fallback. | [^page][^cite][^indexed][^units][^lex] |
| Wiki maintenance | `scope`, `ledger`, and `work` determine ownership and changes; `check` validates structure; `stamp` records accepted evidence and maintains generated files. | [^scope][^ledger][^work][^check][^stamp] |
| Presentation and author context | `bundle`, `view`, `map`, `brief`, and `split` supply bundle loading, JSON views/search, wiki navigation maps, writer context, and suggested page groups; `validate` provides a non-stamping check. | [^bundle][^view][^map][^brief][^split][^validate] |

The normal evidence path is **working-tree bytes → indexed symbols or fallback units → resolved footnotes → ledger comparison → work list**. Writing adds an explicit acceptance/stamping step; rendering instead exposes current locations, signatures when available, and footnote states without asking a model.[^indexed][^work][^stamp][^view]

## Repository indexing

### Extraction and representation

`FileIndex` contains a language plus ordered symbols, imports, and calls. A `Symbol` has a qualified name, kind, inclusive 1-based line span, signature, optional first documentation line, visibility/test flags, parent index, and hash. Imports retain their text and line; calls retain the short callee name, optional receiver text, 1-based line, 0-based byte column, and enclosing symbol index. These are source observations, not compiler-resolved types.[^index-lib][^graph]

The grammar-backed languages are Rust, Python, Go, JavaScript, TypeScript/TSX, Java, C, C++, C#, Ruby, PHP, Swift, Kotlin, Scala, Julia, Bash, Lua, and QML. YAML is indexed as mapping keys two levels deep; SQL recognizes `CREATE` declarations, including tables/views, functions/procedures/triggers, types, and schemas. YAML/SQL indexes contain no imports or calls. Language names accepted by the helper also include common aliases such as `py`, `ts`, `c++`, and `shell`.[^index-lib][^text][^named]

The optional structural search accepts code-shaped patterns with metavariables, parses them separately for each selected language, and can return proposed rewrites without writing files. Its search describes pattern, rewrite, language, paths and result limit; matches retain source positions/text and optional replacement, while rewritten files carry the full proposed text. It excludes comments and strings by matching syntax nodes; invalid patterns are reported per language, while rewrites return changed file text for the caller to apply.[^search-type][^found-type][^rewritten-type][^structural-type][^structural][^okf-structural]

`scan` reuses a common syntax-tree walk with language-specific declaration and call rules. Tree-sitter recovery can produce useful results from incomplete code rather than requiring a successful build. Signatures are capped at 240 characters, documentation lines at 200, and receivers at 80; a card is consequently a navigation aid rather than a complete source listing.[^scan][^defs][^rules]

The fallback scanner additionally handles Markdown headings and TOML tables, and recognizes common forms of Rust, Go, Python, JavaScript/TypeScript, Swift, C/C++/Objective-C headers, and QML. Its lexer masks comments and literals while preserving byte/line positions, then declaration scanning uses braces or indentation. This is not a full grammar: uncommon literal forms can distort declaration boundaries.[^units][^lex]

### Walking and caching

`index` canonicalizes the repository root and walks current disk files with ignore rules enabled and hidden entries skipped. Unsupported languages, vendored/build output, minified names, and source files over 1 MiB are excluded. Unlike wiki ownership scope, the index does not generally exclude test files; test flags matter later to navigation and graph filtering. Recognized manifests are collected separately, before the source-size filter.[^index][^scope][^rules]

The cache defaults to `$XDG_CACHE_HOME/harlo/index`, falling back to `~/.cache/harlo/index`. A repository's canonical path determines its cache filename; schema and crate version invalidate incompatible cache contents. Up to eight workers reuse parsers. Every candidate source file is still read and hashed: an unchanged file avoids **parsing**, not disk reading. Results can also be reused by content hash after a rename, and `Index::parsed` counts newly parsed files.[^index]

Cache writes use a temporary file and rename; write failures are intentionally nonfatal. Unreadable source files and non-UTF-8 source contents can be skipped. Manifests are read again independently of the cached source indexes. `--no-cache` disables both cache loading and saving in the helper commands that accept it.[^index][^main]

### Manifests and package boundaries

Manifest readers cover Cargo, npm, Python, Go, Maven, Gradle, Ruby, Composer, SwiftPM, Dart, and Mix. Dependencies retain requested version text, development/build status, source line, and an optional local path. These are small format-specific readers, not complete package-manager implementations; unsupported constructs are omitted. Cargo dependencies lacking a version/local path can inherit matching root-workspace dependency information during repository indexing.[^manifests][^index-lib][^index]

A file belongs to its nearest manifest folder. Local dependencies resolve first by path, then by normalized name; package reachability is transitive and includes the package itself. A missing package assignment is permissive rather than an isolation boundary. Import-name matching supplies the dependency-user lists exposed by `deps`.[^packages][^main]

### Call graph and lookup limits

`Graph` resolves names on demand. Receiver calls first consider the caller's own type, then named/inferred types, then module names; likely external type/module calls are omitted, and variable/field names can yield possible method matches. Bare calls consider implied `this` where appropriate, the caller's file, imports, language-specific folder scope, and finally repository-wide scope only for languages with global names. With packages attached, candidates must also be reachable through package dependencies.[^graph]

The first narrowing step yielding candidates decides the edge set. Edges carry `sure`; ambiguous possibilities are retained only within the graph's candidate limit of eight. Bound local callable names are suppressed by scanning, and production callers cannot reach test declarations. “Sure” therefore means resolved by these heuristics, **not** verified by a language server or compiler; missing edges do not prove that no runtime call exists.[^graph][^scan]

`lookup` accepts a full/short name and optional file/folder scope, tolerates `::` versus `.`, suppresses redundant `impl` cards, and suggests nearby names when no exact declaration is found. It prints signatures, doc lines, callers/callees, and `maybe` candidates, with test callers counted separately. Caller depth is clamped to 1–3 and long outputs are bounded: at most five full cards and forty caller-tree lines.[^lookup]

The helper exposes `version`, `file`, `tree`, `repo`, `symbols`, `deps`, `sites`, `lookup`, `map`, and `cards`. Most data commands produce JSON; `sites` emits JSON lines, while lookup/map are text. The default `scan` feature enables parsers and the executable; disabling it retains the shared types and non-scanning navigation modules without the grammar dependencies.[^main][^index-cargo][^index-lib]

## Wiki pages and source evidence

A bundle lives in `.okf/` and follows OKF 0.2. Markdown files are concepts except reserved `index.md`/`log.md` and hidden entries. Root/folder indexes are derived from page metadata, with draft/deprecated grouping; the log uses newest-first dated headings. The hidden `.okf/.harlo/ledger.json` travels with the repository but is not a concept page.[^bundle][^ledger]

Authors own `type`, `title`, `description`, optional `tags`/`status`, `covers`, and the body. Stamping owns `generated`, `sources`, and ledger updates; explicit review updates `verified`. Unknown front-matter keys are preserved. The default status is stable for the page model; deprecated pages are excluded from active coverage/change work while their previous claims are retained.[^page][^stamp][^work]

A source footnote has the form `[^label]: repository/path.rs, fn Type::method`. The path is repository-relative, not relative to the page; optional `repo://`, leading `./`, leading `/`, and surrounding code marks are normalized. A file without a locator is whole-file evidence; a trailing `/` identifies a folder. Footnote references/definitions inside fenced code and references inside inline code are ignored by the page parser.[^page][^work]

Locators may name a kind (`fn`, `type`, `impl`, `property`, `signal`, `component`, `id`, `section`, `table`, and aliases) or just a name. Matching normalizes `::` to `.`, strips code marks/trailing `()`, and tries exact qualified names before suffix names; Markdown headings additionally allow a case-insensitive prefix. Production declarations are preferred over tests when several fit. Ambiguity is reported with candidates, so qualifying a name is safer than relying on a short name such as `new`.[^cite]

Folder coverage and source evidence are different contracts. A folder citation records no content hash, and owning a file does not assert that every declaration is explained. A whole-file citation detects any byte change but does not mark each declaration as individually cited. A declaration citation narrows invalidation to that declaration's extracted own text.[^work][^ledger][^units]

Wiki links such as [overview](/overview.md) resolve from the **bundle root**, while relative links resolve from the page's folder. Links may carry fragments/queries; missing link targets produce notes rather than stopping a stamp. Source footnotes should not be confused with these navigation links.[^page][^check]

## Declaration hashes and invalidation

A declaration hash covers its own lines, excluding nested declaration lines, blank lines, indentation at line edges, and ordinary comments. Documentation comments remain evidence. Editing a method therefore need not invalidate a claim about its containing type; editing a plain comment normally does not invalidate a declaration claim. This is line-based normalization, not semantic equivalence: internal code changes and doc changes still count, and a file-level citation remains byte-sensitive. Hashes are 16 hex digits derived from SHA-256.[^index-lib][^scan][^units][^work]

The ledger stores four independent records: page/label → resolved claim and evidence hash; file → whole-file hash and compact notable-name snapshot; exclusion key → reason; and page → last stamped file hash. Each claim also retains a whole-file hash as a fast path. Older comment-inclusive declaration hashes are accepted through the `legacy` hash compatibility path.[^ledger][^units][^work]

`work` compares these records to uncommitted working-tree contents and emits unowned files, new notable declarations, stale footnotes, unresolved/ambiguous locators, and absent paths. Unchanged file hashes avoid rescanning already-resolved claims. A changed declaration with the same identity becomes stale; a newly introduced or retargeted citation establishes new evidence rather than comparing unrelated declarations.[^work]

Stale claims retain their old accepted evidence until explicitly accepted. Files with outstanding new declarations likewise keep their prior snapshots, preventing a second check from silently forgetting the work. `Work::accept` advances only stale claims, either for one label or a whole page; it does not resolve ambiguity, missing evidence, or uncited new declarations.[^work]

When a previously resolved declaration disappears, the full work pass searches in-scope code for exactly one matching **short name plus current or legacy hash**. An unambiguous move is queued for automatic footnote rewriting by `Work::record`; it can survive a changed enclosing type name or file path, but not an arbitrary short-name change or simultaneous content edit. A pure move does not make `Work::is_empty` false. The cheaper `page_items` check does not perform this repository-wide relocation search.[^work]

## Coverage rules

Coverage uses current disk files, ignore rules, and a 1 MiB limit. Known languages come from the union of the fallback scanner and index language table, regardless of helper installation. Tests/fixtures, hidden entries, vendored/generated/minified paths, licenses/notices/changelogs, agent-instruction files, and the wiki itself are excluded by naming rules. Markdown, TOML, and YAML can supply evidence without needing an owning page; recognized code, including SQL, does need an owner or an explicit exclusion.[^scope]

For each live page, explicit `covers` paths are considered before cited paths. Within either set, the longest matching path wins: an exact file or more specific folder beats a broader folder. Actual directories are treated as folders even if the author omitted `/`; writing the slash makes the intended contract clear. Equal-specificity ownership retains the first match, so overlapping owners are not a way to require several pages to update together.[^work]

A notable declaration is a **public, non-test function, type, module, macro, or QML component**. Constants, properties, signals, object IDs, `impl` blocks, and private/test declarations can be cited but do not independently create new-declaration obligations. Newness uses a name snapshot, not semantic/API compatibility: renamed public declarations can appear new. A declaration is discharged by an exact resolved citation anywhere in the live wiki or by a matching leave decision.[^ledger][^work]

`leave` accepts a file, trailing-slash folder, or `path#Declaration` plus a nonempty reason. File/folder exclusions suppress ownership/new-declaration work; declaration exclusions suppress that declaration's obligation. The stamp implementation checks that the path exists but does not verify that the declaration suffix actually exists, so callers must supply the full correct name. Excluding code does not automatically excuse a broken citation that still points at it.[^ledger][^stamp][^work]

Unowned files are one work item each rather than one per declaration. Large uncovered sets are grouped for authors: `split` targets roughly 6,000 source lines per proposed page, recursively splits folders, and clusters oversized leaf groups by filename prefix. These are suggestions, not enforced page boundaries.[^work][^split]

## Validation, stamping, and review

Structural `check` distinguishes three levels. **Errors** include missing/unreadable front matter or `type`, invalid reserved-file structure, duplicate/undefined/malformed footnotes, and `covers` paths containing `..`. **Warnings** include absent title/description, unknown status, malformed timestamp shape, unused footnotes, and mostly unsourced prose. **Notes** include missing navigation targets and indexes needing regeneration. The prose heuristic warns only with at least four counted blocks and a cited share below 0.34; it is not a claim-by-claim proof checker.[^check]

`validate::validate` loads the bundle and committed ledger, computes structural problems and work, and returns text/JSON reports without stamping pages, recording moves, or updating the ledger. Ordinary mode fails on structural errors; strict mode also fails when work remains. Warnings/notes alone do not fail either mode, and a move-only result can pass strict mode because moves do not count as outstanding work.[^validate][^work]

`stamp::run`, used by `wiki_check` and the JSON `check` operation, is **not** that read-only path. It applies leave decisions, loads/checks pages, computes work, and accepts requested stale-label confirmations. With a writer, each page differing from its last stamped hash is eligible: page-local structural errors or unresolved/ambiguous/missing cited evidence hold it back. An eligible edited page accepts all its stale claims and receives generated identity/time plus regenerated sources.[^stamp][^view]

With a verifier, explicitly named reviewed pages pass the same blockers, accept stale evidence, and receive that verifier's latest `verified` entry without replacing the writer identity. Recording fixes moved footnotes and advances accepted ledger snapshots; stamping logs creation/update/deprecation events and regenerates indexes. Without a writer, generated stamps are not added, but ledger/index updates and automatic move repairs can still occur. Missing covers and outstanding new-declaration work are not the same blockers as broken footnotes: a stamped page can still owe work.[^stamp][^work][^bundle]

Generated `sources` entries come from footnotes, using the label as `id`, `repo://` paths as resources, and locators as titles; uncited covers are added as resource-only entries. Authors should repair body footnotes and coverage, then let the stamp regenerate these fields rather than editing the generated evidence list independently.[^stamp][^page]

## Helper integration and reader-facing views

`indexed` finds the helper through `HARLO_INDEX` (`off` disables it), executable-adjacent/install candidates, or build-qualified copies beside a remote agent. It runs `symbols` once when a declaration set is loaded, drains stdout/stderr concurrently, and enforces a 60-second limit. Missing/failed helpers fall back to local scanning; files absent from a successful helper result also fall back. Explicit lookup instead reports helper absence/failure rather than pretending to have call-graph results.[^indexed]

`view::answer` supplies the same JSON interface for local calls and remote `harlo-agent wiki --okf` requests: state, page, search, page report, out-of-date pages, map, lookup, and mutating check. Page data includes numbered source links, current declaration spans/signatures, backlinks, edit state, and work descriptions. Footnotes are marked current/stale/missing/gone; trust tier is separate, based on `verified` entries (`human:` wins over machine verification, and no verification means unverified). A generated stamp alone is not human review.[^view]

The wiki map is built from live pages and their section citations, not from a model. It fits a 12,000-byte budget by dropping declaration detail, then sections, then truncating at line boundaries. The code map is different: it summarizes packages, entry points, and frequently called declarations from the index. Writer briefs default to the creation map; `HARLO_WIKI_BRIEF=all` also enables update cards and pages citing callers, while `off` disables briefs. Those caller-based affected-page hints are advisory, not transitive hash invalidation.[^map][^index-brief][^brief][^work]

## Pitfalls and diagnostic boundaries

- **A clean work list is not complete documentation.** Existing file snapshots, narrow notable-kind rules, broad folder ownership, and intentionally excluded code all limit what is requested. Review the prose and evidence even when the mechanical check is current.[^ledger][^scope][^work]
- **A type citation is not a citation of every method.** Nested bodies are omitted from the parent's hash; cite the operation responsible for a behavioral claim, or consciously accept whole-file invalidation.[^units][^work]
- **A helper switch can change extraction.** Fallback languages and declaration boundaries are not identical to the grammar index. Also, `validate` reports helper discoverability, not proof that its latest execution succeeded; declaration loading can silently fall back.[^indexed][^validate][^lex]
- **Web footnotes have a boundary mismatch.** Structural checking permits HTTP(S) definitions, but the work pass still attempts repository-path reads for non-folder citations; it does not fetch remote evidence. Do not assume that an accepted URL syntax is automatically stampable source evidence.[^check][^work]
- **Folder citations are ownership, not recursive content fingerprints.** Their claims have empty hashes, and the work pass takes the trailing-slash branch before checking existence; use covers for missing-folder reporting and file/declaration footnotes for change-sensitive claims.[^work][^ledger]
- **Repeated `impl` blocks are not aggregated evidence.** Locator resolution collapses same-name/same-kind hits to the first declaration; cite the particular methods when their implementation matters.[^cite]
- **Do not use a normal check as a dry run.** `stamp::run` may write even without a writer; use `validate` when generated pages, indexes, log, and ledger must remain untouched. The helper may still maintain its external index cache during a validation pass.[^stamp][^validate][^indexed][^index]

[^index-lib]: crates/harlo-index/src/lib.rs
[^okf-lib]: crates/harlo-okf/src/lib.rs
[^scan]: crates/harlo-index/src/scan.rs
[^defs]: crates/harlo-index/src/defs.rs
[^rules]: crates/harlo-index/src/rules.rs
[^text]: crates/harlo-index/src/text.rs
[^index]: crates/harlo-index/src/index.rs
[^named]: crates/harlo-index/src/lang.rs, fn named
[^structural]: crates/harlo-index/src/structural.rs, fn run::search
[^okf-structural]: crates/harlo-okf/src/indexed.rs, fn structural
[^search-type]: crates/harlo-index/src/structural.rs, struct Search
[^found-type]: crates/harlo-index/src/structural.rs, struct Found
[^rewritten-type]: crates/harlo-index/src/structural.rs, struct Rewritten
[^structural-type]: crates/harlo-index/src/structural.rs, struct Structural
[^manifests]: crates/harlo-index/src/manifests.rs
[^packages]: crates/harlo-index/src/packages.rs
[^graph]: crates/harlo-index/src/graph.rs
[^lookup]: crates/harlo-index/src/lookup.rs
[^index-brief]: crates/harlo-index/src/brief.rs
[^main]: crates/harlo-index/src/main.rs
[^index-cargo]: crates/harlo-index/Cargo.toml
[^page]: crates/harlo-okf/src/page.rs
[^cite]: crates/harlo-okf/src/cite.rs
[^indexed]: crates/harlo-okf/src/indexed.rs
[^units]: crates/harlo-okf/src/units.rs
[^lex]: crates/harlo-okf/src/lex.rs
[^scope]: crates/harlo-okf/src/scope.rs
[^ledger]: crates/harlo-okf/src/ledger.rs
[^work]: crates/harlo-okf/src/work.rs
[^check]: crates/harlo-okf/src/check.rs
[^stamp]: crates/harlo-okf/src/stamp.rs
[^bundle]: crates/harlo-okf/src/bundle.rs
[^view]: crates/harlo-okf/src/view.rs
[^map]: crates/harlo-okf/src/map.rs
[^brief]: crates/harlo-okf/src/brief.rs
[^split]: crates/harlo-okf/src/split.rs
[^validate]: crates/harlo-okf/src/validate.rs
