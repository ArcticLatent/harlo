---
name: harlo
description: harlo's tools for running other coding agents (Claude, OpenCode, pi, omp) as sessions you start, prompt, read and stop. Read this before using harlo-handoff, harlo-advisor or harlo-committee, or whenever the user asks you to start, delegate to, or check on another agent in harlo.
---

harlo is the desktop app you're running in. It shows each coding-agent session as a tab, and its MCP server `harlo` lets you run other sessions yourself. The tools reach you as `mcp__harlo__<tool>` (or `harlo_<tool>`, depending on your agent).

The tools exist only inside harlo. If they're missing, tell the user that harlo has to be running, and that pi and omp need harlo's bridge (harlo: Settings → Orchestration). Don't try to work around it.

## Tools

**`list_agents`**: the agents harlo can start (`claude`, `opencode`, `pi`, `omp`) and whether each is installed.

**`start_session`**: `{ agent, prompt, title?, model?, folder?, worktree?, notify? }`. Starts a session and sends it `prompt`.
- `agent`: an id from `list_agents`.
- `model`: `provider/model` (e.g. `anthropic/claude-sonnet-4-5`). Leave it out for the agent's default.
- `folder`: defaults to your own folder.
- `worktree`: a new branch name. The session then works in a new git worktree on that branch, isolated from your checkout. The worktree is made in the background, so the session id comes later (`list_sessions`).
- `notify`: defaults to true.

It returns `{ session, title, agent, folder }`. The session is your subagent: the user sees it under your session and can open it.

**`send_message`**: `{ session, message }`. A follow-up, queued if the session is busy.

**`read_session`**: `{ session, messages? }`. The session's status (`busy`, `idle`, `waiting for input`) and its last reply, or its last `messages` messages.

**`list_sessions`**: harlo's open sessions: id, title, agent, status, folder, the session that started each one (`startedBy`), and which one is you (`you`).

**`stop_session`**: `{ session }`. Stops what it's doing. The session stays open, and `send_message` continues it.

## Briefing

A new session has none of your context: no conversation, no decisions, no knowledge of what you've tried. Its first prompt has to be a self-contained briefing:
- the task, in the imperative;
- why it matters;
- the relevant files, by path (let it read them; don't paste them);
- what's done and what's been ruled out;
- what "done" looks like.

Keep the user's intent exactly. If they asked only for an investigation, say "Do NOT edit files."

## Waiting

Agents take a while; 10–30 minutes is normal. When a session you started finishes, harlo sends you a message by itself:

```
[harlo] The <Agent> session "<title>" (<session>) finished. Its last reply:

<its reply>
```

So don't poll `read_session` or `list_sessions` to check on it, and don't send it hurry-ups. Carry on with other work, or end your turn. The message starts your next turn. A follow-up sent with `send_message` comes back the same way. Use `notify: false` only for fire-and-forget work.

## Choosing an agent

- The user named one: use it.
- A second opinion is worth more from a different agent or model family than your own (say Claude and an OpenCode or pi session on another provider's model).
- Otherwise, use your own agent.
