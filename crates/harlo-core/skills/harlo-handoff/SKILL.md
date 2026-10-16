---
name: harlo-handoff
description: Hand the current task to another agent in harlo, with its full context. Use when the user says "handoff", "hand off", "hand this to", or wants to pass the work to another agent.
---

# Handoff

Pass the current task to a fresh session: its context, decisions, failed attempts and constraints. The new agent starts with **no context**, so the first prompt must be a self-contained briefing.

Read the **harlo** skill first: it has the tools.

## From the user's request

1. **The agent:** the one they named (`claude`, `opencode`, `pi`, `omp`, maybe with a model). Otherwise, your own agent.
2. **Isolation:** "in a worktree" or "worktree" → pass `worktree` with a short branch name taken from the task.
3. **The task:** everything else they said.

## The briefing

```
## Task
[What to do, in the imperative.]

## Context
[Why this task exists; what it needs to know.]

## Relevant files
- `path/to/file` — [what it is, why it matters]

## Current state
[What's done, what works, what doesn't.]

## What was tried
- [Approach] — [why it failed or was dropped]

## Decisions
- [Decision] — [why]

## Done when
- [ ] [Criterion]

## Constraints
- [What it must not do, or must keep]
```

Keep the task's meaning. An investigation stays an investigation ("Do NOT edit files."). A fix says "implement the fix". A refactor says "refactor, don't rewrite". Carry over the user's exact intent.

## Start it

Call `start_session` with a `[Handoff] <task>` title, the briefing as `prompt`, and the agent, model and worktree you chose. Then tell the user which session took it over (they can open it from your session's subagents).

Don't wait for it or poll it. It's the user's session now.
