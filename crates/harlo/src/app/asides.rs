//! Side questions, asked with `/btw`: answered from a session's context
//! without adding to its conversation.
//!
//! The server answers them through the session's `generate` call, which
//! leaves the history as it was and counts the cost toward the session.
//! The questions and answers live only in this window, per session, and
//! show in the Side questions panel.

// Rust guideline compliant 2026-10-09

use harlo_core::composer::ASIDE_PROMPT;

use super::{App, Message, Panel};
use crate::task::Task;

/// Where a side question's answer stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The model is answering.
    Waiting,
    /// The answer, as Markdown.
    Given(String),
    /// Why there's no answer, in words for the user.
    Failed(String),
}

/// A side question and its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aside {
    /// The question, as typed after `/btw`.
    pub question: String,
    /// Its answer, or where that stands.
    pub answer: Answer,
}

/// Something that happened to a side question.
#[derive(Debug, Clone)]
pub enum AsideEvent {
    /// A question was answered, or couldn't be.
    Answered {
        /// The session it was asked about.
        session: String,
        /// Its place in that session's questions.
        index: usize,
        /// The answer, or why there's none.
        result: Result<String, String>,
    },
    /// Ask the open session's question at this place again.
    Retry(usize),
}

impl App {
    /// The side questions asked about `session`, oldest first.
    #[must_use]
    pub fn asides(&self, session: &str) -> &[Aside] {
        self.per_session
            .asides
            .get(session)
            .map_or(&[], Vec::as_slice)
    }

    /// How many of the open session's side questions are still being
    /// answered.
    #[must_use]
    pub fn asides_waiting(&self) -> usize {
        self.active.as_deref().map_or(0, |id| {
            self.asides(id)
                .iter()
                .filter(|a| a.answer == Answer::Waiting)
                .count()
        })
    }

    /// Asks `question` about the open session, and shows the Side
    /// questions panel where its answer will be.
    pub(super) fn ask_aside(&mut self, question: &str) -> Task<Message> {
        let question = question.trim();
        if question.is_empty() {
            self.send_error = Some("Add a question after /btw.".to_owned());
            return Task::none();
        }
        let Some(session) = self.active.clone() else {
            self.send_error = Some(
                "Open a session first: a side question asks about its conversation.".to_owned(),
            );
            return Task::none();
        };
        self.send_error = None;
        self.menu = None;
        self.composer.set("");
        let list = self.per_session.asides.entry(session.clone()).or_default();
        list.push(Aside {
            question: question.to_owned(),
            answer: Answer::Waiting,
        });
        let index = list.len() - 1;
        Task::batch([
            self.request_aside(session, index),
            self.arrange(Message::OpenPanel(Panel::Asides)),
        ])
    }

    /// Handles something that happened to a side question.
    pub(super) fn aside_event(&mut self, event: AsideEvent) -> Task<Message> {
        match event {
            AsideEvent::Answered {
                session,
                index,
                result,
            } => {
                if let Some(aside) = self
                    .per_session
                    .asides
                    .get_mut(&session)
                    .and_then(|l| l.get_mut(index))
                {
                    aside.answer = match result {
                        Ok(text) => Answer::Given(text),
                        Err(why) => Answer::Failed(why),
                    };
                }
                Task::none()
            }
            AsideEvent::Retry(index) => {
                let Some(session) = self.active.clone() else {
                    return Task::none();
                };
                let Some(aside) = self
                    .per_session
                    .asides
                    .get_mut(&session)
                    .and_then(|l| l.get_mut(index))
                else {
                    return Task::none();
                };
                if !matches!(aside.answer, Answer::Failed(_)) {
                    return Task::none();
                }
                aside.answer = Answer::Waiting;
                self.request_aside(session, index)
            }
        }
    }

    /// Sends the question at `index` of `session`'s to the server.
    fn request_aside(&self, session: String, index: usize) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::done(Message::Aside(AsideEvent::Answered {
                session,
                index,
                result: Err("Not connected to Harlo's server yet.".to_owned()),
            }));
        };
        let Some(question) = self.asides(&session).get(index).map(|a| a.question.clone()) else {
            return Task::none();
        };
        let prompt = format!("{ASIDE_PROMPT}\n\n{question}");
        Task::perform(
            async move {
                let result = match client.ask_aside(&session, &prompt).await {
                    Ok(text) if text.trim().is_empty() => {
                        Err("The model replied without any text.".to_owned())
                    }
                    Ok(text) => Ok(text.trim().to_owned()),
                    Err(e) => Err(format!("Couldn't answer that question: {e}")),
                };
                AsideEvent::Answered {
                    session,
                    index,
                    result,
                }
            },
            Message::Aside,
        )
    }
}
