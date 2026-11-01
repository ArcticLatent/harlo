//! pi's `ask_user_question` tool (from a pi package such as
//! `@juicesharp/rpiv-ask-user-question` or `@thinkrail/pi-ask-user-question`)
//! as harlo's question card.
//!
//! Outside its terminal UI the tool asks through pi's ordinary dialogs, a
//! `select` per question (each option as a line of its own, then the
//! typed-answer choice, and maybe a way out) and an `input` for a typed
//! answer or a multiple choice ("1,3", with "; own text" where the
//! package takes both). harlo shows the tool's questions all
//! together instead, as an `ask` request (what omp's and Claude's
//! questions use), answering the first dialog; the dialogs that follow
//! are answered from the card's answers, in order. A dialog that doesn't
//! fit (another package's tool of the same name) is shown as it is.

use std::collections::VecDeque;

use serde_json::{Value, json};

pub(crate) const TOOL: &str = "ask_user_question";

/// What to give one of the tool's dialogs.
#[derive(Debug, Clone, PartialEq)]
enum Reply {
    /// The select's option at this index.
    Choice(usize),
    /// Text for an input.
    Text(String),
    /// A multiple choice's input: the options' numbers (from 1), their
    /// labels, and text of one's own.
    Multi(Vec<usize>, Vec<String>, String),
}

/// What the read loop does with a dialog request.
#[derive(Debug, PartialEq)]
pub(crate) enum Action {
    /// Not the tool's: as usual.
    Pass,
    /// Show this (an `ask` request) instead.
    Show(Value),
    /// Answer it with this `extension_ui_response`.
    Answer(Value),
}

#[derive(Default)]
pub(crate) struct AskBridge {
    /// The running call's questions.
    questions: Vec<Value>,
    /// The card shown, and the dialog it answers.
    held: Option<Value>,
    /// Replies to the dialogs still to come.
    replies: VecDeque<Reply>,
}

fn response(id: &Value, fields: Value) -> Value {
    let mut r = fields;
    r["type"] = json!("extension_ui_response");
    r["id"] = id.clone();
    r
}

impl AskBridge {
    /// Follows the tool's calls (pi's events).
    pub(crate) fn on_event(&mut self, frame: &Value) {
        match frame["type"].as_str() {
            Some("tool_execution_start") if frame["toolName"] == TOOL => {
                *self = Self { questions: frame["args"]["questions"].as_array().cloned().unwrap_or_default(), ..Self::default() };
            }
            Some("tool_execution_end") if frame["toolName"] == TOOL => *self = Self::default(),
            Some("agent_end") => *self = Self::default(),
            _ => {}
        }
    }

    pub(crate) fn on_request(&mut self, frame: &Value) -> Action {
        let method = frame["method"].as_str().unwrap_or_default();
        if self.questions.is_empty() || !matches!(method, "select" | "input") {
            return Action::Pass;
        }
        if let Some(reply) = self.replies.pop_front() {
            return match (method, reply) {
                (_, Reply::Choice(k)) if method == "select" && frame["options"].get(k).is_some() => Action::Answer(response(&frame["id"], json!({ "value": frame["options"][k] }))),
                ("input", Reply::Text(t)) => Action::Answer(response(&frame["id"], json!({ "value": t }))),
                ("input", Reply::Multi(numbers, labels, own)) => {
                    let numbers = numbers.iter().map(usize::to_string).collect::<Vec<_>>().join(",");
                    // "1,3; own" where the package takes both (its
                    // placeholder shows the "; "); else numbers, or text
                    // (the picks in words, then one's own).
                    let both = frame["placeholder"].as_str().is_some_and(|p| p.contains(';'));
                    let text = match (numbers.is_empty(), own.is_empty()) {
                        (_, true) => numbers,
                        (true, false) => own,
                        (false, false) if both => format!("{numbers}; {own}"),
                        (false, false) => format!("{}; {own}", labels.join(", ")),
                    };
                    Action::Answer(response(&frame["id"], json!({ "value": text })))
                }
                // Not what was expected: the dialogs as they are.
                _ => {
                    *self = Self::default();
                    Action::Pass
                }
            };
        }
        // The first question's dialog: the card instead.
        let first = self.questions[0]["question"].as_str().unwrap_or_default();
        let title = frame["title"].as_str().unwrap_or_default();
        if self.held.is_some() || first.is_empty() || !title.contains(first) {
            return Action::Pass;
        }
        self.held = Some(frame.clone());
        Action::Show(json!({
            "type": "extension_ui_request", "id": frame["id"], "method": "ask",
            "questions": self.questions.iter().enumerate().map(|(i, q)| json!({
                "id": format!("q{i}"), "header": q["header"], "question": q["question"], "multi": q["multiSelect"].as_bool().unwrap_or(false),
                "options": q["options"].as_array().into_iter().flatten().map(|o| json!({ "label": o["label"], "description": o["description"] })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }))
    }

    /// harlo's answer to a card this shows (`answer`: `{ answers: [{ id,
    /// selectedOptions, customInput }] }` or `{ cancelled: true }`): the
    /// response to the dialog it held, or None when it isn't one.
    pub(crate) fn answer(&mut self, id: &str, answer: &Value) -> Option<Value> {
        let held = self.held.as_ref().filter(|h| h["id"] == id)?.clone();
        if answer["cancelled"] == true || !answer["answers"].is_array() {
            *self = Self::default();
            return Some(response(&held["id"], json!({ "cancelled": true })));
        }
        let mut replies = VecDeque::new();
        for (i, q) in self.questions.iter().enumerate() {
            let a = answer["answers"].as_array().into_iter().flatten().find(|a| a["id"] == format!("q{i}").as_str()).cloned().unwrap_or_default();
            let labels: Vec<&str> = q["options"].as_array().into_iter().flatten().map(|o| o["label"].as_str().unwrap_or_default()).collect();
            let picked: Vec<usize> = a["selectedOptions"].as_array().into_iter().flatten().filter_map(|s| labels.iter().position(|l| Some(*l) == s.as_str())).collect();
            let own = a["customInput"].as_str().map(str::trim).unwrap_or_default().to_owned();
            if q["multiSelect"].as_bool() == Some(true) {
                replies.push_back(Reply::Multi(picked.iter().map(|k| k + 1).collect(), picked.iter().map(|k| labels[*k].to_owned()).collect(), own));
            } else if let Some(k) = picked.first() {
                replies.push_back(Reply::Choice(*k));
            } else {
                // "Other…" (after the options), then the text.
                replies.push_back(Reply::Choice(labels.len()));
                replies.push_back(Reply::Text(own));
            }
        }
        self.replies = replies;
        match self.on_request(&held) {
            Action::Answer(r) => Some(r),
            _ => Some(response(&held["id"], json!({ "cancelled": true }))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> Value {
        json!({ "type": "tool_execution_start", "toolCallId": "t1", "toolName": TOOL, "args": { "questions": [
            { "header": "Database", "question": "Which database?", "options": [
                { "label": "Postgres", "description": "Relational" }, { "label": "SQLite", "description": "One file" } ] },
            { "header": "Extras", "question": "What else?", "multiSelect": true, "options": [
                { "label": "Auth", "description": "" }, { "label": "Cache", "description": "" }, { "label": "Search", "description": "" } ] },
            { "header": "Name", "question": "Its name?", "options": [{ "label": "app", "description": "" }] },
        ] } })
    }

    fn select(id: &str, title: &str, options: &[&str]) -> Value {
        json!({ "type": "extension_ui_request", "id": id, "method": "select", "title": title, "options": options })
    }

    fn input(id: &str, title: &str) -> Value {
        json!({ "type": "extension_ui_request", "id": id, "method": "input", "title": title, "placeholder": "1,3; your own answer" })
    }

    #[test]
    fn shows_the_questions_together_and_answers_the_dialogs() {
        let mut b = AskBridge::default();
        b.on_event(&start());
        let first = select("d1", "Database (1/3): Which database?", &["Postgres — Relational", "SQLite — One file", "Other…", "Answer in chat"]);
        let Action::Show(card) = b.on_request(&first) else { panic!("no card") };
        assert_eq!(card["method"], "ask");
        assert_eq!(card["id"], "d1");
        assert_eq!(card["questions"][1], json!({ "id": "q1", "header": "Extras", "question": "What else?", "multi": true,
            "options": [{ "label": "Auth", "description": "" }, { "label": "Cache", "description": "" }, { "label": "Search", "description": "" }] }));
        // Not the card's: shown as it is.
        assert!(b.answer("other", &json!({ "answers": [] })).is_none());

        let answers = json!({ "answers": [
            { "id": "q0", "selectedOptions": ["SQLite"] },
            { "id": "q1", "selectedOptions": ["Auth", "Search"], "customInput": "logs" },
            { "id": "q2", "selectedOptions": [], "customInput": "harlo-demo" },
        ] });
        assert_eq!(b.answer("d1", &answers), Some(json!({ "type": "extension_ui_response", "id": "d1", "value": "SQLite — One file" })));
        assert_eq!(b.on_request(&input("d2", "Extras (2/3): What else?\n\n1. Auth…")), Action::Answer(json!({ "type": "extension_ui_response", "id": "d2", "value": "1,3; logs" })));
        assert_eq!(b.on_request(&select("d3", "Name (3/3): Its name?", &["app", "Other…", "Answer in chat"])), Action::Answer(json!({ "type": "extension_ui_response", "id": "d3", "value": "Other…" })));
        assert_eq!(b.on_request(&input("d4", "Its name?")), Action::Answer(json!({ "type": "extension_ui_response", "id": "d4", "value": "harlo-demo" })));
        // Done: anything else is as usual.
        assert_eq!(b.on_request(&select("d5", "Something", &["a"])), Action::Pass);
        b.on_event(&json!({ "type": "tool_execution_end", "toolName": TOOL }));
        assert_eq!(b.on_request(&select("d6", "Which database?", &["a"])), Action::Pass);
    }

    /// rpiv's dialogs: numbered options, "Type something." last, a
    /// multiple choice as numbers or as text.
    #[test]
    fn answers_rpivs_dialogs() {
        let mut b = AskBridge::default();
        b.on_event(&start());
        let first = select("r1", "[Database] Which database?", &["1. Postgres — Relational", "2. SQLite — One file", "3. Type something."]);
        assert!(matches!(b.on_request(&first), Action::Show(_)));
        let answers = json!({ "answers": [
            { "id": "q0", "selectedOptions": [], "customInput": "MariaDB" },
            { "id": "q1", "selectedOptions": ["Auth", "Search"], "customInput": "logs" },
            { "id": "q2", "selectedOptions": ["app"] },
        ] });
        assert_eq!(b.answer("r1", &answers).unwrap()["value"], "3. Type something.");
        let typed = |id: &str, title: &str| json!({ "type": "extension_ui_request", "id": id, "method": "input", "title": title, "placeholder": "" });
        assert_eq!(b.on_request(&typed("r2", "[Database] Which database?\n\nType your answer:")), Action::Answer(json!({ "type": "extension_ui_response", "id": "r2", "value": "MariaDB" })));
        let multi = json!({ "type": "extension_ui_request", "id": "r3", "method": "input", "title": "[Extras] What else?\n\n1. Auth — ", "placeholder": "1,3" });
        assert_eq!(b.on_request(&multi), Action::Answer(json!({ "type": "extension_ui_response", "id": "r3", "value": "Auth, Search; logs" })));
        assert_eq!(b.on_request(&select("r4", "[Name] Its name?", &["1. app — ", "2. Type something."])), Action::Answer(json!({ "type": "extension_ui_response", "id": "r4", "value": "1. app — " })));
    }

    #[test]
    fn skipping_cancels_and_strangers_pass() {
        let mut b = AskBridge::default();
        // No call running.
        assert_eq!(b.on_request(&select("d0", "Which database?", &["a"])), Action::Pass);
        b.on_event(&start());
        // A dialog that isn't its first question.
        assert_eq!(b.on_request(&select("x", "Allow tool: bash", &["Approve", "Deny"])), Action::Pass);
        assert!(matches!(b.on_request(&select("d1", "Database (1/3): Which database?", &["Postgres — Relational", "SQLite — One file", "Other…", "Answer in chat"])), Action::Show(_)));
        assert_eq!(b.answer("d1", &json!({ "cancelled": true })), Some(json!({ "type": "extension_ui_response", "id": "d1", "cancelled": true })));
        assert_eq!(b.on_request(&input("d2", "What else?")), Action::Pass);

        // A dialog of another shape than expected: as it is from then on.
        b.on_event(&start());
        assert!(matches!(b.on_request(&select("e1", "Which database?", &["Postgres", "SQLite", "Other…"])), Action::Show(_)));
        let all = json!({ "answers": [{ "id": "q0", "selectedOptions": ["Postgres"] }, { "id": "q1", "selectedOptions": [] }, { "id": "q2", "selectedOptions": ["app"] }] });
        assert_eq!(b.answer("e1", &all).unwrap()["value"], "Postgres");
        assert_eq!(b.on_request(&select("e2", "What else?", &["a", "b"])), Action::Pass);
    }
}
