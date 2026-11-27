//! Proposed notes: what a session held that was worth keeping and the
//! agent didn't keep, for the person to accept, edit or dismiss.
//!
//! With notes on, a run that goes idle after real work has its new
//! stretch read by a model in the background, which may propose a note
//! for a correction the person gave, a decision about the project, or a
//! pitfall that will repeat. Each proposal names the session and the
//! message it comes from. Nothing proposed reaches a session: only a
//! proposal the person accepts becomes a note, and then it is a note like
//! any other, replacing the note it contradicts when it names one.
//!
//! A session is read once per stretch: where the last look ended is kept
//! with it. A message that already led to proposals leads to no more, and
//! neither does a text already proposed or kept, so a dismissed proposal
//! doesn't come back.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use harlo_engine::memory::{NOTE_CHARS, Note, PROPOSE_SYSTEM, Scope, Source, propose_prompt};
use harlo_engine::{Message, Part, ToolState, ascending_id, now};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::server::{Refusal, Server, lock};
use crate::store::Session;

/// The most notes one look proposes.
const MOST: usize = 3;

/// Model steps a stretch needs, with something the person said, to be
/// worth a look: a question and its one answer hold nothing to keep.
const STEPS: usize = 2;

/// Characters of a stretch shown to the model, about 6,000 tokens: its
/// newest part when it is longer.
const STRETCH_CHARS: usize = 24_000;

/// Characters shown of what a person said, of what the agent said, and
/// of why a call failed.
const PERSON_CHARS: usize = 2_000;
const AGENT_CHARS: usize = 600;
const FAILURE_CHARS: usize = 300;

/// Messages with something said that are shown from before the stretch,
/// so a correction can be read against what it corrects.
const BEFORE: usize = 2;

/// Characters of the source message a proposal keeps, to show why it was
/// suggested.
const EXCERPT_CHARS: usize = 300;

/// What kind of thing a proposal is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The person told the agent to do something differently.
    Correction,
    /// Something settled about the project.
    Decision,
    /// Something that went wrong and will again.
    Pitfall,
}

/// Where a proposal stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Waiting for the person.
    Pending,
    /// Kept as a note.
    Accepted,
    /// Turned down; kept so it isn't proposed again.
    Dismissed,
}

/// A note proposed to the person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    /// Its ID, such as `prp_…`.
    pub id: String,
    /// Who the note would be for.
    pub scope: Scope,
    /// The project it would belong to, for a project note.
    #[serde(rename = "projectID", default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// What the note would say.
    pub text: String,
    /// What kind of thing it is.
    pub kind: Kind,
    /// The session and the message it comes from.
    pub source: Source,
    /// What that message says, cut short.
    pub excerpt: String,
    /// When it was proposed, in Unix milliseconds.
    pub created: i64,
    /// The kept note it contradicts and would replace, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
    /// Where it stands.
    pub state: State,
    /// The note made of it, once accepted.
    #[serde(rename = "noteID", default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A stretch of a session as a model reads it.
#[derive(Debug, Default)]
struct Stretch {
    /// The messages with something said, oldest first, each as its ID
    /// and its lines.
    said: Vec<(String, String)>,
    /// What each of those says, cut short, by ID.
    excerpts: HashMap<String, String>,
    /// How many of them a person wrote, and how many model steps ran.
    persons: usize,
    steps: usize,
}

/// The first `limit` characters of `text`, marked when it is longer.
fn cut(text: &str, limit: usize) -> String {
    let text = text.trim();
    let mut shown: String = text.chars().take(limit).collect();
    if shown.len() < text.len() {
        shown.push('…');
    }
    shown
}

/// `text` as it is compared with another: its letters and digits, in
/// lower case, a space between words.
fn plain(text: &str) -> String {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// What `message` says to someone reading the session back, with the
/// calls of a step that failed; nothing when it says nothing.
fn lines(message: &Message) -> Option<String> {
    let mut out = match message {
        Message::User { text, .. } => cut(text, PERSON_CHARS),
        Message::Shell(run) => format!(
            "`{}` ({})",
            cut(&run.command, FAILURE_CHARS),
            run.exit
                .map_or_else(|| run.status.clone(), |code| format!("exit {code}"))
        ),
        Message::Assistant(_) => cut(&crate::store::searched(message), AGENT_CHARS),
        Message::Synthetic { .. } | Message::Idle { .. } | Message::Compaction(_) => String::new(),
    };
    if let Message::Assistant(step) = message {
        for part in &step.content {
            let Part::Tool { name, state, .. } = part else {
                continue;
            };
            let failed = match state {
                ToolState::Error { error, .. } => Some(cut(&error.message, FAILURE_CHARS)),
                ToolState::Completed {
                    input, metadata, ..
                } => metadata
                    .get("exit")
                    .and_then(Value::as_i64)
                    .filter(|code| *code != 0)
                    .map(|code| {
                        let command = input.get("command").and_then(Value::as_str);
                        format!(
                            "`{}` ended with exit {code}",
                            cut(command.unwrap_or_default(), FAILURE_CHARS)
                        )
                    }),
                ToolState::Running { .. } => None,
            };
            if let Some(why) = failed {
                if !out.is_empty() {
                    out.push('\n');
                }
                let _ = write!(out, "(a call to {name} failed: {why})");
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Who wrote `message`, as a transcript names them.
fn who(message: &Message) -> &'static str {
    match message {
        Message::User { .. } => "Person",
        Message::Shell(_) => "Command",
        _ => "Agent",
    }
}

/// `messages` as a stretch.
fn stretch(messages: &[Message]) -> Stretch {
    let mut out = Stretch::default();
    for message in messages {
        match message {
            Message::User { .. } => out.persons += 1,
            Message::Assistant(_) => out.steps += 1,
            _ => {}
        }
        if let Some(text) = lines(message) {
            let id = message.id().to_owned();
            out.excerpts.insert(id.clone(), cut(&text, EXCERPT_CHARS));
            out.said
                .push((id.clone(), format!("{} ({id}):\n{text}\n", who(message))));
        }
    }
    out
}

impl Stretch {
    /// Whether the stretch is worth a model's look.
    fn substantive(&self) -> bool {
        self.persons >= 2 || (self.persons == 1 && self.steps >= STEPS)
    }

    /// The stretch as text: its newest [`STRETCH_CHARS`], whole messages.
    fn text(&self) -> String {
        let mut size = 0;
        let mut from = self.said.len();
        for (index, (_, text)) in self.said.iter().enumerate().rev() {
            if size + text.len() > STRETCH_CHARS && from < self.said.len() {
                break;
            }
            size += text.len();
            from = index;
        }
        let mut out = String::new();
        if from > 0 {
            out.push_str("(Earlier messages of the stretch are left out.)\n");
        }
        for (_, text) in self.said.iter().skip(from) {
            out.push_str(text);
        }
        out
    }
}

/// The proposals in a model's `answer` that may be shown to the person,
/// as ones of `session`.
///
/// One is left out when the message it names isn't in the stretch
/// (`excerpts`), when that message led to proposals in an earlier look, or
/// when its text is one already proposed (`earlier`, whatever came of it)
/// or still kept (`notes`). A note it says it replaces must still hold.
fn admitted(
    answer: &str,
    session: &Session,
    excerpts: &HashMap<String, String>,
    notes: &[Note],
    earlier: &[Proposal],
) -> Vec<Proposal> {
    let listed = answer
        .find('[')
        .zip(answer.rfind(']'))
        .and_then(|(from, to)| answer.get(from..=to))
        .and_then(|list| serde_json::from_str::<Vec<Value>>(list).ok())
        .unwrap_or_default();
    let holding: Vec<&Note> = notes.iter().filter(|n| n.superseded_by.is_none()).collect();
    let mut taken: Vec<String> = earlier
        .iter()
        .map(|p| plain(&p.text))
        .chain(holding.iter().map(|n| plain(&n.text)))
        .collect();
    let mut out = Vec::new();
    for item in listed {
        let field = |key: &str| item[key].as_str().map(str::trim).filter(|s| !s.is_empty());
        let (Some(text), Some(message)) = (field("text"), field("message")) else {
            continue;
        };
        let Some(excerpt) = excerpts.get(message) else {
            continue;
        };
        let seen = earlier
            .iter()
            .any(|p| p.source.session == session.id && p.source.message == message);
        if seen || taken.contains(&plain(text)) || out.len() >= MOST {
            continue;
        }
        taken.push(plain(text));
        let scope = match field("scope") {
            Some("user") => Scope::User,
            _ => Scope::Project,
        };
        out.push(Proposal {
            id: ascending_id("prp"),
            scope,
            project: (scope == Scope::Project).then(|| session.project_id.clone()),
            text: text.chars().take(NOTE_CHARS).collect(),
            kind: match field("kind") {
                Some("correction") => Kind::Correction,
                Some("pitfall") => Kind::Pitfall,
                _ => Kind::Decision,
            },
            source: Source {
                session: session.id.clone(),
                message: message.to_owned(),
                title: session.title.clone(),
            },
            excerpt: excerpt.clone(),
            created: now(),
            replaces: field("replaces")
                .filter(|id| holding.iter().any(|n| n.id == *id))
                .map(str::to_owned),
            state: State::Pending,
            note: None,
        });
    }
    out
}

impl Server {
    /// The proposals waiting for the person: the user's and, with
    /// `project`, that project's, oldest first.
    pub(crate) fn proposals(&self, project: Option<&str>) -> Result<Vec<Proposal>, Refusal> {
        let mut all = self.store.proposals(project)?;
        all.retain(|p| p.state == State::Pending);
        Ok(all)
    }

    /// Proposal `id`, while it waits for the person.
    fn pending(&self, id: &str) -> Result<Proposal, Refusal> {
        let found = self
            .store
            .proposal(id)?
            .ok_or_else(|| Refusal::NotFound(format!("No proposal {id}.")))?;
        if found.state == State::Pending {
            Ok(found)
        } else {
            Err(Refusal::Bad(
                "That proposal was already decided.".to_owned(),
            ))
        }
    }

    /// Keeps proposal `id` as a note saying `text`, or what it proposed.
    ///
    /// The note names the proposal's session and message as its source.
    /// The note the proposal would replace, when it still holds, is
    /// replaced by it, as when the agent keeps a note that replaces one.
    pub(crate) fn accept_proposal(&self, id: &str, text: Option<&str>) -> Result<Note, Refusal> {
        let mut proposal = self.pending(id)?;
        let text = text.unwrap_or(&proposal.text).trim();
        if text.is_empty() {
            return Err(Refusal::Bad("A note needs text.".to_owned()));
        }
        let note = Note {
            id: ascending_id("not"),
            scope: proposal.scope,
            project: proposal.project.clone(),
            text: text.to_owned(),
            source: proposal.source.clone(),
            created: now(),
            superseded_by: None,
        };
        self.store.put_note(&note)?;
        let replaced = match &proposal.replaces {
            Some(old) => self.store.note(old)?,
            None => None,
        };
        if let Some(mut old) = replaced.filter(|old| old.superseded_by.is_none()) {
            old.superseded_by = Some(note.id.clone());
            self.store.put_note(&old)?;
        }
        proposal.text.clone_from(&note.text);
        proposal.state = State::Accepted;
        proposal.note = Some(note.id.clone());
        self.store.put_proposal(&proposal)?;
        self.refresh_prompts();
        Ok(note)
    }

    /// Turns proposal `id` down. It is kept, so it isn't proposed again.
    pub(crate) fn dismiss_proposal(&self, id: &str) -> Result<(), Refusal> {
        let mut proposal = self.pending(id)?;
        proposal.state = State::Dismissed;
        self.store.put_proposal(&proposal)?;
        Ok(())
    }

    /// Looks, in the background, for notes worth proposing in what
    /// session `id` did since it was last looked at. Nothing happens with
    /// notes off.
    pub(crate) fn propose_later(self: &Arc<Self>, id: &str) {
        if !self.memory_on() || !lock(&self.live).proposing.insert(id.to_owned()) {
            return;
        }
        let (server, id) = (Arc::clone(self), id.to_owned());
        tokio::spawn(async move {
            if let Err(why) = server.propose(&id).await {
                tracing::warn!(name: "notes.propose.failed", error = %why, "couldn't look for notes to propose");
            }
            lock(&server.live).proposing.remove(&id);
        });
    }

    /// Reads session `id`'s new stretch and stores what a model proposes
    /// from it; returns how many proposals that made.
    ///
    /// # Errors
    ///
    /// When the session can't be read or the model can't be asked. The
    /// stretch is then looked at again with the next one.
    async fn propose(self: &Arc<Self>, id: &str) -> Result<usize, String> {
        // (A session deleted meanwhile has nothing left to read, and a
        // subagent's holds no words of the person's.)
        let Some(session) = self.store.session(id).map_err(|e| e.to_string())? else {
            return Ok(0);
        };
        if session.parent_id.is_some() {
            return Ok(0);
        }
        let mark = format!("proposals/{id}");
        let past = self.store.setting(&mark).map_err(|e| e.to_string())?;
        let all = self.store.messages(id).map_err(|e| e.to_string())?;
        let from = past
            .as_deref()
            .map_or(0, |past| all.partition_point(|m| m.id() <= past));
        let (old, new) = all.split_at(from);
        let Some(newest) = new.last().map(|m| m.id().to_owned()) else {
            return Ok(0);
        };
        let read = stretch(new);
        if !read.substantive() {
            return Ok(0);
        }
        let before: Vec<String> = old
            .iter()
            .rev()
            .filter_map(|m| lines(m).map(|text| format!("{}:\n{text}\n", who(m))))
            .take(BEFORE)
            .collect();
        let before: String = before.into_iter().rev().collect();
        let notes = self
            .store
            .notes(Some(&session.project_id))
            .map_err(|e| e.to_string())?;
        let prompt = propose_prompt(&notes, &before, &read.text());

        self.fresh_sign_ins().await;
        let folder = self.folder(&session.location.directory);
        let model = crate::server::small_model(&folder, &session)
            .ok_or_else(|| "no model is set up".to_owned())?;
        let (provider, _) = folder
            .config
            .resolve(&model)
            .ok_or_else(|| "the model isn't in the configuration".to_owned())?;
        let answer = harlo_engine::generate::generate(
            provider,
            &model,
            PROPOSE_SYSTEM,
            &prompt,
            &CancellationToken::new(),
        )
        .await
        .map_err(|e| e.to_string())?;

        let earlier = self
            .store
            .proposals(Some(&session.project_id))
            .map_err(|e| e.to_string())?;
        let found = admitted(&answer, &session, &read.excerpts, &notes, &earlier);
        for proposal in &found {
            self.store
                .put_proposal(proposal)
                .map_err(|e| e.to_string())?;
        }
        self.store
            .put_setting(&mark, &newest)
            .map_err(|e| e.to_string())?;
        Ok(found.len())
    }
}

#[cfg(test)]
mod tests {
    use harlo_engine::Created;
    use serde_json::json;

    use super::*;

    fn session() -> Session {
        serde_json::from_value(json!({
            "id": "ses_1", "projectID": "prj_1", "title": "Cache layer",
            "location": { "directory": "/tmp/p" },
            "time": { "created": 0, "updated": 0 },
            "cost": 0.0, "tokens": harlo_engine::Tokens::default(),
        }))
        .expect("a session")
    }

    fn person(id: &str, text: &str) -> Message {
        Message::User {
            id: id.to_owned(),
            time: Created { created: 0 },
            text: text.to_owned(),
            skills: Vec::new(),
            files: Vec::new(),
        }
    }

    fn note(id: &str, text: &str, superseded_by: Option<&str>) -> Note {
        Note {
            id: id.to_owned(),
            scope: Scope::Project,
            project: Some("prj_1".to_owned()),
            text: text.to_owned(),
            source: Source {
                session: "ses_0".to_owned(),
                message: "msg_0".to_owned(),
                title: None,
            },
            created: 0,
            superseded_by: superseded_by.map(str::to_owned),
        }
    }

    #[test]
    fn a_stretch_needs_the_person_and_some_work_to_be_looked_at() {
        let asked = [person("msg_1", "What does this do?")];
        assert!(!stretch(&asked).substantive(), "a question alone");
        let twice = [
            person("msg_1", "Do it."),
            person("msg_2", "No, use SQLite."),
        ];
        let read = stretch(&twice);
        assert!(read.substantive(), "the person spoke twice");
        assert_eq!(read.excerpts["msg_2"], "No, use SQLite.");
        assert_eq!(
            read.text(),
            "Person (msg_1):\nDo it.\nPerson (msg_2):\nNo, use SQLite.\n"
        );
        // A long stretch shows its newest part, in whole messages.
        let long: Vec<Message> = (0..40)
            .map(|n| person(&format!("msg_{n:02}"), &"word ".repeat(300)))
            .collect();
        let text = stretch(&long).text();
        assert!(text.len() <= STRETCH_CHARS + 60, "{}", text.len());
        assert!(text.starts_with("(Earlier messages of the stretch are left out.)\nPerson ("));
        assert!(text.contains("(msg_39)") && !text.contains("(msg_00)"));
    }

    #[test]
    fn a_proposal_names_its_message_and_only_a_holding_note_to_replace() {
        let excerpts = HashMap::from([("msg_2".to_owned(), "No, use SQLite.".to_owned())]);
        let notes = [
            note("not_1", "Cache in Redis.", None),
            note("not_0", "Cache in files.", Some("not_1")),
        ];
        let answer = r#"Here: [
            {"text":"Cache in SQLite: one file.","kind":"correction","message":"msg_2","replaces":"not_1"},
            {"text":"Made up.","kind":"decision","message":"msg_9"},
            {"text":"cache in redis","kind":"decision","message":"msg_2"},
            {"text":"Prefers short answers.","kind":"pitfall","scope":"user","message":"msg_2","replaces":"not_0"},
            {"kind":"decision","message":"msg_2"}
        ] done"#;
        let found = admitted(answer, &session(), &excerpts, &notes, &[]);
        assert_eq!(found.len(), 2, "{found:#?}");
        let first = &found[0];
        assert_eq!(first.text, "Cache in SQLite: one file.");
        assert_eq!(first.kind, Kind::Correction);
        assert_eq!(first.project.as_deref(), Some("prj_1"));
        assert_eq!(first.source.session, "ses_1");
        assert_eq!(first.source.message, "msg_2");
        assert_eq!(first.source.title.as_deref(), Some("Cache layer"));
        assert_eq!(first.excerpt, "No, use SQLite.");
        assert_eq!(first.replaces.as_deref(), Some("not_1"));
        assert_eq!(first.state, State::Pending);
        // A user note has no project, and a replaced note isn't replaced again.
        assert_eq!(found[1].scope, Scope::User);
        assert_eq!(found[1].project, None);
        assert_eq!(found[1].replaces, None);
        assert!(admitted("no list here", &session(), &excerpts, &notes, &[]).is_empty());
    }

    #[test]
    fn a_dismissed_proposal_is_not_proposed_again() {
        let excerpts = HashMap::from([
            ("msg_2".to_owned(), "No, use SQLite.".to_owned()),
            ("msg_5".to_owned(), "As I said, SQLite.".to_owned()),
        ]);
        let answer =
            r#"[{"text":"Cache in SQLite: one file.","kind":"correction","message":"msg_2"}]"#;
        let mut first = admitted(answer, &session(), &excerpts, &[], &[]);
        assert_eq!(first.len(), 1);
        first[0].state = State::Dismissed;
        // The same answer again, as when the session is looked at again.
        assert!(admitted(answer, &session(), &excerpts, &[], &first).is_empty());
        // Other words about the same message.
        let reworded =
            r#"[{"text":"Use SQLite for the cache.","kind":"decision","message":"msg_2"}]"#;
        assert!(admitted(reworded, &session(), &excerpts, &[], &first).is_empty());
        // The same words from a later message.
        let later =
            r#"[{"text":"cache in SQLite, one file","kind":"correction","message":"msg_5"}]"#;
        assert!(admitted(later, &session(), &excerpts, &[], &first).is_empty());
        // Something else from a later message is proposed.
        let other = r#"[{"text":"Tests run with nextest.","kind":"decision","message":"msg_5"}]"#;
        assert_eq!(admitted(other, &session(), &excerpts, &[], &first).len(), 1);
    }
}
