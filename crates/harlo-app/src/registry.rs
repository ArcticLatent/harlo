//! Live sessions shared between the `Workspace` (which creates and closes
//! them) and any `ChatSession` view attached to one.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use harlo_core::SessionHandle;

#[derive(Clone)]
pub struct LiveRef {
    pub handle: Arc<SessionHandle>,
    pub harness: String,
    pub cwd: PathBuf,
}

fn map() -> &'static Mutex<HashMap<String, LiveRef>> {
    static MAP: OnceLock<Mutex<HashMap<String, LiveRef>>> = OnceLock::new();
    MAP.get_or_init(Default::default)
}

pub fn insert(key: &str, live: LiveRef) {
    map().lock().unwrap().insert(key.to_owned(), live);
}

pub fn get(key: &str) -> Option<LiveRef> {
    map().lock().unwrap().get(key).cloned()
}

/// Restarts an agent's idle sessions (after harlo changed its config, which
/// it reads only when it starts): each starts again when next used.
pub fn restart_idle(harness: &str) {
    for live in map().lock().unwrap().values().filter(|l| l.harness == harness) {
        live.handle.restart_if_idle();
    }
}

/// Forgets a session. Its harness stops once no view holds the handle.
pub fn remove(key: &str) -> Option<LiveRef> {
    map().lock().unwrap().remove(key)
}
