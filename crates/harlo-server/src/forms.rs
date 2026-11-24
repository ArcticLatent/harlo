//! The agent's questions, as forms: one form per `question`
//! call, a field per question, waiting in the session's run until a person
//! answers or dismisses it.

// Rust guideline compliant 2026-10-09

use harlo_engine::question::Asked;
use serde_json::{Map, Value, json};
use tokio::sync::oneshot;

use crate::server::{Refusal, Server, lock};

/// A form waiting for answers.
pub(crate) struct PendingForm {
    /// Its ID.
    pub id: String,
    /// What clients are shown: the form without its state.
    pub data: Value,
    /// Where the answers go, one list per question; `None` dismisses it.
    pub answer: oneshot::Sender<Option<Vec<Vec<String>>>>,
}

/// The form for `asked`: title "Questions", a field
/// `q0`, `q1`, … per question, own answers allowed.
pub(crate) fn form_of(asked: &Asked, session: &str) -> Value {
    let fields: Vec<Value> = asked
        .questions
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let options: Vec<Value> = q
                .options
                .iter()
                .map(|o| {
                    let mut option = json!({ "value": o.label, "label": o.label });
                    if let Some(d) = &o.description {
                        option["description"] = json!(d);
                    }
                    option
                })
                .collect();
            let mut field = json!({
                "key": format!("q{i}"),
                "description": q.question,
                "type": if q.multiple { "multiselect" } else { "string" },
                "options": options,
                "custom": true,
            });
            if let Some(h) = &q.header {
                field["title"] = json!(h);
            }
            field
        })
        .collect();
    json!({
        "id": asked.id,
        "sessionID": session,
        "title": "Questions",
        "metadata": { "kind": "question", "tool": { "messageID": asked.message, "id": asked.call } },
        "fields": fields,
    })
}

/// An answer's values in field order: a list per field, empty when it
/// wasn't answered.
///
/// # Errors
///
/// When it names a field the form lacks, or a value isn't text or a list
/// of text.
fn answers_of(form: &Value, answer: &Map<String, Value>) -> Result<Vec<Vec<String>>, Refusal> {
    let keys: Vec<&str> = form["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f["key"].as_str())
        .collect();
    if let Some(stray) = answer.keys().find(|k| !keys.contains(&k.as_str())) {
        return Err(Refusal::Bad(format!("The form has no field {stray}.")));
    }
    keys.iter()
        .map(|key| match answer.get(*key) {
            None => Ok(Vec::new()),
            Some(Value::String(s)) => Ok(vec![s.clone()]),
            Some(Value::Array(items)) => items
                .iter()
                .map(|i| {
                    i.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| Refusal::Bad(format!("{key}'s answers must be text.")))
                })
                .collect(),
            Some(other) => Ok(vec![other.to_string()]),
        })
        .collect()
}

impl Server {
    /// Registers `asked` with session `session`'s run and announces it;
    /// the receiver gets the answers. `None` when the run has ended.
    pub(crate) fn ask_questions(
        &self,
        session: &str,
        directory: &str,
        asked: &Asked,
    ) -> Option<oneshot::Receiver<Option<Vec<Vec<String>>>>> {
        self.ask_form(session, directory, &asked.id, form_of(asked, session))
    }

    /// Registers form `data`, with ID `id`, with session `session`'s run
    /// and announces it; the receiver gets the answers, one list per
    /// field. `None` when the run has ended.
    pub(crate) fn ask_form(
        &self,
        session: &str,
        directory: &str,
        id: &str,
        data: Value,
    ) -> Option<oneshot::Receiver<Option<Vec<Vec<String>>>>> {
        let (tx, rx) = oneshot::channel();
        let announced = json!({ "form": data.clone() });
        let mut live = lock(&self.live);
        live.runs.get_mut(session)?.forms.push(PendingForm {
            id: id.to_owned(),
            data,
            answer: tx,
        });
        drop(live);
        self.emit("form.created", announced, Some(directory), Some(session));
        Some(rx)
    }

    /// Session `session`'s forms waiting for answers.
    pub(crate) fn forms(&self, session: &str) -> Vec<Value> {
        lock(&self.live)
            .runs
            .get(session)
            .map(|r| r.forms.iter().map(|f| f.data.clone()).collect())
            .unwrap_or_default()
    }

    /// Every form waiting for answers in `directory`'s sessions.
    pub(crate) fn forms_in(&self, directory: &str) -> Vec<Value> {
        let sessions: Vec<String> = lock(&self.live).runs.keys().cloned().collect();
        sessions
            .iter()
            .filter(|id| {
                self.store
                    .session(id)
                    .ok()
                    .flatten()
                    .is_some_and(|s| s.location.directory == directory)
            })
            .flat_map(|id| self.forms(id))
            .collect()
    }

    /// Form `form` of session `session`, with its state.
    ///
    /// # Errors
    ///
    /// When there's no such form.
    pub(crate) fn form(&self, session: &str, form: &str) -> Result<Value, Refusal> {
        if let Some(mut data) = self.forms(session).into_iter().find(|f| f["id"] == form) {
            data["state"] = json!({ "status": "pending" });
            return Ok(data);
        }
        lock(&self.live)
            .settled_forms
            .get(form)
            .filter(|f| f["sessionID"] == session)
            .cloned()
            .ok_or_else(|| Refusal::NotFound(format!("There's no form {form}.")))
    }

    /// Answers form `form` of session `session`, or with `None` dismisses
    /// it. The same settlement again changes nothing; a different one is
    /// refused.
    ///
    /// # Errors
    ///
    /// When there's no such form, the answer doesn't fit it, or it was
    /// settled otherwise.
    pub(crate) fn settle_form(
        &self,
        session: &str,
        form: &str,
        answer: Option<&Map<String, Value>>,
    ) -> Result<(), Refusal> {
        let (pending, values) = {
            let mut live = lock(&self.live);
            let found = live.runs.get_mut(session).and_then(|r| {
                let at = r.forms.iter().position(|p| p.id == form)?;
                Some((at, r.forms[at].data.clone()))
            });
            let Some((at, data)) = found else {
                drop(live);
                return self.settled_again(session, form, answer);
            };
            let values = answer.map(|a| answers_of(&data, a)).transpose()?;
            let Some(pending) = live.runs.get_mut(session).map(|r| r.forms.remove(at)) else {
                return Err(Refusal::NotFound(format!("There's no form {form}.")));
            };
            let mut settled = data;
            settled["state"] = match answer {
                Some(a) => json!({ "status": "answered", "answer": a }),
                None => json!({ "status": "cancelled" }),
            };
            live.settled_forms.insert(form.to_owned(), settled);
            (pending, values)
        };
        let directory = self
            .store
            .session(session)
            .ok()
            .flatten()
            .map(|s| s.location.directory);
        let kind = if values.is_some() {
            "form.replied"
        } else {
            "form.cancelled"
        };
        self.emit(
            kind,
            json!({ "sessionID": session, "id": form }),
            directory.as_deref(),
            Some(session),
        );
        // (The run may have been stopped meanwhile.)
        let _ = pending.answer.send(values);
        Ok(())
    }

    /// A settlement of a form already settled: fine when it's the same.
    fn settled_again(
        &self,
        session: &str,
        form: &str,
        answer: Option<&Map<String, Value>>,
    ) -> Result<(), Refusal> {
        let settled = self.form(session, form)?;
        let same = match answer {
            Some(a) => settled["state"]["answer"] == Value::Object(a.clone()),
            // (A dismissal is refused the second time.)
            None => false,
        };
        if same {
            Ok(())
        } else {
            Err(Refusal::Settled(format!(
                "The form {form} was already {}.",
                settled["state"]["status"].as_str().unwrap_or("settled")
            )))
        }
    }
}
