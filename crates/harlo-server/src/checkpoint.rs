//! Goal checkpoints: where a goal's work stood each time its reviewer
//! looked, kept with the goal, so a run that was cut short picks up from
//! what was reviewed instead of reading the whole conversation again.
//!
//! A checkpoint is written from what the run and its reviewer already
//! said: no model is asked for it. It belongs to its goal, which belongs
//! to one session, so a session in another worktree never sees it.
//!
//! When the goal is taken up again, what the session did after the
//! checkpoint (and so after any review) is listed beside it, read from
//! the session's own messages.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;

use harlo_engine::{Message, Part, ToolState, now};
use serde_json::{Map, Value, json};

/// Characters of one line a checkpoint or a digest keeps: a command, a
/// path or a sentence fits, a pasted file doesn't.
const LINE: usize = 160;

/// Characters kept of what the agent or the reviewer said.
const SAID: usize = 600;

/// The newest things a checkpoint or a digest lists; older ones are
/// counted. With [`LINE`] it keeps a checkpoint under about 1,000 tokens.
const LISTED: usize = 20;

/// The tools that change a file, by the name of their `path` argument.
const CHANGING: &[&str] = &["edit", "write"];

/// What the reviewer said with its verdict, for the checkpoint.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Reviewed {
    /// What shows the goal is met, when it is.
    pub verified: Option<String>,
    /// What is still missing, or what the goal waits for.
    pub missing: Option<String>,
    /// The next step.
    pub next: Option<String>,
}

/// What a stretch of a session shows without asking a model.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Digest {
    /// What was done, in order, a line each: files changed, commands run.
    pub done: Vec<String>,
    /// The calls that failed or never finished.
    pub failed: Vec<String>,
    /// The last command run and how it ended.
    pub command: Option<String>,
    /// What the person said.
    pub said: Vec<String>,
    /// The agent's last words.
    pub words: Option<String>,
    /// Why the stretch stopped short, when it did.
    pub cut: Option<String>,
}

impl Digest {
    /// Whether the stretch shows nothing worth telling.
    pub(crate) fn is_empty(&self) -> bool {
        self.done.is_empty() && self.failed.is_empty() && self.said.is_empty() && self.cut.is_none()
    }
}

/// `text` on one line, at most `most` characters.
fn clip(text: &str, most: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= most {
        return line;
    }
    let mut cut: String = line.chars().take(most).collect();
    cut.push('…');
    cut
}

/// An argument of a tool call, as text.
fn argument<'a>(input: &'a Map<String, Value>, name: &str) -> &'a str {
    input.get(name).and_then(Value::as_str).unwrap_or_default()
}

/// How a command ended, from its exit code.
fn ended(exit: Option<i64>) -> String {
    exit.map_or_else(|| "no exit code".to_owned(), |code| format!("exit {code}"))
}

/// Adds `line` to `lines` as its newest; a line already there moves.
fn add(lines: &mut Vec<String>, line: String) {
    lines.retain(|l| *l != line);
    lines.push(line);
}

/// What `messages` show was done, said and left unfinished.
///
/// The goal's own prompts and other messages the server wrote in the
/// person's place are left out: they are not something that happened.
pub(crate) fn digest(messages: &[Message]) -> Digest {
    let mut out = Digest::default();
    for message in messages {
        match message {
            Message::User { text, .. } => out.said.push(clip(text, LINE)),
            Message::Shell(run) => {
                let line = format!(
                    "the person ran `{}` ({})",
                    clip(&run.command, LINE),
                    ended(run.exit.map(i64::from))
                );
                out.command = Some(line.clone());
                add(&mut out.done, line);
            }
            Message::Assistant(step) => {
                for part in &step.content {
                    match part {
                        Part::Text { text } if !text.trim().is_empty() => {
                            out.words = Some(clip(text, SAID));
                        }
                        Part::Tool { name, state, .. } => call(&mut out, name, state),
                        Part::Text { .. } | Part::Reasoning { .. } => {}
                    }
                }
                if let Some(error) = &step.error {
                    out.cut = Some(clip(&error.message, LINE));
                }
            }
            Message::Compaction(_) => {
                add(&mut out.done, "the conversation was summarized".to_owned());
            }
            Message::Idle { outcome, error, .. } if outcome != "succeeded" => {
                let why = error
                    .as_ref()
                    .map(|e| format!(": {}", clip(&e.message, LINE)))
                    .unwrap_or_default();
                out.cut
                    .get_or_insert_with(|| format!("the run ended as {outcome}{why}"));
            }
            Message::Synthetic { .. } | Message::Idle { .. } => {}
        }
    }
    out
}

/// Adds a tool call to `out`; a call that only looked adds nothing.
fn call(out: &mut Digest, name: &str, state: &ToolState) {
    match state {
        ToolState::Completed {
            input, metadata, ..
        } => {
            if CHANGING.contains(&name) {
                let line = format!("changed {}", clip(argument(input, "path"), LINE));
                add(&mut out.done, line);
            } else if name == "shell" {
                let line = format!(
                    "ran `{}` ({})",
                    clip(argument(input, "command"), LINE),
                    ended(metadata.get("exit").and_then(Value::as_i64))
                );
                out.command = Some(line.clone());
                add(&mut out.done, line);
            }
        }
        ToolState::Error { error, .. } => add(
            &mut out.failed,
            format!("a call to {name} failed: {}", clip(&error.message, LINE)),
        ),
        ToolState::Running { .. } => {
            add(&mut out.failed, format!("a call to {name} never finished"));
        }
    }
}

/// The lines kept under `key` in checkpoint `kept`.
fn lines(kept: Option<&Value>, key: &str) -> Vec<String> {
    kept.and_then(|k| k[key].as_array())
        .into_iter()
        .flatten()
        .filter_map(|l| l.as_str().map(str::to_owned))
        .collect()
}

/// `lines` as list items.
fn listed(lines: &[String]) -> String {
    lines.iter().fold(String::new(), |mut out, line| {
        let _ = writeln!(out, "- {line}");
        out
    })
}

/// The checkpoint after the reviewer's look number `round`, when it had
/// seen the session up to message `through`.
///
/// It adds what the `run` since the checkpoint `before` it did to what
/// that one held, so a checkpoint never knows less than the last one.
/// Only the newest [`LISTED`] things done are kept; the rest are counted.
pub(crate) fn checkpoint(
    round: u64,
    through: Option<&str>,
    reviewed: &Reviewed,
    run: &Digest,
    before: Option<&Value>,
) -> Value {
    let mut completed = lines(before, "completed");
    for line in &run.done {
        add(&mut completed, line.clone());
    }
    let dropped = completed.len().saturating_sub(LISTED);
    completed.drain(..dropped);
    let earlier = before
        .and_then(|b| b["earlier"].as_u64())
        .unwrap_or_default()
        + u64::try_from(dropped).unwrap_or(u64::MAX);
    let kept = |key: &str| before.and_then(|b| b[key].as_str()).map(str::to_owned);
    let mut out = json!({ "round": round, "at": now(), "completed": completed });
    if earlier > 0 {
        out["earlier"] = json!(earlier);
    }
    let mut put = |key: &str, value: Option<String>| {
        if let Some(value) = value.filter(|v| !v.trim().is_empty()) {
            out[key] = json!(clip(&value, SAID));
        }
    };
    put("through", through.map(str::to_owned));
    put(
        "verified",
        reviewed.verified.clone().or_else(|| kept("verified")),
    );
    put("command", run.command.clone().or_else(|| kept("command")));
    put("said", run.words.clone().or_else(|| kept("said")));
    put("missing", reviewed.missing.clone());
    put("next", reviewed.next.clone());
    out
}

/// What the agent is told when a goal with `objective` is taken up again:
/// its last `checkpoint`, if it has one, and what the session did `since`.
pub(crate) fn resume_text(objective: &str, checkpoint: Option<&Value>, since: &Digest) -> String {
    let mut out = format!(
        "Pick the goal up again where it stopped.\n\n<objective>\n{objective}\n</objective>\n"
    );
    if let Some(kept) = checkpoint {
        let _ = write!(
            out,
            "\n<checkpoint>\nThe reviewer last looked after run {} for this goal. This was kept then.\n",
            kept["round"].as_u64().unwrap_or_default()
        );
        let completed = lines(Some(kept), "completed");
        if !completed.is_empty() {
            out.push_str("Completed:\n");
            if let Some(earlier) = kept["earlier"].as_u64() {
                let _ = writeln!(out, "- ({earlier} earlier things aren't listed)");
            }
            out.push_str(&listed(&completed));
        }
        for (label, key) in [
            ("The agent's last words", "said"),
            ("Last command", "command"),
            ("Shown to be met", "verified"),
            ("Still missing", "missing"),
            ("Next", "next"),
        ] {
            if let Some(text) = kept[key].as_str() {
                let _ = writeln!(out, "{label}: {text}");
            }
        }
        out.push_str("</checkpoint>\n");
    }
    if since.is_empty() {
        return out;
    }
    let _ = write!(
        out,
        "\n<since>\n{} nobody reviewed:\n",
        if checkpoint.is_some() {
            "After that checkpoint the session went on. This is what it did, which"
        } else {
            "No checkpoint was kept yet. This is what the session did for the goal, which"
        }
    );
    let skipped = since.done.len().saturating_sub(LISTED);
    if skipped > 0 {
        let _ = writeln!(out, "- ({skipped} earlier things aren't listed)");
    }
    out.push_str(&listed(since.done.get(skipped..).unwrap_or_default()));
    out.push_str(&listed(&since.failed));
    for said in &since.said {
        let _ = writeln!(out, "- the person said: \"{said}\"");
    }
    if let Some(cut) = &since.cut {
        let _ = writeln!(out, "- it stopped short: {cut}");
    }
    out.push_str(
        "Look at what these left behind before you rely on them: a change may be half made.\n\
         </since>\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use harlo_engine::{Assistant, Created, ErrorInfo, ModelRef, StepTime, Tokens, ToolTime};

    use super::*;

    fn tool(name: &str, input: &Value, exit: Option<i64>) -> Part {
        let mut metadata = Map::new();
        if let Some(exit) = exit {
            metadata.insert("exit".to_owned(), json!(exit));
        }
        Part::Tool {
            id: format!("call_{name}"),
            name: name.to_owned(),
            state: ToolState::Completed {
                input: input.as_object().cloned().unwrap_or_default(),
                content: Vec::new(),
                metadata,
            },
            time: ToolTime {
                created: 0,
                ran: None,
                completed: None,
            },
        }
    }

    fn step(id: &str, content: Vec<Part>, error: Option<&str>) -> Message {
        Message::Assistant(Box::new(Assistant {
            id: id.to_owned(),
            time: StepTime::default(),
            agent: "build".to_owned(),
            model: ModelRef {
                provider: "fake".to_owned(),
                model: "fake-coder".to_owned(),
                variant: None,
            },
            content,
            finish: None,
            raw_finish: None,
            cost: 0.0,
            tokens: Tokens::default(),
            error: error.map(|message| ErrorInfo {
                kind: "provider".to_owned(),
                message: message.to_owned(),
            }),
            snapshot: None,
        }))
    }

    fn said(id: &str, text: &str) -> Message {
        Message::User {
            id: id.to_owned(),
            time: Created { created: 0 },
            text: text.to_owned(),
            skills: Vec::new(),
            files: Vec::new(),
        }
    }

    #[test]
    fn a_digest_lists_changes_commands_and_what_stopped_short() {
        let mut cut_short = step(
            "msg_4",
            vec![Part::Tool {
                id: "call_write".to_owned(),
                name: "write".to_owned(),
                state: ToolState::Running {
                    input: Map::new(),
                    metadata: Map::new(),
                },
                time: ToolTime {
                    created: 0,
                    ran: None,
                    completed: None,
                },
            }],
            None,
        );
        if let Message::Assistant(step) = &mut cut_short {
            assert!(step.end_unfinished(1, "The server stopped before this step ended."));
        }
        let messages = [
            Message::Synthetic {
                id: "msg_1".to_owned(),
                time: Created { created: 0 },
                text: "This session now has a goal.".to_owned(),
                description: None,
                metadata: Some(json!({ "source": "goal" })),
            },
            step(
                "msg_2",
                vec![
                    tool("read", &json!({ "path": "a.rs" }), None),
                    tool("edit", &json!({ "path": "a.rs" }), None),
                    tool("shell", &json!({ "command": "cargo test" }), Some(1)),
                    tool("edit", &json!({ "path": "a.rs" }), None),
                    Part::Text {
                        text: "One test fails.".to_owned(),
                    },
                ],
                None,
            ),
            said("msg_3", "Skip the flaky\ntest."),
            cut_short,
        ];
        let found = digest(&messages);
        // A file changed twice is listed once, where it last changed.
        assert_eq!(found.done, ["ran `cargo test` (exit 1)", "changed a.rs"]);
        assert_eq!(
            found.failed,
            ["a call to write failed: The server stopped before this step ended."]
        );
        assert_eq!(found.command.as_deref(), Some("ran `cargo test` (exit 1)"));
        assert_eq!(found.said, ["Skip the flaky test."]);
        assert_eq!(found.words.as_deref(), Some("One test fails."));
        assert_eq!(
            found.cut.as_deref(),
            Some("The server stopped before this step ended.")
        );
        // The server's own prompts and a clean end aren't things done.
        assert!(digest(&messages[..1]).is_empty());
    }

    #[test]
    fn a_checkpoint_keeps_what_the_last_one_knew() {
        let run = Digest {
            done: vec![
                "changed a.rs".to_owned(),
                "ran `cargo test` (exit 0)".to_owned(),
            ],
            command: Some("ran `cargo test` (exit 0)".to_owned()),
            words: Some("a.rs is done.".to_owned()),
            ..Digest::default()
        };
        let first = checkpoint(
            1,
            Some("msg_2"),
            &Reviewed {
                missing: Some("b.rs is untouched.".to_owned()),
                next: Some("Change b.rs.".to_owned()),
                ..Reviewed::default()
            },
            &run,
            None,
        );
        assert_eq!(
            first["completed"],
            json!(["changed a.rs", "ran `cargo test` (exit 0)"])
        );
        assert_eq!(first["command"], "ran `cargo test` (exit 0)");
        assert_eq!(first["through"], "msg_2");
        assert_eq!(first["missing"], "b.rs is untouched.");
        // A round that ran no command keeps the last one that was run.
        let later = Digest {
            done: vec!["changed b.rs".to_owned(), "changed a.rs".to_owned()],
            ..Digest::default()
        };
        let second = checkpoint(
            2,
            Some("msg_9"),
            &Reviewed {
                verified: Some("Both files hold the change.".to_owned()),
                ..Reviewed::default()
            },
            &later,
            Some(&first),
        );
        assert_eq!(
            second["completed"],
            json!(["ran `cargo test` (exit 0)", "changed b.rs", "changed a.rs"])
        );
        assert_eq!(second["command"], "ran `cargo test` (exit 0)");
        assert_eq!(second["said"], "a.rs is done.");
        assert_eq!(second["verified"], "Both files hold the change.");
        assert!(second.get("missing").is_none(), "nothing is missing now");
        // A long goal keeps its newest lines and counts the rest.
        let long = Digest {
            done: (1..=30).map(|n| format!("changed f{n}.rs")).collect(),
            ..Digest::default()
        };
        let third = checkpoint(3, None, &Reviewed::default(), &long, Some(&second));
        assert_eq!(third["completed"].as_array().map(Vec::len), Some(LISTED));
        assert_eq!(third["earlier"], 13);
        assert_eq!(third["completed"][LISTED - 1], "changed f30.rs");
    }

    #[test]
    fn a_resume_tells_the_checkpoint_and_what_came_after_it() {
        let kept = json!({
            "round": 2, "completed": ["changed a.rs"], "earlier": 4,
            "command": "ran `cargo test` (exit 0)", "said": "a.rs is done.",
            "missing": "b.rs is untouched.", "next": "Change b.rs.",
        });
        let since = Digest {
            done: (1..=22).map(|n| format!("changed f{n}.rs")).collect(),
            failed: vec!["a call to write never finished".to_owned()],
            said: vec!["Leave c.rs alone.".to_owned()],
            cut: Some("The server stopped before this step ended.".to_owned()),
            ..Digest::default()
        };
        let text = resume_text("Ship it", Some(&kept), &since);
        for expected in [
            "<objective>\nShip it\n</objective>",
            "after run 2 for this goal",
            "Completed:\n- (4 earlier things aren't listed)\n- changed a.rs\n",
            "The agent's last words: a.rs is done.",
            "Last command: ran `cargo test` (exit 0)",
            "Still missing: b.rs is untouched.",
            "Next: Change b.rs.",
            "- (2 earlier things aren't listed)\n- changed f3.rs\n",
            "- changed f22.rs",
            "- a call to write never finished",
            "- the person said: \"Leave c.rs alone.\"",
            "- it stopped short: The server stopped before this step ended.",
        ] {
            assert!(text.contains(expected), "{expected}\n{text}");
        }
        // Nothing after the checkpoint: only the checkpoint is told.
        let quiet = resume_text("Ship it", Some(&kept), &Digest::default());
        assert!(!quiet.contains("<since>"), "{quiet}");
        // No checkpoint yet: what the session did is all there is.
        let early = resume_text("Ship it", None, &since);
        assert!(!early.contains("<checkpoint>"), "{early}");
        assert!(early.contains("No checkpoint was kept yet."), "{early}");
    }
}
