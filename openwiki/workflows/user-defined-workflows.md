---
type: workflow
title: User-defined task workflows
description: How reusable implementation, check, and review sequences become persisted runs, retry failed stages, and recover across application restarts.
tags: [workflow, orchestration, retries, worktrees, persistence]
verified:
  - by: openwiki/0.9.0
    at: 2026-11-17T18:09:09.933Z
sources:
  - id: openwiki-source-b40a189805c9df2ec5c0212b
    resource: repo://crates/harlo-app/src/workspace/workflows.rs
  - id: openwiki-source-5c64f648a13e5cdcc6879a6f
    resource: repo://crates/harlo-core/src/workflow.rs
generated: { by: "harlo", at: "2026-11-17T18:09:09.933Z" }
---

# User-defined task workflows

## Definitions and constraints

A workflow definition is an ordered sequence with exactly one implementation stage first, followed by one or more check or review stages. Checks run a command in the run's worktree; an empty command is resolved from the project's test command when the run starts. Reviews use a selected agent, falling back to the reviewer chosen for the run. Names must be non-empty, the definition is limited to 12 stages, and each check/review can return work to implementation at most 10 times. The built-in **Implement and verify** definition runs the project's tests and then asks the selected reviewer to inspect the change.

The app's Settings → Workflows page lists the built-in definition and saved user definitions. Users can create, duplicate, edit and delete their own definitions. Editing a reusable definition does not alter an active run: each run stores a resolved snapshot, including concrete test commands and reviewer selection. Definitions are persisted through the core database's workflow-definition operations; project settings remember the last selected workflow and last used test command.

## Starting and progressing a run

A run starts from a project's menu with a task, optional acceptance criteria, implementation agent/model, reviewer/model, and approval mode. Orchestration must be enabled, the definition must still exist and validate, and the task must be non-empty. If any check still needs a test command, the user must supply one. An empty project workflow selection means the built-in definition.

Core's workflow state machine produces actions; the workspace performs them. Beginning first creates a dedicated Git worktree/branch. The implementation session works there, then each configured check and review runs in order. A check's process exit result, not an agent's summary, is its verdict. A failed check or review requesting changes returns to the same implementation session with feedback, then reruns every later check and review. Review approvals and successful checks advance to the next stage. Per-stage retry counts are bounded by each stage's configured `tries`; once exhausted, the workflow pauses for the user rather than treating failure as success.

Dispatch sequence numbers and attempt numbers bind results to the stage that requested them. Late reports or check results from earlier dispatches/attempts are logged and ignored, preventing stale asynchronous completions from advancing a newer attempt. If an agent ends without `report_done`, it can be reminded once; repeated missing reports or a failed launch hold the run for user action.

## Persistence, pause, and recovery

The workspace saves a run after every state transition and before executing the resulting actions. The run record includes its workflow snapshot, worktree and session locations, stage/attempt, waits, feedback, checks, lifecycle state, and event log. On application startup, unfinished runs are loaded from SQLite and upgraded from older serialized forms. Once each machine's sessions have been listed, recovery reconnects to matching sessions or pauses with a reason; remote recovery waits up to 90 seconds for host session discovery.

Pause lets work already underway finish but schedules no new action; resume continues from the stored wait or next stage. Stop ends orchestration but deliberately leaves its sessions and worktree. Once all stages pass, the user can merge the worktree branch, publish it for a pull request, or remove the worktree. Only a completed run may be merged or published; removing an unfinished run stops it first. These workflow runs are not the shared plan card described in [plans and project changes](plans-and-project-changes.md).

## Desktop and tests

The workflow card is associated with the implementation/reviewer session, displays stages, attempt and waiting reason, and provides pause/resume, stop, and expanded run-log controls. Workflow definitions and run state are exposed through workspace properties; run updates also notify the phone bridge. This UI reflects the durable run state but does not own the transition logic.

Core tests exercise ordering, retries, stale dispatch rejection, pause/resume and recovery. App workspace tests cover persistence and restart boundaries; run `cargo test -p harlo-core workflow` and `cargo test -p harlo-app workflows` for focused checks. For the separate user-authored task-plan and helper workflow, see [plans and project changes](plans-and-project-changes.md); for the desktop session boundary, see [sessions and desktop](../architecture/sessions-and-desktop.md).
