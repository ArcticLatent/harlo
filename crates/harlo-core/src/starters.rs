//! Conversation starters for the new-session page: a visible prompt plus
//! instructions the agent receives with it (adapted from OpenChamber's
//! session prompts, MIT: see THIRD-PARTY-NOTICES.md).
//!
//! A starter travels inside the user message as a trailing
//! `<harlo-starter id="…">…</harlo-starter>` block, so a resumed session
//! still shows which starter it began with.

use serde_json::json;

pub struct Starter {
    pub id: &'static str,
    pub label: &'static str,
    /// Icon name (qml/Icon.qml).
    pub icon: &'static str,
    /// What the message says when the user adds nothing.
    pub prompt: &'static str,
    pub instructions: &'static str,
}

pub const STARTERS: &[Starter] = &[
    Starter {
        id: "explore",
        label: "Explore the codebase",
        icon: "compass-3",
        prompt: "Give me a high-level tour of this codebase.",
        instructions: r#"The user wants to get oriented in this codebase — a high-level tour, as if you were onboarding a new contributor. Investigate first, then explain; do not guess from file or symbol names alone.

Explore the actual repository: entry points, the top-level structure, how it is built and run, and the main modules and how they connect. Read enough real code to be accurate.

Then give a clear orientation covering:
- The big picture: what this project is and how it is structured at a high level.
- Main parts: the key modules, packages, or directories, what each is responsible for, and where they live.
- How it fits together: the main flow — how a request or action moves through the system, and how the pieces talk to each other.
- Conventions worth knowing: notable patterns, where shared code, types, and config live, and anything non-obvious a newcomer would trip on.
- Where to start: a few concrete pointers for finding your way around or making a first change.

Keep it a readable orientation, not an exhaustive file-by-file dump — favor the structure and the mental model over listing everything. Lead with the big picture, then drill down. If the user named a specific area, focus the tour there.

Respond in the same language the user uses."#,
    },
    Starter {
        id: "catch-up",
        label: "Catch me up",
        icon: "history",
        prompt: "Catch me up on where this project is right now.",
        instructions: r#"The user is returning to this project after stepping away and wants to quickly get their bearings — a quick, easy-to-digest "here's where you are and where to pick up", not a status report. Investigate the actual repository state first, then orient them conversationally. Do not assume; check.

Quietly inspect git state first, and do this work silently — the user wants the takeaway, not a play-by-play of the commands you ran. Look at: the current branch and whether it is the repo's default branch (main/master, or whatever this repo uses), uncommitted changes (status and diff), recent commits, and where the branch stands relative to its remote.

Build context in LAYERS — they combine, they are not either/or. Uncommitted changes (when present) are the focal point, but understand them THROUGH the surrounding context, because work in progress is usually part of something bigger.

First, get the branch context:
- If this is NOT the default branch (a feature branch): understand what the branch is for as a whole. Read its recent commits and their diffs — not all of them, just enough, going back until the intent and how it is being implemented become clear. Also check whether the branch has its OWN open pull request, even when there are uncommitted changes — the PR explains what the current diff is in service of (continuing the feature, or addressing review feedback) and helps you judge whether the work looks finished or still mid-flight. If the branch is behind its remote (someone pushed), mention that as a heads-up.
- If this IS the default branch: take a light skim of the last few commits (no deep dive) to see whether the uncommitted work is a continuation of recent work, and of what.

Then focus and synthesize:
- If there are uncommitted changes, lead with them — what they were doing and why, what looks done versus still in progress, and where they likely stopped — interpreted through the branch context above (is this completing the feature? addressing review? a new direction?). Open with that, e.g. "Looks like you were in the middle of X…".
- If the tree is clean, orient from the branch's own work and PR (feature branch) or the recent commits (default branch).

End with a clear next step, and make it about continuing the actual work, not housekeeping. The fact that they ran this command means they stepped away — if the work were finished they would most likely have shipped it already, so assume there is more to do and point to the substantive next piece ("next you'd wire X into Y and handle Z"). Only suggest housekeeping — pushing, opening a PR, running checks — when there is genuinely nothing left to build, or when it is truly the most useful thing to do next.

Hard rules:
- Only ever discuss the CURRENT branch and its own work. Never mention unrelated branches, other people's PRs, review requests assigned to the user, or PRs that belong to other branches — that is noise here.
- Use ahead/behind and commit history to understand intent, not as something to dump. Don't pad with raw git mechanics (exact commit counts, "ahead of origin by N", remote-tracking detail) unless it is genuinely the single most useful thing to say.
- Depth goes into your understanding, not the length of the reply. Keep the output short, easy to digest, and scannable — a couple of sentences of orientation plus a clear next step. Write like a teammate catching them up, not a CI summary.

Respond in the same language the user uses."#,
    },
    Starter {
        id: "weigh",
        label: "Weigh my options",
        icon: "scales-3",
        prompt: "Help me decide how to approach this.",
        instructions: r#"The user knows WHAT they want to do but not HOW to approach it. Help them choose a direction — this is about weighing options and recommending one, not producing a detailed plan and not writing code.

If you have an `ask` tool, use it only for clarifying decisions that have a small set of concrete answer options you already know from the conversation or your investigation — choices like option A/B/C, scope boundaries, or edge-case behavior. Ask open-ended questions, including what the user wants in the first place, in plain assistant text. Never invent speculative options just to fit the ask tool.

First, investigate. Once the user describes the goal, read the relevant code, existing patterns, and constraints so your options are grounded in this codebase rather than generic advice. Make sure you actually understand what they are trying to achieve and why. Ask a clarifying question only if a key constraint is missing and would actually change the options.

Then lay out 2-3 genuinely distinct approaches — real alternatives, not minor variations of one idea. Include the approaches that properly deliver what the user wants, even when they are more involved; never leave out a strong option just because it is harder to build. For each, cover:
- what it involves, in a sentence or two
- how well it actually satisfies the user's goal — does it fully solve it, or only partially?
- how it fits (or fights) the existing patterns in this codebase
- trade-offs and consequences: complexity, risk, blast radius, effort, long-term maintainability
- when it is the right choice

Then give a clear recommendation. Anchor it on what best serves the user's actual need and intent — NOT on whatever is fastest, easiest, or the path of least resistance. Effort and complexity are consequences to lay out honestly, never reasons to steer the user toward a weaker option. Never recommend a watered-down or partial solution just because the proper one is more work: if the approach that truly fits is also the hard one, recommend it and be upfront about what it will cost. Favor a simpler option only when it genuinely meets the goal about as well. State which one you would pick and why, and name what would change your mind (for example, "go with A unless you expect X, in which case B").

Keep it concrete and scannable. Do not start implementing and do not write a step-by-step plan — once the user picks a direction, they can take it into planning or build it directly.

Respond in the same language the user uses."#,
    },
    Starter {
        id: "plan-feature",
        label: "Start feature planning",
        icon: "survey",
        prompt: "I want to start planning a feature.",
        instructions: r#"The user wants to plan a feature through a guided, back-and-forth conversation. They will describe an idea — often briefly and informally. Your job is to turn that idea into a concrete, validated implementation plan, without guessing.

Run this as a dialogue, not a one-shot answer.

If you have an `ask` tool, use it only for clarifying decisions that have a small set of concrete answer options you already know from the conversation or your investigation — choices like option A/B/C, scope boundaries, or edge-case behavior. Ask open-ended questions, including what the user wants in the first place, in plain assistant text. Never invent speculative options just to fit the ask tool.

1. Understand before asking. Once the user describes the idea, first investigate the codebase yourself — read the relevant files, existing patterns, data flow, and constraints. Ground every question in what the code actually shows, not in assumptions.

2. Ask in small batches. Ask at most 3 clarifying questions at a time — a number a person can comfortably answer in one reply. Prefer concrete, decision-oriented questions (option A/B/C, edge cases, scope boundaries) over vague open-ended ones. Number them.

3. Keep going until it is resolved. After each batch of answers, integrate them, do any further code investigation the answers require, then ask the next batch. Continue until there are no unresolved decisions or implementation details left. Do not stop early or start summarizing prematurely.

4. Surface what the user has not considered. Proactively raise edge cases, pitfalls, affected modules, migration/backward-compatibility concerns, and trade-offs the user likely did not think about. Fold these into your questions so the user decides — never silently decide for them.

5. Do not write code or begin implementing during this phase. Planning is for understanding and deciding only.

6. When everything is settled, produce the final implementation plan: a clear, ordered breakdown of the work, the files and areas affected, the decisions that were made (and why), known risks, and any remaining assumptions flagged explicitly. The plan must reflect the user's actual answers — never fill gaps with guesses.

Respond in the same language the user uses."#,
    },
    Starter {
        id: "debug",
        label: "Debug an issue",
        icon: "bug",
        prompt: "I want to debug an issue.",
        instructions: r#"The user wants help debugging an issue. Drive this as a focused root-cause investigation — not a plan, and not an immediate fix.

If you have an `ask` tool, use it only for clarifying decisions that have a small set of concrete answer options you already know from the conversation or your investigation — choices like option A/B/C, scope boundaries, or edge-case behavior. Ask open-ended questions, including what the user wants in the first place, in plain assistant text. Never invent speculative options just to fit the ask tool.

1. Get the symptom. When the user describes the problem, capture exactly what is observed versus expected — error messages, stack traces, failing behavior, and when it started. If a key detail is missing to even begin, ask for it briefly.

2. Form hypotheses. List the most likely causes, ordered by probability given the symptom and the code, and be explicit about your reasoning.

3. Investigate to confirm or rule out. Read the relevant code, trace the data and control flow, and check the leading hypotheses against what the code actually does. Prefer evidence from the code over speculation.

4. Ask only what you need. If you need a reproduction, logs, environment details, or a specific value to narrow it down, ask for the minimum required — in small batches — rather than guessing.

5. Identify the root cause. Before touching any code, state the actual cause and the evidence for it, and distinguish the root cause from its symptoms.

6. Only then propose a fix — the smallest change that addresses the root cause, plus how to verify it. Do not start editing code until the cause is confirmed or the user asks you to.

Respond in the same language the user uses."#,
    },
    Starter {
        id: "review",
        label: "Review my changes",
        icon: "search-eye",
        prompt: "Review the changes made in this workspace.",
        instructions: r#"Review the changes in this workspace and judge whether they are correct and adequate — not just whether they contain catastrophic bugs.

The diff is the source of truth. Read the relevant code around the diff too, not only the diff itself, so you understand the change in context.

First, understand the intent and whether it was achieved:
- Work out what these changes are trying to do — the intent behind them — from the diff and the surrounding code.
- Judge whether the implementation actually achieves that intent, and whether it is the smallest correct way to do it. Call out where the change is incomplete, only partially solves the goal, misses cases it clearly set out to handle, or solves it in a way that will not hold up.

Then look for concrete problems. Report real failure modes, not abstract suspicions, and do not nitpick without impact.

Correctness focus:
- race conditions, stale async results, event ordering
- data loss or failed writes
- lifecycle and cleanup (listeners, timers, subscriptions, resources)
- non-transitive comparators or unstable sorting
- state/store fanout and render performance
- optimistic state rollback and reconciliation
- accessibility semantics
- regressions introduced by the changes
- missing implementations across affected modules or targets when the diff clearly introduced the gap
- missing targeted tests for risky or regression-prone changes
- clear CLAUDE.md or AGENTS.md violations that apply to the changed files

Security and supply-chain focus (when the diff touches these):
- dependencies, build/release/CI scripts
- auth, tokens, secrets, credentials
- filesystem boundaries and path traversal
- shell execution
- network calls, telemetry, exfiltration
- IPC, native bridge, updater, desktop shell
- hidden behavior behind small diffs or broad refactors

Do not report:
- pre-existing issues unrelated to the diff
- pedantic nitpicks a senior engineer would not flag, or issues a linter would catch
- subjective style preferences not required by CLAUDE.md or AGENTS.md
- speculative concerns you cannot tie to a concrete failure
- rules mentioned in CLAUDE.md or AGENTS.md but explicitly silenced in the code

Validation pass:
- Before reporting an issue, re-check it against the diff plus only the context you actually read.
- For CLAUDE.md or AGENTS.md violations, verify the rule applies to the affected file path and cite the exact rule.
- If you cannot tie a finding to a concrete impact, drop it.

Classify each finding:
- blocker: likely regression, data loss, security issue, broken invariant, or a serious correctness problem — or the change does not actually achieve its intent
- non-blocker: a real but minor issue, a test gap, or a maintainability concern
- nit: mention only if useful, never treat as blocking

This is a review only — do not edit, fix, or commit anything unless the user asks you to.

Output:
- Start with one or two sentences: what the change does and whether it achieves its intent.
- Then list findings grouped by severity. For each: short title, why it is a real problem, the affected file path, and category (correctness / security / rule violation / adequacy gap).
- If you find nothing real, say so plainly instead of inventing findings.

Keep the review concise and practical. Respond in the same language the user uses."#,
    },
];

pub fn find(id: &str) -> Option<&'static Starter> {
    STARTERS.iter().find(|s| s.id == id)
}

/// The message for a starter: the user's text (or the starter's prompt)
/// followed by the instructions block.
pub fn wrap(starter: &Starter, text: &str) -> String {
    let text = text.trim();
    let text = if text.is_empty() { starter.prompt } else { text };
    format!("{text}\n\n<harlo-starter id=\"{}\">\n{}\n</harlo-starter>", starter.id, starter.instructions)
}

/// The message without a trailing starter block, and the starter it named.
pub fn strip(text: &str) -> (&str, Option<&'static Starter>) {
    const OPEN: &str = "<harlo-starter id=\"";
    let trimmed = text.trim_end();
    if !trimmed.ends_with("</harlo-starter>") {
        return (text, None);
    }
    let Some(start) = trimmed.rfind(OPEN) else { return (text, None) };
    let id = trimmed[start + OPEN.len()..].split('"').next().unwrap_or_default();
    (trimmed[..start].trim_end(), find(id))
}

/// `[{ id, label, icon, prompt }]` for the new-session page.
pub fn json() -> String {
    serde_json::Value::Array(STARTERS.iter().map(|s| json!({ "id": s.id, "label": s.label, "icon": s.icon, "prompt": s.prompt })).collect()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_and_strips() {
        let s = find("debug").unwrap();
        let sent = wrap(s, "");
        assert!(sent.starts_with(s.prompt));
        let (rest, found) = strip(&sent);
        assert_eq!((rest, found.map(|f| f.id)), (s.prompt, Some("debug")));
        let sent = wrap(s, "  The build fails on CI  ");
        let (rest, found) = strip(&sent);
        assert_eq!((rest, found.map(|f| f.id)), ("The build fails on CI", Some("debug")));
        assert_eq!(strip("plain").1.map(|f| f.id), None);
        for s in STARTERS {
            assert!(!s.instructions.contains("question` tool") && !s.instructions.to_lowercase().contains("openchamber"), "{}", s.id);
        }
    }
}
