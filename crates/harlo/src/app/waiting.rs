//! What a session waits on the user for: approvals, questions, the shield,
//! and the notification when one arrives while the user looks elsewhere.

// Rust guideline compliant 2026-10-09

use std::borrow::Cow;

use crate::task::Task;
use harlo_core::approvals::{Draft, Shield};
use harlo_core::server::{self, PermissionReply};
use harlo_core::workspace::{Form, Permission};

use super::{App, Message};
use crate::ids;

impl App {
    /// Applies a message about approvals, questions, the shield or window focus.
    pub(super) fn waiting_action(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Approve { request, reply } => {
                let reason = self
                    .refusing
                    .take_if(|(id, _)| *id == request)
                    .map(|(_, reason)| reason);
                return self.approve(request, reply, reason);
            }
            Message::Refuse(request) => {
                self.refusing = Some((request, String::new()));
                // Ready to type why.
                return Task::effect(crate::task::Effect::Focus(ids::REFUSE_REASON));
            }
            Message::RefuseReason(reason) => {
                if let Some((_, typed)) = &mut self.refusing {
                    *typed = reason;
                }
            }
            Message::RefuseSend => {
                if let Some((request, reason)) = self.refusing.take() {
                    let reason = Some(reason).filter(|r| !r.trim().is_empty());
                    return self.approve(request, PermissionReply::Reject, reason);
                }
            }
            Message::RefuseCancel => self.refusing = None,
            Message::Answered(session, id, result) => {
                self.answering.remove(&id);
                match result {
                    // Answered here, or elsewhere first: either way it's settled.
                    Ok(()) => {
                        self.workspace.settle_permission(&session, &id);
                        self.workspace.settle_form(&session, &id);
                        self.drafts.remove(&id);
                        self.problems.remove(&id);
                        // The sidebar stops marking the session.
                        self.refresh();
                    }
                    Err(reason) => self.send_error = Some(reason),
                }
            }
            Message::FormChoose { form, key, value } => {
                self.edit_draft(&form, &key, |draft, field| draft.choose(field, &value));
            }
            Message::FormType { form, key, text } => {
                self.edit_draft(&form, &key, |draft, field| draft.type_text(field, text));
            }
            Message::FormFlag { form, key, flag } => {
                self.edit_draft(&form, &key, |draft, field| draft.set_flag(field, flag));
            }
            Message::FormSend(id) => return self.answer(&id),
            Message::FormDismiss(id) => return self.dismiss(&id),
            Message::OpenLink(url) => {
                return Task::future(async move {
                    // A link that can't be opened leaves the question as it is.
                    let _ = open::that_detached(&url);
                })
                .discard();
            }
            Message::PickShield(mode) => return self.choose_shield(mode),
            Message::Focused(focused) => {
                self.notices.focused = focused;
                if focused {
                    // Files may have changed in another app meanwhile.
                    self.changes_stale();
                    self.files_stale();
                }
            }
            // Other messages are routed elsewhere by `apply`.
            _ => {}
        }
        Task::none()
    }

    /// Returns the open session's requests waiting for approval, oldest first.
    #[must_use]
    pub fn waiting_approvals(&self) -> &[Permission] {
        self.active
            .as_deref()
            .map_or(&[], |id| self.workspace.permissions(id))
    }

    /// Returns the open session's questions waiting for an answer, oldest first.
    #[must_use]
    pub fn waiting_questions(&self) -> &[Form] {
        self.active
            .as_deref()
            .map_or(&[], |id| self.workspace.forms(id))
    }

    /// The open session's agent, or the next new one's, as a name: "Build".
    #[must_use]
    pub fn agent_name(&self) -> String {
        self.current_agent()
            .map_or_else(|| "The agent".to_owned(), |name| capitalized(&name))
    }

    /// Returns whether a session waits for the user: an approval or an answer.
    #[must_use]
    pub fn needs_user(&self, id: &str) -> bool {
        self.workspace.needs_user(id)
    }

    /// Returns the answers given so far to a question.
    #[must_use]
    pub fn draft<'a>(&'a self, form: &Form) -> Cow<'a, Draft> {
        self.drafts
            .get(&form.id)
            .map_or_else(|| Cow::Owned(Draft::new(form)), Cow::Borrowed)
    }

    /// How much the open session, or the next new one, may do without asking.
    #[must_use]
    pub fn current_shield(&self) -> Shield {
        match self.active.as_deref() {
            Some(id) => self
                .workspace
                .session(id)
                .map_or(Shield::Agent, |s| Shield::of(&s.permissions)),
            None => self.new_shield,
        }
    }

    /// Sends a reply to an approval request in the open session.
    fn approve(
        &mut self,
        request: String,
        reply: PermissionReply,
        reason: Option<String>,
    ) -> Task<Message> {
        let (Some(client), Some(session)) = (self.client.clone(), self.active.clone()) else {
            return Task::none();
        };
        if !self.answering.insert(request.clone()) {
            return Task::none();
        }
        Task::perform(
            async move {
                let result = client
                    .reply_permission(&session, &request, reply, reason.as_deref())
                    .await;
                (session, request, settled(result))
            },
            |(session, request, result)| Message::Answered(session, request, result),
        )
    }

    /// Changes one field of a question's answers, and clears its problems.
    fn edit_draft(&mut self, id: &str, key: &str, edit: impl FnOnce(&mut Draft, &server::Field)) {
        let Some(form) = self
            .waiting_questions()
            .iter()
            .find(|f| f.id == id)
            .cloned()
        else {
            return;
        };
        let Some(field) = form.fields.iter().find(|f| f.key == key) else {
            return;
        };
        let draft = self
            .drafts
            .entry(form.id.clone())
            .or_insert_with(|| Draft::new(&form));
        edit(draft, field);
        if let Some(problems) = self.problems.get_mut(id) {
            problems.retain(|p| p.key != key);
        }
    }

    /// Checks a question's answers and sends them, or shows what's missing.
    fn answer(&mut self, id: &str) -> Task<Message> {
        let Some(form) = self
            .waiting_questions()
            .iter()
            .find(|f| f.id == id)
            .cloned()
        else {
            return Task::none();
        };
        let answers = match self.draft(&form).answers(&form) {
            Ok(answers) => answers,
            Err(problems) => {
                self.problems.insert(form.id, problems);
                return Task::none();
            }
        };
        self.problems.remove(id);
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        if !self.answering.insert(form.id.clone()) {
            return Task::none();
        }
        Task::perform(
            async move {
                let result = client
                    .reply_form(&form.session_id, &form.id, &answers)
                    .await;
                (form.session_id, form.id, settled(result))
            },
            |(session, id, result)| Message::Answered(session, id, result),
        )
    }

    /// Dismisses a question without answering it.
    fn dismiss(&mut self, id: &str) -> Task<Message> {
        let Some(form) = self
            .waiting_questions()
            .iter()
            .find(|f| f.id == id)
            .cloned()
        else {
            return Task::none();
        };
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        if !self.answering.insert(form.id.clone()) {
            return Task::none();
        }
        Task::perform(
            async move {
                let result = client.cancel_form(&form.session_id, &form.id).await;
                (form.session_id, form.id, settled(result))
            },
            |(session, id, result)| Message::Answered(session, id, result),
        )
    }

    /// Sets how much the open session may do without asking, or the next new one.
    fn choose_shield(&mut self, mode: Shield) -> Task<Message> {
        let Some(rules) = mode.rules() else {
            return Task::none();
        };
        if self.active.is_none() {
            self.new_shield = mode;
            return Task::none();
        }
        self.act(|client, id| async move { client.set_permissions(&id, &rules).await })
    }
}

/// Treats "another client answered first" as done.
fn settled(result: Result<(), server::Error>) -> Result<(), String> {
    match result {
        Err(e) if !e.is_settled() => Err(format!("Couldn't answer: {e}")),
        _ => Ok(()),
    }
}

/// `word` with its first letter in upper case.
pub(crate) fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
