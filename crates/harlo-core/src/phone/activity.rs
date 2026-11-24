//! Live Activities: a session at work on the iPhone's lock screen and in its
//! Dynamic Island ("Running cargo test · 3/5 · 2m"), while the app is closed.
//!
//! The app starts one when it sends a message and hands Harlo the
//! activity's push token (the `activity` request). Harlo then follows the
//! session and sends updates through the relay as it goes (what it's doing,
//! its todos, waiting for you), then the end when the run finishes. What
//! it's doing is sealed for the phone, as notifications are: Apple and the
//! relay see only the phase, the counts and the times.
//!
//! The activities followed are kept in `activities.json`, so ones left
//! running when Harlo stopped are ended when it's next on the relay rather
//! than staying on a lock screen for hours.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};

use super::rows::Rows;
use super::{Channel, Device, Hub, Identity, key_bytes, lock};
use crate::timeline::{Entry, TodoStatus};
use crate::workspace::{EventKind, ToolStatus};

/// Updates that only change what it's doing go at most this often (Apple
/// limits them); a new phase (waiting for you, done) goes at once.
const MIN_GAP: Duration = Duration::from_secs(8);
/// Idle this long after working: the run's over, not a pause between a
/// turn and its queued follow-up.
const SETTLE: Duration = Duration::from_secs(3);
/// Registered while the run hadn't started yet: how long it may take to.
const GRACE: Duration = Duration::from_secs(15);
/// How long a finished one stays on the lock screen, in seconds.
const DISMISS_AFTER: i64 = 15 * 60;
/// Without news this long, in seconds, the phone shows it as out of date
/// (the computer may sleep, Harlo may have stopped) rather than "Working"
/// for hours…
const STALE_AFTER: i64 = 25 * 60;
/// …so a quiet run (one long command) is sent again this often.
const KEEPALIVE: Duration = Duration::from_mins(10);
/// How long the relay has to take an update.
const SEND_WAIT: Duration = Duration::from_secs(15);
/// The file in phone access's folder that lists the activities followed.
const LEFT: &str = "activities.json";

/// One phone's activity for a session.
#[derive(Debug, Clone)]
struct Reg {
    device: String,
    token: String,
    sandbox: bool,
    public: [u8; 32],
}

enum Msg {
    Add(Reg),
    /// A phone's activity ended on the phone.
    Remove(String),
}

/// The sessions followed for activities now: each one's follower's ID and
/// inbox.
#[derive(Default)]
pub(crate) struct Activities {
    followers: Mutex<HashMap<String, (u64, mpsc::UnboundedSender<Msg>)>>,
    next: AtomicU64,
    /// Guards the file of activities followed.
    file: Mutex<()>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// `text` on one line, cut to `n` characters.
fn clip(text: &str, n: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= n {
        line
    } else {
        format!("{}…", line.chars().take(n).collect::<String>())
    }
}

/// Takes phone `device`'s activity for session `key`: its push token, or
/// `""` when it ended on the phone.
///
/// # Errors
///
/// When the token or the phone's key is unreadable, or Harlo isn't on the
/// relay, which updates go through.
pub(crate) fn register(
    hub: &Arc<Hub>,
    device: &Device,
    key: &str,
    token: &str,
    sandbox: bool,
) -> Result<(), String> {
    let mut followers = lock(&hub.activities.followers);
    if token.is_empty() {
        if let Some((_, tx)) = followers.get(key) {
            let _ = tx.send(Msg::Remove(device.id.clone()));
        }
        return Ok(());
    }
    if token.len() > 200 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("That isn't a push token.".to_owned());
    }
    if lock(&hub.push).is_none() {
        return Err("Live Activities go through the relay: turn it on first.".to_owned());
    }
    let public = key_bytes(&device.public_key)?;
    let reg = Reg {
        device: device.id.clone(),
        token: token.to_ascii_lowercase(),
        sandbox,
        public,
    };
    if let Some((_, tx)) = followers.get(key)
        && tx.send(Msg::Add(reg.clone())).is_ok()
    {
        return Ok(());
    }
    let id = hub.activities.next.fetch_add(1, Ordering::Relaxed);
    note(hub, key, std::slice::from_ref(&reg), now_ms());
    let (tx, rx) = mpsc::unbounded_channel();
    let _ = tx.send(Msg::Add(reg));
    followers.insert(key.to_owned(), (id, tx));
    tokio::spawn(follow(Arc::clone(hub), key.to_owned(), id, rx));
    Ok(())
}

/// What the activity shows.
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    /// `working`, `input`, `done` or `failed`.
    phase: &'static str,
    /// What it's doing ("Running Bash cargo test"), or, done, its last reply.
    now: String,
    title: String,
    done: usize,
    total: usize,
    /// When the run started, in milliseconds since the Unix epoch.
    since: i64,
}

/// The session, as far as the activity cares.
#[derive(Debug, Default)]
struct Tracker {
    busy: bool,
    /// It worked since the activity started.
    worked: bool,
    /// Idle since then, after working.
    idle_since: Option<Instant>,
    failed: bool,
    /// Something waits on the user: whether an approval.
    asks: Option<bool>,
    /// When it started working, as far as Harlo saw.
    busy_at: i64,
    title: String,
}

impl Tracker {
    /// Takes one of the session's events.
    fn apply(&mut self, kind: &EventKind) {
        match kind {
            EventKind::Busy { .. } => {
                if !self.busy && !self.worked {
                    self.busy_at = now_ms();
                }
                self.busy = true;
                self.worked = true;
                self.failed = false;
                self.idle_since = None;
            }
            EventKind::Done { outcome, .. } => {
                self.busy = false;
                self.failed = outcome == "failed";
                if self.worked {
                    self.idle_since = Some(Instant::now());
                }
            }
            _ => {}
        }
    }

    /// The run is over: idle a moment after working, nothing waiting.
    fn finished(&self) -> bool {
        self.asks.is_none() && self.idle_since.is_some_and(|t| t.elapsed() >= SETTLE)
    }

    /// Idle, but maybe only between a turn and its follow-up: nothing to
    /// say yet.
    fn settling(&self) -> bool {
        self.asks.is_none() && self.idle_since.is_some() && !self.finished()
    }

    /// What to show, from the session's timeline `entries`.
    fn state(&self, entries: &[Entry]) -> State {
        // This run: since the user's latest message.
        let turn = entries
            .iter()
            .rposition(|e| matches!(e, Entry::Prompt { .. }))
            .map_or(0, |i| i + 1);
        let run = &entries[turn..];
        let (phase, now) = if let Some(approval) = self.asks {
            let now = if approval {
                "Waiting for your approval"
            } else {
                "Waiting for your answer"
            };
            ("input", now.to_owned())
        } else if self.finished() || (!self.busy && self.worked) {
            let last = run
                .iter()
                .rev()
                .find_map(|e| match e {
                    Entry::Answer { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .unwrap_or_default();
            (if self.failed { "failed" } else { "done" }, clip(last, 140))
        } else {
            let running = run.iter().rev().find_map(|e| match e {
                Entry::Tool {
                    label,
                    detail,
                    status: ToolStatus::Running | ToolStatus::Streaming,
                    ..
                } => Some(format!("Running {label} {detail}")),
                Entry::Subagent {
                    agent,
                    status: ToolStatus::Running | ToolStatus::Streaming,
                    ..
                } => Some(format!("Subagent {agent} at work")),
                _ => None,
            });
            let now = match (running, run.last()) {
                (Some(running), _) => clip(&running, 80),
                (None, Some(Entry::Thought { .. })) => "Thinking".to_owned(),
                (None, Some(Entry::Answer { .. })) => "Writing".to_owned(),
                (None, _) => "Working".to_owned(),
            };
            ("working", now)
        };
        let (done, total) = entries
            .iter()
            .rev()
            .find_map(|e| match e {
                Entry::Tool {
                    todos: Some(todos), ..
                } => Some((
                    todos
                        .iter()
                        .filter(|t| t.status == TodoStatus::Completed)
                        .count(),
                    todos.len(),
                )),
                _ => None,
            })
            .unwrap_or_default();
        State {
            phase,
            now,
            title: clip(&self.title, 80),
            done,
            total,
            since: self.busy_at,
        }
    }
}

/// The activity's update, or its end, with what it's doing sealed for the
/// phone.
fn payload(state: &State, end: bool, channel: &Channel) -> Value {
    let secs = now_ms() / 1000;
    let sealed = channel.seal(
        json!({ "now": state.now, "title": state.title })
            .to_string()
            .as_bytes(),
    );
    #[expect(
        clippy::cast_precision_loss,
        reason = "seconds since the epoch fit a double exactly"
    )]
    let (since, until) = (
        state.since as f64 / 1000.0,
        if end { secs as f64 } else { 0.0 },
    );
    let mut aps = json!({
        "timestamp": secs,
        "event": if end { "end" } else { "update" },
        "content-state": {
            "phase": state.phase, "since": since, "until": until,
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

/// How handing an update to the relay went.
enum Sent {
    Ok,
    /// The activity's gone from the phone.
    Gone,
    /// It didn't reach the relay, or the relay refused it.
    Failed,
}

/// Hands an activity's update `body` for `reg` to the relay at `base`.
async fn forward(base: &str, token: &str, sandbox: bool, body: &Value, urgent: bool) -> Sent {
    let Ok(http) = reqwest::Client::builder().timeout(SEND_WAIT).build() else {
        return Sent::Failed;
    };
    let url = format!("{}/push", base.trim_end_matches('/'));
    let forward = json!({ "token": token, "sandbox": sandbox, "payload": body, "collapse": null,
                          "activity": true, "urgent": urgent });
    match http.post(&url).json(&forward).send().await {
        Ok(r) if r.status().is_success() => Sent::Ok,
        Ok(r) if r.status().as_u16() == 410 => Sent::Gone,
        Ok(_) | Err(_) => Sent::Failed,
    }
}

/// The session's timeline, from the server, if Harlo is connected to it.
async fn load(hub: &Hub, key: &str) -> Option<Rows> {
    let server = hub.current()?;
    let messages = server.client.messages(key).await.ok()?;
    Some(Rows::from_messages(&messages))
}

/// Follows session `key` while phones show its activity, until the run ends
/// or every phone's activity did.
async fn follow(hub: Arc<Hub>, key: String, id: u64, mut inbox: mpsc::UnboundedReceiver<Msg>) {
    let mut events = hub.events.subscribe();
    let started = Instant::now();
    let mut regs: Vec<Reg> = Vec::new();
    // Registered as the phone sent its message: about when the run began.
    let mut t = Tracker {
        busy_at: now_ms(),
        ..Tracker::default()
    };
    if let Some(server) = hub.current() {
        t.busy = lock(&server.workspace).is_busy(&key);
        t.worked = t.busy;
    }
    let mut rows = load(&hub, &key).await;
    // What each phone got last, and when.
    let mut sent: HashMap<String, (State, Instant)> = HashMap::new();
    loop {
        tokio::select! {
            m = inbox.recv() => match m {
                Some(Msg::Add(r)) => {
                    regs.retain(|x| x.device != r.device);
                    sent.remove(&r.device);
                    regs.push(r);
                    note(&hub, &key, &regs, t.busy_at);
                }
                Some(Msg::Remove(device)) => {
                    regs.retain(|x| x.device != device);
                    note(&hub, &key, &regs, t.busy_at);
                    if regs.is_empty() {
                        break;
                    }
                }
                None => break,
            },
            event = events.recv() => match event {
                Ok(event) if event.kind.session_id() == Some(key.as_str()) => {
                    t.apply(&event.kind);
                    let reload = rows.as_mut().is_none_or(|r| r.apply(&event.kind, event.time));
                    if reload {
                        rows = load(&hub, &key).await;
                    }
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => rows = load(&hub, &key).await,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            () = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
        let server = hub.current();
        if let Some(server) = &server {
            let workspace = lock(&server.workspace);
            t.asks = workspace
                .needs_user(&key)
                .then(|| !workspace.permissions(&key).is_empty());
            t.title = workspace
                .session(&key)
                .and_then(|s| s.title.clone())
                .unwrap_or_default();
        }
        let gone = server.is_some_and(|s| lock(&s.workspace).session(&key).is_none());
        // Over: the run ended, the session went, or the run never started.
        let end = t.finished() || gone || (!t.worked && started.elapsed() > GRACE);
        if !end && t.settling() {
            continue;
        }
        let Some(base) = lock(&hub.push).clone() else {
            continue;
        };
        let state = t.state(rows.as_ref().map(Rows::entries).unwrap_or_default());
        let lost = send(&hub, &base, &regs, &mut sent, &state, end).await;
        regs.retain(|r| !lost.contains(&r.device));
        if end || regs.is_empty() {
            break;
        }
        if !lost.is_empty() {
            note(&hub, &key, &regs, t.busy_at);
        }
    }
    // Ended, or none is left to end: off the list.
    note(&hub, &key, &[], 0);
    let mut followers = lock(&hub.activities.followers);
    if followers.get(&key).is_some_and(|(i, _)| *i == id) {
        followers.remove(&key);
    }
}

/// Sends `state` to each phone in `regs` it's due for (`end`: the end, to
/// all); returns the phones whose activity is gone.
async fn send(
    hub: &Hub,
    base: &str,
    regs: &[Reg],
    sent: &mut HashMap<String, (State, Instant)>,
    state: &State,
    end: bool,
) -> Vec<String> {
    let paused = hub.paused.load(Ordering::Relaxed);
    let mut lost = Vec::new();
    for r in regs {
        let due = sent.get(&r.device).is_none_or(|(last, at)| {
            end || last.phase != state.phase
                || (*last != *state && at.elapsed() >= MIN_GAP)
                || at.elapsed() >= KEEPALIVE
        });
        // Paused: none start or change; one already on the phone still
        // ends.
        if !due || (paused && (!end || !sent.contains_key(&r.device))) {
            continue;
        }
        let urgent = end
            || sent
                .get(&r.device)
                .is_none_or(|(last, _)| last.phase != state.phase);
        let body = payload(state, end, &hub.identity.channel(&r.public));
        match forward(base, &r.token, r.sandbox, &body, urgent).await {
            Sent::Gone => lost.push(r.device.clone()),
            // Tried: not again until the next change, or the gap.
            Sent::Ok | Sent::Failed => {
                sent.insert(r.device.clone(), (state.clone(), Instant::now()));
            }
        }
    }
    lost
}

fn read_left(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

fn write_left(path: &Path, list: &[Value]) {
    if list.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    let part = path.with_extension("part");
    if std::fs::write(&part, Value::Array(list.to_vec()).to_string()).is_ok() {
        let _ = std::fs::rename(&part, path);
    }
}

/// Session `key`'s activities are now `regs`, none when it's off the list;
/// `since`: when its run began, in milliseconds since the Unix epoch.
fn note(hub: &Hub, key: &str, regs: &[Reg], since: i64) {
    let _held = lock(&hub.activities.file);
    note_in(&hub.folder.join(LEFT), key, regs, since);
}

fn note_in(path: &Path, key: &str, regs: &[Reg], since: i64) {
    use base64::Engine as _;
    let mut list = read_left(path);
    list.retain(|v| v["key"] != key);
    list.extend(regs.iter().map(|r| {
        json!({ "key": key, "device": r.device, "token": r.token, "sandbox": r.sandbox,
                "public": base64::engine::general_purpose::STANDARD.encode(r.public), "since": since })
    }));
    write_left(path, &list);
}

/// Ends the activities left on the list, taking them off it: Harlo stopped
/// while following them. They go from the lock screen at once, saying so.
pub(crate) async fn end_left(hub: Arc<Hub>, base: String) {
    let ends = {
        let _held = lock(&hub.activities.file);
        left_ends(&hub.folder.join(LEFT), &hub.identity)
    };
    for (token, sandbox, body) in ends {
        let _ = forward(&base, &token, sandbox, &body, true).await;
    }
}

/// The ends of the activities on the list, which is emptied: each one's
/// push token, whether it's a development build's, and the update.
fn left_ends(path: &Path, identity: &Identity) -> Vec<(String, bool, Value)> {
    let list = read_left(path);
    write_left(path, &[]);
    let mut out = Vec::new();
    for v in list {
        let token = v["token"].as_str().unwrap_or_default();
        let Ok(public) = key_bytes(v["public"].as_str().unwrap_or_default()) else {
            continue;
        };
        if token.is_empty() {
            continue;
        }
        let state = State {
            phase: "failed",
            now: "Harlo stopped".to_owned(),
            title: String::new(),
            done: 0,
            total: 0,
            since: v["since"]
                .as_i64()
                .filter(|t| *t > 0)
                .unwrap_or_else(now_ms),
        };
        let mut body = payload(&state, true, &identity.channel(&public));
        body["aps"]["dismissal-date"] = json!(now_ms() / 1000);
        out.push((
            token.to_owned(),
            v["sandbox"].as_bool().unwrap_or(false),
            body,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::Todo;

    fn tool(label: &str, detail: &str, status: ToolStatus) -> Entry {
        Entry::Tool {
            key: format!("{label}{detail}"),
            name: label.to_lowercase(),
            label: label.to_owned(),
            detail: detail.to_owned(),
            status,
            took: None,
            files: Vec::new(),
            todos: None,
            pictures: Vec::new(),
            error: None,
            script: None,
        }
    }

    fn prompt() -> Entry {
        Entry::Prompt {
            key: "p".to_owned(),
            text: "go".to_owned(),
            attachments: Vec::new(),
            pictures: Vec::new(),
            skills: Vec::new(),
            agent: None,
        }
    }

    #[test]
    fn says_what_the_run_does() {
        let mut t = Tracker::default();
        t.apply(&EventKind::Busy {
            session_id: "s".to_owned(),
        });
        let mut entries = vec![prompt(), tool("Bash", "cargo test", ToolStatus::Running)];
        let s = t.state(&entries);
        assert_eq!(
            (s.phase, s.now.as_str()),
            ("working", "Running Bash cargo test")
        );
        let mut todo = tool("Todo", "", ToolStatus::Completed);
        if let Entry::Tool { todos, .. } = &mut todo {
            *todos = Some(vec![
                Todo {
                    content: "a".to_owned(),
                    status: TodoStatus::Completed,
                },
                Todo {
                    content: "b".to_owned(),
                    status: TodoStatus::InProgress,
                },
            ]);
        }
        entries.insert(1, todo);
        assert_eq!((t.state(&entries).done, t.state(&entries).total), (1, 2));
        t.asks = Some(true);
        let s = t.state(&entries);
        assert_eq!(
            (s.phase, s.now.as_str()),
            ("input", "Waiting for your approval")
        );
        t.asks = None;
        entries.push(Entry::Answer {
            key: "a".to_owned(),
            text: "All  tests\npass.".to_owned(),
        });
        t.apply(&EventKind::Done {
            session_id: "s".to_owned(),
            outcome: "succeeded".to_owned(),
        });
        // Idle, but not long enough to call it over.
        assert!(!t.finished() && t.settling());
        let s = t.state(&entries);
        assert_eq!((s.phase, s.now.as_str()), ("done", "All tests pass."));
        t.idle_since = Instant::now().checked_sub(SETTLE);
        assert!(t.finished());
    }

    #[test]
    fn sealed_for_the_phone() {
        let harlo = Identity::generate();
        let phone = Identity::generate();
        let harlo_public = key_bytes(&harlo.public_b64()).expect("key");
        let phone_public = key_bytes(&phone.public_b64()).expect("key");
        let state = State {
            phase: "working",
            now: "Running Bash rm -rf target".to_owned(),
            title: "Fix it".to_owned(),
            done: 1,
            total: 3,
            since: 1_700_000_000_000,
        };
        let p = payload(&state, false, &harlo.channel(&phone_public));
        let text = p.to_string();
        assert!(
            !text.contains("rm -rf") && !text.contains("Fix it"),
            "Apple sees no words"
        );
        let cs = &p["aps"]["content-state"];
        assert_eq!(
            (
                cs["phase"].as_str(),
                cs["done"].as_u64(),
                cs["since"].as_f64()
            ),
            (Some("working"), Some(1), Some(1_700_000_000.0))
        );
        let opened = phone
            .channel(&harlo_public)
            .open(cs["sealed"].as_str().expect("sealed"))
            .expect("opens");
        let opened: Value = serde_json::from_slice(&opened).expect("JSON");
        assert_eq!(opened["now"], "Running Bash rm -rf target");
        let stale_for = p["aps"]["stale-date"].as_i64().unwrap_or_default()
            - p["aps"]["timestamp"].as_i64().unwrap_or_default();
        assert_eq!(stale_for, STALE_AFTER, "an update goes stale without news");
        let end = payload(
            &State {
                phase: "done",
                ..state
            },
            true,
            &harlo.channel(&phone_public),
        );
        assert_eq!(end["aps"]["event"], "end");
        assert!(end["aps"]["dismissal-date"].is_i64() && end["aps"].get("stale-date").is_none());
    }

    #[test]
    fn activities_left_over_are_ended() {
        let dir = std::env::temp_dir().join(format!("harlo-activities-{}", std::process::id()));
        let path = dir.join(LEFT);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let harlo = Identity::generate();
        let phone = Identity::generate();
        let public = key_bytes(&phone.public_b64()).expect("key");
        let reg = |device: &str, token: &str| Reg {
            device: device.to_owned(),
            token: token.to_owned(),
            sandbox: true,
            public,
        };
        // Two sessions followed; one ends, the other's phone leaves.
        note_in(
            &path,
            "a",
            &[reg("p1", "aa"), reg("p2", "bb")],
            1_700_000_000_000,
        );
        note_in(&path, "b", &[reg("p1", "cc")], 1_700_000_100_000);
        note_in(&path, "a", &[reg("p1", "aa")], 1_700_000_000_000);
        note_in(&path, "b", &[], 0);
        assert_eq!(read_left(&path).len(), 1);
        // Harlo stopped with "a" still going: it ends, and the list empties.
        let ends = left_ends(&path, &harlo);
        assert_eq!(ends.len(), 1);
        let (token, sandbox, body) = &ends[0];
        assert_eq!((token.as_str(), *sandbox), ("aa", true));
        assert_eq!(body["aps"]["event"], "end");
        assert_eq!(body["aps"]["content-state"]["phase"], "failed");
        assert_eq!(
            body["aps"]["content-state"]["since"].as_f64(),
            Some(1_700_000_000.0)
        );
        assert!(!path.exists() && left_ends(&path, &harlo).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
