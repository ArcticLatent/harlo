//! The server's state: the store, the event stream, and the sessions'
//! runs, inboxes and pending approvals.
//!
//! Every change is announced as an event in OpenCode 2's shape. Durable
//! events are stored first and numbered, so a client that reconnects gets
//! the ones it missed, once and in order.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use harlo_engine::question::Asked;
use harlo_engine::{
    Answer, Ask, Assistant, Config, Created, ErrorInfo, Event, Gate, Message, Outcome, Part, Rule,
    Sink, StepTime, ToolState, Turn, ascending_id, descending_id, now,
};
use serde_json::{Value, json};
use tokio::sync::{broadcast, oneshot};
use tokio_util::sync::CancellationToken;

use crate::git;
use crate::store::{Location, Session, SessionTime, Store, StoredEvent};

/// Whether `message` in an inbox calls for a run: a prompt, or a
/// subagent's answer, but not a notice alone, such as a change of agent.
fn wakes(message: &Message) -> bool {
    match message {
        Message::User { .. } => true,
        // A person's own command waits for the next turn; a background
        // job's result starts one.
        Message::Synthetic {
            metadata: Some(meta),
            ..
        } => !(meta["source"] == "shell" && meta.get("jobID").is_none()),
        _ => false,
    }
}

/// How full, in percent of the model's context, a session gets before its
/// next step starts from a summary.
/// The instructions for text written outside a session.
const WRITE_SYSTEM: &str = "Write what the request asks for, and only that: no preamble, no \
    explanation, no quotes around it.";

const COMPACT_PERCENT: u64 = 85;
/// Events a slow client may fall behind by before it must reconnect.
const BACKLOG: usize = 4096;

/// An event on its way to clients: its number (durable ones only) and JSON.
#[derive(Debug, Clone)]
pub struct Outgoing {
    /// Its sequence number, when it was stored.
    pub seq: Option<u64>,
    /// The event.
    pub json: Arc<str>,
}

/// A run in progress.
pub(crate) struct Run {
    stop: CancellationToken,
    asks: Vec<Pending>,
    /// The agent's questions waiting for answers.
    pub(crate) forms: Vec<crate::forms::PendingForm>,
}

/// A permission request waiting for an answer.
struct Pending {
    id: String,
    data: Value,
    answer: oneshot::Sender<Answer>,
}

/// What runs and what waits, guarded together.
#[derive(Default)]
pub(crate) struct Live {
    pub(crate) runs: HashMap<String, Run>,
    /// Forms answered or dismissed, with their state, by ID.
    pub(crate) settled_forms: HashMap<String, Value>,
    /// Prompts not yet delivered to a run, by session.
    pub(crate) inbox: HashMap<String, Vec<(Message, String)>>,
    /// Background jobs, such as subagents, by ID.
    pub(crate) jobs: HashMap<String, Value>,
    /// Files clients watch.
    pub(crate) watches: crate::watch::Watches,
    /// The files sessions read and wrote.
    pub(crate) known: crate::stale::Known,
    /// Background commands going, by job ID, to stop them.
    pub(crate) shells: HashMap<String, CancellationToken>,
    /// The terminals the server runs, by ID.
    pub(crate) ptys: HashMap<String, Arc<crate::pty::Terminal>>,
}

/// The server.
pub struct Server {
    pub(crate) store: Store,
    pub(crate) config: Config,
    /// The config file's text as read, for `/api/config`.
    pub(crate) config_text: Option<Value>,
    pub(crate) config_dir: PathBuf,
    pub(crate) password: Option<String>,
    /// Phone access, hosted here.
    pub phone: Arc<crate::PhoneHost>,
    /// Where the projects' shadow repositories are kept.
    pub(crate) snapshots: PathBuf,
    /// The shadow repositories in use, by folder.
    shadows: Mutex<HashMap<String, harlo_engine::snapshot::Shadow>>,
    events: broadcast::Sender<Outgoing>,
    emitting: Mutex<()>,
    pub(crate) live: Mutex<Live>,
    /// The agents sessions run as.
    pub(crate) agents: Vec<harlo_engine::agents::Agent>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("config", &self.config.source)
            .finish_non_exhaustive()
    }
}

/// Why a request failed.
#[derive(Debug)]
pub enum Refusal {
    /// There's no such session or request.
    NotFound(String),
    /// The request was wrong.
    Bad(String),
    /// The request was already answered differently.
    Settled(String),
    /// The store failed.
    Store(String),
    /// What's to be made is there already.
    Exists(String),
    /// A worktree couldn't be made or removed; `force` when removing it
    /// with force would.
    Worktree {
        /// What went wrong.
        message: String,
        /// Whether forcing would remove it.
        force: bool,
    },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(why)
            | Self::Bad(why)
            | Self::Settled(why)
            | Self::Store(why)
            | Self::Exists(why)
            | Self::Worktree { message: why, .. } => f.write_str(why),
        }
    }
}

impl From<rusqlite::Error> for Refusal {
    fn from(e: rusqlite::Error) -> Self {
        Self::Store(e.to_string())
    }
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Server {
    /// A server over `store`, with the configuration from `config_home`.
    #[must_use]
    pub fn new(
        store: Store,
        config: Config,
        config_home: &std::path::Path,
        password: Option<String>,
        phone: Arc<crate::PhoneHost>,
        snapshots: PathBuf,
    ) -> Self {
        let config_text = config
            .source
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| jsonc_value(&t));
        let (events, _) = broadcast::channel(BACKLOG);
        let agents = harlo_engine::agents::builtins(&snapshots.with_file_name("plans"));
        end_unfinished(&store);
        Self {
            store,
            config,
            config_text,
            config_dir: config_home.join("opencode"),
            password,
            phone,
            snapshots,
            shadows: Mutex::default(),
            events,
            emitting: Mutex::new(()),
            live: Mutex::default(),
            agents,
        }
    }

    /// The shadow repository recording the files of the project in
    /// `directory`.
    pub(crate) fn shadow(&self, directory: &str) -> harlo_engine::snapshot::Shadow {
        use sha2::Digest as _;
        lock(&self.shadows)
            .entry(directory.to_owned())
            .or_insert_with(|| {
                let key = hex::encode(&sha2::Sha256::digest(directory.as_bytes())[..16]);
                harlo_engine::snapshot::Shadow::new(
                    &self.snapshots.join(key),
                    std::path::Path::new(directory),
                )
            })
            .clone()
    }

    /// The server's password, if it has one.
    #[must_use]
    pub fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }

    /// Announces an event. With `durable`, it's stored under that ID first
    /// and numbered.
    pub(crate) fn emit(
        &self,
        kind: &str,
        data: Value,
        directory: Option<&str>,
        durable: Option<&str>,
    ) {
        let mut body = json!({ "id": ascending_id("evt"), "created": now(), "type": kind });
        body["data"] = data;
        if let Some(d) = directory {
            body["location"] = json!({ "directory": d });
        }
        let _order = lock(&self.emitting);
        let seq = durable.and_then(|aggregate| match self.store.append_event(aggregate, &body) {
            Ok(seq) => Some(seq),
            Err(e) => {
                tracing::error!(name: "event.store.failed", kind, error = %e, "couldn't store an event");
                None
            }
        });
        if let (Some(seq), Some(aggregate)) = (seq, durable) {
            body["durable"] = durable_field(aggregate, seq);
        }
        // (No one listening is fine.)
        let _ = self.events.send(Outgoing {
            seq,
            json: body.to_string().into(),
        });
    }

    /// New events from now on.
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<Outgoing> {
        self.events.subscribe()
    }

    /// The stored events after `after`, as clients receive them.
    pub(crate) fn replay(&self, after: u64) -> Vec<Outgoing> {
        self.store
            .events_after(after)
            .unwrap_or_default()
            .into_iter()
            .map(
                |StoredEvent {
                     seq,
                     aggregate,
                     mut body,
                 }| {
                    body["durable"] = durable_field(&aggregate, seq);
                    Outgoing {
                        seq: Some(seq),
                        json: body.to_string().into(),
                    }
                },
            )
            .collect()
    }

    /// Makes a session working in `directory`.
    ///
    /// # Errors
    ///
    /// Fails when the folder doesn't exist or the store fails.
    pub(crate) async fn create_session(&self, directory: &str) -> Result<Session, Refusal> {
        if !std::path::Path::new(directory).is_dir() {
            return Err(Refusal::Bad(format!("{directory} isn't a folder.")));
        }
        let project_id = git::project_id(directory).await;
        let created = now();
        let session = Session {
            id: descending_id("ses"),
            project_id: project_id.clone(),
            location: Location {
                directory: directory.to_owned(),
            },
            title: None,
            time: SessionTime {
                created,
                updated: created,
                busy: None,
                idle: None,
                archived: None,
            },
            cost: 0.0,
            tokens: harlo_engine::Tokens::default(),
            outcome: None,
            revert: None,
            permissions: Vec::new(),
            model: None,
            agent: None,
            parent_id: None,
            fork: None,
        };
        self.store.put_session(&session)?;
        self.emit(
            "session.created",
            json!({
                "sessionID": session.id, "slug": session.id, "version": env!("CARGO_PKG_VERSION"),
                "projectID": project_id, "location": session.location, "subpath": "",
            }),
            Some(directory),
            Some(&session.id),
        );
        Ok(session)
    }

    /// Makes a child of session `parent` for a subagent running as `agent`,
    /// titled `title`.
    ///
    /// # Errors
    ///
    /// Fails when the store fails.
    pub(crate) fn create_child(
        &self,
        parent: &Session,
        agent: &str,
        title: &str,
        model: Option<harlo_engine::ModelRef>,
    ) -> Result<Session, Refusal> {
        let created = now();
        let session = Session {
            id: descending_id("ses"),
            project_id: parent.project_id.clone(),
            location: parent.location.clone(),
            title: Some(title.to_owned()),
            time: SessionTime {
                created,
                updated: created,
                busy: None,
                idle: None,
                archived: None,
            },
            cost: 0.0,
            tokens: harlo_engine::Tokens::default(),
            outcome: None,
            revert: None,
            permissions: Vec::new(),
            model,
            agent: Some(agent.to_owned()),
            parent_id: Some(parent.id.clone()),
            fork: None,
        };
        self.store.put_session(&session)?;
        self.emit(
            "session.created",
            json!({
                "sessionID": session.id, "slug": session.id, "version": env!("CARGO_PKG_VERSION"),
                "projectID": session.project_id, "parentID": parent.id,
                "location": session.location, "subpath": "", "title": title, "agent": agent,
            }),
            Some(&session.location.directory),
            Some(&session.id),
        );
        Ok(session)
    }

    /// The session `id`.
    ///
    /// # Errors
    ///
    /// Fails when there's no such session or the store fails.
    pub(crate) fn session(&self, id: &str) -> Result<Session, Refusal> {
        self.store
            .session(id)?
            .ok_or_else(|| Refusal::NotFound(format!("There's no session {id}.")))
    }

    /// Changes a session's permission rules or title.
    ///
    /// # Errors
    ///
    /// Fails when there's no such session or the store fails.
    pub(crate) fn update_session(
        &self,
        id: &str,
        permissions: Option<Vec<Rule>>,
        title: Option<String>,
    ) -> Result<(), Refusal> {
        let mut session = self.session(id)?;
        session.time.updated = now();
        if let Some(rules) = permissions {
            session.permissions = rules;
            self.store.put_session(&session)?;
            self.emit(
                "session.permissions",
                json!({ "sessionID": id, "permissions": session.permissions }),
                None,
                Some(id),
            );
        }
        if let Some(title) = title {
            session.title = Some(title.clone());
            self.store.put_session(&session)?;
            self.emit(
                "session.renamed",
                json!({ "sessionID": id, "title": title }),
                None,
                Some(id),
            );
        }
        Ok(())
    }

    /// Takes a prompt for session `id`: delivered now when it's idle, or
    /// when its run is ready for it. Sending the same message ID again
    /// changes nothing.
    ///
    /// # Errors
    ///
    /// Fails when there's no such session or the store fails.
    pub(crate) fn prompt(
        self: &Arc<Self>,
        id: &str,
        message_id: Option<String>,
        text: &str,
        delivery: &str,
        skills: Vec<harlo_engine::skills::Attached>,
        files: Vec<harlo_engine::attachment::File>,
    ) -> Result<Value, Refusal> {
        let session = self.session(id)?;
        let message_id = message_id.unwrap_or_else(|| ascending_id("msg"));
        let created = now();
        let mut payload = json!({ "text": text });
        if !skills.is_empty() {
            payload["skills"] = json!(skills);
        }
        if !files.is_empty() {
            payload["files"] = json!(files);
        }
        let answer = json!({ "data": {
            "id": message_id, "sessionID": id, "time": { "created": created }, "type": "user",
            "payload": payload, "delivery": delivery,
        }});
        let queued = lock(&self.live)
            .inbox
            .get(id)
            .is_some_and(|q| q.iter().any(|(m, _)| m.id() == message_id));
        if queued || self.store.has_message(&message_id)? {
            return Ok(answer);
        }
        if session.title.is_none() {
            let first = title_of(text);
            self.update_session(id, None, Some(first.clone()))?;
            tokio::spawn(Arc::clone(self).name_session(session.clone(), text.to_owned(), first));
        }
        // A new prompt keeps a staged undo: the undone messages go.
        if session.revert.is_some() {
            self.commit_revert(id)?;
        }
        let message = Message::User {
            id: message_id.clone(),
            time: Created { created },
            text: text.to_owned(),
            skills,
            files,
        };
        self.emit(
            "session.inbox.enqueued",
            json!({ "inboxID": message_id, "sessionID": id, "item": { "type": "user", "payload": payload, "delivery": delivery } }),
            Some(&session.location.directory),
            Some(id),
        );
        self.enqueue(id, message, delivery);
        Ok(answer)
    }

    /// Puts `message` in session `id`'s inbox, and starts a run when none
    /// is going.
    pub(crate) fn enqueue(self: &Arc<Self>, id: &str, message: Message, delivery: &str) {
        let start = {
            let mut live = lock(&self.live);
            live.inbox
                .entry(id.to_owned())
                .or_default()
                .push((message, delivery.to_owned()));
            if live.runs.contains_key(id) {
                None
            } else {
                let stop = CancellationToken::new();
                live.runs.insert(
                    id.to_owned(),
                    Run {
                        stop: stop.clone(),
                        asks: Vec::new(),
                        forms: Vec::new(),
                    },
                );
                Some(stop)
            }
        };
        if let Some(stop) = start {
            tokio::spawn(Arc::clone(self).drive(id.to_owned(), stop));
        }
    }

    /// Asks for session `id` to be summarized at its next safe point;
    /// returns the inbox item.
    ///
    /// # Errors
    ///
    /// When there's no such session.
    pub(crate) fn compact(self: &Arc<Self>, id: &str) -> Result<Value, Refusal> {
        let session = self.session(id)?;
        let pending = harlo_engine::compaction::Compaction::pending(ascending_id("msg"), "manual");
        let item = json!({
            "id": pending.id, "sessionID": id, "time": pending.time, "type": "compaction",
            "payload": {}, "delivery": "steer",
        });
        self.emit(
            "session.inbox.enqueued",
            json!({ "inboxID": pending.id, "sessionID": id, "item": { "type": "compaction", "payload": {}, "delivery": "steer" } }),
            Some(&session.location.directory),
            Some(id),
        );
        self.enqueue(id, Message::Compaction(Box::new(pending)), "steer");
        Ok(json!({ "data": item }))
    }

    /// Changes how a waiting item in session `id`'s inbox reaches its run.
    ///
    /// # Errors
    ///
    /// When it isn't waiting there: delivered already, or never sent.
    pub(crate) fn set_delivery(&self, id: &str, item: &str, delivery: &str) -> Result<(), Refusal> {
        let session = self.session(id)?;
        let mut live = lock(&self.live);
        let waiting = live
            .inbox
            .get_mut(id)
            .and_then(|q| q.iter_mut().find(|(m, _)| m.id() == item))
            .ok_or_else(|| Refusal::NotFound(format!("{item} isn't waiting in {id}.")))?;
        delivery.clone_into(&mut waiting.1);
        drop(live);
        self.emit(
            "session.inbox.delivery.changed",
            json!({ "sessionID": id, "inboxID": item, "delivery": delivery }),
            Some(&session.location.directory),
            Some(id),
        );
        Ok(())
    }

    /// Takes a waiting item out of session `id`'s inbox; one already gone
    /// is fine.
    ///
    /// # Errors
    ///
    /// When there's no such session.
    pub(crate) fn cancel_inbox(&self, id: &str, item: &str) -> Result<(), Refusal> {
        let session = self.session(id)?;
        let removed = {
            let mut live = lock(&self.live);
            live.inbox.get_mut(id).is_some_and(|q| {
                let before = q.len();
                q.retain(|(m, _)| m.id() != item);
                q.len() < before
            })
        };
        if removed {
            self.emit(
                "session.inbox.cancelled",
                json!({ "sessionID": id, "inboxID": item }),
                Some(&session.location.directory),
                Some(id),
            );
        }
        Ok(())
    }

    /// Prompts waiting for session `id`'s run.
    pub(crate) fn inbox(&self, id: &str) -> Vec<Value> {
        lock(&self.live)
            .inbox
            .get(id)
            .map(|q| {
                q.iter()
                    .map(|(m, delivery)| match m {
                        Message::User { id, text, time, .. } => json!({
                            "id": id, "type": "user", "payload": { "text": text }, "delivery": delivery, "time": time,
                        }),
                        Message::Synthetic { id, text, time, description, metadata } => json!({
                            "id": id, "type": "synthetic", "delivery": delivery, "time": time,
                            "payload": { "text": text, "description": description, "metadata": metadata },
                        }),
                        Message::Compaction(c) => json!({
                            "id": c.id, "type": "compaction", "payload": {}, "delivery": delivery, "time": c.time,
                        }),
                        _ => Value::Null,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether session `id` is running.
    pub(crate) fn busy(&self) -> Vec<String> {
        lock(&self.live).runs.keys().cloned().collect()
    }

    /// Stops session `id`'s run, if it has one.
    pub(crate) fn interrupt(&self, id: &str) {
        if let Some(run) = lock(&self.live).runs.get(id) {
            run.stop.cancel();
        }
    }

    /// Session `id`'s pending permission requests.
    pub(crate) fn asks(&self, id: &str) -> Vec<Value> {
        lock(&self.live)
            .runs
            .get(id)
            .map(|r| r.asks.iter().map(|a| a.data.clone()).collect())
            .unwrap_or_default()
    }

    /// Answers permission request `request` of session `id`.
    ///
    /// Each request is decided once. The same reply again (a client
    /// retrying, or a phone and a desktop both sending it) succeeds and
    /// changes nothing; a different one is refused as settled.
    ///
    /// # Errors
    ///
    /// Fails when there's no such request, it was answered differently, or
    /// the decision isn't one of `once`, `always` and `reject`.
    pub(crate) fn reply(
        &self,
        id: &str,
        request: &str,
        decision: &str,
        message: Option<String>,
    ) -> Result<(), Refusal> {
        let answer = match decision {
            "once" => Answer::Once,
            "always" => Answer::Always,
            "reject" => Answer::Reject(message.filter(|m| !m.trim().is_empty())),
            other => {
                return Err(Refusal::Bad(format!(
                    "{other} isn't a decision: once, always or reject."
                )));
            }
        };
        // Taking the request and recording its decision happen under one
        // lock, so two replies racing each other decide it once.
        let pending = {
            let mut live = lock(&self.live);
            let asks = live.runs.get_mut(id).map(|r| &mut r.asks);
            let pending = asks.and_then(|a| {
                a.iter()
                    .position(|p| p.id == request)
                    .map(|at| a.remove(at))
            });
            if pending.is_some() {
                let said = match &answer {
                    Answer::Reject(m) => m.as_deref(),
                    _ => None,
                };
                self.store
                    .record_decision(request, id, decision, said, now())?;
            }
            pending
        };
        let Some(pending) = pending else {
            return match self.store.decision(id, request)? {
                Some(made) if made == decision => Ok(()),
                Some(made) => Err(Refusal::Settled(format!(
                    "The permission request {request} was already answered: {made}."
                ))),
                None => Err(Refusal::NotFound(format!(
                    "There's no permission request {request}."
                ))),
            };
        };
        let directory = self
            .store
            .session(id)
            .ok()
            .flatten()
            .map(|s| s.location.directory);
        self.emit(
            "permission.replied",
            json!({ "sessionID": id, "requestID": request, "reply": decision }),
            directory.as_deref(),
            Some(id),
        );
        if answer == Answer::Always {
            self.save_approvals(id, &pending.data)?;
        }
        // (The run may have been stopped meanwhile.)
        let _ = pending.answer.send(answer);
        Ok(())
    }

    /// Asks the session's model for a title from its first message, and
    /// uses it unless the session was renamed meanwhile.
    async fn name_session(self: Arc<Self>, session: Session, first: String, provisional: String) {
        let folder = self.folder(&session.location.directory);
        let Some(model) = session
            .model
            .clone()
            .or_else(|| folder.config.default_model())
        else {
            return;
        };
        let Some((provider, _)) = folder.config.resolve(&model) else {
            return;
        };
        let asked = harlo_engine::generate::generate(
            provider,
            &model,
            harlo_engine::generate::TITLE_SYSTEM,
            &harlo_engine::generate::title_prompt(&first),
            &CancellationToken::new(),
        )
        .await;
        let title = match asked {
            Ok(raw) => harlo_engine::generate::clean_title(&raw),
            Err(e) => {
                tracing::warn!(name: "session.title.failed", error = %e, "couldn't name a session");
                None
            }
        };
        let renamed = self
            .session(&session.id)
            .is_ok_and(|s| s.title.as_deref() != Some(provisional.as_str()));
        if let Some(title) = title.filter(|_| !renamed) {
            let _ = self.update_session(&session.id, None, Some(title));
        }
    }

    /// Runs session `id` until its inbox is empty or it's stopped.
    async fn drive(self: Arc<Self>, id: String, stop: CancellationToken) {
        while let Ok(mut session) = self.session(&id) {
            let mut delivered = lock(&self.live).inbox.remove(&id).unwrap_or_default();
            let inbox_ids: Vec<String> = delivered.iter().map(|(m, _)| m.id().to_owned()).collect();
            order_notices(&mut delivered);
            let started = now();
            session.time.busy = Some(started);
            session.time.updated = started;
            let _ = self.store.put_session(&session);
            self.emit(
                "session.execution.started",
                json!({ "sessionID": id }),
                None,
                Some(&id),
            );
            let directory = session.location.directory.clone();
            for ((message, _), inbox_id) in delivered.iter().zip(&inbox_ids) {
                if let Err(e) = self.store.put_message(&id, message) {
                    tracing::error!(name: "message.store.failed", error = %e, "couldn't store a prompt");
                }
                self.emit(
                    "session.inbox.delivered",
                    json!({ "sessionID": id, "inboxID": inbox_id }),
                    Some(&directory),
                    Some(&id),
                );
            }
            let asked: Vec<_> = delivered
                .iter()
                .filter_map(|(m, _)| match m {
                    Message::Compaction(c) if c.status == "pending" => Some((**c).clone()),
                    _ => None,
                })
                .collect();
            let prompted = delivered.iter().any(|(m, _)| wakes(m));
            let mut ended = ("succeeded", None);
            for pending in asked {
                ended = self.run_compaction(&session, pending, &stop).await;
            }
            // A summary asked for on its own ends the run there.
            let (word, error) = if prompted && ended.0 == "succeeded" {
                self.run_once(&session, &stop).await
            } else {
                ended
            };
            if prompted {
                self.review_goal(&session, word, &stop).await;
            }
            self.finish(&id, word, error);

            let mut live = lock(&self.live);
            // (Notices alone, such as a change of agent, wait for a prompt.)
            let more = live.inbox.get(&id).is_some_and(|q| {
                q.iter()
                    .any(|(m, _)| wakes(m) || matches!(m, Message::Compaction(_)))
            });
            if !more || stop.is_cancelled() {
                live.runs.remove(&id);
                drop(live);
                // A subagent's answer goes to the session that asked.
                self.report_to_parent(&id, word);
                break;
            }
        }
    }

    /// Writes the summary a person asked for; returns how it ended, as
    /// [`Self::run_once`] does.
    async fn run_compaction(
        self: &Arc<Self>,
        session: &Session,
        pending: harlo_engine::compaction::Compaction,
        stop: &CancellationToken,
    ) -> (&'static str, Option<String>) {
        let hooks = Hooks {
            server: Arc::clone(self),
            session: session.id.clone(),
            directory: session.location.directory.clone(),
            streaming: Mutex::default(),
        };
        let folder = self.folder(&session.location.directory);
        let agent = folder.agent(session.agent.as_deref());
        let Some(model) = agent
            .model
            .clone()
            .or_else(|| session.model.clone())
            .or_else(|| folder.config.default_model())
        else {
            return (
                "failed",
                Some("No model is set up to write the summary.".to_owned()),
            );
        };
        let Some((provider, _)) = folder.config.resolve(&model) else {
            return (
                "failed",
                Some(format!(
                    "The model {}/{} isn't in the configuration.",
                    model.provider, model.model
                )),
            );
        };
        let history = match self.store.messages(&session.id) {
            Ok(h) => h,
            Err(e) => return ("failed", Some(format!("Couldn't read the session: {e}"))),
        };
        let mut system = self
            .system_prompt(&session.location.directory, &folder)
            .await;
        if let Some(own) = &agent.system {
            system = format!("{own}\n\n{system}");
        }
        let done = harlo_engine::compaction::compact(
            pending, provider, &model, &system, &history, &hooks, stop,
        )
        .await;
        match (done.status.as_str(), done.error) {
            ("completed", _) => ("succeeded", None),
            _ if stop.is_cancelled() => ("interrupted", None),
            (_, error) => ("failed", error.map(|e| e.message)),
        }
    }

    /// One turn of the engine over the session's history; returns how it
    /// ended, in OpenCode's word, and why it failed.
    async fn run_once(
        self: &Arc<Self>,
        session: &Session,
        stop: &CancellationToken,
    ) -> (&'static str, Option<String>) {
        let folder = self.folder(&session.location.directory);
        let agent = folder.agent(session.agent.as_deref());
        let Some(model) = agent
            .model
            .clone()
            .or_else(|| session.model.clone())
            .or_else(|| folder.config.default_model())
        else {
            return (
                "failed",
                Some(
                    "No model is set up. Add a provider and model to OpenCode's config.".to_owned(),
                ),
            );
        };
        let Some((provider, spec)) = folder.config.resolve(&model) else {
            return (
                "failed",
                Some(format!(
                    "The model {}/{} isn't in the configuration.",
                    model.provider, model.model
                )),
            );
        };
        let history = match self.store.messages(&session.id) {
            Ok(h) => h,
            Err(e) => return ("failed", Some(format!("Couldn't read the session: {e}"))),
        };
        let directory = session.location.directory.clone();
        let mut system = self.system_prompt(&directory, &folder).await;
        if let Some(own) = &agent.system {
            system = format!("{own}\n\n{system}");
        }
        let turn = Turn {
            cwd: PathBuf::from(&directory),
            agent: agent.id.clone(),
            model,
            provider: provider.clone(),
            system,
            rules: vec![
                ("agent".to_owned(), agent.rules.clone()),
                ("config".to_owned(), folder.config.permissions.clone()),
                ("saved".to_owned(), self.saved_rules(&session.project_id)),
                ("session".to_owned(), session.permissions.clone()),
            ],
            history,
            shadow: folder.config.snapshots.then(|| self.shadow(&directory)),
            memory: self.memory_for(session),
            jobs: Some(Arc::new(crate::shell::SessionJobs {
                server: Arc::clone(self),
                session: session.id.clone(),
            })),
            files: Some(Arc::new(crate::stale::SessionFiles {
                server: Arc::clone(self),
                session: session.id.clone(),
            })),
            subagents: Some(Arc::new(crate::subagents::SessionSubagents {
                server: Arc::clone(self),
                session: session.id.clone(),
            })),
            subagent_agents: folder.subagents(),
            // Near the model's context limit, the next step starts from a summary.
            compact_at: spec.context.map(|c| c / 100 * COMPACT_PERCENT),
            skills: self.skills_for(&session.location.directory).await,
            todo: folder.config.todo,
            steering: Some(Arc::new(crate::agents::SessionSteering {
                server: Arc::clone(self),
                session: session.id.clone(),
                directory: directory.clone(),
            })),
        };
        let hooks = Hooks {
            server: Arc::clone(self),
            session: session.id.clone(),
            directory,
            streaming: Mutex::default(),
        };
        match harlo_engine::run(turn, &hooks, &hooks, stop).await {
            Outcome::Succeeded => ("succeeded", None),
            Outcome::Interrupted => ("interrupted", None),
            Outcome::Failed(e) => ("failed", Some(e.to_string())),
        }
    }

    /// Answers `prompt` from session `id`'s conversation, leaving the
    /// session as it was: no tools, nothing stored.
    ///
    /// # Errors
    ///
    /// When there's no such session or model, or the model fails.
    pub(crate) async fn aside(&self, id: &str, prompt: &str) -> Result<Value, Refusal> {
        let session = self.session(id)?;
        let (answer, cost) = self
            .ask_model(&session, prompt, &CancellationToken::new())
            .await
            .map_err(Refusal::Bad)?;
        Ok(json!({ "text": answer.text, "tokens": answer.tokens, "cost": cost }))
    }

    /// What session `session`'s model answers to `prompt` after the
    /// conversation so far, with no tools and nothing stored; and what it
    /// cost.
    ///
    /// # Errors
    ///
    /// When no model is set up, the store fails, or the model fails.
    pub(crate) async fn ask_model(
        &self,
        session: &Session,
        prompt: &str,
        stop: &CancellationToken,
    ) -> Result<(harlo_engine::generate::Answer, f64), String> {
        let folder = self.folder(&session.location.directory);
        let agent = folder.agent(session.agent.as_deref());
        let model = agent
            .model
            .clone()
            .or_else(|| session.model.clone())
            .or_else(|| folder.config.default_model())
            .ok_or_else(|| "No model is set up.".to_owned())?;
        let (provider, spec) = folder.config.resolve(&model).ok_or_else(|| {
            format!(
                "The model {}/{} isn't in the configuration.",
                model.provider, model.model
            )
        })?;
        let mut system = self
            .system_prompt(&session.location.directory, &folder)
            .await;
        if let Some(own) = &agent.system {
            system = format!("{own}\n\n{system}");
        }
        let stored = self
            .store
            .messages(&session.id)
            .map_err(|e| format!("Couldn't read the session: {e}"))?;
        let mut history = harlo_engine::compaction::since_summary(&stored).to_vec();
        history.push(harlo_engine::generate::user(prompt));
        let answer = harlo_engine::generate::answer(provider, &model, &system, &history, stop)
            .await
            .map_err(|e| e.to_string())?;
        let cost = spec.cost(&answer.tokens);
        Ok((answer, cost))
    }

    /// Answers `prompt` with `model`, or the default one, outside any
    /// session, such as a commit message.
    ///
    /// # Errors
    ///
    /// When there's no such model, or the model fails.
    pub(crate) async fn write_text(
        &self,
        prompt: &str,
        model: Option<harlo_engine::ModelRef>,
    ) -> Result<String, Refusal> {
        let model = model
            .or_else(|| self.config.default_model())
            .ok_or_else(|| Refusal::Bad("No model is set up.".to_owned()))?;
        let (provider, _) = self.config.resolve(&model).ok_or_else(|| {
            Refusal::NotFound(format!(
                "The model {}/{} isn't in the configuration.",
                model.provider, model.model
            ))
        })?;
        harlo_engine::generate::generate(
            provider,
            &model,
            WRITE_SYSTEM,
            prompt,
            &CancellationToken::new(),
        )
        .await
        .map_err(|e| Refusal::Bad(e.to_string()))
    }

    /// The system prompt for a session in `directory`, as OpenCode orders
    /// it: the agent's prompt, the project's instructions, then the date
    /// and environment.
    async fn system_prompt(&self, directory: &str, folder: &crate::folder::Folder) -> String {
        let root = git::toplevel(directory).await;
        let home = dirs::home_dir().unwrap_or_default();
        let search = harlo_engine::instructions::Search {
            workspace: std::path::Path::new(directory),
            project_root: root.as_deref().map(std::path::Path::new),
            home: &home,
            config_dir: &self.config_dir,
            configured: &folder.config.instructions,
            ..Default::default()
        }
        .with_env();
        let found = harlo_engine::instructions::load(&search);
        let mut prompt = BASE_PROMPT.to_owned();
        if !found.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(&harlo_engine::instructions::render(&found));
        }
        let notes = self.notes_prompt(&git::project_id(directory).await);
        if !notes.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(notes.trim_end());
        }
        let _ = write!(
            prompt,
            "\n\nWorking folder: {directory}\nIs a git repository: {}\nPlatform: {}\nToday's date: {}",
            if root.is_some() { "yes" } else { "no" },
            std::env::consts::OS,
            today(),
        );
        prompt
    }

    /// Records how a run ended.
    fn finish(&self, id: &str, word: &str, error: Option<String>) {
        let time = now();
        let idle = Message::Idle {
            id: ascending_id("msg"),
            time: Created { created: time },
            outcome: word.to_owned(),
            error: error.clone().map(|message| ErrorInfo {
                kind: "run".to_owned(),
                message,
            }),
        };
        if let Err(e) = self.store.put_message(id, &idle) {
            tracing::error!(name: "message.store.failed", error = %e, "couldn't store a run's end");
        }
        if let Ok(mut session) = self.session(id) {
            session.outcome = Some(word.to_owned());
            session.time.idle = Some(time);
            session.time.updated = time;
            let _ = self.store.put_session(&session);
        }
        let mut data = json!({ "sessionID": id });
        if let Some(message) = error {
            data["error"] = json!({ "type": "run", "message": message });
        }
        self.emit(&format!("session.execution.{word}"), data, None, Some(id));
    }

    /// Stores a step and adds its usage to the session's.
    fn store_step(&self, id: &str, step: &Assistant, count: bool) {
        if let Err(e) = self
            .store
            .put_message(id, &Message::Assistant(Box::new(step.clone())))
        {
            tracing::error!(name: "message.store.failed", error = %e, "couldn't store a step");
        }
        if count {
            self.add_usage(id, &step.tokens, step.cost);
        }
    }

    /// Adds tokens and cost to session `id`'s, and says so.
    fn add_usage(&self, id: &str, tokens: &harlo_engine::Tokens, cost: f64) {
        if let Ok(mut session) = self.session(id) {
            let (t, s) = (&mut session.tokens, tokens);
            t.input += s.input;
            t.output += s.output;
            t.reasoning += s.reasoning;
            t.cache.read += s.cache.read;
            t.cache.write += s.cache.write;
            session.cost += cost;
            session.time.updated = now();
            let _ = self.store.put_session(&session);
            self.emit(
                "session.usage.updated",
                json!({ "sessionID": id, "cost": session.cost, "tokens": session.tokens }),
                None,
                None,
            );
        }
    }
}

/// What a step that stopped midway is told on the server's next start.
const STOPPED_MIDWAY: &str = "The server stopped before this step ended.";

/// Ends, as stopped, each run and step that never ended: the server
/// stopped during it. What the step had streamed stays.
fn end_unfinished(store: &Store) {
    let Ok(sessions) = store.sessions() else {
        return;
    };
    for mut session in sessions {
        let (busy, idle) = (session.time.busy, session.time.idle);
        if busy.is_some_and(|b| idle.is_none_or(|i| i < b)) {
            session.time.idle = Some(now());
            session.outcome = Some("interrupted".to_owned());
            if let Err(e) = store.put_session(&session) {
                tracing::error!(name: "session.store.failed", error = %e, "couldn't end a run");
            }
        }
        if let Ok(Some(Message::Assistant(mut step))) = store.last_message(&session.id)
            && step.end_unfinished(now(), STOPPED_MIDWAY)
            && let Err(e) = store.put_message(&session.id, &Message::Assistant(step))
        {
            tracing::error!(name: "message.store.failed", error = %e, "couldn't end a step");
        }
    }
}

/// How often a streaming step is saved, so a server that stops keeps
/// most of what was streamed.
const SAVE_STREAMING: std::time::Duration = std::time::Duration::from_secs(1);

/// The step being streamed, as far as it has come.
#[derive(Default)]
struct Streaming {
    /// The step, without its calls (they're saved once it's streamed).
    step: Option<Assistant>,
    /// Its text and reasoning, by place in its content.
    parts: std::collections::BTreeMap<usize, Part>,
    /// When it was last saved.
    saved: Option<std::time::Instant>,
}

/// The engine's sink and gate for one session's run.
struct Hooks {
    server: Arc<Server>,
    session: String,
    directory: String,
    streaming: Mutex<Streaming>,
}

impl Hooks {
    /// Follows the step being streamed, and saves it now and then.
    fn follow(&self, event: &Event) {
        let mut s = lock(&self.streaming);
        match event {
            Event::StepStarted {
                message,
                agent,
                model,
                time,
            } => {
                *s = Streaming {
                    step: Some(Assistant {
                        id: message.clone(),
                        time: StepTime {
                            created: *time,
                            ..StepTime::default()
                        },
                        agent: agent.clone(),
                        model: model.clone(),
                        content: Vec::new(),
                        finish: None,
                        raw_finish: None,
                        cost: 0.0,
                        tokens: harlo_engine::Tokens::default(),
                        error: None,
                        snapshot: None,
                    }),
                    parts: std::collections::BTreeMap::new(),
                    saved: Some(std::time::Instant::now()),
                };
                return;
            }
            Event::FirstToken { .. } => {
                if let Some(step) = s.step.as_mut() {
                    step.time.first_token = Some(now());
                }
                return;
            }
            Event::TextStarted { at, .. } => {
                s.parts.insert(
                    *at,
                    Part::Text {
                        text: String::new(),
                    },
                );
            }
            Event::ReasoningStarted { at, .. } => {
                s.parts.insert(
                    *at,
                    Part::Reasoning {
                        text: String::new(),
                    },
                );
            }
            Event::TextDelta { at, delta, .. } | Event::ReasoningDelta { at, delta, .. } => {
                if let Some(Part::Text { text } | Part::Reasoning { text }) = s.parts.get_mut(at) {
                    text.push_str(delta);
                }
            }
            Event::TextEnded { at, text, .. } | Event::ReasoningEnded { at, text, .. } => {
                if let Some(Part::Text { text: t } | Part::Reasoning { text: t }) =
                    s.parts.get_mut(at)
                {
                    t.clone_from(text);
                }
            }
            Event::StepStreamed { .. } | Event::StepEnded { .. } => {
                *s = Streaming::default();
                return;
            }
            _ => return,
        }
        if s.saved.is_some_and(|t| t.elapsed() < SAVE_STREAMING) {
            return;
        }
        s.saved = Some(std::time::Instant::now());
        let parts: Vec<Part> = s.parts.values().cloned().collect();
        if let Some(step) = s.step.as_mut() {
            step.content = parts;
            self.server.store_step(&self.session, step, false);
        }
    }
}

impl Hooks {
    /// Stores a finished compaction, counts what it used and says how it
    /// went.
    fn compacted(&self, compaction: &harlo_engine::compaction::Compaction) {
        if let Err(e) = self.server.store.put_message(
            &self.session,
            &Message::Compaction(Box::new(compaction.clone())),
        ) {
            tracing::error!(name: "message.store.failed", error = %e, "couldn't store a compaction");
        }
        self.server
            .add_usage(&self.session, &compaction.tokens, compaction.cost);
        match (&compaction.summary, &compaction.error) {
            (Some(summary), _) if compaction.status == "completed" => self.send(
                "session.compaction.ended",
                json!({ "reason": compaction.reason, "model": compaction.model, "text": summary }),
                true,
            ),
            (_, error) => self.send(
                "session.compaction.failed",
                json!({ "reason": compaction.reason, "error": error }),
                true,
            ),
        }
    }

    fn send(&self, kind: &str, mut data: Value, durable: bool) {
        data["sessionID"] = json!(self.session);
        self.server.emit(
            kind,
            data,
            Some(&self.directory),
            durable.then_some(self.session.as_str()),
        );
    }
}

impl Sink for Hooks {
    fn emit(&self, event: Event) {
        self.follow(&event);
        match event {
            Event::StepStarted { message, agent, model, time } => self.send(
                "session.step.started",
                json!({ "agent": agent, "model": model, "assistantMessageID": message, "started": time }),
                true,
            ),
            Event::RetryScheduled { message, attempt, at, error, status } => self.send(
                "session.retry.scheduled",
                json!({
                    "assistantMessageID": message, "attempt": attempt, "at": at,
                    "error": { "type": "provider", "message": error, "status": status },
                }),
                true,
            ),
            Event::FirstToken { message } => self.send("session.step.first-token", json!({ "assistantMessageID": message }), true),
            Event::TextStarted { message, at } => self.send("session.text.started", json!({ "assistantMessageID": message, "ordinal": at }), true),
            Event::TextDelta { message, at, delta } => {
                self.send("session.text.delta", json!({ "assistantMessageID": message, "ordinal": at, "delta": delta }), false);
            }
            Event::TextEnded { message, at, text } => {
                self.send("session.text.ended", json!({ "assistantMessageID": message, "ordinal": at, "text": text }), true);
            }
            Event::ReasoningStarted { message, at } => {
                self.send("session.reasoning.started", json!({ "assistantMessageID": message, "ordinal": at }), true);
            }
            Event::ReasoningDelta { message, at, delta } => {
                self.send("session.reasoning.delta", json!({ "assistantMessageID": message, "ordinal": at, "delta": delta }), false);
            }
            Event::ReasoningEnded { message, at, text } => {
                self.send("session.reasoning.ended", json!({ "assistantMessageID": message, "ordinal": at, "text": text }), true);
            }
            Event::ToolInputStarted { message, call, name } => {
                self.send("session.tool.input.started", json!({ "assistantMessageID": message, "id": call, "name": name }), true);
            }
            Event::ToolInputEnded { message, call, text } => {
                self.send("session.tool.input.ended", json!({ "assistantMessageID": message, "id": call, "text": text }), true);
            }
            Event::ToolCalled { message, call, input, .. } => self.send(
                "session.tool.called",
                json!({ "assistantMessageID": message, "id": call, "input": input, "executed": false }),
                true,
            ),
            Event::StepStreamed { step } => {
                self.server.store_step(&self.session, &step, false);
                self.send("session.step.streamed", json!({ "assistantMessageID": step.id }), true);
            }
            Event::ToolEnded { message, call, state } => match state {
                ToolState::Completed { content, metadata, .. } => self.send(
                    "session.tool.success",
                    json!({ "assistantMessageID": message, "id": call, "content": content, "metadata": metadata, "executed": false }),
                    true,
                ),
                ToolState::Error { error, metadata, .. } => self.send(
                    "session.tool.failed",
                    json!({ "assistantMessageID": message, "id": call, "error": error, "metadata": metadata, "executed": false }),
                    true,
                ),
                ToolState::Running { .. } => {}
            },
            Event::CompactionStarted { id, reason } => {
                self.send("session.compaction.started", json!({ "reason": reason, "inputID": id }), true);
            }
            Event::CompactionEnded { compaction } => self.compacted(&compaction),
            Event::Noted { message } => {
                if let Err(e) = self.server.store.put_message(&self.session, &message) {
                    tracing::error!(name: "message.store.failed", error = %e, "couldn't store a note");
                }
            }
            Event::StepEnded { step } => {
                self.server.store_step(&self.session, &step, true);
                if let Some(error) = &step.error {
                    self.send("session.step.failed", json!({ "assistantMessageID": step.id, "error": error }), true);
                }
                let files = step.snapshot.as_ref().map(|s| s.files.clone()).unwrap_or_default();
                self.send(
                    "session.step.ended",
                    json!({
                        "assistantMessageID": step.id, "finish": step.finish.as_deref().unwrap_or("unknown"),
                        "rawFinish": step.raw_finish, "tokens": step.tokens, "cost": step.cost, "files": files,
                    }),
                    true,
                );
            }
        }
    }
}

impl Gate for Hooks {
    fn ask(&self, ask: Ask) -> impl Future<Output = Answer> + Send {
        let (tx, rx) = oneshot::channel();
        let metadata = ask.metadata;
        let data = json!({
            "id": ask.id, "sessionID": self.session, "action": ask.action, "resources": ask.resources,
            "save": ask.save,
            "message": ask.why, "metadata": metadata,
            "source": { "type": "tool", "messageID": ask.message, "id": ask.call },
        });
        let registered = {
            let mut live = lock(&self.server.live);
            live.runs.get_mut(&self.session).map(|run| {
                run.asks.push(Pending {
                    id: ask.id.clone(),
                    data: data.clone(),
                    answer: tx,
                });
            })
        };
        if registered.is_some() {
            self.server.emit(
                "permission.asked",
                data,
                Some(&self.directory),
                Some(&self.session),
            );
        }
        async move { rx.await.unwrap_or(Answer::Reject(None)) }
    }

    fn question(&self, asked: Asked) -> impl Future<Output = Option<Vec<Vec<String>>>> + Send {
        let rx = self
            .server
            .ask_questions(&self.session, &self.directory, &asked);
        // (A run that ended, or a server going down, dismisses them.)
        async move { rx?.await.ok().flatten() }
    }
}

/// Gives the server's notices IDs that keep them before what was
/// delivered after them: a client's ID made in the same millisecond can
/// sort before one the server made earlier.
fn order_notices(delivered: &mut [(Message, String)]) {
    for at in (0..delivered.len().saturating_sub(1)).rev() {
        let next = delivered[at + 1].0.id().to_owned();
        if let Message::Synthetic { id, .. } = &mut delivered[at].0
            && *id >= next
            && let Some(before) = harlo_engine::just_before(&next)
        {
            *id = before;
        }
    }
}

fn durable_field(aggregate: &str, seq: u64) -> Value {
    json!({ "aggregateID": aggregate, "seq": seq, "version": 1 })
}

/// A short title from a session's first prompt: its first line, cut.
fn title_of(text: &str) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("New session")
        .trim();
    let mut title: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        title.push('…');
    }
    title
}

/// The parts of the system prompt that don't depend on the project.
const BASE_PROMPT: &str = "You are a coding agent.\n\
    Use the tools to read and change files and to run commands. Paths are relative to the \
    project's folder.\n\
    Change only what the task needs. When you're done, say briefly what you did.";

/// Today's date, `YYYY-MM-DD`, in UTC.
fn today() -> String {
    let days = now().div_euclid(86_400_000);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// A config file's text as JSON (comments allowed).
fn jsonc_value(text: &str) -> Option<Value> {
    jsonc_parser::parse_to_serde_value(text, &jsonc_parser::ParseOptions::default()).ok()
}
