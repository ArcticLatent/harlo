---
type: Workflow
title: Git operations and remote machines
description: Host-qualified paths, SSH execution, worktree snapshots, selective integration, and conservative commit boundaries.
covers:
  - crates/harlo-core/src/git.rs
  - crates/harlo-core/src/git_hosting.rs
  - crates/harlo-core/src/commitmsg.rs
  - crates/harlo-core/src/splitdiff.rs
  - crates/harlo-core/src/remote.rs
  - crates/harlo-core/src/remote_sessions.rs
  - crates/harlo-core/src/remote_helper.rs
  - crates/harlo-core/src/plan.rs
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: remote
    resource: "repo://crates/harlo-core/src/remote.rs"
  - id: launch
    resource: "repo://crates/harlo-harness/src/process.rs"
  - id: git
    resource: "repo://crates/harlo-core/src/git.rs"
  - id: ssh
    resource: "repo://crates/harlo-core/src/remote.rs"
    title: fn ssh_options
  - id: quote
    resource: "repo://crates/harlo-core/src/remote.rs"
    title: fn quote_path
  - id: run
    resource: "repo://crates/harlo-core/src/remote.rs"
  - id: scan
    resource: "repo://crates/harlo-core/src/remote_sessions.rs"
  - id: mirror
    resource: "repo://crates/harlo-harness/src/mirror.rs"
  - id: agent
    resource: "repo://crates/harlo-core/src/agent_host.rs"
  - id: helper
    resource: "repo://crates/harlo-core/src/remote_helper.rs"
  - id: locking
    resource: "repo://crates/harlo-core/src/git.rs"
    title: fn repo_lock
  - id: splitdiff
    resource: "repo://crates/harlo-core/src/splitdiff.rs"
  - id: operations
    resource: "repo://crates/harlo-core/src/git.rs"
  - id: hosting
    resource: "repo://crates/harlo-core/src/git_hosting.rs"
  - id: commitmsg
    resource: "repo://crates/harlo-core/src/commitmsg.rs"
  - id: turn
    resource: "repo://crates/harlo-core/src/git.rs"
  - id: snapshot
    resource: "repo://crates/harlo-core/src/git.rs"
    title: fn snapshot_worktree
  - id: undo
    resource: "repo://crates/harlo-core/src/git.rs"
    title: fn undo_turn
  - id: merge
    resource: "repo://crates/harlo-core/src/git.rs"
    title: fn merge_branch
  - id: bring
    resource: "repo://crates/harlo-core/src/git.rs"
    title: fn bring_over
  - id: plan-check
    resource: "repo://crates/harlo-core/src/plan.rs"
    title: fn changed_since
  - id: plan-commit
    resource: "repo://crates/harlo-core/src/plan.rs"
    title: fn commit
  - id: plan
    resource: "repo://crates/harlo-core/src/plan.rs"
---
# Git operations and remote machines

Core represents remote locations as `ssh://<host>/<absolute path>` in the same path-bearing records used for local projects, session directories, and session files. That representation supplies identity and joining; it does **not** make a remote path suitable for a local filesystem call. Actual reads, writes, and execution must route through the host-aware layer.[^remote]

## Resolve the machine before executing

`remote::split`, `join`, and host lookup bridge the stored path and configured SSH destination. Harness launch keeps SSH arguments/prelude separate from the remote program, directory, and session path; `on_host` removes the URI prefix only when preparing the remote operation. Git's command wrapper similarly strips host prefixes from path arguments and runs on the directory's host.[^remote][^launch][^git]

SSH uses OpenSSH multiplexing with a ten-minute persistence default, noninteractive authentication, connection timeout, and keepalives. Host-provided options precede defaults because SSH takes the first value. The default host-key policy accepts new keys but refuses changed keys; this is not a password-prompting remote terminal.[^ssh]

Shell construction quotes individual words and has a separate `quote_path` helper that intentionally expands `~`/`~/`. Commands receive an augmented PATH. `run_with_input` drains stdout and stderr on separate threads to avoid a full-pipe deadlock, enforces a deadline, and kills/waits for the SSH child on timeout. `run_ok` variants additionally turn unsuccessful exit status into an error: merely obtaining `Output` is not success.[^quote][^run]

## Remote data is not local state

Remote session discovery returns host-qualified paths and batches changed-file reads; remote mirrors stream file bytes into local copies and validate a previous suffix before appending. Offline viewing can use that copy, but executing a resumed session still needs its host. The host's own agent installation, credentials, settings, and session directory remain there.[^scan][^mirror][^agent]

Helper upload and model-server control are described in [agents and engines](/architecture/core-engines.md). A helper is cached by build on the host, not copied for every Git/status/read operation.[^helper]

## Git preserves the user's Git environment

Git operations use the CLI, preserving user configuration, credentials, hooks, and signing behavior. Status parses porcelain v2 and retains index/worktree status independently; untracked and conflicted states are explicit. A file can have both staged and unstaged changes, so one boolean “modified” cannot describe all actions.[^git]

Mutating paths such as stage, unstage, commit, push, and undo take an in-process lock keyed by repository path to reduce index-lock races. This is coordination within these APIs, not a lock over external Git commands or the agent's own shell. Hunk operations build patches from the original Git headers and raw hunk text; the side-by-side diff renderer is a presentation layer with alignment fillers, not the patch source.[^locking][^git][^splitdiff]

The operation contracts are deliberately narrow:[^operations]

- Stage-file uses `git add -- <path>`; stage-all includes untracked files.
- Unstage keeps the working tree; revert-file discards unstaged tracked changes and leaves untracked-file disposal to the caller.
- Commit commits staged changes and rejects an empty message; hooks can still reject it.
- First push can set `origin HEAD` as upstream; subsequent push uses ordinary `git push`.
- Pull is fast-forward-only rather than an implicit merge of diverged histories.

Git-hosting integration is separate: it interprets GitHub/GitLab CLI sign-in information and Forgejo token/API behavior. Model-generated commit messages are also separate from committing: `commitmsg` sends the request/diff through a tool-free, nonpersisted one-shot agent invocation, preferring Harlo and falling back to Claude when needed.[^hosting][^commitmsg]

## Turn snapshots and undo

Before a turn, `snapshot_turn` snapshots a repository, or attempts snapshots of nested repositories when the session directory is a folder of repositories. It skips a local root/home repository to avoid snapshotting a broad dotfiles tree. Worktrees share Git refs, so turn refs are namespaced by a stable hash of each worktree's path; old unnamespaced turn refs are not used.[^turn]

`snapshot_worktree` uses a scratch `GIT_INDEX_FILE`, adds tracked and nonignored untracked content, writes a tree and a synthetic commit, and updates a private ref. The real index, HEAD, and branch are untouched. This captures working-tree content; it is not a copy of staged-versus-unstaged bookkeeping.[^snapshot]

`undo_plan` computes files to restore/remove and whether HEAD moved relative to the snapshot parent. `undo_turn` first saves the current state under undo refs, restores files with `git restore --worktree`, and removes files added since the target snapshot. It leaves index and commits alone. Failure is not transactional rollback: files already restored remain restored, so inspect the returned error and current tree before retrying.[^undo]

## Integrate helper work without claiming too much

`merge_branch` refuses tracked uncommitted changes in the destination, merges with `--no-ff --no-edit`, and attempts `merge --abort` on failure. Untracked files do not preemptively block the operation; Git itself refuses an overwrite.[^merge]

`bring_over` instead imports helper work **uncommitted**. It requires distinct checkouts of the same repository, resolves a start point from the recorded start commit or merge base, includes the helper's committed and uncommitted/nonignored untracked content, and checks whether the patch applies. If the whole patch does not fit, it applies only clean files and reports clashing ones left alone. Neither checkout is staged or committed by this operation; partial application is a supported result.[^bring]

Shared-plan step commits are conservative. A step snapshot records HEAD and dirty-file hashes. `changed_since` rejects a moved HEAD and files mixed with preexisting modifications, including previously dirty files restored to HEAD. `plan::commit` commits only the selected paths while unrelated staged changes remain. The plan module also documents the caller's requirement to exclude intervening work by another session: file comparison alone is not authorship proof.[^plan-check][^plan-commit][^plan]

## Pitfalls to retain in new operations

- Do not canonicalize or read an `ssh://` path locally and mistake failure for absence; route by machine first.[^remote]
- Keep transport success, command exit success, and useful application output distinct.[^run]
- Do not describe turn undo as reset, branch rewind, or index restoration.[^undo]
- Do not describe bring-over as an all-or-nothing merge or a commit.[^bring]
- A finished plan step does not guarantee a safe automatic commit; attribution checks may deliberately leave changes for the user.[^plan]

Related: [sessions](/workflows/sessions.md), [persistence](/architecture/core-persistence.md), and [core coordination](/architecture/core.md).

[^remote]: crates/harlo-core/src/remote.rs
[^launch]: crates/harlo-harness/src/process.rs
[^git]: crates/harlo-core/src/git.rs
[^ssh]: crates/harlo-core/src/remote.rs, fn ssh_options
[^quote]: crates/harlo-core/src/remote.rs, fn quote_path
[^run]: crates/harlo-core/src/remote.rs
[^scan]: crates/harlo-core/src/remote_sessions.rs
[^mirror]: crates/harlo-harness/src/mirror.rs
[^agent]: crates/harlo-core/src/agent_host.rs
[^helper]: crates/harlo-core/src/remote_helper.rs
[^locking]: crates/harlo-core/src/git.rs, fn repo_lock
[^splitdiff]: crates/harlo-core/src/splitdiff.rs
[^operations]: crates/harlo-core/src/git.rs
[^hosting]: crates/harlo-core/src/git_hosting.rs
[^commitmsg]: crates/harlo-core/src/commitmsg.rs
[^turn]: crates/harlo-core/src/git.rs
[^snapshot]: crates/harlo-core/src/git.rs, fn snapshot_worktree
[^undo]: crates/harlo-core/src/git.rs, fn undo_turn
[^merge]: crates/harlo-core/src/git.rs, fn merge_branch
[^bring]: crates/harlo-core/src/git.rs, fn bring_over
[^plan-check]: crates/harlo-core/src/plan.rs, fn changed_since
[^plan-commit]: crates/harlo-core/src/plan.rs, fn commit
[^plan]: crates/harlo-core/src/plan.rs
