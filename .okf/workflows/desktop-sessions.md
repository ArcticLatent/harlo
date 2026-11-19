---
type: Workflow
title: Desktop prompts and durable workflow runs
description: Follow a prompt from the composer into a shared session and distinguish it from persisted multi-stage workflow execution.
covers:
  - crates/harlo-app/src/chat_session.rs
  - crates/harlo-app/src/workspace.rs
  - crates/harlo-app/src/workspace/
  - crates/harlo-app/src/registry.rs
  - crates/harlo-app/src/orchestration.rs
  - crates/harlo-app/src/plans.rs
  - crates/harlo-app/qml/Composer.qml
  - crates/harlo-app/qml/WorkflowDialog.qml
  - crates/harlo-app/qml/WorkflowCard.qml
  - crates/harlo-app/qml/RunView.qml
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: session
    resource: "repo://crates/harlo-app/src/chat_session.rs"
  - id: workflow
    resource: "repo://crates/harlo-app/src/workspace/workflows.rs"
  - id: send
    resource: "repo://crates/harlo-app/src/chat_session.rs"
    title: "fn ChatSession::send"
  - id: binding
    resource: "repo://crates/harlo-app/src/chat_session.rs"
  - id: answers
    resource: "repo://crates/harlo-app/src/chat_session.rs"
    title: "fn ChatSession::answer"
  - id: branch
    resource: "repo://crates/harlo-app/src/chat_session.rs"
    title: "fn ChatSession::edit_and_resend"
  - id: undo
    resource: "repo://crates/harlo-app/src/phone_server.rs"
    title: fn undo_op
  - id: dispatch
    resource: "repo://crates/harlo-app/src/workspace/workflows.rs"
  - id: sequence
    resource: "repo://crates/harlo-app/src/workspace/workflows.rs"
  - id: recovery
    resource: "repo://crates/harlo-app/src/workspace/workflows.rs"
    title: "fn Workspace::recover_workflow"
  - id: plans
    resource: "repo://crates/harlo-app/src/plans.rs"
  - id: orchestration
    resource: "repo://crates/harlo-app/src/orchestration.rs"
  - id: startup
    resource: "repo://crates/harlo-app/src/main.rs"
  - resource: "repo://crates/harlo-app/src/workspace.rs"
  - resource: "repo://crates/harlo-app/src/workspace/"
  - resource: "repo://crates/harlo-app/src/registry.rs"
  - resource: "repo://crates/harlo-app/qml/Composer.qml"
  - resource: "repo://crates/harlo-app/qml/WorkflowDialog.qml"
  - resource: "repo://crates/harlo-app/qml/WorkflowCard.qml"
  - resource: "repo://crates/harlo-app/qml/RunView.qml"
---
# Desktop prompts and durable workflow runs

There are two different loops: a **session turn**, driven by a harness-backed `SessionHandle`, and a **workflow run**, whose state machine can create sessions, run checks, request reviews, and survive desktop restarts. Do not infer workflow completion from an idle chat alone.[^session][^workflow]

## Sending a turn

1. `ChatSession::send` ignores an empty prompt only when there are no attachments. It translates the UI's `steer` and `followUp` strings to explicit send modes; other values mean normal sending.
2. Normal plan-mode prompts receive the plan instruction suffix, except `/btw` side questions. A separate `Typed` record preserves the original words and file paths so an unsent message can be returned without the injected context.
3. A simple local prompt can go directly to `prompt_typed`. Attachments, mentioned sessions, remote destinations, or managed model-server startup use blocking background preparation instead.
4. Preparation starts a needed model server, prepares files for the destination host, reads mentioned saved sessions, and prefixes their context before sending. If server startup fails, the UI gets the original words/files back and a send failure.[^send]

**Pitfall:** the displayed composer text and the final agent prompt are intentionally different. Debugging context injection requires examining plan notes, attachment prefixes, and session-reference blocks rather than assuming the composer is a byte-for-byte transport log.[^send]

## Observing and answering

The registry lets desktop views and other app integrations share the same live session handle. `ChatSession` subscribes through a generation-tagged Qt sink; rebinding removes the old subscription and discards stale queued updates. Updates drive transcript changes and status/request properties. Requests are answered through the handle, not by directly marking the dialog complete.[^binding][^answers]

Editing/resending an earlier user message resolves its saved entry by ordinal on the current branch, then asks the harness to fork and reloads the session. A visible row number is not itself the persisted entry ID. File-change undo is a different operation, backed by turn snapshots rather than transcript branching.[^branch][^undo]

## Executing a workflow

The app delegates transition logic to `harlo_core::workflow`. `workflow_event` clones the run, applies `step`, calls `keep_workflow`, then executes returned actions. Actions include making a worktree, starting implementation/review sessions, prompting a session, running a check, and notifying the user.[^dispatch]

The intended ordering is **persist the new dispatch before its side effect**. There is an important limitation: `keep_workflow` logs a database save failure but still updates memory and dispatches actions. This is not a transactional exactly-once guarantee under persistence failure.[^dispatch]

Each awaited session/check carries a dispatch sequence; stale session events are rejected when their sequence no longer matches the run's current wait. Check output is bounded to a 256 KiB tail, with a 12-line card preview and roughly half-second progress updates. Checks have a 30-minute limit, and remote checks run on the selected machine rather than silently falling back locally.[^workflow][^sequence]

## Restart and recovery

Saved unfinished runs are loaded and upgraded, but recovery waits for the relevant machines' sessions to be listed. Remote listing has a 90-second wait before pausing. During recovery the app can find an unrecorded session by its workflow marker, matching machine and worktree path; it then asks core recovery logic what to do next. Live keys are useful during one process lifetime; saved session files plus machine identity are the cross-restart identity.[^workflow][^recovery]

Crash-injection points exist before dispatch and after session start but before its file is recorded (`HARLO_TEST_CRASH`). These specifically exercise the dangerous interval between external work beginning and its identity reaching storage. Use isolated test data when testing these paths: the hook deliberately kills the process.[^workflow]

## Plans, helpers, and the UI

Shared plans are serialized through the app's plan store because UI edits and agent tool calls arrive from different threads. The orchestration bridge is installed at startup for agents Harlo launches; Workspace remains the coordinator for opening helpers and publishing their progress. These mechanisms overlap with workflow sessions but are not substitutes for the workflow state machine.[^plans][^orchestration][^startup]

Closing the window may leave runs active; quitting the process stops agents and disconnects hosts. See [desktop lifecycle](/architecture/desktop.md). For a stalled run, first distinguish a pending approval/question, a check still running, a machine still unlisted, and a paused recovery; do not create a replacement session before inspecting the persisted run and its dispatch.[^workflow][^startup]

[^session]: crates/harlo-app/src/chat_session.rs
[^workflow]: crates/harlo-app/src/workspace/workflows.rs
[^send]: crates/harlo-app/src/chat_session.rs, fn ChatSession::send
[^binding]: crates/harlo-app/src/chat_session.rs
[^answers]: crates/harlo-app/src/chat_session.rs, fn ChatSession::answer
[^branch]: crates/harlo-app/src/chat_session.rs, fn ChatSession::edit_and_resend
[^undo]: crates/harlo-app/src/phone_server.rs, fn undo_op
[^dispatch]: crates/harlo-app/src/workspace/workflows.rs
[^sequence]: crates/harlo-app/src/workspace/workflows.rs
[^recovery]: crates/harlo-app/src/workspace/workflows.rs, fn Workspace::recover_workflow
[^plans]: crates/harlo-app/src/plans.rs
[^orchestration]: crates/harlo-app/src/orchestration.rs
[^startup]: crates/harlo-app/src/main.rs
