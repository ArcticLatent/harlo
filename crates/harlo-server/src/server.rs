//! The server's state: the store, the event stream, and the sessions'
//! runs, inboxes and pending approvals.
//!
//! Every change is announced as an event. Durable
//! events are stored first and numbered, so a client that reconnects gets
//! the ones it missed, once and in order.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use harlo_engine::instructions::Instruction;
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

/// What a session's turn runs with, once chosen from its folder's
/// configuration.
struct Chosen {
    folder: crate::folder::Folder,
    agent: harlo_engine::agents::Agent,
    model: harlo_engine::ModelRef,
    provider: harlo_engine::ProviderSpec,
    spec: harlo_engine::ModelSpec,
    rules: Vec<(String, Vec<Rule>)>,
    history: Vec<Message>,
}

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
    /// The commands agents are waiting for, by session and call, each
    /// with what tells its turn a person asked not to wait for it.
    pub(crate) waited: HashMap<(String, String), Arc<tokio::sync::Notify>>,
    /// The terminals the server runs, by ID.
    pub(crate) ptys: HashMap<String, Arc<crate::pty::Terminal>>,
    /// Where random web searches stand.
    pub(crate) searches: crate::search::Searches,
    /// The parts of sessions' system prompts that are held still, by
    /// session.
    steady: HashMap<String, Steady>,
}

/// What a session's system prompt keeps saying while its conversation may
/// be in a provider's cache: the notes, the date and the instruction
/// files as they were.
///
/// A provider's cache holds a conversation from its start, system prompt
/// first. A note kept in the session, an instruction file edited, or
/// midnight, would change that start, and the whole conversation would be
/// charged again as new.
#[derive(Debug)]
struct Steady {
    notes: String,
    date: String,
    /// The instruction files as the system prompt carries them.
    instructions: Vec<Instruction>,
    /// The instruction files as the session last heard of them: from the
    /// prompt, or from an update since.
    told: Vec<Instruction>,
    /// When a turn last used it, in Unix milliseconds.
    used: i64,
}

/// How long after a session's last turn a provider may still hold its
/// conversation in cache, in milliseconds: the longest any offers, an
/// hour. A session idle for longer gets today's notes and date.
const WARM_MS: i64 = 60 * 60 * 1000;

/// The global config file as last read.
#[derive(Debug)]
struct Global {
    /// Where config files are looked for.
    home: PathBuf,
    /// The configuration.
    config: Config,
    /// The file's text as read, for `/api/config`.
    text: Option<Value>,
    /// When the file read was last changed, to notice edits.
    stamp: Option<std::time::SystemTime>,
}

/// When the global config file in `home` was last changed.
fn stamp(home: &std::path::Path) -> Option<std::time::SystemTime> {
    let folder = harlo_engine::config_folder(home);
    harlo_engine::CONFIG_FILES
        .iter()
        .find_map(|n| std::fs::metadata(folder.join(n)).ok())
        .and_then(|m| m.modified().ok())
}

/// The text of the config file `config` was read from.
fn text_of(config: &Config) -> Option<Value> {
    config
        .source
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| jsonc_value(&t))
}

/// The server.
pub struct Server {
    pub(crate) store: Store,
    /// The global config file, read again when it changes.
    global: Mutex<Global>,
    /// The model catalog: the bundled one, or the last fetched.
    pub(crate) catalog: Mutex<Arc<harlo_engine::catalog::Catalog>>,
    /// Browser sign-ins under way.
    pub(crate) attempts: crate::oauth::Attempts,
    /// MCP server connections.
    pub(crate) mcp: crate::mcp::Mcp,
    /// The models Claude Code last listed.
    pub(crate) claude_models: Mutex<Vec<harlo_engine::ModelSpec>>,
    pub(crate) config_dir: PathBuf,
    pub(crate) password: Option<String>,
    /// Phone access, hosted here.
    pub phone: Arc<crate::PhoneHost>,
    /// Where the projects' shadow repositories are kept.
    pub(crate) snapshots: PathBuf,
    /// The shadow repositories in use, by folder.
    shadows: Mutex<HashMap<String, harlo_engine::snapshot::Shadow>>,
    /// The server's own files, which no agent's tool reads or changes.
    pub(crate) sealed: Vec<PathBuf>,
    /// What the person answered about projects' own settings since the
    /// server started, by the key they're saved under.
    pub(crate) trust: Mutex<HashMap<String, bool>>,
    events: broadcast::Sender<Outgoing>,
    emitting: Mutex<()>,
    pub(crate) live: Mutex<Live>,
    /// The agents sessions run as.
    pub(crate) agents: Vec<harlo_engine::agents::Agent>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("config", &self.config_dir)
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
        sealed: Vec<PathBuf>,
    ) -> Self {
        let global = Global {
            home: config_home.to_path_buf(),
            text: text_of(&config),
            stamp: stamp(config_home),
            config,
        };
        let (events, _) = broadcast::channel(BACKLOG);
        let agents = harlo_engine::agents::builtins(&snapshots.with_file_name("plans"));
        end_unfinished(&store);
        Self {
            store,
            global: Mutex::new(global),
            catalog: Mutex::new(Arc::new(harlo_engine::catalog::Catalog::bundled())),
            attempts: Mutex::default(),
            mcp: crate::mcp::Mcp::default(),
            claude_models: Mutex::default(),
            config_dir: harlo_engine::config_folder(config_home),
            password,
            phone,
            snapshots,
            shadows: Mutex::default(),
            trust: Mutex::default(),
            sealed,
            events,
            emitting: Mutex::new(()),
            live: Mutex::default(),
            agents,
        }
    }

    /// The global configuration and its file's text, read again when the
    /// file has changed since; an unreadable edit keeps what was read.
    pub(crate) fn global(&self) -> (Config, Option<Value>) {
        let mut global = lock(&self.global);
        let now_stamp = stamp(&global.home);
        if now_stamp != global.stamp {
            global.stamp = now_stamp;
            match Config::load(&global.home) {
                Ok(config) => {
                    global.text = text_of(&config);
                    global.config = config;
                }
                Err(e) => {
                    tracing::warn!(name: "config.global.unreadable", error = %e, "keeping the config as last read");
                }
            }
        }
        (global.config.clone(), global.text.clone())
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

    /// Changes session `id` as `change` says and saves it, as one step, so
    /// nothing else changing it meanwhile is undone.
    ///
    /// # Errors
    ///
    /// Fails when there's no such session or the store fails.
    pub(crate) fn change(
        &self,
        id: &str,
        change: impl FnOnce(&mut Session),
    ) -> Result<Session, Refusal> {
        self.store
            .change_session(id, change)?
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
        self.session(id)?;
        if let Some(rules) = permissions {
            let session = self.change(id, |s| {
                s.permissions = rules;
                s.time.updated = now();
            })?;
            self.emit(
                "session.permissions",
                json!({ "sessionID": id, "permissions": session.permissions }),
                None,
                Some(id),
            );
        }
        if let Some(title) = title {
            self.change(id, |s| {
                s.title = Some(title.clone());
                s.time.updated = now();
            })?;
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

    /// Asks a model for a title from the session's first message, and uses
    /// it unless the session was renamed meanwhile.
    ///
    /// The model is the config's small one when it names one, else the
    /// session's own, without its variant: a title needs no long thought.
    async fn name_session(self: Arc<Self>, session: Session, first: String, provisional: String) {
        let folder = self.folder(&session.location.directory);
        let small = folder
            .config
            .small_model
            .clone()
            .filter(|m| folder.config.resolve(m).is_some());
        let own = || {
            let mut model = session
                .model
                .clone()
                .or_else(|| folder.config.default_model())?;
            model.variant = None;
            Some(model)
        };
        let Some(model) = small.or_else(own) else {
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
        let started = |s: &mut Session| {
            let at = now();
            s.time.busy = Some(at);
            s.time.updated = at;
        };
        while let Ok(session) = self.change(&id, started) {
            self.tell_instructions(&session).await;
            let mut delivered = lock(&self.live).inbox.remove(&id).unwrap_or_default();
            let inbox_ids: Vec<String> = delivered.iter().map(|(m, _)| m.id().to_owned()).collect();
            order_notices(&mut delivered);
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
        let hooks = Hooks::new(self, &session.id, &session.location.directory);
        let chosen = match self.choose(session, stop).await {
            Ok(chosen) => chosen,
            Err(failed) => return failed,
        };
        // The turn as a step would send it: the summary request then reads
        // the conversation from the provider's cache.
        let turn = self.turn_for(session, chosen).await;
        let done = harlo_engine::compaction::compact(pending, &turn, &hooks, stop).await;
        match (done.status.as_str(), done.error) {
            ("completed", _) => ("succeeded", None),
            _ if stop.is_cancelled() => ("interrupted", None),
            (_, error) => ("failed", error.map(|e| e.message)),
        }
    }

    /// What session `session`'s next turn runs with: its folder's
    /// configuration (after asking about the project's own settings, if
    /// they wait), its agent, model, rules and history.
    ///
    /// # Errors
    ///
    /// How the run ends, as [`Self::run_once`] returns it, when no model is
    /// set up or the session can't be read.
    async fn choose(
        self: &Arc<Self>,
        session: &Session,
        stop: &CancellationToken,
    ) -> Result<Chosen, (&'static str, Option<String>)> {
        let mut folder = self.folder(&session.location.directory);
        // What the project's own config asks for waits for the person.
        if let Some(held) = folder.withheld.clone()
            && self
                .ask_trust(&session.id, &session.location.directory, &held, stop)
                .await
        {
            folder = self.folder(&session.location.directory);
        }
        let agent = folder.agent(session.agent.as_deref());
        let Some(model) = agent
            .model
            .clone()
            .or_else(|| session.model.clone())
            .or_else(|| folder.config.default_model())
        else {
            return Err((
                "failed",
                Some("No model is set up. Add a provider in Settings → Providers.".to_owned()),
            ));
        };
        let Some((provider, spec)) = folder.config.resolve(&model) else {
            return Err((
                "failed",
                Some(format!(
                    "The model {}/{} isn't in the configuration.",
                    model.provider, model.model
                )),
            ));
        };
        let (provider, spec) = (provider.clone(), spec.clone());
        let history = self
            .store
            .messages(&session.id)
            .map_err(|e| ("failed", Some(format!("Couldn't read the session: {e}"))))?;
        let rules = vec![
            (harlo_engine::AGENT.to_owned(), agent.rules.clone()),
            ("config".to_owned(), folder.config.permissions.clone()),
            ("saved".to_owned(), self.saved_rules(&session.project_id)),
            ("session".to_owned(), session.permissions.clone()),
        ];
        Ok(Chosen {
            folder,
            agent,
            model,
            provider,
            spec,
            rules,
            history,
        })
    }

    /// The engine's turn for session `session`, from what was `chosen`.
    async fn turn_for(self: &Arc<Self>, session: &Session, chosen: Chosen) -> Turn {
        let Chosen {
            folder,
            agent,
            model,
            provider,
            spec,
            rules,
            history,
        } = chosen;
        let directory = session.location.directory.clone();
        let links = self.mcp_ready(&directory).await;
        let mut system = self
            .system_prompt(&directory, &folder, Some(&session.id))
            .await;
        if let Some(own) = &agent.system {
            system = format!("{own}\n\n{system}");
        }
        Turn {
            cwd: PathBuf::from(&directory),
            agent: agent.id.clone(),
            model,
            provider,
            system,
            rules,
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
            script_files: folder.config.script_files,
            search: self.searcher(&session.id, &session.location.directory),
            code: Some(Arc::new(crate::code::ServerCatalog {
                server: Arc::clone(self),
                session: Some(session.id.clone()),
                links,
            })),
            // With room left for the reply and a step's results, and never
            // past the size the configuration names, the next step starts
            // from a summary.
            compact_at: Some(harlo_engine::compaction::threshold(
                spec.context,
                spec.output,
                folder.config.compact_at,
            )),
            skills: self.skills_for(&session.location.directory).await,
            todo: folder.config.todo,
            sealed: self.sealed.clone(),
            steering: Some(Arc::new(crate::agents::SessionSteering {
                server: Arc::clone(self),
                session: session.id.clone(),
                directory: directory.clone(),
            })),
        }
    }

    /// One turn of the engine over the session's history; returns how it
    /// ended, as a stored word, and why it failed.
    async fn run_once(
        self: &Arc<Self>,
        session: &Session,
        stop: &CancellationToken,
    ) -> (&'static str, Option<String>) {
        let chosen = match self.choose(session, stop).await {
            Ok(chosen) => chosen,
            Err(failed) => return failed,
        };
        if chosen.provider.id == harlo_engine::claude::PROVIDER {
            let Chosen {
                agent,
                model,
                rules,
                history,
                ..
            } = chosen;
            return self
                .run_claude(session, model, &agent.id, rules, &history, stop)
                .await;
        }
        let mut chosen = chosen;
        self.clear_cold(session, &mut chosen);
        let turn = self.turn_for(session, chosen).await;
        let hooks = Hooks::new(self, &session.id, &session.location.directory);
        match harlo_engine::run(turn, &hooks, &hooks, stop).await {
            Outcome::Succeeded => ("succeeded", None),
            Outcome::Interrupted => ("interrupted", None),
            Outcome::Failed(e) => ("failed", Some(e.to_string())),
        }
    }

    /// Clears the old tool results of a session taken up again after its
    /// provider's cache let go of it, unless the configuration says not
    /// to, and stores the messages it changed.
    fn clear_cold(&self, session: &Session, chosen: &mut Chosen) {
        let after = match chosen.folder.config.prune {
            harlo_engine::Prune::Never => return,
            harlo_engine::Prune::After(rest) => rest,
            harlo_engine::Prune::Cold => {
                harlo_engine::compaction::cold_after(&chosen.provider.wire)
            }
        };
        let now = harlo_engine::now();
        let changed = harlo_engine::compaction::clear_cold(&mut chosen.history, now, after);
        for at in changed {
            if let Err(why) = self.store.put_message(&session.id, &chosen.history[at]) {
                tracing::warn!(name: "clear.unsaved", %why, "couldn't store a cleared tool result");
            }
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
            .system_prompt(&session.location.directory, &folder, Some(&session.id))
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
        let (global, _) = self.global();
        let model = model
            .or_else(|| global.default_model())
            .ok_or_else(|| Refusal::Bad("No model is set up.".to_owned()))?;
        let (provider, _) = global.resolve(&model).ok_or_else(|| {
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

    /// The notes section, the date and the instruction files for the
    /// system prompt of `session` (none: a prompt of its own) in
    /// `project`, where the files now read `found`.
    ///
    /// A session gets the ones its last turn got, while that turn is
    /// recent enough for a provider to still hold the conversation.
    fn steady_parts(
        &self,
        session: Option<&str>,
        project: &str,
        found: Vec<Instruction>,
    ) -> (String, String, Vec<Instruction>) {
        let Some(session) = session else {
            return (self.notes_prompt(project), today(), found);
        };
        let time = now();
        if let Some(kept) = lock(&self.live).steady.get_mut(session)
            && time - kept.used < WARM_MS
        {
            kept.used = time;
            return (
                kept.notes.clone(),
                kept.date.clone(),
                kept.instructions.clone(),
            );
        }
        let (notes, date) = (self.notes_prompt(project), today());
        lock(&self.live).steady.insert(
            session.to_owned(),
            Steady {
                notes: notes.clone(),
                date: date.clone(),
                instructions: found.clone(),
                told: found.clone(),
                used: time,
            },
        );
        (notes, date, found)
    }

    /// Queues, for session `session`, word of instruction files that
    /// changed since it last heard of them.
    ///
    /// A session a provider may still hold in cache keeps the system
    /// prompt it began with, so an edited file reaches it as a message
    /// beside the prompt that starts its next turn. A session with no
    /// prompt waiting, or whose prompt is read afresh anyway, gets none.
    async fn tell_instructions(&self, session: &Session) {
        let id = session.id.as_str();
        {
            let live = lock(&self.live);
            let warm = live
                .steady
                .get(id)
                .is_some_and(|kept| now() - kept.used < WARM_MS);
            let prompted = live
                .inbox
                .get(id)
                .is_some_and(|q| q.iter().any(|(m, _)| wakes(m)));
            if !warm || !prompted {
                return;
            }
        }
        let directory = session.location.directory.clone();
        let found = self
            .instructions(&directory, &self.folder(&directory))
            .await;
        let text = {
            let mut live = lock(&self.live);
            let Some(kept) = live.steady.get_mut(id) else {
                return;
            };
            let Some(text) = harlo_engine::instructions::update(&kept.told, &found) else {
                return;
            };
            kept.told = found;
            text
        };
        let message = Message::Synthetic {
            id: ascending_id("msg"),
            time: Created { created: now() },
            text: text.clone(),
            description: None,
            metadata: None,
        };
        self.emit(
            "session.inbox.enqueued",
            json!({
                "inboxID": message.id(), "sessionID": id,
                "item": { "type": "synthetic", "payload": { "text": text }, "delivery": "steer" },
            }),
            Some(&directory),
            Some(id),
        );
        // Ahead of the prompt it comes with, as the prompt's text reads
        // against it.
        lock(&self.live)
            .inbox
            .entry(id.to_owned())
            .or_default()
            .insert(0, (message, "steer".to_owned()));
    }

    /// The instruction files that apply in `directory`, as they read now.
    async fn instructions(
        &self,
        directory: &str,
        folder: &crate::folder::Folder,
    ) -> Vec<Instruction> {
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
        harlo_engine::instructions::load(&search)
    }

    /// Lets session `id`'s next turn read the notes and the date afresh:
    /// its conversation starts over (a summary), or it is gone.
    pub(crate) fn refresh_prompt(&self, id: &str) {
        lock(&self.live).steady.remove(id);
    }

    /// Lets every session's next turn read the notes afresh: a person
    /// changed them, and means it for the sessions going too.
    pub(crate) fn refresh_prompts(&self) {
        lock(&self.live).steady.clear();
    }

    /// The system prompt for `session` (none: a prompt of its own) in
    /// `directory`: the agent's prompt, the project's instructions, the
    /// notes, then the environment and the date.
    async fn system_prompt(
        &self,
        directory: &str,
        folder: &crate::folder::Folder,
        session: Option<&str>,
    ) -> String {
        let root = git::toplevel(directory).await;
        let found = self.instructions(directory, folder).await;
        let (notes, date, found) =
            self.steady_parts(session, &git::project_id(directory).await, found);
        let mut prompt = BASE_PROMPT.to_owned();
        if !found.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(&harlo_engine::instructions::render(&found));
        }
        if !notes.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(notes.trim_end());
        }
        let _ = write!(
            prompt,
            "\n\nWorking folder: {directory}\nIs a git repository: {}\nPlatform: {}\nToday's date: {}",
            if root.is_some() { "yes" } else { "no" },
            std::env::consts::OS,
            date,
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
        let _ = self.change(id, |s| {
            s.outcome = Some(word.to_owned());
            s.time.idle = Some(time);
            s.time.updated = time;
        });
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
        let added = self.change(id, |session| {
            let (t, s) = (&mut session.tokens, tokens);
            t.input += s.input;
            t.output += s.output;
            t.reasoning += s.reasoning;
            t.cache.read += s.cache.read;
            t.cache.write += s.cache.write;
            session.cost += cost;
            session.time.updated = now();
        });
        if let Ok(session) = added {
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
pub(crate) struct Hooks {
    server: Arc<Server>,
    session: String,
    directory: String,
    streaming: Mutex<Streaming>,
}

impl Hooks {
    /// Hooks for a turn of session `session` in `directory`.
    pub(crate) fn new(server: &Arc<Server>, session: &str, directory: &str) -> Self {
        Self {
            server: Arc::clone(server),
            session: session.to_owned(),
            directory: directory.to_owned(),
            streaming: Mutex::default(),
        }
    }

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
                        signature: None,
                    },
                );
            }
            Event::TextDelta { at, delta, .. } | Event::ReasoningDelta { at, delta, .. } => {
                if let Some(Part::Text { text } | Part::Reasoning { text, .. }) =
                    s.parts.get_mut(at)
                {
                    text.push_str(delta);
                }
            }
            Event::TextEnded { at, text, .. } | Event::ReasoningEnded { at, text, .. } => {
                if let Some(Part::Text { text: t } | Part::Reasoning { text: t, .. }) =
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
        // (The conversation starts over from the summary.)
        if compaction.status == "completed" {
            self.server.refresh_prompt(&self.session);
        }
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

    /// The place of the text or reasoning block at `at` among the step's
    /// blocks of its kind, as clients and the stored history count them.
    fn ordinal(&self, at: usize) -> usize {
        let s = lock(&self.streaming);
        let reasoning = matches!(s.parts.get(&at), Some(Part::Reasoning { .. }));
        s.parts
            .range(..at)
            .filter(|(_, p)| matches!(p, Part::Reasoning { .. }) == reasoning)
            .count()
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
            Event::TextStarted { message, at } => self.send("session.text.started", json!({ "assistantMessageID": message, "ordinal": self.ordinal(at) }), true),
            Event::TextDelta { message, at, delta } => {
                self.send("session.text.delta", json!({ "assistantMessageID": message, "ordinal": self.ordinal(at), "delta": delta }), false);
            }
            Event::TextEnded { message, at, text } => {
                self.send("session.text.ended", json!({ "assistantMessageID": message, "ordinal": self.ordinal(at), "text": text }), true);
            }
            Event::ReasoningStarted { message, at } => {
                self.send("session.reasoning.started", json!({ "assistantMessageID": message, "ordinal": self.ordinal(at) }), true);
            }
            Event::ReasoningDelta { message, at, delta } => {
                self.send("session.reasoning.delta", json!({ "assistantMessageID": message, "ordinal": self.ordinal(at), "delta": delta }), false);
            }
            Event::ReasoningEnded { message, at, text } => {
                self.send("session.reasoning.ended", json!({ "assistantMessageID": message, "ordinal": self.ordinal(at), "text": text }), true);
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
        let mut data = json!({
            "id": ask.id, "sessionID": self.session, "action": ask.action, "resources": ask.resources,
            "save": ask.save,
            "message": ask.why, "metadata": metadata,
        });
        // (An ask about the project's own settings comes from no tool call.)
        if !ask.call.is_empty() {
            data["source"] = json!({ "type": "tool", "messageID": ask.message, "id": ask.call });
        }
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
