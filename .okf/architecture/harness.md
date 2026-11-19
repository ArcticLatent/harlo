---
type: Architecture
title: Harness protocols and session files
description: How Harlo and Claude Code become one event stream without pretending their storage and runtime protocols are identical.
covers:
  - crates/harlo-harness/
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: boundary
    resource: "repo://crates/harlo-harness/src/lib.rs"
  - id: process
    resource: "repo://crates/harlo-harness/src/process.rs"
  - id: remote
    resource: "repo://crates/harlo-harness/src/process.rs"
    title: fn remote_script
  - id: events
    resource: "repo://crates/harlo-harness/src/event.rs"
  - id: translate
    resource: "repo://crates/harlo-harness/src/event.rs"
    title: "fn Translator::translate"
  - id: framing
    resource: "repo://crates/harlo-harness/src/framing.rs"
  - id: claude
    resource: "repo://crates/harlo-harness/src/claude.rs"
  - id: claude-run
    resource: "repo://crates/harlo-harness/src/claude_run.rs"
  - id: store
    resource: "repo://crates/harlo-harness/src/store.rs"
  - id: branches
    resource: "repo://crates/harlo-harness/src/branches.rs"
  - id: mirror
    resource: "repo://crates/harlo-harness/src/mirror.rs"
  - id: plans
    resource: "repo://crates/harlo-harness/src/coding_plans.rs"
  - id: plan-data
    resource: "repo://crates/harlo-harness/src/coding_plans.json"
  - id: hooks
    resource: "repo://crates/harlo-harness/src/lib.rs"
  - id: log
    resource: "repo://crates/harlo-harness/src/protocol_log.rs"
  - id: errors
    resource: "repo://crates/harlo-harness/src/error.rs"
  - id: files
    resource: "repo://crates/harlo-harness/src/file_blocks.rs"
  - id: demo
    resource: "repo://crates/harlo-harness/src/demo.rs"
  - id: fixture-tests
    resource: "repo://crates/harlo-harness/src/claude_run.rs"
    title: fn replays_a_real_session
  - id: example
    resource: "repo://crates/harlo-harness/examples/list_sessions.rs"
  - id: manifest
    resource: "repo://crates/harlo-harness/Cargo.toml"
  - resource: "repo://crates/harlo-harness/"
---
# Harness protocols and session files

`harlo-harness` is the boundary between agent-specific processes/files and the application's common vocabulary. `HarnessKind` identifies Harlo, Claude, or the synthetic Demo; only Harlo and Claude belong to the provider list. The crate exposes process configuration, normalized events, session summaries, and file readers, while keeping the Claude runtime adapter private.[^boundary]

Read [session driving](/workflows/sessions.md) for the consumer, [persistence](/architecture/core-persistence.md) for derived indexes, and [remote execution](/workflows/git-and-remote.md) for machine boundaries.

## Ownership and request routing

`Harness::spawn` returns a handle and an unbounded event receiver. Harlo gets a child with piped stdin/stdout/stderr, its own process group, and `kill_on_drop`; separate tasks write commands, read frames, collect stderr, and wait for exit. Claude dispatches to `claude_run::spawn`, and Demo to its synthetic implementation. `SessionTarget` distinguishes a new persisted session, ephemeral execution, and resuming a particular file: these are not interchangeable ways to obtain an empty transcript.[^process]

Commands awaiting answers have IDs and oneshot senders in the pending map. Fire-and-forget sends still receive IDs, but their replies can surface as `HarnessEvent::Response`; callers must not interpret every response as transcript content. Shutdown tracks process groups so quitting can terminate descendants, first with SIGTERM and then SIGKILL after a bounded wait.[^process]

Remote launch separates the host identity and SSH arguments from the working directory and program **on that host**. The command builder quotes words, changes directory, and applies environment overrides. When cleanup is needed it preserves the child's exit status and runs cleanup rather than replacing the shell with `exec`.[^remote]

## Normalize at the boundary, preserve authority

The shared event stream separates run lifecycle, message lifecycle, blocks, tool execution, queues, questions, status, limits, and errors. A block delta is provisional; `BlockEnd` carries authoritative content and `MessageEnd` carries the authoritative final message. `RunEnded` and `Settled` remain distinct events, and background-task counts have their own event. Unknown top-level frames become `Other` rather than silently disappearing.[^events]

The pi-shaped translator generates message IDs from a sequence and assumes messages do not interleave. Block updates identify their content index and kind; unrecognized assistant block-event shapes yield no event. A new protocol feature therefore needs deliberate normalization, not a UI branch on an agent's raw JSON.[^translate]

Claude's adapter accepts the common pi-shaped commands but drives Claude in stream-json mode. It converts Claude content to the common message shape and exposes approvals/questions as UI requests. Its saved-file reader is separate from this live adapter: a displayable history is not itself a running Claude process.[^claude][^claude-run]

## Framing invariants

Records split on LF, not Unicode line separators; CRLF is accepted, and EOF can deliver a final non-newline-terminated record. `ChunkAssembler` accepts one sequence at a time, checks identity/count/declared size/next index, decodes base64, verifies the final byte count, then validates UTF-8 and JSON. Interleaving or out-of-order chunks are protocol failures, not separate messages. The declared chunk length is limited to `MAX_LINE_BYTES` (64 MiB).[^framing]

Do not replace this reader with text splitting on arbitrary line boundaries: valid JSON strings can contain U+2028/U+2029. Nor should an interrupted chunk sequence be treated as successful partial content.[^framing]

## Saved sessions are trees, not flat append logs

Harlo files live under its XDG data session directory, with a session header and `id`/`parentId` entries. The latest written entry selects the current branch; `branches` can enumerate branches and copy one into a new file. Reading every message in physical order would include abandoned conversation branches.[^store][^branches]

Sidebar metadata is deliberately cheaper than reading a conversation: files up to 512 KiB are read whole; larger files use their first and last 64 KiB, cut to complete lines. A fixed-width title slot may precede the header and be rewritten on rename. `SessionInfo::model` is only what the sampled portions establish; an empty value is not proof that the session never selected a model.[^store]

Claude files are read from its project session layout and translated into the common message representation. Remote session copies use `mirror`: append-only growth is optimized by checking the copy's last bytes before fetching a suffix; otherwise the file is fetched whole. SSH output streams into the copy rather than collecting the whole file in memory, and the copy remains readable offline.[^claude][^mirror]

## Configuration and diagnostics

Coding-plan subscriptions are a Claude-specific integration, not direct provider calls by Harlo. The built-in catalog is `coding_plans.json`; keys live in the user's Harlo coding-plan config. A Claude run receives endpoint, models, and key through a user-readable-only settings file, and validation is a short Claude request.[^plans][^plan-data]

The bridge, remote-start, and remote-program hooks are installed through `OnceLock`; orchestration itself is an atomic on/off flag. Remote-start runs before bridge selection, including when orchestration is disabled. `nearest_level` maps known thinking-level names to the nearest available rank, preferring the higher rank on a tie; unknown names and unknown model levels are preserved rather than guessed.[^hooks]

Protocol logging applies to subsequently started harnesses and records sent/received frames and stderr in JSONL. Treat those files as conversation-bearing diagnostics, not just timing logs. Errors distinguish startup, communication, and protocol failures and preserve their explanatory cause text.[^log][^errors]

## Remaining files and test aids

- `file_blocks` removes attached-file markup and the ask-first note for display; stripping display decoration does not change what the agent received.[^files]
- `demo` emits normalized events directly, including synthetic streaming, questions, history, and subagents. It tests downstream behavior, not a real provider's transport.[^demo]
- `tests/fixtures/claude-turns.jsonl` replays real Claude wire frames through the runtime adapter, checking tool results, assistant text, and turn boundaries.[^fixture-tests]
- `examples/list_sessions.rs` exercises provider enumeration and summary reading. `Cargo.toml` declares the crate's async/process/serialization dependencies.[^example][^manifest]

[^boundary]: crates/harlo-harness/src/lib.rs
[^process]: crates/harlo-harness/src/process.rs
[^remote]: crates/harlo-harness/src/process.rs, fn remote_script
[^events]: crates/harlo-harness/src/event.rs
[^translate]: crates/harlo-harness/src/event.rs, fn Translator::translate
[^framing]: crates/harlo-harness/src/framing.rs
[^claude]: crates/harlo-harness/src/claude.rs
[^claude-run]: crates/harlo-harness/src/claude_run.rs
[^store]: crates/harlo-harness/src/store.rs
[^branches]: crates/harlo-harness/src/branches.rs
[^mirror]: crates/harlo-harness/src/mirror.rs
[^plans]: crates/harlo-harness/src/coding_plans.rs
[^plan-data]: crates/harlo-harness/src/coding_plans.json
[^hooks]: crates/harlo-harness/src/lib.rs
[^log]: crates/harlo-harness/src/protocol_log.rs
[^errors]: crates/harlo-harness/src/error.rs
[^files]: crates/harlo-harness/src/file_blocks.rs
[^demo]: crates/harlo-harness/src/demo.rs
[^fixture-tests]: crates/harlo-harness/src/claude_run.rs, fn replays_a_real_session
[^example]: crates/harlo-harness/examples/list_sessions.rs
[^manifest]: crates/harlo-harness/Cargo.toml
