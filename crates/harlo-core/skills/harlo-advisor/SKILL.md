---
name: harlo-advisor
description: Ask one other agent in harlo for a second opinion on the current task. Use when the user says "advisor", "second opinion", "what does X think", or wants an outside view without handing the work over.
---

# Advisor

One agent. It looks at your situation and gives a judgment. You decide what to do; the advisor doesn't do the work.

Read the **harlo** skill first: it has the tools.

## Picking the advisor

1. **The user named an agent or model:** use it.
2. **Otherwise:** prefer a different agent or model family from your own, so the opinion is really fresh. `list_agents` shows what's installed.

## The briefing

The advisor has no context, so make the briefing self-contained:
- the question, sharply;
- what you've considered and what you've ruled out;
- the relevant files, by path (let it read them);
- what you want back: "Give me a recommendation, with your reasoning."

End it with:

```
This is analysis only. Do NOT edit, create, or delete any files. Do NOT write code.
```

If the user's request names another skill (`/review`, `$diagnose`, …), they want the advisor to run that skill on this task. Add to the briefing: "Use the `<name>` skill on this task first." Pass along whatever they wrote after it.

## Ask, wait, sum up

Start the advisor with `start_session`, with an `[Advisor] <topic>` title and the briefing as `prompt`. End your turn. Its reply comes back to you as a `[harlo]` message. Then give the user the advisor's verdict and your own recommendation.

## Keeping the advisor

If the user wants it for the next few decisions too, ask it again with `send_message`; it keeps its context. When they're done, or the topic changes enough that a fresh context would serve better, stop using it.
