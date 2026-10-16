---
name: harlo-committee
description: Form a committee of two agents in harlo to step back, find the root cause and agree on a plan. Use when stuck, going in circles, tunnel-visioned, or facing a hard planning problem.
---

# Committee

Two agents with fresh context and different backgrounds plan a solution in parallel, then argue until they agree.

Read the **harlo** skill first: it has the tools.

## Members

Contrast is the point, so pick two agents or models from different families. For example: `claude`, and `opencode`, `pi` or `omp` on another provider's model. Use the strongest reasoning models available. If the user names members, use those. Check with `list_agents` what's installed. If only one family is available, tell the user the committee will be less independent.

## Rules

- **No edits.** Every message to a member ends with:

  ```
  This is analysis only. Do NOT edit, create, or delete any files. Do NOT write code.
  ```

- **Trust the notifications.** Each member's reply comes back to you as a `[harlo]` message. Don't poll, hurry them, or stop them. Thinking for 15–30 minutes is normal.

## Steps

1. Write one prompt about the problem: the goal, the symptoms, what's been tried, the relevant files by path, and the question ("What's the root cause, and what's the plan?"). Don't steer them toward your own theory.
2. Start both with `start_session`, each with a `[Committee] <task>` title and the same prompt. Then end your turn.
3. Wait for both replies.
4. Where they disagree, pass each one the other's arguments with `send_message` and ask them to respond.
5. Repeat until they agree.

Then give the user the agreed plan, where the members disagreed, and how that was settled.
