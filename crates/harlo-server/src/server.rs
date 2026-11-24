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

use harlo_engine::{
    Answer, Ask, Assistant, Config, Created, ErrorInfo, Event, Gate, Message, Outcome, Part, Rule,
    Sink, StepTime, ToolState, Turn, agent_rules, ascending_id, descending_id, now,
};
use serde_json::{Value, json};
use tokio::sync::{broadcast, oneshot};
use tokio_util::sync::CancellationToken;

use crate::git;
use crate::store::{Location, Session, SessionTime, Store, StoredEvent};

/// The name of the one agent M1 has.
const AGENT: &str = "build";
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
struct Run {
    stop: CancellationToken,
    asks: Vec<Pending>,
}

/// A permission request waiting for an answer.
struct Pending {
    id: String,
    data: Value,
    answer: oneshot::Sender<Answer>,
}

/// What runs and what waits, guarded together.
#[derive(Default)]
struct Live {
    runs: HashMap<String, Run>,
    /// Prompts not yet delivered to a run, by session.
    inbox: HashMap<String, Vec<(Message, String)>>,
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
    live: Mutex<Live>,
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
}

impl From<rusqlite::Error> for Refusal {
    fn from(e: rusqlite::Error) -> Self {
        Self::Store(e.to_string())
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
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
            },
            cost: 0.0,
            tokens: harlo_engine::Tokens::default(),
            outcome: None,
            revert: None,
            permissions: Vec::new(),
            model: None,
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
    ) -> Result<Value, Refusal> {
        let session = self.session(id)?;
        let message_id = message_id.unwrap_or_else(|| ascending_id("msg"));
        let created = now();
        let answer = json!({ "data": {
            "id": message_id, "sessionID": id, "time": { "created": created }, "type": "user",
            "payload": { "text": text }, "delivery": delivery,
        }});
        let queued = lock(&self.live)
            .inbox
            .get(id)
            .is_some_and(|q| q.iter().any(|(m, _)| m.id() == message_id));
        if queued || self.store.has_message(&message_id)? {
            return Ok(answer);
        }
        if session.title.is_none() {
            self.update_session(id, None, Some(title_of(text)))?;
        }
        // A new prompt keeps a staged undo: the undone messages go.
        if session.revert.is_some() {
            self.commit_revert(id)?;
        }
        let message = Message::User {
            id: message_id.clone(),
            time: Created { created },
            text: text.to_owned(),
        };
        self.emit(
            "session.inbox.enqueued",
            json!({ "inboxID": message_id, "sessionID": id, "item": { "type": "user", "payload": { "text": text }, "delivery": delivery } }),
            Some(&session.location.directory),
            Some(id),
        );
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
                    },
                );
                Some(stop)
            }
        };
        if let Some(stop) = start {
            tokio::spawn(Arc::clone(self).drive(id.to_owned(), stop));
        }
        Ok(answer)
    }

    /// Prompts waiting for session `id`'s run.
    pub(crate) fn inbox(&self, id: &str) -> Vec<Value> {
        lock(&self.live)
            .inbox
            .get(id)
            .map(|q| {
                q.iter()
                    .map(|(m, delivery)| match m {
                        Message::User { id, text, time } => json!({
                            "id": id, "type": "user", "payload": { "text": text }, "delivery": delivery, "time": time,
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
        // (The run may have been stopped meanwhile.)
        let _ = pending.answer.send(answer);
        Ok(())
    }

    /// Runs session `id` until its inbox is empty or it's stopped.
    async fn drive(self: Arc<Self>, id: String, stop: CancellationToken) {
        while let Ok(mut session) = self.session(&id) {
            let delivered = lock(&self.live).inbox.remove(&id).unwrap_or_default();
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
            for (message, _) in &delivered {
                if let Err(e) = self.store.put_message(&id, message) {
                    tracing::error!(name: "message.store.failed", error = %e, "couldn't store a prompt");
                }
                self.emit(
                    "session.inbox.delivered",
                    json!({ "sessionID": id, "inboxID": message.id() }),
                    Some(&directory),
                    Some(&id),
                );
            }
            let (word, error) = self.run_once(&session, &stop).await;
            self.finish(&id, word, error);

            let mut live = lock(&self.live);
            let more = live.inbox.get(&id).is_some_and(|q| !q.is_empty());
            if !more || stop.is_cancelled() {
                live.runs.remove(&id);
                break;
            }
        }
    }

    /// One turn of the engine over the session's history; returns how it
    /// ended, in OpenCode's word, and why it failed.
    async fn run_once(
        self: &Arc<Self>,
        session: &Session,
        stop: &CancellationToken,
    ) -> (&'static str, Option<String>) {
        let Some(model) = session
            .model
            .clone()
            .or_else(|| self.config.default_model())
        else {
            return (
                "failed",
                Some(
                    "No model is set up. Add a provider and model to OpenCode's config.".to_owned(),
                ),
            );
        };
        let Some((provider, _)) = self.config.resolve(&model) else {
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
        let turn = Turn {
            cwd: PathBuf::from(&directory),
            agent: AGENT.to_owned(),
            model,
            provider: provider.clone(),
            system: self.system_prompt(&directory).await,
            rules: vec![
                ("agent".to_owned(), agent_rules()),
                ("config".to_owned(), self.config.permissions.clone()),
                ("session".to_owned(), session.permissions.clone()),
            ],
            history,
            shadow: Some(self.shadow(&directory)),
            memory: self.memory_for(session),
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

    /// The system prompt for a session in `directory`, as OpenCode orders
    /// it: the agent's prompt, the project's instructions, then the date
    /// and environment.
    async fn system_prompt(&self, directory: &str) -> String {
        let root = git::toplevel(directory).await;
        let home = dirs::home_dir().unwrap_or_default();
        let search = harlo_engine::instructions::Search {
            workspace: std::path::Path::new(directory),
            project_root: root.as_deref().map(std::path::Path::new),
            home: &home,
            config_dir: &self.config_dir,
            configured: &self.config.instructions,
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
        if !count {
            return;
        }
        if let Ok(mut session) = self.session(id) {
            let (t, s) = (&mut session.tokens, &step.tokens);
            t.input += s.input;
            t.output += s.output;
            t.reasoning += s.reasoning;
            t.cache.read += s.cache.read;
            t.cache.write += s.cache.write;
            session.cost += step.cost;
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
