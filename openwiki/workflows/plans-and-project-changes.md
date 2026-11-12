---
type: workflow
title: Plans, helpers, and project changes
description: How session-scoped orchestration tools update shared plans and safely coordinate Git changes, per-step commits, worktrees, and helper bring-over.
tags: [workflow, plans, orchestration, git, worktrees]
verified:
  - by: openwiki/0.9.0
    at: 2026-11-12T20:55:29.850Z
sources:
  - id: openwiki-source-c5e0556ff4a54e3a14503a14
    resource: repo://crates/harlo-app/qml/ChangesPanel.qml
  - id: openwiki-source-0120ee12ce1363bc8be995c8
    resource: repo://crates/harlo-app/qml/PlanDock.qml
  - id: openwiki-source-707e4f109038162ab1383c58
    resource: repo://crates/harlo-app/src/plans.rs
  - id: openwiki-source-3da8c8a8e687e436a4a8c49a
    resource: repo://crates/harlo-core/src/git.rs
  - id: openwiki-source-0605559ba1474db590a7724b
    resource: repo://crates/harlo-core/src/orchestration.rs
  - id: openwiki-source-75bcb97e87a29c42db5d4dfd
    resource: repo://crates/harlo-core/src/plan.rs
generated: { by: "harlo", at: "2026-11-12T20:55:29.850Z" }
---

# Plans, helpers, and project changes

## Tools cross an application boundary

Harlo's orchestration bridge is `harlo --mcp-bridge`: a stdio MCP server forwarding calls to the running desktop over a local socket, with `HARLO_SESSION` identifying the caller. The socket name incorporates the app database path so isolated test instances do not share the same endpoint. Without a running desktop, the bridge offers no tools.

Harlo's own agent can also send `harlo_call` frames on its RPC pipe when started with `--harlo-tools`; replies return as `harlo_result`. Both routes ultimately reach desktop orchestration. Calls needing workspace state cross the Qt thread; slower finishing work such as reading histories, manipulating plans, bringing over changes, or calling OpenWiki runs in the background.

Helper-start policy is configurable: only when asked, offer a split for the user's decision (the default), or start independently. This is desktop orchestration, not the in-process `task` mechanism described in [agent execution](../concepts/agent-execution-and-safety.md).

## Shared plan state

A plan has stable step IDs, titles/details, pending/active/done/skipped status, summaries, user-edit markers, optional helper/branch assignments, and commit outcomes. Core owns the state transitions; `harlo-app::plans` serializes both UI edits and tool mutations under one mutex and persists one JSON file per session under the app data folder's `plans/` directory. The identity uses the saved session file when known, otherwise the live key.

The intended ordinary flow is:

1. Read the latest plan, including user changes.
2. Set remaining work: `Plan::set` replaces pending steps while retaining active, done, and skipped ones.
3. Start a step and capture its repository baseline if needed. Ordinary start resets other active steps to pending.
4. Finish with a summary or mark skipped.

For helper runs, `alongside` joins the preceding step's entire stage. Stages run sequentially; members of a stage can run together. Therefore, marking a step alongside is a dependency assertion: it must not require the output of another member of that stage.

The desktop plan dock opens unfinished plans by default and folds a nonempty plan when every step is done or skipped. Manual folding or opening overrides that default per session within the dock's lifetime; it is presentation state, not a persisted plan mutation. Switching sessions or completion state also closes any step editor.

## Optional per-step commits are conservative

Finishing a step does **not** always commit. The project must enable step commits, the step must not be skipped, and a start snapshot must exist. The snapshot records HEAD, hashes of dirty files, and start time.

Before committing, Harlo checks:

- HEAD is unchanged: a commit or checkout during the step makes attribution ambiguous.
- The step did not modify a file already dirty at the baseline, including restoring such a file to HEAD.
- No other session is recorded as working under the repository root since the step started.

Untouched pre-existing dirty files are allowed and excluded. If attribution fails, the step can still be done; `commit_note` explains why nothing was committed and leaves the changes for the user. Successful commits explicitly name the step's files rather than consuming unrelated staged changes.

This is deliberately more conservative than “commit all current changes.” Preserve that distinction when adding new tool-driven edits or helper workflows.

## Git, turn undo, and helper worktrees

Core uses the user's Git CLI so normal configuration, credentials, hooks, and signing apply. Git operations are serialized per repository to avoid index-lock races; porcelain-v2 status and parsed diffs support individual-hunk staging/reversion.

The Changes panel retains a known pull-request row while its lookup refreshes, avoiding a disappearing row on reopening. An existing URL remains openable, but creating a new request is disabled until lookup reaches `ready`; retained display data is not treated as permission to create during loading.

Turn undo is separate from step commits: snapshots under `refs/harlo/turns/<worktree-key>/` record state before turns, and separate undo refs preserve state before undo. Worktree-qualified prefixes prevent one checkout's snapshot being mistaken for another's.

`bring_over` transfers a helper worktree's committed, uncommitted, and untracked changes since its start into the current checkout **as uncommitted changes**. It verifies that both checkouts belong to the same Git repository, determines the baseline, and checks patch applicability. Conflicting files are left unchanged and returned in `clashed`; applicable files are reported in `applied`. Nothing is staged or committed by this operation. A helper already working in the same folder has nothing to bring over.

Review clashes from the helper's worktree rather than treating partial application as a complete merge. For SSH-backed checkouts, use the [remote routing layer](../integrations/remote-machines-and-models.md) rather than issuing local filesystem operations on an `ssh://` URI.

## Verification and neighboring workflows

`plan::tests::commits_a_steps_own_changes` checks that new/modified/deleted step files are committed while a user's pre-existing file remains untracked, then verifies refusals for mixed changes and moved HEAD. A corresponding remote test requires SSH. Plan tests also exercise helper-stage grouping and user modifications.

```sh
cargo test -p harlo-core --no-default-features plan
cargo test -p harlo-core --no-default-features stages_and_reverts_single_hunks
```

[OpenWiki generation](../integrations/openwiki.md) reaches agents through the same orchestration boundary but owns a separate durable page queue. A shared task plan is not the wiki queue, and marking a task step done cannot substitute for OpenWiki's successful page submission and finish calls.
