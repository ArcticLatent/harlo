//! `ChatSession`: QML list model of one live session's transcript rows.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel = crate::qt_base::qobject::QAbstractListModel;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(QString, key)]
        #[qproperty(QString, status)]
        #[qproperty(QString, harness)]
        /// "Ask first" toggle of the bound session (omp): prompts ask the
        /// agent to clarify with its `ask` tool before starting.
        #[qproperty(bool, ask_first, cxx_name = "askFirst")]
        /// Plan mode (set from the workspace's per-session state): messages
        /// ask for a plan, not changes.
        #[qproperty(bool, plan_mode, cxx_name = "planMode")]
        #[qproperty(QString, model_name, cxx_name = "modelName")]
        /// The model as `provider/id` (for its provider logo).
        #[qproperty(QString, model_id, cxx_name = "modelId")]
        #[qproperty(QString, thinking)]
        #[qproperty(QString, title)]
        #[qproperty(QString, cwd)]
        #[qproperty(QStringList, queued)]
        #[qproperty(QString, status_line, cxx_name = "statusLine")]
        /// "42% · 115k/272k · $0.31"
        #[qproperty(QString, context_text, cxx_name = "contextText")]
        #[qproperty(f64, context_fraction, cxx_name = "contextFraction")]
        /// `name\tdescription\thint` per command.
        #[qproperty(QStringList, commands)]
        /// `provider\tid\tname\treasoning(0|1)` per model.
        #[qproperty(QStringList, models)]
        #[qproperty(QStringList, thinking_levels, cxx_name = "thinkingLevels")]
        #[qproperty(QStringList, file_matches, cxx_name = "fileMatches")]
        /// Image paths to send with the next prompt.
        #[qproperty(QStringList, attachments)]
        /// Context report as JSON (see `harlo_core::context::report`).
        #[qproperty(QString, context_json, cxx_name = "contextJson")]
        #[qproperty(bool, has_request, cxx_name = "hasRequest")]
        #[qproperty(QString, request_id, cxx_name = "requestId")]
        #[qproperty(QString, request_method, cxx_name = "requestMethod")]
        #[qproperty(QString, request_title, cxx_name = "requestTitle")]
        #[qproperty(QString, request_message, cxx_name = "requestMessage")]
        #[qproperty(QStringList, request_options, cxx_name = "requestOptions")]
        #[qproperty(QString, request_prefill, cxx_name = "requestPrefill")]
        #[qproperty(bool, request_is_approval, cxx_name = "requestIsApproval")]
        /// The tool row an omp approval is for (its row key), when found.
        #[qproperty(QString, approval_row, cxx_name = "approvalRow")]
        /// The whole request as JSON (omp `ask` questions, `select` option details).
        #[qproperty(QString, request_json, cxx_name = "requestJson")]
        /// omp subagents of this session: the `get_subagents` list as JSON.
        #[qproperty(QString, subagents_json, cxx_name = "subagentsJson")]
        /// Result of the last subagent action (cancel, steer), for a toast.
        #[qproperty(QString, subagent_message, cxx_name = "subagentMessage")]
        /// omp todo phases as JSON (`[{ name, tasks: [{ content, status }] }]`).
        #[qproperty(QString, todos_json, cxx_name = "todosJson")]
        /// omp goal: `{ objective, status, tokenBudget?, tokensUsed, timeUsedSeconds, … }`, or "".
        #[qproperty(QString, goal_json, cxx_name = "goalJson")]
        /// Why the last goal command failed (e.g. plan mode), or "".
        #[qproperty(QString, goal_error, cxx_name = "goalError")]
        /// omp side questions of this session (records, newest last) as JSON.
        #[qproperty(QString, btw_json, cxx_name = "btwJson")]
        /// Undo a turn: your messages with a snapshot from before them, as
        /// JSON `{ <row key>: { snapshot, next: <your next message's row
        /// key, "" for the latest turn>, files: <what the latest turn
        /// changed, -1 for the others> } }` (`loadUndo` fills it).
        #[qproperty(QString, undo_json, cxx_name = "undoJson")]
        /// The undo being looked at or done: `{ row, state: "loading" |
        /// "ready" | "undoing" | "done" | "failed", restore: [paths], remove:
        /// [paths], later, headMoved, message }`; "" for none.
        #[qproperty(QString, undo_plan, cxx_name = "undoPlanJson")]
        type ChatSession = super::ChatSessionRust;
    }

    impl cxx_qt::Threading for ChatSession {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// Attaches this view to a live session from the `Workspace`
        /// (empty detaches).
        #[qinvokable]
        fn bind(self: Pin<&mut ChatSession>, key: &QString);
        /// `mode`: normal | steer | followUp.
        #[qinvokable]
        fn send(self: Pin<&mut ChatSession>, text: &QString, mode: &QString);
        /// Finds which of your messages a turn can be undone to (`undoJson`).
        #[qinvokable]
        fn load_undo(self: Pin<&mut ChatSession>);
        /// What undoing from this message would do (`undoPlanJson`).
        #[qinvokable]
        fn preview_undo(self: Pin<&mut ChatSession>, row_key: &QString);
        /// Puts the folder's files back as they were before this message
        /// (`undoPlanJson` says how it went).
        #[qinvokable]
        fn undo_turn(self: Pin<&mut ChatSession>, row_key: &QString);
        /// Turns "Ask first" on or off for the bound session.
        #[qinvokable]
        fn set_ask_first_for_session(self: Pin<&mut ChatSession>, on: bool);
        #[qinvokable]
        fn abort(self: Pin<&mut ChatSession>);
        /// Answers the current dialog with a value (select/input/editor).
        #[qinvokable]
        fn answer_value(self: Pin<&mut ChatSession>, value: &QString);
        #[qinvokable]
        fn answer_confirm(self: Pin<&mut ChatSession>, confirmed: bool);
        #[qinvokable]
        fn cancel_request(self: Pin<&mut ChatSession>);
        /// Answers an omp `ask` request: `answers` is a JSON array of
        /// `{ id, selectedOptions, customInput? }`, one per question.
        #[qinvokable]
        fn answer_ask(self: Pin<&mut ChatSession>, answers: &QString);
        /// Re-reads the omp subagent list.
        #[qinvokable]
        fn refresh_subagents(self: Pin<&mut ChatSession>);
        #[qinvokable]
        fn cancel_subagent(self: Pin<&mut ChatSession>, id: &QString);
        /// Sends `message` to a running subagent as its user.
        #[qinvokable]
        fn steer_subagent(self: Pin<&mut ChatSession>, id: &QString, message: &QString);
        /// Branching from one of your messages (`row`: its transcript row):
        /// omp starts a branch in this session, pi a forked session; the
        /// message's text comes back in `editRequested` to edit and resend.
        #[qinvokable]
        fn edit_and_resend(self: Pin<&mut ChatSession>, row: i32);
        /// omp: copies the session up to this message into a new session.
        #[qinvokable]
        fn fork_at(self: Pin<&mut ChatSession>, row: i32);
        /// Index of the first row whose key starts with `prefix`, or -1.
        #[qinvokable]
        fn index_of_key_prefix(self: &ChatSession, prefix: &QString) -> i32;
        /// Row index of the row with this key, or -1.
        #[qinvokable]
        fn index_of_key(self: &ChatSession, key: &QString) -> i32;
        /// Every row as plain text, one per line (for tests).
        #[qinvokable]
        fn transcript_text(self: &ChatSession) -> QString;
        /// Your messages, for the prompt rail: JSON `[{i: row, text: preview}]`.
        #[qinvokable]
        fn user_prompts(self: &ChatSession) -> QString;
        /// omp: drops a queued message (`queued` index).
        #[qinvokable]
        fn remove_queued(self: Pin<&mut ChatSession>, index: i32);
        /// omp: sends a queued follow-up now, as steering.
        #[qinvokable]
        fn promote_queued(self: Pin<&mut ChatSession>, index: i32);
        /// omp goal mode: `op` is create (with `objective`, and a token
        /// budget when `budget` > 0), pause, resume or drop.
        #[qinvokable]
        fn goal_command(self: Pin<&mut ChatSession>, op: &QString, objective: &QString, budget: i32);
        /// omp side question (`btw`); `record_id` continues a topic.
        #[qinvokable]
        fn ask_btw(self: Pin<&mut ChatSession>, question: &QString, record_id: &QString);
        #[qinvokable]
        fn cancel_btw(self: Pin<&mut ChatSession>);
        /// Shows a session file read-only (a subagent's transcript), without
        /// starting a harness.
        #[qinvokable]
        fn bind_file(self: Pin<&mut ChatSession>, path: &QString, harness: &QString, cwd: &QString);
        /// Demo harness only: emit `count` finished exchanges at once.
        #[qinvokable]
        fn load_demo_history(self: Pin<&mut ChatSession>, count: i32);
        /// Fills `models` (starts the harness if needed).
        #[qinvokable]
        fn load_models(self: Pin<&mut ChatSession>);
        #[qinvokable]
        fn set_model(self: Pin<&mut ChatSession>, provider: &QString, id: &QString);
        /// Fills `thinkingLevels`.
        #[qinvokable]
        fn load_thinking_levels(self: Pin<&mut ChatSession>);
        #[qinvokable]
        fn set_thinking_level(self: Pin<&mut ChatSession>, level: &QString);
        /// Fills `fileMatches` with files under the session's folder.
        #[qinvokable]
        fn search_files(self: Pin<&mut ChatSession>, query: &QString);
        /// Adds a file (path or `file://` URL) to the next prompt. Returns
        /// false for folders and missing files.
        #[qinvokable]
        fn add_attachment(self: Pin<&mut ChatSession>, path: &QString) -> bool;
        #[qinvokable]
        fn remove_attachment(self: Pin<&mut ChatSession>, index: i32);
        /// Refreshes `contextJson`.
        #[qinvokable]
        fn load_context(self: Pin<&mut ChatSession>);
    }

    unsafe extern "RustQt" {
        /// Text to put back in the composer (after a branch or fork).
        #[qsignal]
        #[cxx_name = "editRequested"]
        fn edit_requested(self: Pin<&mut ChatSession>, text: &QString);
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut ChatSession>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QVector_i32);
    }

    extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(self: Pin<&mut ChatSession>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut ChatSession>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(self: Pin<&mut ChatSession>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut ChatSession>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut ChatSession>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut ChatSession>);
    }

    unsafe extern "RustQt" {
        #[inherit]
        fn index(self: &ChatSession, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &ChatSession, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &ChatSession) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &ChatSession, parent: &QModelIndex) -> i32;
    }
}

use std::collections::VecDeque;

/// The entry id of a session's `ordinal`-th user message (counting from 0),
/// from its file's current branch: what `branch` and `fork` take. Blocking:
/// a remote host's is read over SSH.
pub fn user_entry_id(file: Option<PathBuf>, harness: &str, ordinal: usize) -> Option<String> {
    file.and_then(|f| match harness {
        // (A remote host's transcript, read over SSH.)
        "claude" if harlo_core::remote::is_remote(&f) => harlo_core::remote::read(&f).ok().map(|b| harlo_harness::claude::user_entries_text(&String::from_utf8_lossy(&b))),
        "claude" => Some(harlo_harness::claude::user_entries(&f)),
        // (A remote host's, through its server.)
        "opencode" if harlo_core::remote::is_remote(&f) => harlo_core::remote::host_of(&f)
            .and_then(|h| harlo_core::remote::host(&h))
            .map(|h| harlo_harness::opencode_remote_user_entries(&harlo_core::remote::launch(&h), &f)),
        "opencode" => Some(harlo_harness::opencode::user_entries(&f)),
        // (pi / omp: a remote host's file, read over SSH.)
        _ if harlo_core::remote::is_remote(&f) => harlo_core::remote::read(&f).ok().map(|b| harlo_harness::branches::current_user_entries_text(&String::from_utf8_lossy(&b))),
        _ => harlo_harness::branches::current_user_entries(&f).ok(),
    })
    .and_then(|ids| ids.get(ordinal).cloned())
}
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QStringList, QVariant, QVector};
use harlo_core::{Row, RowChange, RowKind, SessionHandle, SessionSink, SessionUpdate, UiRequest};
use harlo_harness::{HarnessConfig, HarnessKind, SendMode, SessionTarget};
use serde_json::{Value, json};

use crate::registry::{self, LiveRef};
use qobject::ChatSession;

// Model roles start at Qt::UserRole.
const ROLE_KEY: i32 = 0x100;
const ROLE_KIND: i32 = 0x101;
const ROLE_HTML: i32 = 0x102;
const ROLE_TEXT: i32 = 0x103;
const ROLE_TITLE: i32 = 0x104;
const ROLE_SUBTITLE: i32 = 0x105;
const ROLE_STATE: i32 = 0x106;
const ROLE_CALL_ID: i32 = 0x107;
const ROLE_GROUP: i32 = 0x108;
const ROLE_SUMMARY: i32 = 0x109;
const ROLE_GROUP_OPEN: i32 = 0x10a;

#[derive(Default)]
pub struct ChatSessionRust {
    key: QString,
    status: QString,
    harness: QString,
    ask_first: bool,
    plan_mode: bool,
    /// Sessions with "Ask first" on (by key), kept while the app runs.
    ask_first_keys: std::collections::HashSet<String>,
    model_name: QString,
    model_id: QString,
    thinking: QString,
    title: QString,
    cwd: QString,
    queued: QStringList,
    status_line: QString,
    context_text: QString,
    context_fraction: f64,
    commands: QStringList,
    models: QStringList,
    thinking_levels: QStringList,
    file_matches: QStringList,
    attachments: QStringList,
    context_json: QString,
    has_request: bool,
    request_id: QString,
    request_method: QString,
    request_title: QString,
    request_message: QString,
    request_options: QStringList,
    request_prefill: QString,
    request_is_approval: bool,
    approval_row: QString,
    request_json: QString,
    subagents_json: QString,
    /// Subagents as their events describe them, and as `get_subagents` lists them.
    subagent_events: Vec<Value>,
    subagents_listed: Vec<Value>,
    subagent_message: QString,
    /// A subagent list refresh is already scheduled.
    subagents_due: bool,
    todos_json: QString,
    goal_json: QString,
    goal_error: QString,
    btw_json: QString,
    undo_json: QString,
    undo_plan: QString,
    /// Side-question records (omp `btw`), oldest first.
    btw: Vec<Value>,
    /// The session's file, once the harness has reported it.
    session_file: Option<PathBuf>,
    /// The queue as (queue name, exact text), in `queued` order.
    queue_raw: Vec<(&'static str, String)>,

    rows: Vec<Arc<Row>>,
    requests: VecDeque<UiRequest>,
    status_entries: Vec<(String, String)>,
    live: Option<(LiveRef, u64)>,
    /// Bumped on every bind; updates for an earlier binding are dropped.
    generation: u64,
}

struct QtSink {
    thread: CxxQtThread<ChatSession>,
    generation: u64,
}

impl SessionSink for QtSink {
    fn deliver(&self, updates: Vec<SessionUpdate>) {
        let generation = self.generation;
        let _ = self.thread.queue(move |this| {
            if this.generation == generation {
                this.apply_updates(updates);
            }
        });
    }
}

impl Drop for ChatSessionRust {
    fn drop(&mut self) {
        if let Some((live, sub)) = self.live.take() {
            live.handle.unsubscribe(sub);
        }
    }
}

impl ChatSession {
    pub fn bind(mut self: Pin<&mut Self>, key: &QString) {
        if &self.key == key && (self.live.is_some() || key.is_empty()) {
            return;
        }
        if let Some((live, sub)) = self.as_mut().rust_mut().live.take() {
            live.handle.unsubscribe(sub);
        }
        self.as_mut().rust_mut().generation += 1;
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().rows.clear();
            self.as_mut().end_reset_model();
        }
        self.as_mut().rust_mut().requests.clear();
        self.as_mut().rust_mut().status_entries.clear();
        self.as_mut().show_front_request();
        let ask_first = self.ask_first_keys.contains(&key.to_string());
        self.as_mut().set_ask_first(ask_first);
        for setter in [
            Self::set_model_name,
            Self::set_model_id,
            Self::set_thinking,
            Self::set_title,
            Self::set_status_line,
            Self::set_status,
            Self::set_harness,
            Self::set_context_text,
            Self::set_cwd,
            Self::set_undo_json,
            Self::set_undo_plan,
        ] {
            setter(self.as_mut(), QString::default());
        }
        for setter in [
            Self::set_queued,
            Self::set_commands,
            Self::set_models,
            Self::set_thinking_levels,
            Self::set_file_matches,
            Self::set_attachments,
        ] {
            setter(self.as_mut(), QStringList::default());
        }
        self.as_mut().set_context_fraction(0.0);
        self.as_mut().set_context_json(QString::default());
        self.as_mut().set_key(key.clone());

        self.as_mut().set_subagents_json(QString::default());
        self.as_mut().rust_mut().subagent_events.clear();
        self.as_mut().rust_mut().subagents_listed.clear();
        self.as_mut().set_subagent_message(QString::default());
        self.as_mut().set_todos_json(QString::default());
        self.as_mut().set_goal_json(QString::default());
        self.as_mut().set_goal_error(QString::default());
        self.as_mut().set_btw_json(QString::default());
        self.as_mut().rust_mut().btw.clear();
        self.as_mut().rust_mut().queue_raw.clear();
        self.as_mut().rust_mut().session_file = None;

        let Some(live) = registry::get(&key.to_string()) else { return };
        self.attach(live);
    }

    fn attach(mut self: Pin<&mut Self>, live: LiveRef) {
        self.as_mut().set_harness(QString::from(&live.harness));
        self.as_mut().set_cwd(QString::from(&live.cwd.to_string_lossy().into_owned()));
        let sub = live.handle.subscribe(QtSink { thread: self.qt_thread(), generation: self.generation });
        self.as_mut().rust_mut().live = Some((live, sub));
    }

    pub fn bind_file(mut self: Pin<&mut Self>, path: &QString, harness: &QString, cwd: &QString) {
        let key = QString::from(&format!("file-view:{path}"));
        self.as_mut().bind(&QString::default());
        self.as_mut().set_key(key);
        let Some(kind) = HarnessKind::parse(&harness.to_string()) else { return };
        // (A remote host's: read through its connection.)
        let remote = harlo_core::remote::host_of(std::path::Path::new(&path.to_string()))
            .and_then(|h| harlo_core::remote::host(&h))
            .map(|h| harlo_core::remote::launch(&h));
        let cfg = HarnessConfig {
            kind,
            program: None,
            cwd: PathBuf::from(cwd.to_string()),
            session: SessionTarget::Resume(PathBuf::from(path.to_string())),
            extra_args: Vec::new(),
            env: Vec::new(),
            remote,
        };
        let handle = {
            let _guard = crate::runtime().enter();
            Arc::new(SessionHandle::open_saved(cfg))
        };
        self.attach(LiveRef { handle, harness: harness.to_string(), cwd: PathBuf::from(cwd.to_string()) });
    }

    /// How many user rows come before `row` (its place among your messages).
    fn user_ordinal(&self, row: i32) -> Option<usize> {
        let row = usize::try_from(row).ok()?;
        (self.rows.get(row)?.kind == harlo_core::RowKind::User).then(|| self.rows[..row].iter().filter(|r| r.kind == harlo_core::RowKind::User).count())
    }

    /// Resolves the entry id of the user message at `row` (from the session
    /// file's current branch), then runs `then` with it.
    fn with_user_entry(self: Pin<&mut Self>, row: i32, then: impl FnOnce(Pin<&mut Self>, String) + Send + 'static) {
        let Some(ordinal) = self.user_ordinal(row) else { return };
        let file = self.session_file.clone().or_else(|| Some(PathBuf::from(self.key.to_string())).filter(|p| p.is_file()));
        let harness = self.harness.to_string();
        // (Off the UI thread: a remote session's are read over SSH.)
        let (thread, generation) = (self.qt_thread(), self.generation);
        crate::runtime().spawn_blocking(move || {
            let entry = user_entry_id(file, &harness, ordinal);
            let _ = thread.queue(move |mut this| {
                if this.generation != generation {
                    return;
                }
                match entry {
                    Some(id) => then(this, id),
                    None => this.as_mut().set_subagent_message(QString::from("That message can't be branched from yet.")),
                }
            });
        });
    }

    pub fn edit_and_resend(self: Pin<&mut Self>, row: i32) {
        let omp = self.harness.to_string() == "omp";
        self.with_user_entry(row, move |this, entry| {
            let command = if omp { json!({ "type": "branch", "entryId": entry }) } else { json!({ "type": "fork", "entryId": entry }) };
            this.request_then(command, |mut this, data| {
                if data["cancelled"].as_bool() == Some(true) {
                    return;
                }
                if let Some(h) = this.handle() {
                    h.reload();
                }
                let text = data["text"].as_str().unwrap_or_default().to_owned();
                this.as_mut().edit_requested(&QString::from(&text));
            });
        });
    }

    pub fn fork_at(self: Pin<&mut Self>, row: i32) {
        if self.harness.to_string() != "omp" {
            return;
        }
        self.with_user_entry(row, |this, entry| {
            this.request_then(json!({ "type": "fork", "entryId": entry }), |this, data| {
                if data["cancelled"].as_bool() != Some(true)
                    && let Some(h) = this.handle() {
                        h.reload();
                    }
            });
        });
    }

    pub fn index_of_key_prefix(&self, prefix: &QString) -> i32 {
        let prefix = prefix.to_string();
        self.rows.iter().position(|r| r.key.starts_with(&prefix)).map_or(-1, |i| i as i32)
    }

    pub fn index_of_key(&self, key: &QString) -> i32 {
        let key = key.to_string();
        self.rows.iter().position(|r| r.key == key).map_or(-1, |i| i as i32)
    }

    pub fn transcript_text(&self) -> QString {
        let lines: Vec<String> = self
            .rows
            .iter()
            .map(|r| {
                let body = if r.html.is_empty() { r.text.clone() } else { harlo_core::markdown::plain_text(&r.html) };
                [r.title.as_str(), r.subtitle.as_str(), body.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · ")
            })
            .collect();
        QString::from(&lines.join("\n"))
    }

    pub fn user_prompts(&self) -> QString {
        let prompts: Vec<Value> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r.kind, RowKind::User))
            .map(|(i, r)| {
                // Attachments-only messages show their file names.
                let source = if r.text.trim().is_empty() { r.title.replace('\n', ", ") } else { r.text.clone() };
                let preview: String = source.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect();
                json!({ "i": i, "text": preview })
            })
            .collect();
        QString::from(&Value::Array(prompts).to_string())
    }

    pub fn remove_queued(self: Pin<&mut Self>, index: i32) {
        let Some((queue, text)) = self.queue_raw.get(index.max(0) as usize).cloned() else { return };
        self.request_then(json!({ "type": "remove_queued_message", "message": text, "queue": queue }), |_, _| {});
    }

    pub fn promote_queued(self: Pin<&mut Self>, index: i32) {
        let Some((queue, text)) = self.queue_raw.get(index.max(0) as usize).cloned() else { return };
        if queue != "followUp" {
            return;
        }
        self.request_then(json!({ "type": "promote_queued_message", "message": text }), |_, _| {});
    }

    pub fn goal_command(mut self: Pin<&mut Self>, op: &QString, objective: &QString, budget: i32) {
        let Some(handle) = self.handle().cloned() else { return };
        let mut command = json!({ "type": "goal", "op": op.to_string() });
        if op.to_string() == "create" {
            command["objective"] = json!(objective.to_string().trim());
            if budget > 0 {
                command["token_budget"] = json!(budget);
            }
        }
        self.as_mut().set_goal_error(QString::default());
        let thread = self.qt_thread();
        let generation = self.generation;
        crate::runtime().spawn(async move {
            let result = handle.request(command).await;
            let _ = thread.queue(move |mut this| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(data) => this.as_mut().apply_goal(&data["goal"]),
                    Err(e) => this.as_mut().set_goal_error(QString::from(&e)),
                }
            });
        });
    }

    /// A dropped goal is gone; anything else (including complete) shows.
    fn apply_goal(self: Pin<&mut Self>, goal: &Value) {
        let json = if goal.is_object() && goal["status"] != "dropped" { goal.to_string() } else { String::new() };
        self.set_goal_json(QString::from(&json));
    }

    pub fn ask_btw(self: Pin<&mut Self>, question: &QString, record_id: &QString) {
        let question = question.to_string();
        if question.trim().is_empty() {
            return;
        }
        let mut cmd = json!({ "type": "btw", "question": question.trim() });
        if !record_id.is_empty() {
            cmd["recordId"] = json!(record_id.to_string());
        }
        self.request_then(cmd, |mut this, data| {
            if let Some(record) = data.get("record") {
                this.as_mut().apply_btw(json!({ "type": "btw_record", "record": record }));
            }
        });
    }

    pub fn cancel_btw(self: Pin<&mut Self>) {
        self.request_then(json!({ "type": "btw_cancel" }), |_, _| {});
    }

    /// Folds a `btw_record` / `btw_delta` frame into the records.
    fn apply_btw(mut self: Pin<&mut Self>, frame: Value) {
        let mut records = std::mem::take(&mut self.as_mut().rust_mut().btw);
        match frame["type"].as_str() {
            Some("btw_record") => {
                let record = frame["record"].clone();
                match records.iter_mut().find(|r| r["id"] == record["id"]) {
                    Some(r) => *r = record,
                    None => records.push(record),
                }
            }
            Some("btw_delta") => {
                let delta = frame["delta"].as_str().unwrap_or_default();
                if let Some(r) = records.iter_mut().find(|r| r["id"] == frame["recordId"]) {
                    // The latest turn: the last follow-up, else the record itself.
                    let turn = match r.get_mut("followUps").and_then(|f| f.as_array_mut()).filter(|f| !f.is_empty()) {
                        Some(f) => f.last_mut().unwrap(),
                        None => r,
                    };
                    let answer = format!("{}{delta}", turn["answer"].as_str().unwrap_or_default());
                    turn["answer"] = json!(answer);
                }
            }
            _ => {}
        }
        let json = Value::Array(records.clone()).to_string();
        self.as_mut().rust_mut().btw = records;
        self.set_btw_json(QString::from(&json));
    }

    pub fn refresh_subagents(self: Pin<&mut Self>) {
        self.request_then(json!({ "type": "get_subagents" }), |mut this, data| {
            let listed = data.get("subagents").and_then(Value::as_array).cloned().unwrap_or_default();
            this.as_mut().rust_mut().subagents_listed = listed;
            this.show_subagents();
        });
    }

    /// Subagents the session's file records (a reopened session has no
    /// live events for them), read off the UI thread. Live events update
    /// them by id afterwards.
    fn load_saved_subagents(self: Pin<&mut Self>, file: PathBuf) {
        let thread = self.qt_thread();
        let generation = self.generation;
        let harness = self.harness.to_string();
        crate::runtime().spawn_blocking(move || {
            // (A remote session's: from its host.)
            let remote = harlo_core::remote::is_remote(&file);
            let saved = match harness.as_str() {
                "claude" if remote => harlo_core::subagents::claude_on_host(&file),
                "claude" => harlo_harness::claude::subagents(&file),
                "opencode" if remote => harlo_core::remote::host_of(&file)
                    .and_then(|h| harlo_core::remote::host(&h))
                    .map(|h| harlo_harness::opencode_remote_subagents(&harlo_core::remote::launch(&h), &file))
                    .unwrap_or_default(),
                "opencode" => harlo_harness::opencode::subagents(&file),
                _ => harlo_core::subagents::from_session_file(&file),
            };
            if saved.is_empty() {
                return;
            }
            let _ = thread.queue(move |mut this| {
                if this.generation != generation {
                    return;
                }
                let events = std::mem::take(&mut this.as_mut().rust_mut().subagent_events);
                let mut list: Vec<Value> = saved.into_iter().map(|s| events.iter().find(|e| e["id"] == s["id"]).cloned().unwrap_or(s)).collect();
                let extra: Vec<Value> = events.into_iter().filter(|e| !list.iter().any(|l| l["id"] == e["id"])).collect();
                list.extend(extra);
                this.as_mut().rust_mut().subagent_events = list;
                this.show_subagents();
            });
        });
    }

    /// `get_subagents` misses detached (background) subagents; the events
    /// cover them, so the view shows both.
    fn show_subagents(self: Pin<&mut Self>) {
        let mut all = harlo_core::subagents::merged(&self.subagent_events, &self.subagents_listed);
        // A remote omp reports its own paths: keep transcripts on its host.
        if let Some(host) = harlo_core::remote::host_of(std::path::Path::new(&self.cwd.to_string())) {
            for a in &mut all {
                if let Some(f) = a["sessionFile"].as_str().filter(|f| f.starts_with('/')) {
                    a["sessionFile"] = json!(harlo_core::remote::join(&host, f).to_string_lossy());
                }
            }
        }
        self.set_subagents_json(QString::from(&Value::Array(all).to_string()));
    }

    /// Coalesces bursts of subagent frames into one refresh.
    fn schedule_subagents(mut self: Pin<&mut Self>) {
        if self.subagents_due {
            return;
        }
        self.as_mut().rust_mut().subagents_due = true;
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let _ = thread.queue(|mut this| {
                this.as_mut().rust_mut().subagents_due = false;
                this.refresh_subagents();
            });
        });
    }

    pub fn cancel_subagent(self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        self.request_then(json!({ "type": "cancel_subagent", "subagentId": id }), move |mut this, data| {
            let done = data.get("cancelled").and_then(|v| v.as_bool()).unwrap_or(false);
            this.as_mut().set_subagent_message(QString::from(&if done { format!("Cancelled {id}") } else { format!("{id} was not running") }));
            this.refresh_subagents();
        });
    }

    pub fn steer_subagent(self: Pin<&mut Self>, id: &QString, message: &QString) {
        let (id, message) = (id.to_string(), message.to_string());
        if message.trim().is_empty() {
            return;
        }
        let Some(handle) = self.handle().cloned() else { return };
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            let result = handle.request(json!({ "type": "steer_subagent", "subagentId": id, "message": message })).await;
            let text = match result {
                Ok(_) => format!("Sent to {id}"),
                Err(e) => format!("Could not reach {id}: {e}"),
            };
            let _ = thread.queue(move |this| this.set_subagent_message(QString::from(&text)));
        });
    }

    fn handle(&self) -> Option<&Arc<SessionHandle>> {
        self.live.as_ref().map(|(l, _)| &l.handle)
    }

    pub fn send(mut self: Pin<&mut Self>, text: &QString, mode: &QString) {
        let text = text.to_string();
        if text.trim().is_empty() && self.attachments.is_empty() {
            return;
        }
        let mode = match mode.to_string().as_str() {
            "steer" => SendMode::Steer,
            "followUp" => SendMode::FollowUp,
            _ => SendMode::Normal,
        };
        let Some(h) = self.handle().cloned() else { return };
        // Plan mode: a plan first, no changes (omp also denies edits).
        let text = if self.plan_mode && mode == SendMode::Normal {
            format!("{text}{}", harlo_harness::file_blocks::PLAN_NOTE)
        } else {
            text
        };
        // "Ask first" (omp): new prompts ask for clarification before work.
        let text = if self.ask_first && mode == SendMode::Normal && matches!(self.harness.to_string().as_str(), "omp" | "demo") {
            format!("{text}{}", harlo_harness::file_blocks::ASK_FIRST_NOTE)
        } else {
            text
        };
        let paths: Vec<PathBuf> = self.attachments.iter().map(|p| PathBuf::from(p.to_string())).collect();
        if paths.is_empty() {
            h.prompt(&text, Vec::new(), mode);
            return;
        }
        self.as_mut().set_attachments(QStringList::default());
        // Reading files and encoding images can take a moment; keep it off
        // the UI thread.
        crate::runtime().spawn_blocking(move || {
            let prepared = harlo_core::attach::prepare(&paths);
            h.prompt(&format!("{}{text}", prepared.prefix), prepared.images, mode);
        });
    }

    /// The session folder (`ssh://` on a host).
    fn folder(&self) -> PathBuf {
        PathBuf::from(self.cwd.to_string())
    }

    pub fn load_undo(mut self: Pin<&mut Self>) {
        let dir = self.folder();
        if dir.as_os_str().is_empty() {
            return;
        }
        // Undo turned off (Settings → Git): none offered, old snapshots neither.
        if !harlo_core::session::turn_snapshots_on() {
            self.as_mut().set_undo_json(QString::default());
            return;
        }
        let prompts: Vec<(String, i64)> = self.rows.iter().filter(|r| r.kind == RowKind::User).map(|r| (r.key.clone(), r.time)).collect();
        if prompts.is_empty() {
            return;
        }
        let key = self.key.to_string();
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let mut map = serde_json::Map::new();
            // (Its repository, or each in a folder of them.)
            let snaps = harlo_core::git::folder_turn_snapshots(&dir);
            if !snaps.is_empty() {
                let times: Vec<i64> = prompts.iter().map(|(_, t)| *t).collect();
                for (i, ((k, _), s)) in prompts.iter().zip(harlo_core::git::match_turns(&snaps, &times)).enumerate() {
                    let Some(snap) = s else { continue };
                    let next = prompts.get(i + 1).map(|(n, _)| n.clone()).unwrap_or_default();
                    // The latest turn: how many files it changed (the button says so, or isn't there).
                    let files = if next.is_empty() {
                        harlo_core::git::folder_undo_plan(&dir, &snap).map(|p| (p.restore.len() + p.remove.len()) as i64).unwrap_or(0)
                    } else {
                        -1
                    };
                    map.insert(k.clone(), json!({ "snapshot": snap, "next": next, "files": files }));
                }
            }
            let json = Value::Object(map).to_string();
            let _ = thread.queue(move |mut this| {
                if this.key.to_string() == key && this.undo_json.to_string() != json {
                    this.as_mut().set_undo_json(QString::from(&json));
                }
            });
        });
    }

    /// A message's snapshot, and how many of your messages follow it.
    fn undo_target(&self, row_key: &str) -> Option<(String, usize)> {
        let map: Value = serde_json::from_str(&self.undo_json.to_string()).ok()?;
        let snap = map[row_key]["snapshot"].as_str()?.to_owned();
        let at = self.rows.iter().position(|r| r.key == row_key)?;
        Some((snap, self.rows[at + 1..].iter().filter(|r| r.kind == RowKind::User).count()))
    }

    pub fn preview_undo(mut self: Pin<&mut Self>, row_key: &QString) {
        let row = row_key.to_string();
        let Some((snap, later)) = self.undo_target(&row) else { return };
        let dir = self.folder();
        self.as_mut().set_undo_plan(QString::from(&json!({ "row": row, "state": "loading", "later": later }).to_string()));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let v = match harlo_core::git::folder_undo_plan(&dir, &snap) {
                Ok(p) => json!({ "row": row, "state": "ready", "restore": p.restore, "remove": p.remove, "headMoved": p.head_moved, "later": later }),
                Err(e) => json!({ "row": row, "state": "failed", "later": later, "message": format!("{e:#}") }),
            };
            let _ = thread.queue(move |mut this| {
                let still = serde_json::from_str::<Value>(&this.undo_plan.to_string()).ok().is_some_and(|w| w["row"] == v["row"]);
                if still {
                    this.as_mut().set_undo_plan(QString::from(&v.to_string()));
                }
            });
        });
    }

    pub fn undo_turn(mut self: Pin<&mut Self>, row_key: &QString) {
        let row = row_key.to_string();
        let Some((snap, later)) = self.undo_target(&row) else { return };
        // (Not while the agent is still changing files.)
        if self.status.to_string() == "busy" {
            return;
        }
        let dir = self.folder();
        let mut plan: Value = serde_json::from_str(&self.undo_plan.to_string()).unwrap_or_else(|_| json!({ "row": row }));
        plan["state"] = json!("undoing");
        self.as_mut().set_undo_plan(QString::from(&plan.to_string()));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let v = match harlo_core::git::folder_undo_turn(&dir, &snap) {
                Ok(p) => {
                    let n = p.restore.len() + p.remove.len();
                    json!({ "row": row, "state": "done", "restore": p.restore, "remove": p.remove, "headMoved": p.head_moved, "later": later,
                            "message": if n == 0 { "Nothing to undo: the files are as they were.".to_owned() } else { format!("Undone: {n} {} back as before this message.", if n == 1 { "file is" } else { "files are" }) } })
                }
                Err(e) => json!({ "row": row, "state": "failed", "later": later, "message": format!("Couldn't undo: {e:#}") }),
            };
            let _ = thread.queue(move |mut this| this.as_mut().set_undo_plan(QString::from(&v.to_string())));
        });
    }

    pub fn add_attachment(mut self: Pin<&mut Self>, path: &QString) -> bool {
        let path = crate::workspace::url_to_path(&path.to_string());
        if !std::path::Path::new(&path).is_file() {
            return false;
        }
        let entry = QString::from(&path);
        if !self.attachments.iter().any(|a| a == &entry) {
            let mut list = self.attachments.clone();
            list.append(entry);
            self.as_mut().set_attachments(list);
        }
        true
    }

    pub fn remove_attachment(mut self: Pin<&mut Self>, index: i32) {
        let mut list = self.attachments.clone();
        if index >= 0 && (index as isize) < list.len() {
            list.remove(index as isize);
            self.as_mut().set_attachments(list);
        }
    }

    pub fn abort(self: Pin<&mut Self>) {
        if let Some(h) = self.handle() {
            h.abort();
        }
    }

    pub fn answer_value(self: Pin<&mut Self>, value: &QString) {
        self.answer(json!({ "value": value.to_string() }));
    }

    pub fn answer_confirm(self: Pin<&mut Self>, confirmed: bool) {
        self.answer(json!({ "confirmed": confirmed }));
    }

    pub fn set_ask_first_for_session(mut self: Pin<&mut Self>, on: bool) {
        let key = self.key.to_string();
        if on {
            self.as_mut().rust_mut().ask_first_keys.insert(key);
        } else {
            self.as_mut().rust_mut().ask_first_keys.remove(&key);
        }
        self.as_mut().set_ask_first(on);
    }

    pub fn answer_ask(self: Pin<&mut Self>, answers: &QString) {
        match serde_json::from_str::<Value>(&answers.to_string()) {
            Ok(list @ Value::Array(_)) => self.answer(json!({ "answers": list })),
            _ => tracing::warn!("ask answers are not a JSON array"),
        }
    }

    pub fn cancel_request(self: Pin<&mut Self>) {
        self.answer(json!({ "cancelled": true }));
    }

    fn answer(mut self: Pin<&mut Self>, answer: Value) {
        let Some(front) = self.as_mut().rust_mut().requests.pop_front() else { return };
        if let Some(h) = self.handle() {
            h.answer_ui(&front.id, answer);
        }
        self.show_front_request();
    }

    pub fn load_demo_history(self: Pin<&mut Self>, count: i32) {
        if let Some(h) = self.handle() {
            h.send_raw(json!({ "type": "harlo_demo_history", "count": count.max(0) }));
        }
    }

    /// Runs `request` against the session and hands the response data to
    /// `apply` on the Qt thread, unless the view was rebound meanwhile.
    fn request_then(
        self: Pin<&mut Self>,
        command: Value,
        apply: impl FnOnce(Pin<&mut Self>, Value) + Send + 'static,
    ) {
        let Some(handle) = self.handle().cloned() else { return };
        let thread = self.qt_thread();
        let generation = self.generation;
        crate::runtime().spawn(async move {
            match handle.request(command).await {
                Ok(data) => {
                    let _ = thread.queue(move |this| {
                        if this.generation == generation {
                            apply(this, data);
                        }
                    });
                }
                Err(e) => tracing::warn!("request failed: {e}"),
            }
        });
    }

    pub fn load_context(self: Pin<&mut Self>) {
        let Some(handle) = self.handle().cloned() else { return };
        let thread = self.qt_thread();
        let generation = self.generation;
        crate::runtime().spawn(async move {
            if let Some(report) = handle.context_report().await {
                let json = report.to_string();
                let _ = thread.queue(move |mut this| {
                    if this.generation == generation {
                        this.as_mut().set_context_json(QString::from(&json));
                    }
                });
            }
        });
    }

    pub fn load_models(self: Pin<&mut Self>) {
        self.request_then(json!({ "type": "get_available_models" }), |mut this, data| {
            let mut list = QStringList::default();
            for m in data["models"].as_array().into_iter().flatten() {
                let s = |k: &str| m[k].as_str().unwrap_or_default().replace('\t', " ");
                let reasoning = if m["reasoning"].as_bool() == Some(true) { "1" } else { "0" };
                list.append(QString::from(&format!("{}\t{}\t{}\t{}", s("provider"), s("id"), s("name"), reasoning)));
            }
            this.as_mut().set_models(list);
        });
    }

    pub fn set_model(self: Pin<&mut Self>, provider: &QString, id: &QString) {
        if let Some(h) = self.handle() {
            h.send_raw(json!({ "type": "set_model", "provider": provider.to_string(), "modelId": id.to_string() }));
        }
    }

    pub fn load_thinking_levels(self: Pin<&mut Self>) {
        self.request_then(json!({ "type": "get_available_thinking_levels" }), |mut this, data| {
            let mut list = QStringList::default();
            for l in data["levels"].as_array().into_iter().flatten().filter_map(|l| l.as_str()) {
                list.append(QString::from(l));
            }
            this.as_mut().set_thinking_levels(list);
        });
    }

    pub fn set_thinking_level(self: Pin<&mut Self>, level: &QString) {
        if let Some(h) = self.handle() {
            h.send_raw(json!({ "type": "set_thinking_level", "level": level.to_string() }));
        }
    }

    pub fn search_files(self: Pin<&mut Self>, query: &QString) {
        let Some((live, _)) = self.live.as_ref() else { return };
        let root = live.cwd.clone();
        let query = query.to_string();
        let thread = self.qt_thread();
        let generation = self.generation;
        crate::runtime().spawn_blocking(move || {
            let matches = harlo_core::files::index_for(&root).search(&query, 30);
            let _ = thread.queue(move |mut this| {
                if this.generation == generation {
                    let mut list = QStringList::default();
                    for m in &matches {
                        list.append(QString::from(m));
                    }
                    this.as_mut().set_file_matches(list);
                }
            });
        });
    }

    fn apply_updates(mut self: Pin<&mut Self>, updates: Vec<SessionUpdate>) {
        for update in updates {
            match update {
                SessionUpdate::Reset(rows) => unsafe {
                    self.as_mut().begin_reset_model();
                    self.as_mut().rust_mut().rows = rows;
                    self.as_mut().end_reset_model();
                },
                SessionUpdate::Rows(changes) => self.as_mut().apply_row_changes(changes),
                SessionUpdate::Status(s) => self.as_mut().set_status(QString::from(s.as_str())),
                SessionUpdate::Meta(m) => {
                    if let Some(file) = m.session_file.clone()
                        && self.session_file.as_ref() != Some(&file)
                    {
                        self.as_mut().rust_mut().session_file = Some(file.clone());
                        if matches!(self.harness.to_string().as_str(), "omp" | "claude" | "opencode") {
                            self.as_mut().load_saved_subagents(file);
                        }
                    }
                    self.as_mut().set_model_name(QString::from(&m.model));
                    self.as_mut().set_model_id(QString::from(&m.model_id));
                    self.as_mut().set_thinking(QString::from(&m.thinking));
                    let (text, fraction) = context_summary(m.context_tokens, m.context_window, m.cost);
                    self.as_mut().set_context_text(QString::from(&text));
                    self.as_mut().set_context_fraction(fraction);
                    let title = if !m.name.is_empty() { m.name } else { m.first_message };
                    self.as_mut().set_title(QString::from(&title));
                }
                SessionUpdate::Commands(commands) => {
                    let mut list = QStringList::default();
                    for c in &commands {
                        list.append(QString::from(&format!("{}\t{}\t{}", c.name, c.description.replace('\t', " "), c.hint)));
                    }
                    self.as_mut().set_commands(list);
                }
                SessionUpdate::Queue { steering, follow_up } => {
                    let mut list = QStringList::default();
                    // Attached file contents stay out of the preview.
                    let short = |s: &String| {
                        let (files, rest) = harlo_core::attach::split_blocks(s);
                        match files.len() {
                            0 => rest,
                            n => format!("{rest} [+{n} file{}]", if n == 1 { "" } else { "s" }),
                        }
                    };
                    for q in steering.iter().map(|s| format!("Steer: {}", short(s))).chain(follow_up.iter().map(|s| format!("Next: {}", short(s)))) {
                        list.append(QString::from(&q));
                    }
                    let raw = steering.iter().map(|t| ("steering", t.clone())).chain(follow_up.iter().map(|t| ("followUp", t.clone()))).collect();
                    self.as_mut().rust_mut().queue_raw = raw;
                    self.as_mut().set_queued(list);
                }
                SessionUpdate::SubagentActivity(frame) => {
                    harlo_core::subagents::apply_event(&mut self.as_mut().rust_mut().subagent_events, &frame);
                    self.as_mut().show_subagents();
                    self.as_mut().schedule_subagents();
                }
                SessionUpdate::Todos(todos) => self.as_mut().set_todos_json(QString::from(&todos.to_string())),
                SessionUpdate::Goal(goal) => self.as_mut().apply_goal(&goal),
                // (The workspace notifies.)
                SessionUpdate::Notable(_) => {}
                SessionUpdate::Btw(frame) => self.as_mut().apply_btw(frame),
                SessionUpdate::UiRequest(req) => {
                    self.as_mut().rust_mut().requests.push_back(req);
                    self.as_mut().show_front_request();
                }
                SessionUpdate::UiCleared(id) => {
                    self.as_mut().rust_mut().requests.retain(|r| r.id != id);
                    self.as_mut().show_front_request();
                }
                SessionUpdate::StatusEntry { key, text } => {
                    let mut this = self.as_mut().rust_mut();
                    this.status_entries.retain(|(k, _)| *k != key);
                    if let Some(t) = text {
                        this.status_entries.push((key, t));
                    }
                    let line = this.status_entries.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("  ·  ");
                    self.as_mut().set_status_line(QString::from(&line));
                }
            }
        }
    }

    fn show_front_request(mut self: Pin<&mut Self>) {
        let front = self.requests.front().cloned();
        match front {
            Some(r) => {
                self.as_mut().set_request_id(QString::from(&r.id));
                self.as_mut().set_request_method(QString::from(&r.method));
                self.as_mut().set_request_title(QString::from(&r.title));
                self.as_mut().set_request_message(QString::from(&r.message));
                let mut options = QStringList::default();
                for o in &r.options {
                    options.append(QString::from(o));
                }
                self.as_mut().set_request_options(options);
                self.as_mut().set_request_prefill(QString::from(&r.prefill));
                self.as_mut().set_request_is_approval(r.is_approval);
                self.as_mut().set_request_json(QString::from(&r.raw.to_string()));
                self.as_mut().set_has_request(true);
            }
            None => {
                self.as_mut().set_has_request(false);
                self.as_mut().set_request_json(QString::default());
                self.as_mut().set_request_id(QString::default());
                self.as_mut().set_request_is_approval(false);
            }
        }
        self.match_approval();
    }

    /// Finds the tool row an omp approval belongs to. omp asks while the
    /// tool executes but doesn't say which call it is, so: the unfinished
    /// row of that tool, preferring one whose argument summary appears in
    /// the prompt (`Allow tool: bash` / `Command: …`).
    fn match_approval(mut self: Pin<&mut Self>) {
        let key = match self.requests.front().filter(|r| r.is_approval) {
            None => String::new(),
            Some(r) => {
                let mut lines = r.title.lines();
                let name = lines.next().unwrap_or_default().trim_start_matches("Allow tool:").trim().to_owned();
                let details: String = lines.collect::<Vec<_>>().join("\n");
                let open: Vec<&Row> = self
                    .rows
                    .iter()
                    .filter(|row| matches!(row.kind, RowKind::Tool) && row.title == name && (row.state == "running" || row.state == "pending"))
                    .map(|row| row.as_ref())
                    .collect();
                let summary_matches = |row: &&&Row| {
                    let summary = row.subtitle.trim_end_matches('…');
                    !summary.is_empty() && details.contains(summary)
                };
                open.iter()
                    .rev()
                    .find(summary_matches)
                    .or_else(|| open.iter().rev().find(|row| row.state == "running"))
                    .or(open.last())
                    .map(|row| row.key.clone())
                    .unwrap_or_default()
            }
        };
        if self.approval_row.to_string() != key {
            self.as_mut().set_approval_row(QString::from(&key));
        }
    }

    fn apply_row_changes(mut self: Pin<&mut Self>, changes: Vec<RowChange>) {
        let root = QModelIndex::default();
        for change in changes {
            match change {
                RowChange::Update { at, row } => {
                    self.as_mut().rust_mut().rows[at] = row;
                    let idx = self.index(at as i32, 0, &root);
                    self.as_mut().data_changed(&idx, &idx, &QVector::<i32>::default());
                }
                RowChange::Remove { at, count } => unsafe {
                    self.as_mut().begin_remove_rows(&root, at as i32, (at + count - 1) as i32);
                    self.as_mut().rust_mut().rows.drain(at..at + count);
                    self.as_mut().end_remove_rows();
                },
                RowChange::Insert { at, rows } => unsafe {
                    let last = at + rows.len() - 1;
                    self.as_mut().begin_insert_rows(&root, at as i32, last as i32);
                    self.as_mut().rust_mut().rows.splice(at..at, rows);
                    self.as_mut().end_insert_rows();
                },
            }
        }
        // An approval can arrive before its tool row shows up.
        if self.request_is_approval && self.approval_row.is_empty() {
            self.match_approval();
        }
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(row) = self.rows.get(index.row() as usize) else {
            return QVariant::default();
        };
        let s = |v: &str| QVariant::from(&QString::from(v));
        match role {
            ROLE_KEY => s(&row.key),
            ROLE_KIND => s(row.kind.as_str()),
            ROLE_HTML => s(&row.html),
            ROLE_TEXT => s(&row.text),
            ROLE_TITLE => s(&row.title),
            ROLE_SUBTITLE => s(&row.subtitle),
            ROLE_STATE => s(&row.state),
            ROLE_CALL_ID => s(&row.call_id),
            ROLE_GROUP => s(&row.group),
            ROLE_SUMMARY => s(&row.summary),
            ROLE_GROUP_OPEN => QVariant::from(&row.group_open),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (ROLE_KEY, "rowKey"),
            (ROLE_KIND, "kind"),
            (ROLE_HTML, "html"),
            (ROLE_TEXT, "text"),
            (ROLE_TITLE, "title"),
            (ROLE_SUBTITLE, "subtitle"),
            (ROLE_STATE, "rowState"),
            (ROLE_CALL_ID, "callId"),
            (ROLE_GROUP, "group"),
            (ROLE_SUMMARY, "summary"),
            (ROLE_GROUP_OPEN, "groupOpen"),
        ] {
            roles.insert(role, QByteArray::from(name));
        }
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rows.len() as i32
    }
}

/// "42% · 115k/272k · $0.31" and the used fraction (0..1).
fn context_summary(tokens: u64, window: u64, cost: f64) -> (String, f64) {
    fn k(n: u64) -> String {
        if n >= 1000 { format!("{}k", (n + 500) / 1000) } else { n.to_string() }
    }
    let mut parts = Vec::new();
    let mut fraction = 0.0;
    if window > 0 {
        fraction = tokens as f64 / window as f64;
        parts.push(format!("{:.0}% · {}/{}", fraction * 100.0, k(tokens), k(window)));
    }
    if cost > 0.0 {
        parts.push(format!("${cost:.2}"));
    }
    (parts.join(" · "), fraction)
}
