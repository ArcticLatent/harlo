//! Side questions asked from a phone with `/btw`: answered from a
//! session's context without adding to its conversation, as the window's
//! are.
//!
//! The server answers them through the session's `generate` call. The
//! questions and answers live here, in memory, per session, for every
//! paired phone: each is a row beside the conversation, with its answer
//! under it and Dismiss once it has one.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tokio::sync::watch;

use crate::composer::ASIDE_PROMPT;
use crate::server::Client;

use super::{Hub, lock};

/// A side question's row ID starts with this, then its number.
const ROW: &str = "aside:";

/// How many side questions a session keeps; the oldest goes first.
///
/// The rows share the space above the phone's message box with the todo
/// list and the background work.
const KEPT: usize = 5;

/// Where a side question's answer stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Answer {
    /// The model is answering.
    Waiting,
    /// The answer, as Markdown.
    Given(String),
    /// Why there's no answer, in words for the user.
    Failed(String),
}

/// A side question and its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Question {
    /// Its number, unique while phone access runs.
    id: u64,
    /// What was asked, as typed after `/btw`.
    text: String,
    /// Its answer, or where that stands.
    answer: Answer,
}

/// The side questions asked from phones, by session.
#[derive(Debug)]
pub(super) struct Questions {
    /// Each session's questions, oldest first.
    asked: Mutex<HashMap<String, Vec<Question>>>,
    /// The next question's number.
    next: AtomicU64,
    /// Bumped when a question is asked, answered or dismissed.
    changed: watch::Sender<u64>,
}

impl Default for Questions {
    fn default() -> Self {
        Self {
            asked: Mutex::default(),
            next: AtomicU64::new(1),
            changed: watch::Sender::new(0),
        }
    }
}

impl Questions {
    /// Tells of each question asked, answered or dismissed.
    pub(super) fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    /// Session `key`'s side questions as rows beside its conversation.
    pub(super) fn rows(&self, key: &str) -> Vec<Value> {
        let asked = lock(&self.asked);
        asked.get(key).into_iter().flatten().map(row).collect()
    }

    /// Takes the row `id` away from session `key`; `None` when `id` isn't
    /// a side question's row, else whether it was there.
    pub(super) fn dismiss(&self, key: &str, id: &str) -> Option<bool> {
        let id: u64 = id.strip_prefix(ROW)?.parse().ok()?;
        let mut asked = lock(&self.asked);
        let list = asked.get_mut(key)?;
        let before = list.len();
        list.retain(|q| q.id != id);
        let gone = list.len() != before;
        drop(asked);
        if gone {
            self.bump();
        }
        Some(gone)
    }

    /// Adds `question` to session `key`'s; returns its number.
    fn add(&self, key: &str, question: &str) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut asked = lock(&self.asked);
        let list = asked.entry(key.to_owned()).or_default();
        list.push(Question {
            id,
            text: question.to_owned(),
            answer: Answer::Waiting,
        });
        if list.len() > KEPT {
            list.remove(0);
        }
        drop(asked);
        self.bump();
        id
    }

    /// Gives question `id` of session `key` its answer.
    fn answer(&self, key: &str, id: u64, answer: Answer) {
        let mut asked = lock(&self.asked);
        if let Some(question) = asked
            .get_mut(key)
            .and_then(|list| list.iter_mut().find(|q| q.id == id))
        {
            question.answer = answer;
        }
        drop(asked);
        self.bump();
    }

    /// Tells the connections that the rows changed.
    fn bump(&self) {
        self.changed.send_modify(|n| *n = n.wrapping_add(1));
    }
}

/// Asks `question` about session `key`; its row, and later its answer,
/// reach the phones that have the session open.
///
/// Must be called inside a Tokio runtime.
///
/// # Errors
///
/// Fails when there is no question.
pub(super) fn ask(
    hub: &Arc<Hub>,
    client: &Client,
    key: &str,
    question: &str,
) -> Result<(), String> {
    let question = question.trim();
    if question.is_empty() {
        return Err("Add a question after /btw.".to_owned());
    }
    let id = hub.questions.add(key, question);
    let (hub, client, key) = (Arc::clone(hub), client.clone(), key.to_owned());
    let prompt = format!("{ASIDE_PROMPT}\n\n{question}");
    tokio::spawn(async move {
        let answer = match client.ask_aside(&key, &prompt).await {
            Ok(text) if text.trim().is_empty() => {
                Answer::Failed("The model replied without any text.".to_owned())
            }
            Ok(text) => Answer::Given(text.trim().to_owned()),
            Err(e) => Answer::Failed(format!("Couldn't answer that question: {e}")),
        };
        hub.questions.answer(&key, id, answer);
    });
    Ok(())
}

/// A side question's row: the question, its answer under it, and Dismiss
/// once the model is done.
fn row(question: &Question) -> Value {
    let (detail, note, working) = match &question.answer {
        Answer::Waiting => ("", "Answering…", true),
        Answer::Given(text) => (text.as_str(), "Answered", false),
        Answer::Failed(why) => (why.as_str(), "No answer", false),
    };
    json!({ "id": format!("{ROW}{}", question.id), "symbol": "questionmark.bubble",
            "title": "Side question", "text": question.text, "detail": detail,
            "note": note, "action": if working { "" } else { "Dismiss" },
            "working": working })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_is_a_row_that_gets_its_answer_and_can_be_dismissed() {
        let questions = Questions::default();
        let id = questions.add("ses_1", "Which port?");
        assert_eq!(
            questions.rows("ses_1"),
            [
                json!({ "id": format!("aside:{id}"), "symbol": "questionmark.bubble",
                     "title": "Side question", "text": "Which port?", "detail": "",
                     "note": "Answering…", "action": "", "working": true })
            ]
        );
        assert!(questions.rows("ses_2").is_empty());

        questions.answer("ses_1", id, Answer::Given("Port **7872**.".to_owned()));
        let rows = questions.rows("ses_1");
        assert_eq!(rows[0]["detail"], "Port **7872**.");
        assert_eq!(rows[0]["note"], "Answered");
        assert_eq!(rows[0]["action"], "Dismiss");
        assert_eq!(rows[0]["working"], false);

        // Another row's action isn't a side question's to do.
        assert_eq!(questions.dismiss("ses_1", "job:job_1"), None);
        assert_eq!(questions.dismiss("ses_1", "aside:999"), Some(false));
        assert_eq!(
            questions.dismiss("ses_1", &format!("aside:{id}")),
            Some(true)
        );
        assert!(questions.rows("ses_1").is_empty());
    }

    #[test]
    fn a_session_keeps_its_last_few_questions() {
        let questions = Questions::default();
        for n in 0..KEPT + 2 {
            questions.add("ses_1", &format!("Question {n}"));
        }
        let rows = questions.rows("ses_1");
        assert_eq!(rows.len(), KEPT);
        assert_eq!(rows[0]["text"], "Question 2");
    }

    #[test]
    fn each_change_is_told() {
        let questions = Questions::default();
        let mut told = questions.subscribe();
        let id = questions.add("ses_1", "Why?");
        assert!(told.has_changed().expect("open"));
        told.borrow_and_update();
        questions.answer("ses_1", id, Answer::Failed("No model.".to_owned()));
        assert!(told.has_changed().expect("open"));
        assert_eq!(questions.rows("ses_1")[0]["note"], "No answer");
    }
}
