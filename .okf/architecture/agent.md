---
type: Architecture
title: Agent execution and model streams
description: How harlo-agent turns provider streams into a replayable, bounded tool execution loop.
covers:
  - crates/harlo-agent/
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: entry
    resource: "repo://crates/harlo-agent/src/main.rs"
  - id: build
    resource: "repo://crates/harlo-agent/build.rs"
  - id: loop
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: "fn Agent::turn"
  - id: send
    resource: "repo://crates/harlo-agent/src/agent.rs"
  - id: responses
    resource: "repo://crates/harlo-agent/src/responses.rs"
  - id: messages
    resource: "repo://crates/harlo-agent/src/messages.rs"
  - id: gemini
    resource: "repo://crates/harlo-agent/src/gemini.rs"
  - id: providers
    resource: "repo://crates/harlo-agent/src/providers.rs"
  - id: presets
    resource: "repo://crates/harlo-agent/src/presets.json"
  - id: calls
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: fn run_calls
  - id: retry
    resource: "repo://crates/harlo-agent/src/retry.rs"
  - id: fallback
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: "fn Agent::ask"
  - id: repeats
    resource: "repo://crates/harlo-agent/src/repeats.rs"
  - id: accounting
    resource: "repo://crates/harlo-agent/src/agent.rs"
  - id: used
    resource: "repo://crates/harlo-agent/src/used.rs"
  - id: usage
    resource: "repo://crates/harlo-agent/src/usage.rs"
  - id: auth
    resource: "repo://crates/harlo-agent/src/auth.rs"
  - id: keys
    resource: "repo://crates/harlo-agent/src/keys.rs"
  - id: login-tests
    resource: "repo://crates/harlo-agent/tests/login.rs"
  - id: router-tests
    resource: "repo://crates/harlo-agent/tests/openrouter_login.rs"
  - id: images
    resource: "repo://crates/harlo-agent/src/images.rs"
  - id: search
    resource: "repo://crates/harlo-agent/src/search.rs"
  - id: todo
    resource: "repo://crates/harlo-agent/src/todo.rs"
  - resource: "repo://crates/harlo-agent/"
---
# Agent execution and model streams

`harlo-agent` is a binary with two task-facing entry points: one-shot `-p` execution and a long-lived JSON-line RPC service. Both use the same agent loop and session representation; authentication, model discovery, language-server status and wiki operations are separate CLI routes. The build script embeds the commit and dirty status for identifying the executable that produced a run.[^entry][^build]

This is the crate-wide coverage page. The deeper boundaries are [tools and permissions](/architecture/agent-tools.md), [MCP and session orchestration](/workflows/agent-orchestration.md), and [persistence and context](/architecture/agent-context.md).

## One canonical conversation, several wire protocols

The loop keeps pi-shaped messages rather than a provider's request body. Each request derives a new wire representation from that history: Responses, OpenAI-compatible chat completions, Anthropic Messages, or Gemini. Streaming deltas are for immediate presentation; the completed assistant message is what enters history before its tools run. This separates UI progress from the durable input to the next request.[^loop][^send]

The adapters are not interchangeable JSON wrappers. Responses sends the history with `store: false`; its completed output can supply encrypted reasoning omitted from earlier item events. Messages places cache markers on the system prompt and conversation end, and returns thinking only to its originating provider/model when thinking is enabled. Gemini preserves signatures on their original parts for the originating provider/model and uses Google's placeholder for foreign calls. Model switching must therefore rebuild requests, not replay another provider's raw response.[^responses][^messages][^gemini]

Provider resolution is repeated for requests, so configuration changes can affect an existing session. User `providers.json` entries replace same-named bundled presets; model metadata controls context windows, image support, thinking mappings, output limits and pricing. Unknown pricing is distinct from explicitly free service: a local address alone must not be treated as proof that a proxy charges nothing.[^providers][^presets]

## The execution boundary is a complete reply

A turn requests a reply, records it, executes its calls, records their results, and requests again. A reply with no calls finishes only after checking pending steering, job notices and follow-ups. Helpers offered `report_done` get one reminder if they would end without handing back a result or asking for help.[^loop]

Three invariants prevent plausible-looking progress from becoming unsafe execution:

- **Length-truncated replies execute no calls.** Every call receives a failure result, even if an earlier call appears complete. A truncated file body must not become a partial write.[^loop]
- **Only an entirely read-only batch runs concurrently.** Mixed batches run in call order; completed read-only results survive cancellation, while unfinished or unstarted calls get explicit results. This preserves dependencies between writes and subsequent reads and keeps call/result history well formed.[^calls]
- **Cancellation is not rollback.** The stop note warns that interrupted calls may have partly run. A subsequent turn must inspect their effects instead of assuming failure means no change.[^loop][^calls]

## Recovery is bounded, and spending survives failure

Transient rate limits, server failures and broken connections receive up to three retries, normally after 1, 2 and 4 seconds. `Retry-After` is respected only up to five minutes; authentication errors, context overflow and exhausted quota are not ordinary retries. Retrying withdraws the partial displayed response and makes a new request; it does not run the partially streamed tool calls.[^retry][^send]

An optional fallback is eligible for exhausted quota or transient failures that exhausted retries, not malformed requests or sign-in refusal. Switching records a model change and rebuilds the next request, including context handling. The original model is reconsidered after the quota reset or on the next prompt for transient failures, unless the user has selected another model.[^fallback]

The loop also caps tool rounds (200 by default), warns near the cap, and detects identical tool/arguments/result/error combinations. It nudges at the third repetition and stops at the fifth; a successful file change resets repetition tracking, so testing another attempted fix is not confused with polling a dead end.[^loop][^repeats]

Accounting covers ordinary replies and auxiliary requests, including compaction, searches, side questions and subagents. Request attempts reserve shared budget room; failed or stopped attempts retain usage already reported by the server. Missing usage can be estimated rather than silently treated as free, while monetary cost retains its known/unknown distinction. Codex plan-window percentages are a separate signal from per-request token and cost accounting.[^accounting][^used][^usage]

## Supporting inputs and services

Authentication separates the registered ChatGPT app flow from the opt-in Codex sign-in. Refresh tokens rotate, so refreshes lock across processes and reread credentials under that lock. Saved provider keys arrive on stdin rather than command-line arguments. Integration tests exercise pasted-callback completion with stdin still open and a local OpenRouter PKCE/key-exchange stand-in; they do not require a live provider account.[^auth][^keys][^login-tests][^router-tests]

Image inputs from prompts, file reads and MCP are fitted to bounded dimensions/bytes and kept as canonical image blocks; adapters can substitute a note when the selected model cannot accept images. Web search is a separate model request whose findings become a tool result, avoiding the provider's search instructions in every ordinary request. The optional todo tool records whole-list updates and reconstructs the latest successful list for the UI; it is off by default.[^images][^search][^todo]

[^entry]: crates/harlo-agent/src/main.rs
[^build]: crates/harlo-agent/build.rs
[^loop]: crates/harlo-agent/src/agent.rs, fn Agent::turn
[^send]: crates/harlo-agent/src/agent.rs
[^responses]: crates/harlo-agent/src/responses.rs
[^messages]: crates/harlo-agent/src/messages.rs
[^gemini]: crates/harlo-agent/src/gemini.rs
[^providers]: crates/harlo-agent/src/providers.rs
[^presets]: crates/harlo-agent/src/presets.json
[^calls]: crates/harlo-agent/src/agent.rs, fn run_calls
[^retry]: crates/harlo-agent/src/retry.rs
[^fallback]: crates/harlo-agent/src/agent.rs, fn Agent::ask
[^repeats]: crates/harlo-agent/src/repeats.rs
[^accounting]: crates/harlo-agent/src/agent.rs
[^used]: crates/harlo-agent/src/used.rs
[^usage]: crates/harlo-agent/src/usage.rs
[^auth]: crates/harlo-agent/src/auth.rs
[^keys]: crates/harlo-agent/src/keys.rs
[^login-tests]: crates/harlo-agent/tests/login.rs
[^router-tests]: crates/harlo-agent/tests/openrouter_login.rs
[^images]: crates/harlo-agent/src/images.rs
[^search]: crates/harlo-agent/src/search.rs
[^todo]: crates/harlo-agent/src/todo.rs
