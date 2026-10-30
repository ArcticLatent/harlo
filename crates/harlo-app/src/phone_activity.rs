//! Live Activities: a session at work on the iPhone's lock screen and in its
//! Dynamic Island ("Running cargo test · 3/5 · 2m"), while the app is closed.
//!
//! The app starts one when it sends a message (or starts a session) and
//! hands harlo the activity's push token (the `activity` op). harlo then
//! follows the session and sends updates through harlo's relay as it goes
//! (what it's doing, its todos, waiting for you), then the end when the run
//! finishes. What it's doing is sealed for the phone, as notifications are
//! (the app's widget opens it): Apple and the relay see only the phase, the
//! counts and the times.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use harlo_apns as apns;
use harlo_core::listdiff::ListChange;
use harlo_core::phone::{self, Device};
use harlo_core::{Notable, Row, RowKind, SessionSink, SessionStatus, SessionUpdate};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::phone_server::Shared;

/// Updates that only change what it's doing go at most this often (Apple
/// limits them); a new phase (waiting for you, done) goes at once.
const MIN_GAP: Duration = Duration::from_secs(8);
/// Idle this long after working: the run's over (not a pause between a
/// turn and its queued follow-up).
const SETTLE: Duration = Duration::from_secs(3);
/// Registered while it hadn't started yet (or had already finished): how
/// long it may take to start.
const GRACE: Duration = Duration::from_secs(15);
/// How long a finished one stays on the lock screen.
const DISMISS_AFTER: i64 = 15 * 60;
/// Without news this long, the phone shows it as stale (the computer may
/// sleep, harlo may have quit) rather than "Working" for hours…
const STALE_AFTER: i64 = 25 * 60;
/// …so a quiet run (one long command) is sent again this often.
const KEEPALIVE: Duration = Duration::from_secs(10 * 60);

/// One phone's activity for a session.
#[derive(Clone)]
struct Reg {
    device: String,
    token: String,
    sandbox: bool,
    public: [u8; 32],
}

enum Msg {
    Add(Reg),
    /// A device's activity ended on the phone.
    Remove(String),
}

/// Sessions followed now: their watcher's id and inbox.
type Watchers = Mutex<HashMap<String, (u64, mpsc::UnboundedSender<Msg>)>>;

fn watchers() -> &'static Watchers {
    static W: OnceLock<Watchers> = OnceLock::new();
    W.get_or_init(Default::default)
}

/// A phone's activity for the session `key` (its live key): its push token,
/// or "" when it ended on the phone.
pub fn register(shared: &Arc<Shared>, device: &Device, key: &str, token: &str, sandbox: bool) -> Result<(), String> {
    let mut w = watchers().lock().unwrap();
    if token.is_empty() {
        if let Some((_, tx)) = w.get(key) {
            let _ = tx.send(Msg::Remove(device.id.clone()));
        }
        return Ok(());
    }
    if token.len() > 200 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("That isn't a push token.".into());
    }
    let public = phone::unb64(&device.public_key).ok().and_then(|b| <[u8; 32]>::try_from(b).ok()).ok_or("This phone's key is unreadable.")?;
    let reg = Reg { device: device.id.clone(), token: token.to_ascii_lowercase(), sandbox, public };
    if let Some((_, tx)) = w.get(key)
        && tx.send(Msg::Add(reg.clone())).is_ok()
    {
        return Ok(());
    }
    let live = crate::registry::get(key).ok_or("That session isn't running.")?;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (tx, rx) = mpsc::unbounded_channel();
    let _ = tx.send(Msg::Add(reg));
    w.insert(key.to_owned(), (id, tx));
    crate::runtime().spawn(watch(shared.clone(), key.to_owned(), id, live, rx));
    Ok(())
}

struct Forward(mpsc::UnboundedSender<Vec<SessionUpdate>>);

impl SessionSink for Forward {
    fn deliver(&self, updates: Vec<SessionUpdate>) {
        let _ = self.0.send(updates);
    }
}

/// What the activity shows.
#[derive(Debug, Clone, PartialEq)]
struct State {
    /// "working", "input", "done", "failed".
    phase: &'static str,
    /// What it's doing ("Running cargo test"), or (done) its last reply.
    now: String,
    title: String,
    done: usize,
    total: usize,
    /// When the run started (unix ms).
    since: i64,
}

/// The session as far as the activity cares, from its updates.
#[derive(Default)]
struct Tracker {
    /// The session's rows came (its snapshot).
    ready: bool,
    rows: Vec<Arc<Row>>,
    title: String,
    busy: bool,
    /// It worked since the activity started.
    worked: bool,
    /// Idle (after working) since then.
    idle_since: Option<Instant>,
    failed: bool,
    /// Questions and approvals waiting: (id, an approval).
    asks: Vec<(String, bool)>,
    todos: (usize, usize),
    /// When it started working, as far as harlo saw (unix ms): the run's
    /// start until its message shows.
    busy_at: i64,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn clip(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= n { s } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}

impl Tracker {
    fn apply(&mut self, batch: &[SessionUpdate]) {
        for u in batch {
            match u {
                SessionUpdate::Reset(rows) => {
                    self.rows = rows.clone();
                    self.ready = true;
                }
                SessionUpdate::Rows(changes) => {
                    for c in changes {
                        match c {
                            ListChange::Update { at, row } if *at < self.rows.len() => self.rows[*at] = row.clone(),
                            ListChange::Remove { at, count } => {
                                let end = (*at + *count).min(self.rows.len());
                                self.rows.drain((*at).min(end)..end);
                            }
                            ListChange::Insert { at, rows } => {
                                let at = (*at).min(self.rows.len());
                                self.rows.splice(at..at, rows.iter().cloned());
                            }
                            _ => {}
                        }
                    }
                }
                SessionUpdate::Status(s) => {
                    let busy = matches!(s, SessionStatus::Busy | SessionStatus::Starting);
                    if busy && !self.busy {
                        if self.busy_at == 0 {
                            self.busy_at = now_ms();
                        }
                        self.worked = true;
                        self.failed = false;
                    }
                    self.idle_since = if busy || !self.worked { None } else { self.idle_since.or(Some(Instant::now())) };
                    self.busy = busy;
                }
                SessionUpdate::Meta(m) => self.title = if m.name.is_empty() { m.first_message.clone() } else { m.name.clone() },
                SessionUpdate::UiRequest(r) => {
                    if !self.asks.iter().any(|(id, _)| *id == r.id) {
                        self.asks.push((r.id.clone(), r.is_approval || r.method == "confirm"));
                    }
                }
                SessionUpdate::UiCleared(id) => self.asks.retain(|(i, _)| i != id),
                SessionUpdate::Todos(phases) => {
                    let tasks: Vec<&Value> = phases.as_array().into_iter().flatten().flat_map(|p| p["tasks"].as_array().into_iter().flatten()).collect();
                    self.todos = (tasks.iter().filter(|t| t["status"] == "completed").count(), tasks.len());
                }
                SessionUpdate::Notable(Notable::Error { .. }) => self.failed = true,
                _ => {}
            }
        }
    }

    /// The run is over: idle a moment after working.
    fn finished(&self) -> bool {
        self.asks.is_empty() && self.idle_since.is_some_and(|t| t.elapsed() >= SETTLE)
    }

    /// Idle, but maybe only between a turn and its follow-up: nothing to
    /// say yet.
    fn settling(&self) -> bool {
        self.asks.is_empty() && self.idle_since.is_some() && !self.finished()
    }

    fn state(&self) -> State {
        // This run: since your latest message.
        let turn = self.rows.iter().rposition(|r| r.kind == RowKind::User);
        let since = turn.map(|i| self.rows[i].time).filter(|t| *t > 0).unwrap_or(self.busy_at);
        let rows = &self.rows[turn.map_or(0, |i| i + 1)..];
        let (phase, now) = if let Some((_, approval)) = self.asks.first() {
            ("input", if *approval { "Waiting for your approval" } else { "Waiting for your answer" }.to_owned())
        } else if self.finished() || (!self.busy && self.worked) {
            let last = rows.iter().rev().find(|r| r.kind == RowKind::Markdown).map(|r| harlo_core::markdown::plain_text(&r.html)).unwrap_or_default();
            (if self.failed { "failed" } else { "done" }, clip(&last, 140))
        } else if let Some(r) = rows.last().filter(|r| r.state == "streaming" && r.kind != RowKind::Tool) {
            ("working", if r.kind == RowKind::Thinking { "Thinking" } else { "Writing" }.to_owned())
        } else if let Some(t) = rows.iter().rev().find(|r| r.kind == RowKind::Tool && r.state == "running") {
            ("working", clip(&format!("Running {} {}", t.title, t.subtitle), 80))
        } else {
            ("working", "Working".to_owned())
        };
        State { phase, now, title: clip(&self.title, 80), done: self.todos.0, total: self.todos.1, since }
    }
}

/// The activity's update (or its end), with what it's doing sealed for the
/// phone.
fn payload(state: &State, end: bool, channel: &phone::Channel) -> Value {
    let secs = now_ms() / 1000;
    let sealed = channel.seal(json!({ "now": state.now, "title": state.title }).to_string().as_bytes());
    let mut aps = json!({
        "timestamp": secs,
        "event": if end { "end" } else { "update" },
        "content-state": {
            "phase": state.phase, "since": state.since as f64 / 1000.0, "until": if end { secs as f64 } else { 0.0 },
            "done": state.done, "total": state.total, "sealed": sealed,
        },
    });
    if end {
        aps["dismissal-date"] = json!(secs + DISMISS_AFTER);
    } else {
        aps["stale-date"] = json!(secs + STALE_AFTER);
    }
    json!({ "aps": aps })
}

/// Follows the session `key` while phones show its activity, until the run
/// ends (or every phone's activity did).
async fn watch(shared: Arc<Shared>, key: String, id: u64, live: crate::registry::LiveRef, mut inbox: mpsc::UnboundedReceiver<Msg>) {
    let (tx, mut updates) = mpsc::unbounded_channel();
    let sub = live.handle.subscribe(Forward(tx));
    let started = Instant::now();
    let mut regs: Vec<Reg> = Vec::new();
    // (Registered as the phone sent it: about when the run began.)
    let mut t = Tracker { busy_at: now_ms(), ..Default::default() };
    // What each phone got last, and when.
    let mut sent: HashMap<String, (State, Instant)> = HashMap::new();
    loop {
        tokio::select! {
            m = inbox.recv() => match m {
                Some(Msg::Add(r)) => {
                    regs.retain(|x| x.device != r.device);
                    sent.remove(&r.device);
                    regs.push(r);
                }
                Some(Msg::Remove(device)) => {
                    regs.retain(|x| x.device != device);
                    if regs.is_empty() {
                        break;
                    }
                }
                None => break,
            },
            batch = updates.recv() => match batch {
                Some(b) => t.apply(&b),
                None => break,
            },
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
        let gone = crate::registry::get(&key).is_none();
        // Over: the run ended, the session closed, or it never started.
        let end = t.finished() || gone || (!t.worked && started.elapsed() > GRACE);
        if !end && (!t.ready || t.settling()) {
            continue;
        }
        let state = t.state();
        let mut lost = Vec::new();
        for r in &regs {
            let due = match sent.get(&r.device) {
                None => true,
                Some((last, at)) => end || last.phase != state.phase || (*last != state && at.elapsed() >= MIN_GAP) || at.elapsed() >= KEEPALIVE,
            };
            // Paused: none start or change; one already on the phone still
            // ends.
            if !due || (crate::phone_push::paused() && (!end || !sent.contains_key(&r.device))) {
                continue;
            }
            let urgent = end || sent.get(&r.device).is_none_or(|(last, _)| last.phase != state.phase);
            let body = payload(&state, end, &shared.identity.channel(&r.public));
            let push = apns::Push { device_token: &r.token, sandbox: r.sandbox, topic: crate::phone_push::TOPIC, payload: &body, collapse_id: None, kind: apns::Kind::Activity { urgent } };
            match crate::phone_push::forward(&push).await {
                apns::Sent::Ok => {
                    sent.insert(r.device.clone(), (state.clone(), Instant::now()));
                }
                apns::Sent::Gone(_) => lost.push(r.device.clone()),
                apns::Sent::Failed(e) => {
                    tracing::info!("live activity for {key}: {e}");
                    // Tried: not again until the next change, or the gap.
                    sent.insert(r.device.clone(), (state.clone(), Instant::now()));
                }
            }
        }
        regs.retain(|r| !lost.contains(&r.device));
        if end || regs.is_empty() {
            break;
        }
    }
    live.handle.unsubscribe(sub);
    let mut w = watchers().lock().unwrap();
    if w.get(&key).is_some_and(|(i, _)| *i == id) {
        w.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: RowKind, state: &str, title: &str, subtitle: &str, time: i64) -> Arc<Row> {
        Arc::new(Row { key: format!("{title}{time}"), kind, state: state.into(), title: title.into(), subtitle: subtitle.into(), time, ..Default::default() })
    }

    #[test]
    fn says_what_the_run_does() {
        let mut t = Tracker::default();
        t.apply(&[SessionUpdate::Status(SessionStatus::Busy), SessionUpdate::Reset(vec![
            row(RowKind::User, "", "", "", 1000),
            row(RowKind::Tool, "running", "bash", "cargo test", 2000),
        ])]);
        let s = t.state();
        assert_eq!((s.phase, s.now.as_str(), s.since), ("working", "Running bash cargo test", 1000));
        t.apply(&[SessionUpdate::Todos(json!([{ "name": "a", "tasks": [{ "status": "completed" }, { "status": "in_progress" }] }]))]);
        assert_eq!((t.state().done, t.state().total), (1, 2));
        t.apply(&[SessionUpdate::UiRequest(harlo_core::UiRequest { id: "r".into(), method: "select".into(), is_approval: true, ..Default::default() })]);
        assert_eq!((t.state().phase, t.state().now.as_str()), ("input", "Waiting for your approval"));
        t.apply(&[SessionUpdate::UiCleared("r".into()), SessionUpdate::Status(SessionStatus::Idle)]);
        // Idle, but not long enough to call it over.
        assert!(!t.finished());
        assert_eq!(t.state().phase, "done");
        t.idle_since = Some(Instant::now() - SETTLE);
        assert!(t.finished());
    }

    #[test]
    fn sealed_for_the_phone() {
        use crypto_box::SecretKey;
        use crypto_box::aead::OsRng;
        let harlo = phone::Identity::from_secret(SecretKey::generate(&mut OsRng).to_bytes());
        let phone_secret = SecretKey::generate(&mut OsRng);
        let harlo_public: [u8; 32] = phone::unb64(&harlo.public_b64()).unwrap().try_into().unwrap();
        let state = State { phase: "working", now: "Running bash rm -rf target".into(), title: "Fix it".into(), done: 1, total: 3, since: 1_700_000_000_000 };
        let p = payload(&state, false, &harlo.channel(phone_secret.public_key().as_bytes()));
        assert!(!p.to_string().contains("rm -rf") && !p.to_string().contains("Fix it"), "Apple sees no words");
        let cs = &p["aps"]["content-state"];
        assert_eq!((cs["phase"].as_str(), cs["done"].as_u64(), cs["since"].as_f64()), (Some("working"), Some(1), Some(1_700_000_000.0)));
        let opened: Value = serde_json::from_slice(&phone::Identity::from_secret(phone_secret.to_bytes()).channel(&harlo_public).open(cs["sealed"].as_str().unwrap()).unwrap()).unwrap();
        assert_eq!(opened["now"], "Running bash rm -rf target");
        let end = payload(&State { phase: "done", ..state }, true, &harlo.channel(phone_secret.public_key().as_bytes()));
        assert_eq!(end["aps"]["event"], "end");
        assert!(end["aps"]["dismissal-date"].is_i64() && end["aps"].get("stale-date").is_none());
        let stale = p["aps"]["stale-date"].as_i64().unwrap() - p["aps"]["timestamp"].as_i64().unwrap();
        assert_eq!(stale, STALE_AFTER, "an update goes stale without news");
    }
}
