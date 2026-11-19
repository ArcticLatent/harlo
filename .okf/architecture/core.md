---
type: Architecture
title: Core application ownership and subsystem map
description: The UI-independent coordination layer, its invariants, and the complete responsibility map for harlo-core.
covers:
  - crates/harlo-core/
generated: { by: harlo/gpt-6-luna, at: "2026-11-19T18:56:17Z" }
sources:
  - id: lib
    resource: "repo://crates/harlo-core/src/lib.rs"
  - id: manifest
    resource: "repo://crates/harlo-core/Cargo.toml"
  - id: session
    resource: "repo://crates/harlo-core/src/session.rs"
  - id: db
    resource: "repo://crates/harlo-core/src/db.rs"
  - id: transcript
    resource: "repo://crates/harlo-core/src/transcript.rs"
  - id: pages
    resource: "repo://crates/harlo-core/src/pages.rs"
  - id: harness
    resource: "repo://crates/harlo-harness/src/lib.rs"
  - id: providers
    resource: "repo://crates/harlo-core/src/providers.rs"
  - id: engines
    resource: "repo://crates/harlo-core/src/engines.rs"
  - id: git
    resource: "repo://crates/harlo-core/src/git.rs"
  - id: remote
    resource: "repo://crates/harlo-core/src/remote.rs"
  - id: phone
    resource: "repo://crates/harlo-core/src/phone.rs"
  - id: workflow
    resource: "repo://crates/harlo-core/src/workflow.rs"
  - id: markdown
    resource: "repo://crates/harlo-core/src/markdown.rs"
  - id: actions
    resource: "repo://crates/harlo-core/src/actions.rs"
  - id: export
    resource: "repo://crates/harlo-core/src/export.rs"
  - id: orchestration
    resource: "repo://crates/harlo-core/src/orchestration.rs"
  - id: bridge
    resource: "repo://crates/harlo-core/bridge/harlo_mcp_bridge.py"
  - id: tools
    resource: "repo://crates/harlo-core/src/tool_calls.rs"
  - id: plan
    resource: "repo://crates/harlo-core/src/plan.rs"
  - id: workflow-test
    resource: "repo://crates/harlo-core/tests/workflow_sim.rs"
  - id: context
    resource: "repo://crates/harlo-core/src/context.rs"
  - id: facts
    resource: "repo://crates/harlo-core/src/tool_facts.rs"
  - id: listdiff
    resource: "repo://crates/harlo-core/src/listdiff.rs"
  - id: listing
    resource: "repo://crates/harlo-core/src/listing.rs"
  - id: splitdiff
    resource: "repo://crates/harlo-core/src/splitdiff.rs"
  - id: pictures
    resource: "repo://crates/harlo-core/src/pictures.rs"
  - id: visuals
    resource: "repo://crates/harlo-core/src/visuals.rs"
  - id: starters
    resource: "repo://crates/harlo-core/src/starters.rs"
  - id: session-features
    resource: "repo://crates/harlo-core/src/session.rs"
  - id: firewall
    resource: "repo://crates/harlo-core/src/firewall.rs"
  - id: notifications
    resource: "repo://crates/harlo-core/src/notifications.rs"
  - id: autostart
    resource: "repo://crates/harlo-core/src/autostart.rs"
  - id: terminal
    resource: "repo://crates/harlo-core/src/terminal.rs"
  - id: dictation
    resource: "repo://crates/harlo-core/src/dictation.rs"
  - id: files
    resource: "repo://crates/harlo-core/src/files.rs"
  - id: palette
    resource: "repo://crates/harlo-core/src/palette.rs"
  - id: icons
    resource: "repo://crates/harlo-core/src/project_icons.rs"
  - id: wiki
    resource: "repo://crates/harlo-core/src/wiki.rs"
  - id: freshness
    resource: "repo://crates/harlo-core/src/wiki.rs"
    title: enum Freshness
  - id: wiki-view
    resource: "repo://crates/harlo-core/src/wiki_view.rs"
  - id: persistence
    resource: "repo://crates/harlo-core/src/lib.rs"
  - id: execution
    resource: "repo://crates/harlo-core/src/lib.rs"
  - id: example
    resource: "repo://crates/harlo-core/examples/usage_stats.rs"
  - id: memory
    resource: "repo://crates/harlo-core/tests/memory.rs"
  - id: scratch
    resource: "repo://crates/harlo-core/src/scratch.rs"
  - id: servers
    resource: "repo://crates/harlo-core/src/servers.rs"
  - resource: "repo://crates/harlo-core/"
---
# Core application ownership and subsystem map

`harlo-core` contains UI-independent application logic: sessions, transcript reduction, rendering, persistence, and sidebar construction. It depends on `harlo-harness`, not Qt; the default `dictation` feature brings in whisper.cpp, while the remote engine helper can be built without it. “UI-independent” does not mean side-effect-free: this crate also runs Git/SSH/agent commands, writes settings, manages servers, and implements phone cryptography.[^lib][^manifest][^git][^remote][^phone]

## Read by boundary

| Question | Page | Source boundary |
| --- | --- | --- |
| Who owns a running conversation? | [Sessions](/workflows/sessions.md) | Handle/control channel → session driver → transcript and subscribers.[^session] |
| Which state survives restart, and where? | [Persistence](/architecture/core-persistence.md) | Application SQLite versus harness files, derived indexes, and page caches.[^db][^pages] |
| What does an agent-specific protocol become? | [Harness normalization](/architecture/harness.md) | Process/file adapters produce shared events and messages.[^harness] |
| How do agents and inference engines differ? | [Agents and engines](/architecture/core-engines.md) | Agent discovery/configuration versus engine installation, server readiness, and model registration.[^providers][^engines] |
| Where does an operation execute, and what can undo promise? | [Git and remote](/workflows/git-and-remote.md) | Host-qualified locations and explicit Git operations.[^remote][^git] |

## Ownership rules that cross modules

1. **Conversation ownership stays with the harness.** `Db` owns projects, flags, settings, and run associations; it does not store the conversation. Saved-session readers and live event reduction converge on display data without making the display rows authoritative history.[^db][^session][^transcript]
2. **A view does not own the session process.** Subscribers receive a snapshot and then batched changes; a session can continue with no view. Sink delivery runs on the session task, leaving UI-thread forwarding to the frontend.[^session]
3. **Machine identity must survive path handling.** Remote locations remain host-qualified in application records; the host-aware code routes disk access and process execution.[^remote]
4. **A successful agent report is not a successful check command.** Workflow checks use the command's exit status, with dispatch/attempt matching to reject late results.[^workflow]
5. **Rendered material is derived.** Markdown caches, paged row bodies, action summaries, and exported Markdown are views of content, not replacements for its original message structure.[^markdown][^pages][^actions][^export]

## Coordination, plans, and workflow execution

`orchestration` defines the MCP bridge between an agent and the running application. The bridge identifies the caller using `HARLO_SESSION`, speaks MCP over stdio, and forwards tool calls over a local socket. With no application reachable it offers no tools. The Python bridge in `bridge/harlo_mcp_bridge.py` implements the corresponding remote-host endpoint using a forwarded socket; it answers MCP protocol requests itself and maps application errors into tool results.[^orchestration][^bridge]

`tool_calls` is the policy boundary: it decides offered tools, wiki/shared-plan access, and argument interpretation, while the application holds sessions and performs the requested actions. Offering a tool is therefore not equivalent to allowing every caller to execute every target operation.[^tools]

`plan` models shared editable task steps, assignment, progress, and conservative per-step commits. `workflow` is a separate staged run state machine: implementation, command checks, and agent reviews in sequence. A failed later stage returns to the same implementation session/worktree for bounded retries, then all post-implementation stages run again. Each run retains its own definition; `step` returns actions for the application to perform, and `recover` determines what can reconnect or must wait after restart.[^plan][^workflow]

Every workflow wait carries a dispatch number and attempt. Stale results cannot advance the current attempt. The randomized `workflow_sim` test injects failures, late/duplicate/out-of-order results, serialization/recovery, and user pause/resume/stop events; it checks that terminal runs stay terminal and that success requires all stages on the latest attempt.[^workflow][^workflow-test]

## Derived conversation features

These modules are deliberately reusable for saved as well as live messages; do not make their outputs depend solely on transient UI events.[^actions][^context][^export]

| Modules | Responsibility and important boundary |
| --- | --- |
| `actions`, `tool_facts` | Action log from tool calls/results; compact outcomes such as edits, tests, exit codes, reads, and matches. Tool naming/output dialects are normalized here.[^actions][^facts] |
| `context`, `export` | Message/token/cost report and Markdown sharing. Export includes brief tool activity but omits thinking.[^context][^export] |
| `markdown`, `listdiff` | Cached top-level rich-text blocks and keyed minimal list changes. Stable shared blocks reduce rerendering; update/remove/insert order matters.[^markdown][^listdiff] |
| `listing`, `splitdiff` | Numbered/highlighted file-output recognition and aligned side-by-side diffs; unknown tail offsets are not invented.[^listing][^splitdiff] |
| `pictures`, `visuals` | Materialize conversation images in cache/find named image files, and parse Mermaid/options/mockup fences. Visual blocks remain ordinary transcript text.[^pictures][^visuals] |
| `starters` | Visible starter prompt plus hidden-in-display instruction payload carried as a trailing `harlo-starter` block, preserving the starter across resume.[^starters] |
| `attach`, `session_ref`, `subagents`, `plugins`, `spending` | Prompt augmentation, excerpt snapshots, subagent aggregation, user plugin commands, and cost holds; details are in [sessions](/workflows/sessions.md).[^session-features] |

## Platform-facing services in core

`phone` owns pairing identity/device records and boxed messages. A one-time code registers a device key; subsequent admission uses the key, and removing the device revokes it. Frames after plaintext hello use NaCl box with a connection identifier and sequence number to resist cross-connection replay and reordering. This module is the cryptographic/state boundary, not a declaration that every network transport belongs to core.[^phone]

`firewall` detects whether ufw/firewalld allow phone access and constructs commands for authorization through pkexec; `notifications` supplies event choices and title/message templates. `autostart` manages the XDG autostart desktop entry. These are application policies around the platform, not QML widget implementations.[^firewall][^notifications][^autostart]

`terminal` owns an alacritty-terminal-backed PTY and exposes rendered cells/input translation; the UI renders after a wakeup. Optional `dictation` receives microphone samples from the app and runs local whisper.cpp recognition, downloading its model on first use. The frontend still supplies capture/display behavior.[^terminal][^dictation]

`files` maintains a gitignore-aware fuzzy file list per working directory, rebuilding stale lists in the background; `palette` ranks command candidates while retaining input order for equal scores. `project_icons` validates stored icon names against presets and falls back to the folder representation for unknown values.[^files][^palette][^icons]

## Wiki integration is orchestration, not a second wiki engine

`wiki` supplies project discovery, reading notes, writer prompts, and `wiki_check`; the actual OKF engine is delegated to `harlo-okf`. Its read-only freshness query distinguishes pages with outstanding work from an unchecked wiki, so a failed check is not presented as proof that all pages are current. A writer edits Markdown with ordinary file tools, while checks validate/stamp qualifying pages and report outstanding work. `wiki_view` reads the wiki on its repository's machine, lays out Markdown for readers, and offers updates as the default branch moves. Remote reads route through the agent rather than assuming a local `.okf` copy.[^wiki][^freshness][^wiki-view]

## Complete folder responsibility map

The broad `covers` declaration includes the entire crate; the following groups locate the remaining files without confusing shared ownership with a new subsystem.[^lib][^manifest]

- **Persistence/discovery:** `db`, `migrate`, `scanner`, `remote_sessions`, `search`, `stats`, `sidebar`, `shelf`, `pages`, `templates`, and `schedules` are explained in [persistence](/architecture/core-persistence.md).[^persistence]
- **Execution/configuration:** `agents`, `providers`, `signin`, `quota`, `updates`, `engines`, `engine_setup`, `engine_host`, `servers`, `model_servers`, `agent_host`, `remote_helper`, `mcp`, `mcp_catalog`, `mcp_probe`, and `src/bin/harlo-engines.rs` are explained in [agents and engines](/architecture/core-engines.md).[^execution]
- **Repository operations:** `git`, `git_hosting`, `commitmsg`, `remote`, and the plan commit boundary are explained in [Git and remote](/workflows/git-and-remote.md).[^git][^remote][^plan]
- **Entry/build boundary:** `src/lib.rs` exports the public modules and selected session/transcript types; `Cargo.toml` records dependencies/features. `bridge/harlo_mcp_bridge.py` is the remote MCP entry point, not a Rust library module.[^lib][^manifest][^bridge]
- **Examples/test support:** `examples/usage_stats.rs` measures cold/warm reports; `tests/memory.rs` guards long-history allocation behavior; `tests/workflow_sim.rs` tests workflow invariants under faults. `src/scratch.rs` supplies test scratch folders removed on drop and is exposed only in test builds.[^example][^memory][^workflow-test][^scratch][^lib]

## Maintenance pitfalls

Preserve the separation between a pure decision and the action that fulfills it: workflow progression returns actions; tool policy returns decisions; sessions deliver changes to sinks. Conversely, do not assume that calling a core module is harmless on the UI thread—Git, SSH, wiki work-list reads, setup, and server readiness can perform blocking work. Consult the owning page and implementation before moving these operations into a frontend callback.[^workflow][^tools][^session][^git][^remote][^wiki-view][^servers]

[^lib]: crates/harlo-core/src/lib.rs
[^manifest]: crates/harlo-core/Cargo.toml
[^session]: crates/harlo-core/src/session.rs
[^db]: crates/harlo-core/src/db.rs
[^transcript]: crates/harlo-core/src/transcript.rs
[^pages]: crates/harlo-core/src/pages.rs
[^harness]: crates/harlo-harness/src/lib.rs
[^providers]: crates/harlo-core/src/providers.rs
[^engines]: crates/harlo-core/src/engines.rs
[^git]: crates/harlo-core/src/git.rs
[^remote]: crates/harlo-core/src/remote.rs
[^phone]: crates/harlo-core/src/phone.rs
[^workflow]: crates/harlo-core/src/workflow.rs
[^markdown]: crates/harlo-core/src/markdown.rs
[^actions]: crates/harlo-core/src/actions.rs
[^export]: crates/harlo-core/src/export.rs
[^orchestration]: crates/harlo-core/src/orchestration.rs
[^bridge]: crates/harlo-core/bridge/harlo_mcp_bridge.py
[^tools]: crates/harlo-core/src/tool_calls.rs
[^plan]: crates/harlo-core/src/plan.rs
[^workflow-test]: crates/harlo-core/tests/workflow_sim.rs
[^context]: crates/harlo-core/src/context.rs
[^facts]: crates/harlo-core/src/tool_facts.rs
[^listdiff]: crates/harlo-core/src/listdiff.rs
[^listing]: crates/harlo-core/src/listing.rs
[^splitdiff]: crates/harlo-core/src/splitdiff.rs
[^pictures]: crates/harlo-core/src/pictures.rs
[^visuals]: crates/harlo-core/src/visuals.rs
[^starters]: crates/harlo-core/src/starters.rs
[^session-features]: crates/harlo-core/src/session.rs
[^firewall]: crates/harlo-core/src/firewall.rs
[^notifications]: crates/harlo-core/src/notifications.rs
[^autostart]: crates/harlo-core/src/autostart.rs
[^terminal]: crates/harlo-core/src/terminal.rs
[^dictation]: crates/harlo-core/src/dictation.rs
[^files]: crates/harlo-core/src/files.rs
[^palette]: crates/harlo-core/src/palette.rs
[^icons]: crates/harlo-core/src/project_icons.rs
[^wiki]: crates/harlo-core/src/wiki.rs
[^freshness]: crates/harlo-core/src/wiki.rs, enum Freshness
[^wiki-view]: crates/harlo-core/src/wiki_view.rs
[^persistence]: crates/harlo-core/src/lib.rs
[^execution]: crates/harlo-core/src/lib.rs
[^example]: crates/harlo-core/examples/usage_stats.rs
[^memory]: crates/harlo-core/tests/memory.rs
[^scratch]: crates/harlo-core/src/scratch.rs
[^servers]: crates/harlo-core/src/servers.rs
