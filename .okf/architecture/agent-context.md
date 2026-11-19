---
type: Architecture
title: Agent persistence and context management
description: How append-only session branches, recoverable outputs and compaction preserve continuity within a bounded model window.
covers:
  - crates/harlo-agent/src/session.rs
  - crates/harlo-agent/src/compact.rs
  - crates/harlo-agent/src/stubs.rs
  - crates/harlo-agent/src/skills.rs
  - crates/harlo-agent/src/clock.rs
  - crates/harlo-agent/src/used.rs
generated: { by: harlo/gpt-6-luna, at: "2026-11-19T18:56:17Z" }
sources:
  - id: session
    resource: "repo://crates/harlo-agent/src/session.rs"
  - id: push
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: "fn Agent::push"
  - id: branch
    resource: "repo://crates/harlo-agent/src/session.rs"
    title: "fn Log::load_branch"
  - id: append
    resource: "repo://crates/harlo-agent/src/session.rs"
    title: "fn Log::append"
  - id: usage
    resource: "repo://crates/harlo-agent/src/session.rs"
  - id: failed
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: "fn Agent::failed"
  - id: estimate
    resource: "repo://crates/harlo-agent/src/agent.rs"
  - id: compact
    resource: "repo://crates/harlo-agent/src/compact.rs"
  - id: elide
    resource: "repo://crates/harlo-agent/src/compact.rs"
    title: fn elide
  - id: stubs
    resource: "repo://crates/harlo-agent/src/stubs.rs"
  - id: overflow
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: "fn Agent::turn"
  - id: set-aside
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: "fn Agent::set_aside"
  - id: recall
    resource: "repo://crates/harlo-agent/src/tools/mod.rs"
    title: "fn Tools::recall"
  - id: summarize
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: "fn Agent::compact"
  - id: splice
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: "fn Agent::splice_summary"
  - id: carry
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: "fn Agent::carry_over"
  - id: skills
    resource: "repo://crates/harlo-agent/src/skills.rs"
  - id: instructions
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: "fn Agent::reread_instructions"
  - id: clock
    resource: "repo://crates/harlo-agent/src/clock.rs"
  - resource: "repo://crates/harlo-agent/src/used.rs"
---
# Agent persistence and context management

The session log is the source for conversation replay, not a dump of UI stream fragments. Canonical messages drive provider requests both while running and after reopening; context reduction is represented by new log entries rather than deleting the original conversation. This is the persistence counterpart of the [execution boundary](/architecture/agent.md).[^session][^push]

## Append history; select a branch when reopening

Sessions use pi-compatible JSONL: a versioned session header followed by entries with ids, parent ids and timestamps. Files are grouped by working directory below the XDG data directory (or the home-directory fallback), created on first entry, and can be disabled for ephemeral runs. Resuming the latest session selects the newest JSONL file for that working directory.[^session]

Opening a log follows parent links backwards from the latest entry, then replays that branch forwards. Editing a previous user message branches from its parent in the same file; abandoned branches remain available rather than being overwritten. Model/thinking changes and the session name replay alongside messages, while compaction and stub entries reconstruct the active model-visible history.[^branch]

Malformed lines are skipped, which tolerates a crash-truncated final record. Before appending to a file whose last byte is not a newline, the logger inserts a separator so the next record does not join the torn one. Each append updates the in-memory parent only after the write succeeds. This supports recovery, but is not an fsync-backed transaction or a guarantee that all externally performed tool effects have corresponding durable records after a crash.[^append][^session]

Usage reconstruction scans request-bearing entries, including compaction and standalone usage records, and includes subagent logs beside the parent. It deliberately does not add the parent's aggregate task-result usage on top of those child records. Failed UI error messages are recorded without inventing a second request charge.[^usage][^failed]

## Measure the next request, not just the last response

The context estimate includes instructions, tool schemas and history. When the model/instructions/tools fingerprint still matches, the next estimate uses the last measured input plus newly added messages; otherwise it estimates the whole request again. Model switches and MCP schema refreshes invalidate that anchor, preventing an old smaller request measurement from hiding newly added overhead.[^estimate]

Automatic compaction reserves room for the reply and subsequent work: normally 16,384 tokens, reduced to a quarter of a small window. Unknown windows disable the normal threshold unless explicitly overridden. The retained recent tail is normally up to 20,000 estimated tokens, reduced for smaller windows; explicit manual compaction keeps a much smaller tail so even a shorter conversation can be compacted.[^compact]

## Two reductions with different recovery properties

**Setting output aside** replaces sufficiently large old tool results with provenance-bearing stubs and preserves originals for `recall`. It batches replacements only when enough space is recovered to justify disturbing the prompt cache. Proactive stubbing is off by default, but context-overflow recovery tries it before summarizing. A `harlo_stubs` entry records the substitutions, so replay can restore both the short active history and the original-result lookup.[^stubs][^overflow][^set-aside]

Recall is historical evidence, not a fresh filesystem read. For a recalled read result whose file has changed, the tool adds a warning to read it again. This distinction avoids using an archived observation as authorization to overwrite today's file.[^recall]

**Compaction** asks for a summary of the older prefix and retains a recent suffix verbatim. The cut is at a user/assistant boundary, never between a tool call and its result. The code accumulates read/modified file lists across compactions rather than relying on the model to remember every path. Ordinary summarization disables tool use while retaining request-prefix compatibility for caching.[^compact][^summarize]

A successful compaction is appended before the in-memory prefix is replaced; its `firstKeptEntryId` lets reopening reproduce the same summary-plus-tail view. An empty or length-truncated summary is rejected and its usage retained separately. Threshold compaction failure can leave the ordinary request to proceed unchanged; overflow recovery is bounded, first trying stubs and then compaction rather than endlessly retrying the same oversized input.[^splice][^summarize][^overflow]

For eligible Responses providers, the agent can first request native server compaction and fall back to textual summarization. Native opaque state belongs to its originating provider/model. On switching away, the new model can summarize the pre-compaction conversation recovered from the log, constrained to its available window and supplemented by the retained request note. If that cannot be done, the note remains; opaque state is not assumed portable.[^summarize][^carry]

After compaction, a successful whole-file read kept in the recent tail is replaced by a short reference when that same path is successfully read whole again later. Range reads, failed reads and the latest successful whole read retain their output; this trims superseded evidence without hiding the newest observation.[^elide]

## Instructions are context too

Skills are discovered from user/project folders and enabled Claude plugin installations, deduplicated with project/nearer definitions taking precedence. The prompt lists bounded descriptions and paths rather than eagerly including every skill body. Explicit `/skill:` expansion can include a body; skills marked for manual invocation are not automatically advertised in the same way.[^skills]

The agent rereads its project instructions when a summary is spliced in, and rebuilds the tool layer's record of rules visible in the surviving history. The clock deliberately keeps the session-start date in instructions rather than a constantly changing timestamp; `current_time` supplies the live clock when needed. These choices balance renewed context with a stable reusable request prefix.[^splice][^instructions][^clock]

[^session]: crates/harlo-agent/src/session.rs
[^push]: crates/harlo-agent/src/agent.rs, fn Agent::push
[^branch]: crates/harlo-agent/src/session.rs, fn Log::load_branch
[^append]: crates/harlo-agent/src/session.rs, fn Log::append
[^usage]: crates/harlo-agent/src/session.rs
[^failed]: crates/harlo-agent/src/agent.rs, fn Agent::failed
[^estimate]: crates/harlo-agent/src/agent.rs
[^compact]: crates/harlo-agent/src/compact.rs
[^elide]: crates/harlo-agent/src/compact.rs, fn elide
[^stubs]: crates/harlo-agent/src/stubs.rs
[^overflow]: crates/harlo-agent/src/agent.rs, fn Agent::turn
[^set-aside]: crates/harlo-agent/src/agent.rs, fn Agent::set_aside
[^recall]: crates/harlo-agent/src/tools/mod.rs, fn Tools::recall
[^summarize]: crates/harlo-agent/src/agent.rs, fn Agent::compact
[^splice]: crates/harlo-agent/src/agent.rs, fn Agent::splice_summary
[^carry]: crates/harlo-agent/src/agent.rs, fn Agent::carry_over
[^skills]: crates/harlo-agent/src/skills.rs
[^instructions]: crates/harlo-agent/src/agent.rs, fn Agent::reread_instructions
[^clock]: crates/harlo-agent/src/clock.rs
