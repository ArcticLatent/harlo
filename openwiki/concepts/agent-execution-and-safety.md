---
type: concept
title: Agent execution and safety boundaries
description: Harlo agent model-loop ordering, resumable history, RPC approvals, sandbox restrictions, and the distinction between subagents and desktop helpers.
tags: [agent, safety, sandbox, rpc, tools]
verified:
  - by: openwiki/0.9.0
    at: 2026-11-14T06:51:12.859Z
sources:
  - id: openwiki-source-0b93973e66ce9fb1db2bd77c
    resource: repo://crates/harlo-agent/src/agent.rs
  - id: openwiki-source-f9cde46e266df585b9f891eb
    resource: repo://crates/harlo-agent/src/chat.rs
  - id: openwiki-source-7afaa5ecb5f0234d809b2ace
    resource: repo://crates/harlo-agent/src/compact.rs
  - id: openwiki-source-b51f99006fb5c853445948b7
    resource: repo://crates/harlo-agent/src/isolate.rs
  - id: openwiki-source-6563011f1979331a41458e2d
    resource: repo://crates/harlo-agent/src/mcp.rs
  - id: openwiki-source-ae4a49158d3f1eb8769fdbd5
    resource: repo://crates/harlo-agent/src/program.rs
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
generated: { by: "harlo", at: "2026-11-14T06:51:12.859Z" }
---

# Agent execution and safety boundaries

## Runtime and history

`harlo-agent` is a separate executable, not the Qt application. It supports a standalone prompt (`-p`), saved-session resume, authentication commands, and `--mode rpc`. In RPC mode, newline-delimited JSON commands arrive on stdin and responses/events leave on stdout. The [session driver](../architecture/sessions-and-desktop.md) owns that process from the desktop side.

The agent repeatedly sends history to a model, executes returned tool calls, records results, and requests another response until the model answers without calls. Models are addressed as `provider/model`: built-in ChatGPT and OpenAI routes use Responses, while configured providers can use chat completions or Responses. Provider configuration is read per request.

The JSONL session log is the source of model history, not merely an activity export. Entries carry IDs, parent IDs and timestamps; resuming reconstructs the active history. A crash-truncated line is skipped when reopening. Compaction records a summary plus retained recent messages without removing the older log entries, and cuts are chosen not to split a tool call from its result.

## Usage and provider capabilities

`used::Used` is the shared accounting representation for requests, including compaction, side questions, searches, and subagents. Input includes cached tokens internally; persisted assistant usage separates uncached input from `cacheRead`. Token budgets count uncached input plus output and estimates for missing usage. Cost is classified as plan, local, priced, unknown, or partly known when priced and unknown requests mix; unknown cost is not reported as a known zero.

Configured model prices are dollars per million tokens, with cached input defaulting to the input price. Provider model metadata can map offered thinking levels to wire efforts through `thinkingLevelMap`; a null mapping suppresses that level. Built-in Responses web search is offered for plan/OpenAI providers unless `HARLO_AGENT_WEB_SEARCH=off`.

An unpriced provider is free only when explicitly configured with `"free": true`; a loopback or LAN endpoint could proxy a paid service. An explicit model price or server-listed price takes precedence over that flag. See [model-server configuration](../integrations/remote-machines-and-models.md) for desktop defaults and migration.

Missing usage is not measured zero: `Used.unreported` tracks requests missing either input or output token reporting, persisted as `tokensUnknown` in assistant usage and `unreported` in aggregate tool details. A priced request with incomplete token reporting becomes unknown-cost; plan and explicitly free requests retain their no-extra-cost classification. Missing input and output can be estimated from request size and reply length for budget enforcement only: `tokensEstimated` in logs and `estimated` in tool details never become measured tokens or a price. Subagent aggregation preserves costs, cost kinds, unreported counts, and estimates.

Responses attempts cut off by the server retain their usage even if retried; a cutoff without reported usage is counted as unknown and estimated for the budget. Failed responses carrying a usage object also count. If an error carries no accounting usage, the agent falls back to the stream's retained usage snapshot—for example, when an oversized event interrupts a chat stream after it has reported tokens. A failure with neither error usage nor a retained snapshot adds no failed-attempt charge. Each ended attempt is charged immediately and put in a session-owned ledger, separately from the eventual successful reply. Main requests settle the ledger into totals and logs before fallback or error propagation, after tool batches, and when stopped. Dropping a request during retry backoff does not discard attempts already recorded. Side questions use their originating session's side-usage queue; compaction and web searches use the session ledger.

Chat-completions usage reports merge field by field, including nested details: later non-null values replace earlier totals rather than being added, while omitted or null fields preserve earlier values. An error-local report overlays the accumulated report using the same rule. Counts are normalized to the shared Responses-style accounting shape. Reported zero remains measured zero; an error without any usage remains absent rather than becoming an unknown-sized request.

Premature stream closure, connection failure, and idle timeout also carry any usage already reported by a chat-completions stream. The agent charges that dropped attempt through the same ledger before deciding whether to retry; a dropped stream without a usage object adds no failed-attempt charge. Retrying does not erase spend from the broken reply.

Stopping a chat request while its stream is still running also preserves reported spend. The stream keeps the latest normalized usage in the request's `Tries` guard. `Tries::stopped` takes and clears that snapshot, charges it once, and sends it to the accounting sink; dropping the guard invokes the same operation as a safety net. Model-change summaries, native compaction, and ordinary compaction summaries explicitly call it immediately after their cancellation-aware send and before settling the ledger, so cancellation does not leave the current attempt waiting for a later settlement. A normally ended attempt takes the snapshot before success/error accounting, preventing a second charge on drop. A stop before any recognized token count has arrived adds no charge through this guard. Settlement prices and logs the retained usage separately from a completed reply.

Input and output counts normalize independently. An output-only report keeps its measured output and leaves input unknown; the request-size estimate covers missing input for budget enforcement, not pricing. An input-only report likewise does not invent measured zero output. `each_count_said_is_kept_on_its_own` verifies these partial-report cases and an entirely empty report.

## Request admission and auxiliary work

Main requests, side questions, summaries/compaction, and built-in web searches pass through shared budget admission. In-flight requests hold estimated input plus 2,000 tokens of reply room; concurrent reservations use atomic updates. Once spent plus held tokens reaches the limit, another request is refused. This is a soft limit, not a hard output cap: an admitted request can cross it. Unreported usage consumes the budget through estimates, without being presented as measured spend. Web searches settle their own spend before releasing the reservation, so a later search in the same batch sees it and parent aggregation does not charge it twice.

Every retry releases its previous reservation and obtains admission again before waiting to resend. Thus a failed attempt can exhaust the budget and prevent the next retry. A summary following native compaction likewise gets fresh admission rather than inheriting the native request's hold.

Fallback does not reuse the failed model's prepared request. After recording and announcing the model change, the agent returns to its run loop so context preparation, capabilities, tools, and budget admission are evaluated for the fallback model. Exhausted-plan limits and transient server failures can trigger this route; request/sign-in refusals do not.

Side-question usage carries the originating session ID and log path. If a session has been switched before the answer finishes, accounting is appended to the original log rather than charged to the newly open conversation. RPC settles completed side usage after a run and waits up to ten seconds for outstanding side tasks at shutdown; this is bounded cleanup rather than a guarantee that every unfinished request completes.

## Tool-round ordering

An entirely read-only, permitted tool batch runs concurrently with `join_all`. Other batches are processed in call order. This prevents a mixed edit/read batch from acquiring surprising parallel semantics. Invalid JSON arguments return a tool error; unavailable tools are rejected. On cancellation, a concurrent batch retains results and usage from calls that already completed and marks only unfinished calls interrupted. Sequential batches distinguish interrupted work from later calls that did not run—an interrupted command may already have effects.

A run has a default cap of 200 tool rounds, configurable with positive `HARLO_AGENT_MAX_ROUNDS`. RPC permits only one active run: steering and follow-up messages queue rather than starting overlapping model loops.

MCP servers start concurrently before the first model request. Their default startup timeout is ten seconds, overridable by configuration; after two seconds the agent names any servers still pending and their remaining timeout allowance. Startup failures become notices rather than an unexplained wait.

Stop can interrupt discovery of desktop tools and MCP startup; an incomplete MCP start is retried on a later prompt rather than permanently marking discovery complete. When combined MCP tool schemas exceed 12,000 bytes, the agent offers `mcp_tools` search and `mcp_call` instead of every full schema on every request. Search returns up to eight matches. `HARLO_AGENT_MCP_LISTED_PAST` overrides the threshold; zero disables this indirection. Calls resolve to the underlying tool before read-only classification, approval and plan-mode checks, preserving those boundaries.

An indirect MCP call accepts arguments as an object or a JSON string containing an object. Missing/null arguments and blank strings normalize to an empty object; arrays, scalars, malformed JSON and strings containing non-object JSON are rejected rather than forwarded. Names may be the full exposed name, an unambiguous bare tool name, or a server-qualified name. `run_code` receives concrete read-only MCP tools, not the generic `mcp_call` dispatcher, so indirection does not widen its callable set.

## Tool context and bounded I/O

File reads append applicable nested `AGENTS.md` (or fallback `CLAUDE.md`) text to successful results once per folder while that text remains in model history. A write, edit or patch that encounters unseen rules is refused before any change, returns the rules, and must be requested again after considering them. Patch checks include deletions and both ends of moves. Top-level rules are already in the instructions; hidden folders, `node_modules`, and `vendor` are skipped. Each nested rule is capped at 8 KB with a continuation note. Resume, compaction, and output set-aside rebuild which rules are still present, allowing missing rules to be sent again.

Rules first delivered during a tool batch also block later changes under that folder in the same batch: the model requested those changes before it could read the results. This includes rules obtained by directly reading the rules file, repeated writes, and moves into the folder. The next model tool batch clears this unread-rules barrier; unrelated folders are not blocked.

Large file reads stream selected lines while counting and hashing the full file, retaining stale-edit protection without retaining the whole file in memory. Grep streams matches into a bounded preview and spills excess output to a file. Foreground shell commands drain stdout and stderr independently, including when one stream closes before the other. Their process-group guard ends descendants on cancellation or timeout and also cleans up remaining group members after completion. Interrupted output warns that the command may already have partly run.

A single overlong read line explicitly reports truncation and byte-range continuation guidance. File-name search also streams its results: it retains only the first bounded set in name order and a total count rather than collecting every path before sorting.

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

`unsaid_usage_is_estimated` verifies separate estimated spend and log/detail round trips. `a_cut_long_line_says_so` and `find_shows_the_first_names_and_the_count` cover bounded I/O guidance and ordering; nested-rule tests also verify that first edits, deletes and moves are held until their rules have been returned.

`rules_from_a_batch_hold_its_later_changes` covers the same-batch rules barrier, including a direct rules-file read and a move. `call_arguments_are_an_object_or_not_sent` covers MCP argument normalization and rejection.

`each_try_is_counted_as_it_ends_and_the_next_admitted` verifies immediate charging, refusal after exhaustion, and ledger retention when the request is dropped. Chat's `a_failed_reply_keeps_what_it_said_it_used` checks error-local and earlier usage reports, measured zero, and absent usage.

The attempt-accounting test also checks stopping mid-attempt after usage was reported: spend is charged once, the budget hold is released, and an empty snapshot adds no duplicate ledger entry.

`a_stream_that_breaks_keeps_what_it_said_it_used` covers usage followed by closure or an idle timeout, including continued retry eligibility. `a_later_report_of_part_keeps_the_rest` verifies partial-report merging and replacement of totals rather than double counting.

Suggested checks:

```sh
cargo test -p harlo-agent sandbox
```

Use [testing and releases](../operations/testing-and-releases.md) for broader RPC, MCP, image, and integration checks.
