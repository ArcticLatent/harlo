---
type: Architecture
title: Agent tools, sandbox and permissions
description: The distinct safety boundaries around file changes, subprocesses, approvals and tool output.
covers:
  - crates/harlo-agent/src/tools/
  - crates/harlo-agent/src/sandbox.rs
  - crates/harlo-agent/src/danger.rs
  - crates/harlo-agent/src/patch.rs
  - crates/harlo-agent/src/hashline.rs
  - crates/harlo-agent/src/generated.rs
  - crates/harlo-agent/src/program.rs
  - crates/harlo-agent/src/jobs.rs
  - crates/harlo-agent/src/sys.rs
generated: { by: harlo/gpt-6-luna, at: "2026-11-19T18:59:56Z" }
sources:
  - id: dispatch
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: fn run_calls
  - id: files
    resource: "repo://crates/harlo-agent/src/tools/mod.rs"
  - id: sandbox
    resource: "repo://crates/harlo-agent/src/sandbox.rs"
  - id: startup
    resource: "repo://crates/harlo-agent/src/main.rs"
  - id: danger
    resource: "repo://crates/harlo-agent/src/danger.rs"
  - id: approvals
    resource: "repo://crates/harlo-agent/src/rpc.rs"
  - id: bash
    resource: "repo://crates/harlo-agent/src/tools/bash.rs"
  - id: edits
    resource: "repo://crates/harlo-agent/src/tools/edit.rs"
  - id: hashline
    resource: "repo://crates/harlo-agent/src/hashline.rs"
  - id: patch-apply
    resource: "repo://crates/harlo-agent/src/tools/mod.rs"
    title: "fn Tools::patch"
  - id: patch
    resource: "repo://crates/harlo-agent/src/patch.rs"
  - id: generated
    resource: "repo://crates/harlo-agent/src/generated.rs"
  - id: sys
    resource: "repo://crates/harlo-agent/src/sys.rs"
  - id: jobs
    resource: "repo://crates/harlo-agent/src/jobs.rs"
  - id: outline
    resource: "repo://crates/harlo-agent/src/outline.rs"
  - id: fetch
    resource: "repo://crates/harlo-agent/src/fetch.rs"
  - id: images
    resource: "repo://crates/harlo-agent/src/images.rs"
  - id: formats
    resource: "repo://crates/harlo-agent/src/tools/formats.rs"
    title: fn text
  - id: selector
    resource: "repo://crates/harlo-agent/src/tools/formats.rs"
    title: fn selector
  - id: range
    resource: "repo://crates/harlo-agent/src/tools/formats.rs"
    title: enum Range
  - id: kind-type
    resource: "repo://crates/harlo-agent/src/tools/formats.rs"
    title: enum Kind
  - id: kind
    resource: "repo://crates/harlo-agent/src/tools/formats.rs"
    title: fn kind
  - id: program
    resource: "repo://crates/harlo-agent/src/program.rs"
  - id: lsp
    resource: "repo://crates/harlo-agent/src/lsp.rs"
  - id: rename-spec
    resource: "repo://crates/harlo-agent/src/lsp.rs"
    title: fn rename_spec
  - id: rename
    resource: "repo://crates/harlo-agent/src/lsp.rs"
    title: "fn Servers::rename"
  - id: lookup
    resource: "repo://crates/harlo-agent/src/lookup.rs"
  - id: ast
    resource: "repo://crates/harlo-agent/src/ast.rs"
    title: fn find
  - id: ast-on
    resource: "repo://crates/harlo-agent/src/ast.rs"
    title: fn on
  - id: ast-offered
    resource: "repo://crates/harlo-agent/src/ast.rs"
    title: fn offered
  - id: ast-grep-spec
    resource: "repo://crates/harlo-agent/src/ast.rs"
    title: fn grep_spec
  - id: ast-edit-spec
    resource: "repo://crates/harlo-agent/src/ast.rs"
    title: fn edit_spec
  - id: aside
    resource: "repo://crates/harlo-agent/src/tools/bash.rs"
    title: struct Aside
  - id: adopt
    resource: "repo://crates/harlo-agent/src/jobs.rs"
    title: "fn Jobs::adopt"
  - id: ready
    resource: "repo://crates/harlo-agent/src/jobs.rs"
    title: "fn Jobs::ready"
  - id: room
    resource: "repo://crates/harlo-agent/src/jobs.rs"
    title: "fn Jobs::has_room"
  - id: glob-rule
    resource: "repo://crates/harlo-agent/src/rules.rs"
    title: struct GlobRule
  - id: glob-matches
    resource: "repo://crates/harlo-agent/src/rules.rs"
    title: "fn GlobRule::matches"
  - id: glob-rules
    resource: "repo://crates/harlo-agent/src/rules.rs"
    title: fn glob_rules
  - id: glob
    resource: "repo://crates/harlo-agent/src/rules.rs"
    title: fn glob
  - id: debug-on
    resource: "repo://crates/harlo-agent/src/debug.rs"
    title: fn on
  - id: debug-spec
    resource: "repo://crates/harlo-agent/src/debug.rs"
    title: fn spec
  - id: debugger
    resource: "repo://crates/harlo-agent/src/debug.rs"
    title: struct Debugger
  - id: debug-call
    resource: "repo://crates/harlo-agent/src/debug.rs"
    title: "fn Debugger::call"
  - id: locate-on
    resource: "repo://crates/harlo-agent/src/locate.rs"
    title: fn on
  - id: locate-spec
    resource: "repo://crates/harlo-agent/src/locate.rs"
    title: fn spec
  - id: window
    resource: "repo://crates/harlo-agent/src/locate.rs"
    title: struct Window
  - id: words
    resource: "repo://crates/harlo-agent/src/locate.rs"
    title: fn words
  - id: windows
    resource: "repo://crates/harlo-agent/src/locate.rs"
    title: fn windows
  - id: question
    resource: "repo://crates/harlo-agent/src/locate.rs"
    title: fn question
  - id: hits
    resource: "repo://crates/harlo-agent/src/locate.rs"
    title: fn hits
  - id: locate-files
    resource: "repo://crates/harlo-agent/src/locate.rs"
    title: fn files
  - id: refused
    resource: "repo://crates/harlo-agent/src/locate.rs"
    title: fn refused
  - id: keywords
    resource: "repo://crates/harlo-agent/src/locate.rs"
    title: fn keywords
  - resource: "repo://crates/harlo-agent/src/tools/"
---
# Agent tools, sandbox and permissions

The safety model has separate layers: the dispatcher decides which tools a caller may use and when to ask; file tools check visibility and stale content; bubblewrap constrains ordinary subprocesses. An approval is not a replacement for filesystem confinement, and confinement does not protect project files from every destructive command.[^dispatch][^files][^sandbox][^danger]

See [execution](/architecture/agent.md) for batching and cancellation, and [orchestration](/workflows/agent-orchestration.md) for MCP's separate trust boundary.

## Confinement must fail closed

Session startup configures the sandbox on by default, with optional off/no-network modes and user-level `sandbox.json` additions. Repository configuration cannot loosen that policy. Bubblewrap starts with a read-only root, opens the project, temporary storage and caches for writes, and overlays hidden credential/session locations. Mounts are ordered shallow-to-deep so a secret nested inside a writable cache stays hidden, while a designated worktree nested inside hidden agent storage can be exposed.[^sandbox][^startup]

File tools mirror this view in process: paths are cleaned and resolved through existing ancestors, then checked for hidden and writable locations. Thus an ordinary symlink spelling is not an escape from path checks. A look-only helper removes project write access and explicitly remounts the project read-only even beneath a writable ancestor such as `/tmp`. These are filesystem policy checks, not a claim of transactional exclusion against all concurrent external writers.[^sandbox]

A broken sandbox refuses commands rather than silently running them unrestricted. Commands inside it receive a filtered environment; an explicitly approved outside-sandbox command runs without that sandbox and with the host environment. This escape is therefore a credential boundary as well as a write-access boundary.[^files][^bash][^sandbox]

## Permission decisions precede effects

RPC approvals distinguish ordinary tool access from exceptional confirmation. In always-ask mode, modifying tools, commands and fetch ask; write mode permits file edits but asks for commands, fetch, `run_code` and non-read-only MCP tools; auto/yolo skip ordinary prompts. Read-only tools bypass this gate. Plan mode rejects file changes and non-read-only MCP calls; it is not itself a universal operating-system read-only sandbox.[^approvals][^dispatch]

`outside_sandbox: true` requires a fresh yes whenever the sandbox policy is on, regardless of mode or session-wide allowance. Without a gate to ask, the dispatcher refuses that escape. Dangerous bash text receives a separate nonpersistent confirmation even in auto/yolo, and is refused when no person can answer. The detector deliberately describes itself as a second look, not a security wall: shell-text inspection cannot prove an arbitrary command safe.[^approvals][^dispatch][^danger]

## File mutations are checked against what was seen

Reads and successful writes retain content fingerprints with metadata. Whole-file overwrite requires an existing file to have been seen; ordinary edit and patch paths reject a known stale version. Exact replacement edits must identify one unambiguous region, reject overlapping edits, and preserve BOM/CRLF conventions. Experimental hashline editing instead validates addressed line tags, allowing unrelated lines to change without rejecting the whole file.[^files][^edits][^hashline]

A patch computes and validates all file changes before starting writes. **This is validation-before-write, not a filesystem transaction:** an I/O failure during the write phase can leave earlier files changed, and the error explicitly says so. Do not turn the tool's all-hunks-apply promise into a rollback guarantee.[^patch-apply][^patch]

Nested project rules are delivered with the first relevant file access. A mutation that discovers unseen rules, or touches rules first delivered by another result in the same batch, is held for reconsideration. Generated-file and lockfile detection likewise refuses the first attempted hand edit with advice to change the source; repeating it can proceed. These are model-facing correction mechanisms, not permanent access controls.[^files][^generated]

Scoped rule files in the project top folder's `.cursor/rules` and `.claude/rules` can declare path globs; only rules matching the current file apply, and filename-only patterns can match in any directory. This is separate from rules inherited by directory nesting.[^glob-rule][^glob-matches][^glob-rules][^glob]

## Process lifetime and output are bounded separately

Foreground bash closes stdin, disables interactive pagers/editors/prompts, and owns a process group. Stop or timeout signals the group, escalates to kill, and returns output collected so far. Group wrappers reject nonpositive process-group identifiers so cleanup cannot accidentally target the agent's own group.[^bash][^sys]

Background jobs belong to the session rather than a turn: stopping the turn leaves them running. The jobs tool, timeout, session replacement or process exit ends them; completion queues a notice or wakes an idle agent. Output storage is capped while pipes continue draining, preventing a verbose child from deadlocking when its log is full.[^jobs]

Large tool output is kept out of repeated prompts: bash shows its beginning/end and spills output, searches show a capped sample plus per-file counts, and reads can return an outline instead of a giant prefix. Reads also interpret line-range selectors and render supported notebooks, PDFs, archives and SQLite databases as bounded text views rather than raw binary data. `Range` represents requested line spans; `selector` parses them, while `Kind`/`kind` identify supported formats and `text` converts them. Fetch extracts a page's main content, bounds downloads, and saves long pages/files; images have their own fitting limits. These reduce context pressure without requiring every full result to remain in the conversation.[^bash][^files][^outline][^formats][^selector][^range][^kind-type][^kind][^fetch][^images]

`run_code` is a sandboxed isolated Python process that can filter many tool results before printing useful output. Every JSON-line tool-call frame is checked against a read-only allowlist, including MCP classification, and a call-count limit. That restriction governs its tool bridge, not arbitrary Python computation: running the program still follows command approvals and the subprocess sandbox.[^program]

Semantic navigation is lazy: LSP servers are retained per language/project and receive current disk contents for queries. Rust analysis disables build scripts and proc macros so a query does not intentionally execute project code. The optional `lookup` bridge instead uses the project index helper, requires explicit enablement/helper availability, and filters hidden paths from results.[^lsp][^lookup]

An optional debugger tool can launch Python, Go or native programs under their language's DAP adapter, then stop at breakpoints, step, inspect frames and variables, and collect output. It runs through the same sandbox as bash, is offered only with `HARLO_AGENT_DEBUG=on`, and permits one active debug session.[^debug-on][^debug-spec][^debugger][^debug-call]

An optional `locate` tool searches for an implementation from a plain-language description: lexical ranking selects code windows, then a small model scores them and returns likely line ranges. It stays off unless `HARLO_AGENT_LOCATE=on`; known names remain better served by grep.[^locate-on][^locate-spec][^window][^words][^windows][^question][^hits][^locate-files][^refused][^keywords]

Where supported, a separate language-server `rename` operation returns edits for every affected file. These edits still go through the normal write validation; symbol-aware rename is not a bypass for stale-file or nested-rule checks.[^rename-spec][^rename]

When structural tools are enabled and the index helper is available, `ast_grep` searches syntax patterns and `ast_edit` computes a structural rewrite, then routes each resulting file through the ordinary write checks. This is a separate opt-in from text search and does not bypass file mutation policy.[^ast][^ast-on][^ast-offered][^ast-grep-spec][^ast-edit-spec]

Foreground bash can step aside for a waiting user message after its short grace period; when a background-job slot is available, its process and already captured output are adopted by the session's jobs manager. With no slot, the command stays foreground rather than being abandoned. Jobs can also be awaited until output contains one of a set of readiness marks; a completed job's result is returned there rather than announced a second time.[^aside][^adopt][^ready][^room]

[^dispatch]: crates/harlo-agent/src/agent.rs, fn run_calls
[^files]: crates/harlo-agent/src/tools/mod.rs
[^sandbox]: crates/harlo-agent/src/sandbox.rs
[^startup]: crates/harlo-agent/src/main.rs
[^danger]: crates/harlo-agent/src/danger.rs
[^approvals]: crates/harlo-agent/src/rpc.rs
[^bash]: crates/harlo-agent/src/tools/bash.rs
[^edits]: crates/harlo-agent/src/tools/edit.rs
[^hashline]: crates/harlo-agent/src/hashline.rs
[^patch-apply]: crates/harlo-agent/src/tools/mod.rs, fn Tools::patch
[^patch]: crates/harlo-agent/src/patch.rs
[^generated]: crates/harlo-agent/src/generated.rs
[^sys]: crates/harlo-agent/src/sys.rs
[^jobs]: crates/harlo-agent/src/jobs.rs
[^outline]: crates/harlo-agent/src/outline.rs
[^fetch]: crates/harlo-agent/src/fetch.rs
[^images]: crates/harlo-agent/src/images.rs
[^formats]: crates/harlo-agent/src/tools/formats.rs, fn text
[^selector]: crates/harlo-agent/src/tools/formats.rs, fn selector
[^range]: crates/harlo-agent/src/tools/formats.rs, enum Range
[^kind-type]: crates/harlo-agent/src/tools/formats.rs, enum Kind
[^kind]: crates/harlo-agent/src/tools/formats.rs, fn kind
[^program]: crates/harlo-agent/src/program.rs
[^lsp]: crates/harlo-agent/src/lsp.rs
[^rename-spec]: crates/harlo-agent/src/lsp.rs, fn rename_spec
[^rename]: crates/harlo-agent/src/lsp.rs, fn Servers::rename
[^lookup]: crates/harlo-agent/src/lookup.rs
[^ast]: crates/harlo-agent/src/ast.rs, fn find
[^ast-on]: crates/harlo-agent/src/ast.rs, fn on
[^ast-offered]: crates/harlo-agent/src/ast.rs, fn offered
[^ast-grep-spec]: crates/harlo-agent/src/ast.rs, fn grep_spec
[^ast-edit-spec]: crates/harlo-agent/src/ast.rs, fn edit_spec
[^aside]: crates/harlo-agent/src/tools/bash.rs, struct Aside
[^adopt]: crates/harlo-agent/src/jobs.rs, fn Jobs::adopt
[^ready]: crates/harlo-agent/src/jobs.rs, fn Jobs::ready
[^room]: crates/harlo-agent/src/jobs.rs, fn Jobs::has_room
[^glob-rule]: crates/harlo-agent/src/rules.rs, struct GlobRule
[^glob-matches]: crates/harlo-agent/src/rules.rs, fn GlobRule::matches
[^glob-rules]: crates/harlo-agent/src/rules.rs, fn glob_rules
[^glob]: crates/harlo-agent/src/rules.rs, fn glob
[^debug-on]: crates/harlo-agent/src/debug.rs, fn on
[^debug-spec]: crates/harlo-agent/src/debug.rs, fn spec
[^debugger]: crates/harlo-agent/src/debug.rs, struct Debugger
[^debug-call]: crates/harlo-agent/src/debug.rs, fn Debugger::call
[^locate-on]: crates/harlo-agent/src/locate.rs, fn on
[^locate-spec]: crates/harlo-agent/src/locate.rs, fn spec
[^window]: crates/harlo-agent/src/locate.rs, struct Window
[^words]: crates/harlo-agent/src/locate.rs, fn words
[^windows]: crates/harlo-agent/src/locate.rs, fn windows
[^question]: crates/harlo-agent/src/locate.rs, fn question
[^hits]: crates/harlo-agent/src/locate.rs, fn hits
[^locate-files]: crates/harlo-agent/src/locate.rs, fn files
[^refused]: crates/harlo-agent/src/locate.rs, fn refused
[^keywords]: crates/harlo-agent/src/locate.rs, fn keywords
