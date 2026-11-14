---
type: concept
title: Agent execution and safety boundaries
description: Harlo agent model-loop ordering, resumable history, RPC approvals, sandbox restrictions, and the distinction between subagents and desktop helpers.
tags: [agent, safety, sandbox, rpc, tools]
verified:
  - by: openwiki/0.9.0
    at: 2026-11-13T19:56:31.605Z
sources:
  - id: openwiki-source-0b93973e66ce9fb1db2bd77c
    resource: repo://crates/harlo-agent/src/agent.rs
  - id: openwiki-source-7afaa5ecb5f0234d809b2ace
    resource: repo://crates/harlo-agent/src/compact.rs
  - id: openwiki-source-b51f99006fb5c853445948b7
    resource: repo://crates/harlo-agent/src/isolate.rs
  - id: openwiki-source-6563011f1979331a41458e2d
    resource: repo://crates/harlo-agent/src/mcp.rs
  - id: openwiki-source-f1322101a0183d196da191a3
    resource: repo://crates/harlo-agent/src/providers.rs
  - id: openwiki-source-ebc97281eed08eb8622307f8
    resource: repo://crates/harlo-agent/src/responses.rs
  - id: openwiki-source-49ed9c11d9229443b90bb028
    resource: repo://crates/harlo-agent/src/rpc.rs
  - id: openwiki-source-540dff71c1f64ad401580ea7
    resource: repo://crates/harlo-agent/src/sandbox.rs
  - id: openwiki-source-4161c4acb03fe262f2e511cc
    resource: repo://crates/harlo-agent/src/session.rs
  - id: openwiki-source-4bc18039eb9945d481557773
    resource: repo://crates/harlo-agent/src/task.rs
  - id: openwiki-source-c6e4437e250a2aca2159e14a
    resource: repo://crates/harlo-agent/src/tools/bash.rs
  - id: openwiki-source-9e8017dc1dcc559e5e7c38b9
    resource: repo://crates/harlo-agent/src/tools/mod.rs
  - id: openwiki-source-cc3a6736ec7398d0638b6f38
    resource: repo://crates/harlo-agent/src/used.rs
generated: { by: "harlo", at: "2026-11-13T19:56:31.605Z" }
---

# Agent execution and safety boundaries

## Runtime and history

`harlo-agent` is a separate executable, not the Qt application. It supports a standalone prompt (`-p`), saved-session resume, authentication commands, and `--mode rpc`. In RPC mode, newline-delimited JSON commands arrive on stdin and responses/events leave on stdout. The [session driver](../architecture/sessions-and-desktop.md) owns that process from the desktop side.

The agent repeatedly sends history to a model, executes returned tool calls, records results, and requests another response until the model answers without calls. Models are addressed as `provider/model`: built-in ChatGPT and OpenAI routes use Responses, while configured providers can use chat completions or Responses. Provider configuration is read per request.

The JSONL session log is the source of model history, not merely an activity export. Entries carry IDs, parent IDs and timestamps; resuming reconstructs the active history. A crash-truncated line is skipped when reopening. Compaction records a summary plus retained recent messages without removing the older log entries, and cuts are chosen not to split a tool call from its result.

## Usage and provider capabilities

`used::Used` is the shared accounting representation for requests, including compaction, side questions, searches, and subagents. Input includes cached tokens internally; persisted assistant usage separates uncached input from `cacheRead`. Token budgets count uncached input plus output. Cost is classified as plan, local, priced, unknown, or partly known when priced and unknown requests mix; unknown cost is not reported as a known zero.

Configured model prices are dollars per million tokens, with cached input defaulting to the input price. Provider model metadata can map offered thinking levels to wire efforts through `thinkingLevelMap`; a null mapping suppresses that level. Built-in Responses web search is offered for plan/OpenAI providers unless `HARLO_AGENT_WEB_SEARCH=off`.

An unpriced provider is free only when explicitly configured with `"free": true`; a loopback or LAN endpoint could proxy a paid service. An explicit model price or server-listed price takes precedence over that flag. See [model-server configuration](../integrations/remote-machines-and-models.md) for desktop defaults and migration.

Missing usage is not measured zero: `Used.unreported` tracks requests without input-token reporting, persisted as `tokensUnknown` in assistant usage and `unreported` in aggregate tool details. A priced request without reported tokens becomes unknown-cost; plan and explicitly free requests retain their no-extra-cost classification. Subagent aggregation preserves costs, cost kinds, and unreported counts. Responses cut off by the server still have their reported usage charged and logged before the error propagates.

## Request admission and auxiliary work

Main requests, side questions, summaries/compaction, and built-in web searches pass through shared budget admission. In-flight requests hold estimated input plus 2,000 tokens of reply room; concurrent reservations use atomic updates. Once spent plus held tokens reaches the limit, another request is refused. This is a soft limit, not a hard output cap: an admitted request can cross it, and unreported tokens cannot be counted as measured spend.

Fallback does not reuse the failed model's prepared request. After recording and announcing the model change, the agent returns to its run loop so context preparation, capabilities, tools, and budget admission are evaluated for the fallback model. Exhausted-plan limits and transient server failures can trigger this route; request/sign-in refusals do not.

Side-question usage carries the originating session ID and log path. If a session has been switched before the answer finishes, accounting is appended to the original log rather than charged to the newly open conversation. RPC settles completed side usage after a run and waits up to ten seconds for outstanding side tasks at shutdown; this is bounded cleanup rather than a guarantee that every unfinished request completes.

## Tool-round ordering

An entirely read-only, permitted tool batch runs concurrently with `join_all`. Other batches are processed in call order. This prevents a mixed edit/read batch from acquiring surprising parallel semantics. Invalid JSON arguments return a tool error; unavailable tools are rejected. Cancellation distinguishes interrupted work from later calls that did not run—an interrupted command may already have effects.

A run has a default cap of 200 tool rounds, configurable with positive `HARLO_AGENT_MAX_ROUNDS`. RPC permits only one active run: steering and follow-up messages queue rather than starting overlapping model loops.

MCP servers start concurrently before the first model request. Their default startup timeout is ten seconds, overridable by configuration; after two seconds the agent names any servers still pending and their remaining timeout allowance. Startup failures become notices rather than an unexplained wait.

## Tool context and bounded I/O

File reads and changes append applicable nested `AGENTS.md` (or fallback `CLAUDE.md`) text to tool results once per folder while that text remains in model history. Top-level rules are already in the instructions; hidden folders, `node_modules`, and `vendor` are skipped. Each nested rule is capped at 8 KB with a continuation note. Resume, compaction, and output set-aside rebuild which rules are still present, allowing missing rules to be sent again.

Large file reads stream selected lines while counting and hashing the full file, retaining stale-edit protection without retaining the whole file in memory. Grep streams matches into a bounded preview and spills excess output to a file. Foreground shell commands drain stdout and stderr independently, including when one stream closes before the other. Their process-group guard ends descendants on cancellation or timeout and also cleans up remaining group members after completion. Interrupted output warns that the command may already have partly run.

Provider refusal bodies are retained only up to 64 KiB and read within the response timeout. Event-size enforcement also checks a complete SSE event before returning it, not only an unfinished event buffer.

## Approval is not the sandbox

The RPC gate and the operating-system sandbox protect different boundaries:

| Layer | Responsibility |
| --- | --- |
| RPC approval modes | Decide whether a potentially mutating tool needs the user's answer |
| Plan-mode gate | Reject file mutations and non-read-only MCP calls before execution |
| Sandbox | Restrict the files, environment, and optional network access available to commands and file tools |
| Read-before-edit tracking | Refuse stale edits/overwrites until a changed file is read again |

`always-ask` asks for gated calls; `write` asks for bash, fetch, run-code and mutating MCP calls while permitting ordinary file changes; `yolo`/`auto` bypass normal approval prompts. Read-only calls bypass that gate. An explicit `outside_sandbox` bash call, while sandboxing is enabled, still asks every time, before the ordinary mode exemption. Do not interpret an automatic approval mode as disabling sandbox restrictions.

The implemented plan gate explicitly rejects write/edit/patch and mutating MCP operations. Shell execution is a separate approval path: plan mode is not an OS-level read-only mount policy. Changes to these controls should be checked in `rpc::Approvals`, not inferred only from the UI label.

## Sandbox policy

Sandboxing defaults to on and uses bubblewrap. `--sandbox` takes precedence over `HARLO_AGENT_SANDBOX`; supported modes include on, off and no-network. Extra writable/hidden folders and allowed environment variables come from `sandbox.json` in the agent's configuration directory, not project-controlled configuration. A machine unable to start the sandbox refuses sandboxed commands rather than silently falling back to unrestricted execution.

The normal view makes the system read-only, permits project/temp/tool-cache writes, and hides secrets such as keys, sign-ins, browser profiles, and agent sessions. File tools enforce the corresponding access rules, including symlink and `..` traversal checks. A session whose working directory is the home directory or an ancestor does not gain write access to all of home.

Separate stale-file tracking records size, modification time, and content hash after reads/writes. Editing or overwriting a file changed by another actor requires a new read. Generated files receive an additional first-change refusal. This protects concurrent work even inside otherwise writable locations.

## Two kinds of delegation

Agent `task` subagents run inside the same process with restricted tools and separate logs. They cannot recursively spawn subagents or use harlo's desktop tools, share the parent's approval path, and stop with the parent. Up to eight tasks may be requested, with four running at once. Multi-editor parallel work uses separate file copies; a different model/profile also starts with fresh task context rather than blindly inheriting the conversation.

With sandboxing enabled, explore-only helpers explicitly mount the project and shared Git directory read-only, even below a writable ancestor such as `/tmp`; scratch space remains writable. Without the sandbox this shell-level read-only guarantee is unavailable. Isolated editor snapshots exclude Python bytecode and common Python tool caches from capture and merge, so cache churn does not prevent real changes from being brought back.

Desktop helper sessions are different: orchestration starts independent sessions, potentially using another harness or worktree. See [plans and project changes](../workflows/plans-and-project-changes.md).

## Focused safety checks

`the_file_tools_keep_to_it` covers hidden keys through direct paths, symlinks, and parent traversal as well as allowed project writes. `the_home_folder_is_never_the_writable_project` covers unsafe broad working directories.

Additional focused checks include `a_used_budget_stops_every_kind_of_request`, `helpers_costs_come_back_whole`, `a_reply_without_usage_is_unknown_not_nothing`, `a_folders_rules_come_once_with_its_first_file`, `a_big_file_is_read_as_it_comes_the_same`, `a_timeout_ends_the_whole_group`, and `caches_dont_hold_changes_back`. These cover admission, accounting uncertainty, context rules, bounded reads, command cleanup, and isolated merge boundaries respectively.

Suggested checks:

```sh
cargo test -p harlo-agent sandbox
```

Use [testing and releases](../operations/testing-and-releases.md) for broader RPC, MCP, image, and integration checks.
