//! What the phone server (a background task) needs from the workspace (on
//! the Qt thread): the session list, kept current by the workspace, and
//! requests that only the workspace can answer (making a session live
//! without switching the desktop's tab, starting one, its modes, renaming,
//! archiving, sending with Plan's note).

use std::sync::{Mutex, OnceLock};

use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot, watch};

/// The sessions, as the phone lists them (JSON array, see
/// `Workspace::publish_phone_sessions`).
fn sessions() -> &'static watch::Sender<Value> {
    static S: OnceLock<watch::Sender<Value>> = OnceLock::new();
    S.get_or_init(|| watch::channel(Value::Array(Vec::new())).0)
}

pub fn publish_sessions(list: Value) {
    sessions().send_if_modified(|current| {
        if *current == list {
            return false;
        }
        *current = list;
        true
    });
}

pub fn watch_sessions() -> watch::Receiver<Value> {
    sessions().subscribe()
}

/// Whether a phone server runs (the workspace only builds the list then).
pub fn active() -> bool {
    sessions().receiver_count() > 0
}

/// A phone's request (`{ op, … }`) and where its reply goes.
pub type Request = (Value, oneshot::Sender<Value>);

fn requests() -> &'static Mutex<Option<mpsc::UnboundedSender<Request>>> {
    static R: OnceLock<Mutex<Option<mpsc::UnboundedSender<Request>>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// The workspace takes requests from here.
pub fn serve() -> mpsc::UnboundedReceiver<Request> {
    let (tx, rx) = mpsc::unbounded_channel();
    *requests().lock().unwrap() = Some(tx);
    rx
}

/// Asks the workspace; `{ error }` when it isn't there.
pub async fn call(request: Value) -> Value {
    let gone = || json!({ "error": "harlo is closing." });
    let Some(tx) = requests().lock().unwrap().clone() else { return gone() };
    let (reply, wait) = oneshot::channel();
    if tx.send((request, reply)).is_err() {
        return gone();
    }
    wait.await.unwrap_or_else(|_| gone())
}

/// The live key for a session key, opening it if needed.
pub async fn open(key: &str) -> Option<String> {
    call(json!({ "op": "open", "key": key })).await["key"].as_str().map(str::to_owned)
}
