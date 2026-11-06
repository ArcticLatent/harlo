---
name: harlo
description: harlo's tools for running other coding agents (Claude, OpenCode, pi, omp, Hermes) as sessions you start, prompt, read and stop. Read this before using harlo-handoff, harlo-advisor or harlo-committee, or whenever the user asks you to start, delegate to, or check on another agent in harlo.
---

harlo is the desktop app you're running in. It shows each coding-agent session as a tab, and its MCP server `harlo` lets you run other sessions yourself. The tools reach you as `mcp__harlo__<tool>` (or `harlo_<tool>`, depending on your agent).

The tools exist only inside harlo. If they're missing, tell the user that harlo has to be running, and that pi and omp need harlo's bridge (harlo: Settings → Orchestration). Don't try to work around it.

## Tools

**`list_agents`**: the agents harlo can start (`claude`, `opencode`, `pi`, `omp`, `hermes`) and whether each is installed.

**`start_session`**: `{ agent, prompt, title?, model?, thinking?, folder?, worktree?, notify? }`. Starts a session and sends it `prompt`.
- `agent`: an id from `list_agents`.
- `model`: `provider/model` (e.g. `anthropic/claude-sonnet-4-5`). Leave it out for the agent's default.
- `thinking`: its thinking level (`low`, `medium`, `high`), if not the agent's default.
- `folder`: defaults to your own folder.
- `worktree`: a new branch name. The session then works in a new git worktree on that branch, isolated from your checkout. The worktree is made in the background, so the session id comes later (`list_sessions`).
- `notify`: defaults to true. Its report, its questions for you, and when it waits for the user come back to you. `false`: it's on its own (a handoff).

It returns `{ session, title, agent, folder }`. The session is your helper: it starts in your permission mode, and the user sees it under your session and can open it. harlo tells it it's a helper, and how to report back. Helpers can start helpers of their own, three deep at most. Once a helper has reported success, harlo moves it to its project's Settled shelf; it comes back if it works again.

**`send_message`**: `{ session, message }`. A follow-up, queued if the session is busy. Send it to `"helpers"` to tell every helper you started that's still open the same thing (a change of plan, "stop and report").

**`read_session`**: `{ session, messages? }`. The session's status (`busy`, `idle`, `waiting for input`) and its last reply, or its last `messages` messages.

**`list_sessions`**: harlo's open sessions: id, title, agent, status, folder, the session that started each one (`startedBy`), and which one is you (`you`).

**`stop_session`**: `{ session }`. Stops what it's doing. The session stays open, and `send_message` continues it.

**`request_approval`**: `{ question, options? }`. Asks the user to decide something only they may decide: going ahead with a risky or irreversible step, or a choice that's theirs. harlo shows it in your session and on their phone, and notifies them. No agent can answer it. End your turn after asking; their answer comes back as a `[harlo]` message.

**`propose_helpers`**: `{ why, helpers: [{ title, prompt, agent?, model?, worktree? }] }`. Offered only when harlo is set to have agents offer splits (the default). When a big task splits into parts that could be done at the same time (each substantial, not editing the same files), or a part is better done by another agent or model, offer the split instead of starting helpers yourself. harlo shows the user why and what each helper would do. If they agree, harlo starts the helpers as `start_session` would (`agent` defaults to yours) and tells you their ids; if they don't, do the work yourself. Either way, end your turn after offering.

### When you're a helper

A session another one started gets two more tools:

**`report_done`**: `{ outcome, summary, files? }`. Once, when the task is done or can't go on, then end your turn. `outcome` is `succeeded` or `failed`; never hide a failure in the summary. `summary` is three sentences: what you did, what you found, what's left.

**`ask_parent`**: `{ question }`. A question the task doesn't settle. End your turn after asking; the answer comes as a message.

If your briefing gives you a helper id, pass it as `helper` with each of these tools (and `request_approval`).

## Briefing

A new session has none of your context: no conversation, no decisions, no knowledge of what you've tried. Its first prompt has to be a self-contained briefing:
- the task, in the imperative;
- why it matters;
- the relevant files, by path (let it read them; don't paste them);
- what's done and what's been ruled out;
- what "done" looks like.

Keep the user's intent exactly. If they asked only for an investigation, say "Do NOT edit files."

## Waiting

Agents take a while; 10–30 minutes is normal. Start every independent helper, then end your turn. harlo sends you a message by itself, which starts your next turn:

- when a helper finishes: `[harlo] The <Agent> session "<title>" (<session>) finished and reported: succeeded.` and its summary (or, if it didn't report, its last reply). If it worked in a worktree and the branch it started from has moved on since, harlo says by how many commits, so you know merging may conflict;
- when it asks you something: `… asks you:` and the question. Answer with `send_message` when the answer follows from the task or the user's instructions; otherwise ask the user first;
- when it waits for the user (an approval, or a question that's theirs): `… is waiting for the user`. Only the user can answer it. Tell them if they should know, or go on with other work.

So don't poll `read_session` or `list_sessions` to check on it, and don't send it hurry-ups. A follow-up sent with `send_message` comes back the same way. When every helper has finished, tell the user each one's outcome, what it found, and what's still open. Never call a reported failure a success.

## Diagrams

harlo draws Mermaid diagrams (```` ```mermaid ```` blocks) in replies as pictures, on the desktop and on the phone. When you report a flow, a structure or how helpers' work fits together, a diagram is often clearer than a list.

## Choosing an agent

- The user named one: use it.
- A second opinion is worth more from a different agent or model family than your own (say Claude and an OpenCode or pi session on another provider's model).
- Otherwise, use your own agent.
