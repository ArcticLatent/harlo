//! Commands in the background (`bash` with `background: true`): servers,
//! watchers, long test runs. One answers at once with its id; its output
//! goes to a file (up to [`KEEP_BYTES`]; the rest is read and dropped, so
//! it never blocks), and when it ends the session is told: a notice for
//! the model at its next step, or, when nothing is running, a run of its
//! own (see `wake`).
//!
//! They belong to the session, not to a turn: Stop leaves them running. A
//! new session, the `jobs` tool's stop, their timeout ([`DEFAULT_TIMEOUT`],
//! up to [`MAX_TIMEOUT`]) or the process ending ends them, with everything
//! they started (their process group). Their logs go when the session
//! ends.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::sandbox::Sandbox;

pub const DEFAULT_TIMEOUT: u64 = 30 * 60;
pub const MAX_TIMEOUT: u64 = 2 * 60 * 60;
/// Of a job's output, written to its file at most.
const KEEP_BYTES: u64 = 16 * 1024 * 1024;
/// Of its end, kept in memory for the notice.
const TAIL_BYTES: usize = 4096;
/// Lines of the end the notice shows.
const NOTICE_LINES: usize = 20;
/// Jobs running at once, at most.
const MAX_RUNNING: usize = 8;

/// Process groups of jobs running, to end at exit.
static GROUPS: Mutex<Vec<i32>> = Mutex::new(Vec::new());

/// Ends every job this process started (before it exits).
pub fn stop_all() {
    let groups: Vec<i32> = std::mem::take(&mut *GROUPS.lock().unwrap());
    for g in &groups {
        // SAFETY: signalling process groups this process started.
        unsafe { libc::killpg(*g, libc::SIGKILL) };
    }
}

fn forget(pgid: i32) {
    GROUPS.lock().unwrap().retain(|g| *g != pgid);
}

#[derive(Clone)]
pub struct End {
    pub code: Option<i32>,
    /// "timed out", "stopped", or None (it ended by itself).
    pub why: Option<&'static str>,
    pub after: Duration,
}

pub struct Job {
    pub id: String,
    pub command: String,
    pub log: PathBuf,
    started: Instant,
    pgid: i32,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    ended: Option<End>,
    written: u64,
    tail: VecDeque<u8>,
    /// Stopped by the session (the model asked, or it's ending): no notice.
    quiet: bool,
}

impl Job {
    pub fn ended(&self) -> Option<End> {
        self.state.lock().unwrap().ended.clone()
    }

    fn kill(&self, quiet: bool, why: &'static str) {
        let mut s = self.state.lock().unwrap();
        if s.ended.is_some() {
            return;
        }
        s.quiet |= quiet;
        drop(s);
        STOPPED.lock().unwrap().push((self.pgid, why));
        // SAFETY: the job's own process group.
        unsafe { libc::killpg(self.pgid, libc::SIGTERM) };
        let pgid = self.pgid;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            unsafe { libc::killpg(pgid, libc::SIGKILL) };
        });
    }

    /// One line on it, for `jobs list`.
    fn line(&self) -> String {
        let state = match self.ended() {
            None => format!("running for {}", took(self.started.elapsed())),
            Some(e) => ended(&e),
        };
        format!("{}: {} — {state}. Output: {}", self.id, self.command, self.log.display())
    }
}

/// Why a stopped job's group was signalled (read by its watcher).
static STOPPED: Mutex<Vec<(i32, &'static str)>> = Mutex::new(Vec::new());

/// A duration as people say it: "45 s", "2 min 10 s", "30 min", "1 h",
/// "1 h 5 min" (no zero parts).
fn took(d: Duration) -> String {
    let s = d.as_secs();
    let (big, small) = match s {
        0..=59 => return format!("{s} s"),
        60..=3599 => ((s / 60, "min"), (s % 60, "s")),
        _ => ((s / 3600, "h"), ((s % 3600) / 60, "min")),
    };
    match small.0 {
        0 => format!("{} {}", big.0, big.1),
        n => format!("{} {} {n} {}", big.0, big.1, small.1),
    }
}

fn ended(e: &End) -> String {
    let how = match (e.why, e.code) {
        (Some("timed out"), _) => "stopped: it timed out".to_owned(),
        (Some(w), _) => w.to_owned(),
        (None, Some(c)) => format!("ended, exit code {c}"),
        (None, None) => "ended by a signal".to_owned(),
    };
    format!("{how}, after {}", took(e.after))
}

/// A session's jobs.
pub struct Jobs {
    next: AtomicUsize,
    list: Mutex<Vec<Arc<Job>>>,
    dir: PathBuf,
    /// What ended, for the model (taken at its next step).
    pub notices: Arc<Mutex<Vec<String>>>,
    /// Called when one ends (RPC: starts a run when none is going).
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl Jobs {
    pub fn new(spill_dir: &Path) -> Self {
        let mut b = [0u8; 4];
        let _ = getrandom::fill(&mut b);
        let tag: String = b.iter().map(|x| format!("{x:02x}")).collect();
        Jobs { next: AtomicUsize::new(0), list: Mutex::new(Vec::new()), dir: spill_dir.join(format!("jobs-{}-{tag}", std::process::id())), notices: Arc::default(), wake: Mutex::new(None) }
    }

    pub fn set_wake(&self, f: Arc<dyn Fn() + Send + Sync>) {
        *self.wake.lock().unwrap() = Some(f);
    }

    pub fn running(&self) -> usize {
        self.list.lock().unwrap().iter().filter(|j| j.ended().is_none()).count()
    }

    /// Starts `command`: what to tell the model.
    pub fn start(self: &Arc<Self>, command: &str, cwd: &Path, timeout: Option<u64>, sandbox: Option<&Sandbox>) -> (String, bool) {
        if command.trim().is_empty() {
            return ("Give the command to run.".into(), true);
        }
        if self.running() >= MAX_RUNNING {
            return (format!("{MAX_RUNNING} background commands are running already: wait for one, or stop one with jobs."), true);
        }
        let secs = timeout.filter(|t| *t > 0).unwrap_or(DEFAULT_TIMEOUT).min(MAX_TIMEOUT);
        if let Err(e) = std::fs::create_dir_all(&self.dir) {
            return (format!("Couldn't make the folder for its output: {e}"), true);
        }
        let n = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let id = format!("j{n}");
        let log = self.dir.join(format!("{id}.log"));
        let mut cmd = crate::tools::bash::command(command, cwd, sandbox);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return (format!("Couldn't start {}: {e}", if sandbox.is_some() { "the sandbox (bwrap)" } else { "bash" }), true),
        };
        let pgid = child.id().map_or(0, |p| p as i32);
        GROUPS.lock().unwrap().push(pgid);
        let job = Arc::new(Job { id: id.clone(), command: command.trim().to_owned(), log: log.clone(), started: Instant::now(), pgid, state: Mutex::default() });
        self.list.lock().unwrap().push(job.clone());
        let (mut out, mut err) = (child.stdout.take().expect("piped"), child.stderr.take().expect("piped"));
        let jobs = self.clone();
        tokio::spawn(async move {
            let mut file = tokio::fs::File::create(&job.log).await.ok();
            let keep = |bytes: &[u8], file: &mut Option<tokio::fs::File>| {
                let mut s = job.state.lock().unwrap();
                let room = KEEP_BYTES.saturating_sub(s.written) as usize;
                let take = bytes.len().min(room);
                s.written += take as u64;
                s.tail.extend(bytes);
                let over = s.tail.len().saturating_sub(TAIL_BYTES);
                s.tail.drain(..over);
                (take > 0 && file.is_some()).then(|| bytes[..take].to_vec())
            };
            let collect = async {
                let (mut b1, mut b2) = ([0u8; 8192], [0u8; 8192]);
                let (mut o_open, mut e_open) = (true, true);
                while o_open || e_open {
                    let got = tokio::select! {
                        n = out.read(&mut b1), if o_open => match n { Ok(0) | Err(_) => { o_open = false; None } Ok(n) => Some(b1[..n].to_vec()) },
                        n = err.read(&mut b2), if e_open => match n { Ok(0) | Err(_) => { e_open = false; None } Ok(n) => Some(b2[..n].to_vec()) },
                    };
                    if let Some(bytes) = got
                        && let Some(w) = keep(&bytes, &mut file)
                        && let Some(f) = file.as_mut()
                    {
                        let _ = f.write_all(&w).await;
                    }
                }
                child.wait().await.ok()
            };
            let (status, timed_out) = match tokio::time::timeout(Duration::from_secs(secs), collect).await {
                Ok(s) => (s, false),
                Err(_) => {
                    unsafe { libc::killpg(pgid, libc::SIGTERM) };
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    (None, true)
                }
            };
            // Ended: anything it left goes too.
            unsafe { libc::killpg(pgid, libc::SIGKILL) };
            forget(pgid);
            if let Some(f) = file.as_mut() {
                let _ = f.flush().await;
            }
            let stopped = {
                let mut s = STOPPED.lock().unwrap();
                let why = s.iter().find(|(g, _)| *g == pgid).map(|(_, w)| *w);
                s.retain(|(g, _)| *g != pgid);
                why
            };
            let end = End { code: status.and_then(|s| s.code()), why: if timed_out { Some("timed out") } else { stopped }, after: job.started.elapsed() };
            let (quiet, notice) = {
                let mut s = job.state.lock().unwrap();
                s.ended = Some(end.clone());
                (s.quiet, notice(&job, &end, &s))
            };
            if quiet {
                return;
            }
            jobs.notices.lock().unwrap().push(notice);
            let wake = jobs.wake.lock().unwrap().clone();
            if let Some(w) = wake {
                w();
            }
        });
        (
            format!(
                "Started in the background as {id} (it times out after {}). Its output goes to {}. You'll be told when it ends; meanwhile use jobs to see its output, wait for it or stop it.",
                took(Duration::from_secs(secs)),
                log.display()
            ),
            false,
        )
    }

    fn find(&self, id: &str) -> Option<Arc<Job>> {
        self.list.lock().unwrap().iter().find(|j| j.id == id.trim()).cloned()
    }

    /// The `jobs` tool.
    pub async fn call(&self, args: &serde_json::Value) -> (String, bool) {
        let id = args["id"].as_str().unwrap_or_default();
        let job = || self.find(id).ok_or_else(|| if id.is_empty() { "Give the job's id (j1, j2…).".to_owned() } else { format!("There's no job {id:?}: jobs list shows them.") });
        match args["action"].as_str().unwrap_or("list") {
            "list" => {
                let list = self.list.lock().unwrap();
                if list.is_empty() {
                    return ("No background commands in this session.".into(), false);
                }
                (list.iter().map(|j| j.line()).collect::<Vec<_>>().join("\n"), false)
            }
            "output" => match job() {
                Ok(j) => (self.output(&j, args["lines"].as_u64().unwrap_or(50) as usize), false),
                Err(e) => (e, true),
            },
            "wait" => match job() {
                Ok(j) => {
                    let secs = args["timeout"].as_u64().unwrap_or(60).clamp(1, 600);
                    let until = Instant::now() + Duration::from_secs(secs);
                    while j.ended().is_none() && Instant::now() < until {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                    if j.ended().is_none() {
                        return (format!("{} is still running after {secs} s of waiting.\n{}", j.id, self.output(&j, 20)), false);
                    }
                    // (Told here: no notice later.)
                    j.state.lock().unwrap().quiet = true;
                    self.notices.lock().unwrap().retain(|n| !n.contains(&format!("Background job {} ", j.id)));
                    (self.output(&j, args["lines"].as_u64().unwrap_or(50) as usize), false)
                }
                Err(e) => (e, true),
            },
            "stop" => match job() {
                Ok(j) => {
                    if let Some(e) = j.ended() {
                        return (format!("{} has already {}.", j.id, ended(&e)), false);
                    }
                    j.kill(true, "stopped (you asked)");
                    for _ in 0..40 {
                        if j.ended().is_some() {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    (format!("Stopped {}, with everything it started.\n{}", j.id, self.output(&j, 10)), false)
                }
                Err(e) => (e, true),
            },
            other => (format!("jobs has no action {other:?}: use list, output, wait or stop."), true),
        }
    }

    /// The end of a job's output and how it stands.
    fn output(&self, j: &Job, lines: usize) -> String {
        let text = std::fs::read(&j.log).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        let all: Vec<&str> = text.lines().collect();
        let n = lines.clamp(1, 500);
        let shown = &all[all.len().saturating_sub(n)..];
        let state = match j.ended() {
            None => format!("running for {}", took(j.started.elapsed())),
            Some(e) => ended(&e),
        };
        let more = if all.len() > shown.len() { format!(" (the last {} of {} lines)", shown.len(), all.len()) } else { String::new() };
        let body = if shown.is_empty() { "(no output yet)".to_owned() } else { shown.join("\n") };
        crate::tools::bash::shorten(&format!("{}: {} — {state}. Output{more}, whole in {}:\n{body}", j.id, j.command, j.log.display()), 0, &self.dir)
    }

    /// Ends them all, quietly (a new session, or the session ending), and
    /// removes their logs.
    pub fn end_all(&self) {
        let list: Vec<Arc<Job>> = std::mem::take(&mut *self.list.lock().unwrap());
        for j in &list {
            j.kill(true, "stopped (the session ended)");
        }
        self.notices.lock().unwrap().clear();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        self.end_all();
    }
}

/// What the model's told when a job ends.
fn notice(j: &Job, e: &End, s: &State) -> String {
    let tail = String::from_utf8_lossy(&s.tail.iter().copied().collect::<Vec<u8>>()).into_owned();
    let lines: Vec<&str> = tail.lines().collect();
    let last = &lines[lines.len().saturating_sub(NOTICE_LINES)..];
    let cut = if s.written >= KEEP_BYTES { format!(" (only its first {} MB were kept)", KEEP_BYTES / 1024 / 1024) } else { String::new() };
    let body = if last.is_empty() { "(no output)".to_owned() } else { last.join("\n") };
    format!(
        "Background job {} {}.\nCommand: {}\nIts last lines:\n{body}\nWhole output: {}{cut} (read or grep it, or jobs output).",
        j.id,
        ended(e),
        j.command,
        j.log.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_as_said() {
        let t = |s| took(Duration::from_secs(s));
        assert_eq!([t(45), t(60), t(130), t(1800), t(3600), t(3900), t(7200)], ["45 s", "1 min", "2 min 10 s", "30 min", "1 h", "1 h 5 min", "2 h"]);
    }

    #[tokio::test]
    async fn a_job_runs_on_and_says_when_it_ends() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-jobs-{}", std::process::id()));
        let jobs = Arc::new(Jobs::new(&dir));
        let woken = Arc::new(AtomicUsize::new(0));
        let w = woken.clone();
        jobs.set_wake(Arc::new(move || {
            w.fetch_add(1, Ordering::Relaxed);
        }));
        let (text, failed) = jobs.start("echo one; sleep 0.3; echo two; exit 3", &std::env::temp_dir(), None, None);
        assert!(!failed && text.starts_with("Started in the background as j1"), "{text}");
        let (out, _) = jobs.call(&serde_json::json!({ "action": "wait", "id": "j1", "timeout": 10 })).await;
        assert!(out.contains("exit code 3") && out.contains("one\ntwo"), "{out}");
        // Waited for: told there, no notice too.
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(jobs.notices.lock().unwrap().is_empty());
        jobs.start("echo three", &std::env::temp_dir(), None, None);
        for _ in 0..50 {
            if !jobs.notices.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let notices = jobs.notices.lock().unwrap().clone();
        assert!(notices.len() == 1 && notices[0].starts_with("Background job j2 ended, exit code 0") && notices[0].contains("three"), "{notices:?}");
        // (j1's end woke it too, before wait took its notice back: a wake
        // that finds nothing does nothing.)
        assert_eq!(woken.load(Ordering::Relaxed), 2);
        let (list, _) = jobs.call(&serde_json::json!({ "action": "list" })).await;
        assert!(list.contains("j1: echo one") && list.contains("j2: echo three"), "{list}");
        jobs.end_all();
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn stopping_one_ends_what_it_started() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-jobs-stop-{}", std::process::id()));
        let jobs = Arc::new(Jobs::new(&dir));
        let marker = std::env::temp_dir().join(format!("harlo-agent-job-marker-{}", std::process::id()));
        jobs.start(&format!("(sleep 2; touch {}) & sleep 30", marker.display()), &std::env::temp_dir(), None, None);
        tokio::time::sleep(Duration::from_millis(200)).await;
        let (out, _) = jobs.call(&serde_json::json!({ "action": "stop", "id": "j1" })).await;
        assert!(out.starts_with("Stopped j1"), "{out}");
        tokio::time::sleep(Duration::from_millis(2500)).await;
        assert!(!marker.exists(), "its child was left running");
        assert!(jobs.notices.lock().unwrap().is_empty(), "a job the model stopped isn't announced");
        let (out, failed) = jobs.call(&serde_json::json!({ "action": "output", "id": "j9" })).await;
        assert!(failed && out.contains("no job"), "{out}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
