---
type: Workflow
title: Opening, driving, and observing sessions
description: Session lifetime, normalized transcript updates, prompt preparation, and the distinction between completion and stopping.
covers:
  - crates/harlo-core/src/session.rs
  - crates/harlo-core/src/transcript.rs
  - crates/harlo-core/src/attach.rs
  - crates/harlo-core/src/session_ref.rs
  - crates/harlo-core/src/spending.rs
  - crates/harlo-core/src/subagents.rs
  - crates/harlo-core/src/plugins.rs
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: handle
    resource: "repo://crates/harlo-core/src/session.rs"
  - id: driver
    resource: "repo://crates/harlo-core/src/session.rs"
    title: fn drive
  - id: updates
    resource: "repo://crates/harlo-core/src/session.rs"
  - id: typed
    resource: "repo://crates/harlo-core/src/session.rs"
  - id: attach
    resource: "repo://crates/harlo-core/src/attach.rs"
  - id: refs
    resource: "repo://crates/harlo-core/src/session_ref.rs"
  - id: plugins
    resource: "repo://crates/harlo-core/src/plugins.rs"
  - id: reduce
    resource: "repo://crates/harlo-core/src/transcript.rs"
    title: "fn Transcript::apply"
  - id: events
    resource: "repo://crates/harlo-harness/src/event.rs"
  - id: markdown
    resource: "repo://crates/harlo-core/src/markdown.rs"
  - id: diff
    resource: "repo://crates/harlo-core/src/transcript.rs"
    title: fn diff_rows
  - id: listdiff
    resource: "repo://crates/harlo-core/src/listdiff.rs"
  - id: pages
    resource: "repo://crates/harlo-core/src/pages.rs"
  - id: spending
    resource: "repo://crates/harlo-core/src/spending.rs"
  - id: subagents
    resource: "repo://crates/harlo-core/src/subagents.rs"
  - id: git
    resource: "repo://crates/harlo-core/src/git.rs"
    title: fn snapshot_turn
---
# Opening, driving, and observing sessions

A live session owns a harness process and a transcript independently of any view. `SessionHandle` sends controls to the async driver; views implement `SessionSink`, receiving updates on the session task and forwarding them to their UI thread. Closing a subscription is not the same operation as aborting the agent.[^handle][^driver]

## Open is not start

`spawn` starts with a running harness, whereas `open_saved` reads a saved conversation without starting one. A later command can start it, resuming the configured file. Status explicitly distinguishes `Loading`, `Offline`, `Starting`, `Idle`, `Busy`, and `Exited`; only Starting/Idle/Busy report `has_process`. Context reports and Markdown export deliberately do not start the harness.[^handle]

A new subscriber receives a reset before incremental row updates. This permits multiple observers to share one process rather than replaying work for each window. Updates carry more than rows: metadata, queues, questions and cancellations, status entries, subagents, limits, and noteworthy completion/error signals all travel through the same session-facing API.[^updates]

## Prompt preparation and cancellation

1. Attachments become `<file name="…">…</file>` blocks before the message. Text files larger than 512 KiB are referenced rather than inlined; images also become image content, and other files are named for the agent to open.[^attach]
2. `@session:<id>` references capture recent messages as an attachment at send time. They are snapshots, not live links that silently reread another conversation later.[^refs]
3. `prompt_typed` preserves the original words, attachments, and whether the user actually sent them. A spending hold rejects the prompt before enqueuing it. The normal, steering, and follow-up modes remain explicit.[^handle][^typed]
4. Preparing a turn snapshot may delay the prompt. Stop during preparation returns whole unsent messages, oldest first; `Typed` lets a composer restore the user's draft rather than the expanded attachment/plan text. If no composing sink exists, the session retains those messages for the next one.[^typed][^driver]

`/plugin` is intercepted as a user command rather than sent to the model, but the sender's `by_you` flag matters: an agent or scheduled prompt does not gain that command behavior merely by containing the same words. Plugin execution uses Claude's plugin command in the session's directory and on its machine; Harlo can consume the installed skills too.[^typed][^plugins]

## From events to display rows

The driver consumes the [normalized harness vocabulary](/architecture/harness.md), while `Transcript::apply` reduces events into display state. Final message/block content reconciles streamed content; tool execution has its own call IDs and results. A renderer should not independently append final text after already displaying the deltas.[^reduce][^events]

Markdown is split into top-level blocks and cached by source text. This keeps old blocks stable while a trailing block changes. Rows use shared ownership (`Arc`), and `diff_rows` produces keyed list changes rather than resetting the complete model. The change ordering is contractual: updates use pre-structural indices, then removal, then insertion.[^markdown][^diff][^listdiff]

While events continue, the stream-flush interval is 100 ms. Long saved conversations initially expose 200 rows; settled replies more than 1,000 rows back can move their long text to a page file. These are display/memory policies, not limits on persisted conversation history.[^driver][^pages]

## Completion, stopping, and gates

Run-ended, settled, and background-task information are separate harness events. Completion notifications can include a count of commands left running; Stop has its own `SessionUpdate::Stopped`. Consumers should not equate every transition away from busy with a successful task completion.[^events][^updates]

Agent UI requests and Harlo-owned approvals share display machinery but not answer ownership. `open_gate` generates a `harlo-gate-` ID, and its answer is emitted as `GateAnswered` for the application rather than simply treated as an agent response. Restart requests likewise distinguish restarting only if idle from restarting when the current work has finished.[^handle][^updates]

Spending limits use reported session totals and aggregate daily spending; zero means no limit. Crossing a limit stops work and holds subsequent messages until more is allowed. These are reactive limits based on reported costs, not a guarantee that a request's unknown future charge can never overshoot a budget.[^spending]

Subagent presentation merges lifecycle/progress events with the `get_subagents` list. The event source also includes detached background jobs that the list does not, so rebuilding exclusively from a poll can lose visible work.[^subagents]

## Debugging checklist

- A saved conversation can be offline without being broken; inspect status before assuming a process exists.[^handle]
- Restore returned drafts from `Typed` where available, not from the expanded prompt.[^typed]
- Preserve reset-before-deltas and list-change ordering when connecting another frontend.[^updates][^listdiff]
- Use [persistence ownership](/architecture/core-persistence.md) to distinguish durable conversation files from page caches and search indexes; use [Git turn snapshots](/workflows/git-and-remote.md) before treating undo as transcript editing.[^pages][^git]

[^handle]: crates/harlo-core/src/session.rs
[^driver]: crates/harlo-core/src/session.rs, fn drive
[^updates]: crates/harlo-core/src/session.rs
[^typed]: crates/harlo-core/src/session.rs
[^attach]: crates/harlo-core/src/attach.rs
[^refs]: crates/harlo-core/src/session_ref.rs
[^plugins]: crates/harlo-core/src/plugins.rs
[^reduce]: crates/harlo-core/src/transcript.rs, fn Transcript::apply
[^events]: crates/harlo-harness/src/event.rs
[^markdown]: crates/harlo-core/src/markdown.rs
[^diff]: crates/harlo-core/src/transcript.rs, fn diff_rows
[^listdiff]: crates/harlo-core/src/listdiff.rs
[^pages]: crates/harlo-core/src/pages.rs
[^spending]: crates/harlo-core/src/spending.rs
[^subagents]: crates/harlo-core/src/subagents.rs
[^git]: crates/harlo-core/src/git.rs, fn snapshot_turn
