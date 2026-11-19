---
type: Architecture
title: Core persistence and derived views
description: Ownership of application metadata, harness conversations, search indexes, remote copies, and disposable transcript pages.
covers:
  - crates/harlo-core/src/db.rs
  - crates/harlo-core/src/migrate.rs
  - crates/harlo-core/src/scanner.rs
  - crates/harlo-core/src/remote_sessions.rs
  - crates/harlo-core/src/search.rs
  - crates/harlo-core/src/stats.rs
  - crates/harlo-core/src/sidebar.rs
  - crates/harlo-core/src/shelf.rs
  - crates/harlo-core/src/pages.rs
  - crates/harlo-core/src/templates.rs
  - crates/harlo-core/src/schedules.rs
  - crates/harlo-core/examples/usage_stats.rs
  - crates/harlo-core/tests/memory.rs
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: db
    resource: "repo://crates/harlo-core/src/db.rs"
  - id: store
    resource: "repo://crates/harlo-harness/src/store.rs"
  - id: claude
    resource: "repo://crates/harlo-harness/src/claude.rs"
  - id: search
    resource: "repo://crates/harlo-core/src/search.rs"
  - id: stats
    resource: "repo://crates/harlo-core/src/stats.rs"
  - id: mirror
    resource: "repo://crates/harlo-harness/src/mirror.rs"
  - id: pages
    resource: "repo://crates/harlo-core/src/pages.rs"
  - id: init
    resource: "repo://crates/harlo-core/src/db.rs"
    title: "fn Db::init"
  - id: agent-migration
    resource: "repo://crates/harlo-core/src/db.rs"
    title: "fn Db::move_to_harlo"
  - id: migration
    resource: "repo://crates/harlo-core/src/migrate.rs"
  - id: scanner
    resource: "repo://crates/harlo-core/src/scanner.rs"
  - id: remote-scan
    resource: "repo://crates/harlo-core/src/remote_sessions.rs"
  - id: sidebar
    resource: "repo://crates/harlo-core/src/sidebar.rs"
  - id: shelf
    resource: "repo://crates/harlo-core/src/shelf.rs"
  - id: transcript
    resource: "repo://crates/harlo-core/src/transcript.rs"
    title: "fn Transcript::load_one"
  - id: whole
    resource: "repo://crates/harlo-core/src/transcript.rs"
    title: fn whole
  - id: memory
    resource: "repo://crates/harlo-core/tests/memory.rs"
  - id: example
    resource: "repo://crates/harlo-core/examples/usage_stats.rs"
  - id: templates
    resource: "repo://crates/harlo-core/src/templates.rs"
  - id: schedules
    resource: "repo://crates/harlo-core/src/schedules.rs"
  - id: workflow
    resource: "repo://crates/harlo-core/src/workflow.rs"
---
# Core persistence and derived views

The central ownership rule is that Harlo's database stores application state, **not conversations**. Projects, session flags, worktree records, multirun lanes, settings, schedules, and workflow definitions/runs belong to `Db`; conversation histories stay in the harness session files. A database backup and a transcript backup therefore preserve different things.[^db][^store]

## Stores and their authority

| Store or representation | What it owns | What not to infer |
| --- | --- | --- |
| Application SQLite database | Projects and UI metadata, settings, worktree/run associations, serialized schedules and workflows.[^db] | A session row is not its conversation. |
| Harlo/Claude session files | Persisted messages and agent-specific session structure.[^store][^claude] | Physical line order is not necessarily the selected conversation branch. |
| `search.db` next to the application database | FTS5 index derived from message files, with current-branch message positions.[^search] | A search hit is not a second authoritative copy of the conversation. |
| Statistics cache | Per-file summaries keyed by modified time and size.[^stats] | A warm report need not reread every message. |
| Remote mirror | Locally readable copy of a session file from another host.[^mirror] | An offline copy is not proof that the host has no newer messages. |
| Transcript page file | This process's offloaded long reply text and blocks.[^pages] | It is neither a conversation log nor durable application metadata. |

## Database initialization and migration

`Db::open` creates parent directories, opens SQLite, and initializes WAL mode and tables. Additive migrations inspect existing columns before adding fields such as project setup/icon/color, session modes/model/parent, shelf metadata, and worktree base commit. The schema carries both structured rows and JSON-bearing settings/run records; consumers must preserve the distinction rather than assuming everything is a transcript-shaped JSON document.[^init]

A one-time `move_to_harlo` transaction remaps removed agent choices to Harlo, clears incompatible model selections/settings, and records a migration marker. Historical runs stay as they were. This is preference migration, not conversion of an old agent's conversation into a new agent's history.[^agent-migration]

The earlier piqt-to-Harlo filesystem migration runs only when Harlo has no state of its own. It moves data/config/autostart state but leaves old worktree directories in place, because Git records those paths and the database still refers to them. Renaming the product is not permission to relocate existing worktrees blindly.[^migration]

## Discovery and sidebar projection

The local scanner stats session directories each poll and rereads only new or changed files, returning upserts and removals. The remote scanner lists host files over SSH, batches reads of changed files, and returns host-qualified paths (`ssh://host/…`). Harness readers can summarize large files from a head/tail sample, so discovery is intentionally different from full transcript loading.[^scanner][^remote-scan][^store]

`sidebar` builds a flat list of project headers, sessions, and Other sessions using persisted metadata plus runtime status and worktree records. Its default visible session count is twelve per project before Show more. The list is a projection: grouping or hiding a row does not rewrite the harness's conversation file.[^sidebar][^db]

The Settled shelf is policy rather than a second archive format. Worktree sessions can settle after their pull request merges/closes following the last activity, or after a week of quiet. Pinned, running, and waiting sessions stay put; newer activity can bring a session back, while a manual restore suppresses immediate reshelving until newer work gives another reason.[^shelf]

## Long-history invariants

Search hits identify a message's position on the selected branch, matching saved transcript row keys (`h<index>`). A branch change can change that mapping; using a raw file line number as the row key would target the wrong message.[^search][^transcript]

`pages` keeps old reply bodies in a process-local file while lightweight rows remain in memory. Shared references keep the file alive even after the transcript reloads, and the file is removed once nothing points into it. Consumers can hydrate a row through `transcript::whole`; keeping every hydrated row indefinitely would defeat the paging policy.[^pages][^whole]

The memory integration tests count allocations while reading long sessions and exercise harness readers and transcript paging. Their purpose is to catch accidental whole-file or all-lines retention, not merely to compare visible text. The `usage_stats` example separately demonstrates cold/warm aggregation and a period-limited report.[^memory][^example]

## Persisted intentions versus running work

Templates are JSON in the `templates` setting, capped at forty with the oldest removed. They save agent/model/approval/worktree/first-message setup and fill a draft; selecting one does not start work. Schedules represent day/time, fixed-minute, or one-shot triggers, run while Harlo is running, and can catch up missed work at startup.[^templates][^schedules]

Workflow definitions and workflow runs have separate tables, and the workflow engine keeps each run's own definition copy. Editing a saved workflow must not change a run already underway. Recovery and progression belong to the workflow state machine, not to inferring success from the existence of a session file.[^db][^workflow]

See [session lifetime](/workflows/sessions.md), [core subsystem ownership](/architecture/core.md), and [Git/remote behavior](/workflows/git-and-remote.md) for the operations that consume these stores.

[^db]: crates/harlo-core/src/db.rs
[^store]: crates/harlo-harness/src/store.rs
[^claude]: crates/harlo-harness/src/claude.rs
[^search]: crates/harlo-core/src/search.rs
[^stats]: crates/harlo-core/src/stats.rs
[^mirror]: crates/harlo-harness/src/mirror.rs
[^pages]: crates/harlo-core/src/pages.rs
[^init]: crates/harlo-core/src/db.rs, fn Db::init
[^agent-migration]: crates/harlo-core/src/db.rs, fn Db::move_to_harlo
[^migration]: crates/harlo-core/src/migrate.rs
[^scanner]: crates/harlo-core/src/scanner.rs
[^remote-scan]: crates/harlo-core/src/remote_sessions.rs
[^sidebar]: crates/harlo-core/src/sidebar.rs
[^shelf]: crates/harlo-core/src/shelf.rs
[^transcript]: crates/harlo-core/src/transcript.rs, fn Transcript::load_one
[^whole]: crates/harlo-core/src/transcript.rs, fn whole
[^memory]: crates/harlo-core/tests/memory.rs
[^example]: crates/harlo-core/examples/usage_stats.rs
[^templates]: crates/harlo-core/src/templates.rs
[^schedules]: crates/harlo-core/src/schedules.rs
[^workflow]: crates/harlo-core/src/workflow.rs
