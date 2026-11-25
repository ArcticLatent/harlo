//! The `shell` tool: one command, without a terminal or input.
//!
//! The command runs in its own process group, so stopping it ends
//! everything it started. Pagers and editors are turned off so nothing
//! waits for a person. Long output keeps its start and end, while it is
//! read as well as when it is shown: a command that prints without end
//! takes a fixed amount of memory. What a command printed before it was
//! stopped or ran out of time is kept.
//!
//! A command is a [`Running`] from its start to its end. [`execute`] waits
//! for one and ends it at its time limit; `run_within` hands one still
//! going at its limit, or one a person asked to stop waiting for, back to
//! the caller, which may let it carry on as a background job instead of
//! losing the work it did.

// Rust guideline compliant 2026-10-09

use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Map, Value, json};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::Output;

/// Lines the model sees of a long output.
const MAX_LINES: usize = 600;
/// Bytes the model sees of a long output.
const MAX_BYTES: usize = 16 * 1024;
/// Characters the model sees of one long line: enough for any line of
/// code, a fraction of a minified file's or a data dump's.
const MAX_LINE: usize = 2_000;
/// Bytes kept of the start, and of the end, of each of a command's two
/// streams while it runs. More than any reader of the output shows, and
/// a fixed cost for a command that never stops printing.
const KEPT_BYTES: usize = 1024 * 1024;
/// How many of a command's last lines show where it has got to, when it
/// carries on in the background: its whole output comes when it ends.
const LATEST_LINES: usize = 15;

/// The most characters of each of those lines.
const LATEST_LINE: usize = 200;

/// How long a stopped command gets to end before it's killed.
const GRACE: Duration = Duration::from_secs(2);

/// How a command ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    /// It exited by itself, with its code (none when a signal ended it).
    Exited(Option<i32>),
    /// It ran past its time and was stopped.
    TimedOut,
    /// It was stopped.
    Stopped,
    /// It couldn't be started, with why.
    NotStarted(String),
}

/// A command's run: what it printed, standard error after standard output,
/// and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    /// What it printed, up to when it ended or was stopped. Of a stream
    /// longer than a couple of megabytes, the start and the end, with a
    /// line between them saying how much was left out.
    pub output: String,
    /// How it ended.
    pub ended: Ended,
}

/// A command that was started, and may still be going.
///
/// Dropping it kills the command.
#[derive(Debug)]
pub struct Running {
    child: Child,
    /// Its process group, which holds everything it started.
    group: Option<i32>,
    out: Arc<Mutex<Kept>>,
    err: Arc<Mutex<Kept>>,
    /// The task reading what it prints, until its pipes close.
    readers: Option<JoinHandle<()>>,
}

impl Running {
    /// Starts `command` with `bash` in `cwd`.
    ///
    /// # Errors
    ///
    /// With why, when `bash` couldn't be started.
    pub fn start(command: &str, cwd: &Path) -> Result<Self, String> {
        let mut child = spawn(command, cwd).map_err(|e| format!("Couldn't start bash: {e}"))?;
        let group = child.id().and_then(|id| i32::try_from(id).ok());
        let (out, err) = (Arc::default(), Arc::default());
        let (out_pipe, err_pipe) = (child.stdout.take(), child.stderr.take());
        let (out_kept, err_kept) = (Arc::clone(&out), Arc::clone(&err));
        let readers = tokio::spawn(async move {
            tokio::join!(
                read_into(out_pipe, &out_kept),
                read_into(err_pipe, &err_kept)
            );
        });
        Ok(Self {
            child,
            group,
            out,
            err,
            readers: Some(readers),
        })
    }

    /// Waits for the command to end, for at most `limit`.
    ///
    /// Returns how it ended, or `None` when `limit` passed first: the
    /// command is then still going, and can be waited for again. A
    /// command `stop` cancels is ended, with everything it started.
    pub async fn wait(
        &mut self,
        limit: Option<Duration>,
        stop: &CancellationToken,
    ) -> Option<Ended> {
        let ended = {
            let (readers, child) = (&mut self.readers, &mut self.child);
            let waiting = async {
                if let Some(reading) = readers.as_mut() {
                    // (A reader that failed just ends what we show.)
                    let _ = reading.await;
                    *readers = None;
                }
                child.wait().await
            };
            let limit = async {
                match limit {
                    Some(t) => tokio::time::sleep(t).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                status = waiting => Some(Ended::Exited(status.ok().and_then(|s| s.code()))),
                () = limit => None,
                () = stop.cancelled() => Some(Ended::Stopped),
            }
        };
        if ended == Some(Ended::Stopped) {
            self.end().await;
        }
        ended
    }

    /// Ends the command and everything it started: asks first, then kills.
    pub async fn end(&mut self) {
        end_group(self.group).await;
    }

    /// What it printed so far, standard error after standard output.
    #[must_use]
    pub fn printed(&self) -> String {
        let text = |kept: &Mutex<Kept>| kept.lock().unwrap_or_else(PoisonError::into_inner).text();
        let (mut output, stderr) = (text(&self.out), text(&self.err));
        if !stderr.is_empty() {
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(&stderr);
        }
        output
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(readers) = &self.readers {
            readers.abort();
        }
    }
}

/// Runs `command` with `bash` in `cwd` until it ends, `timeout` passes or
/// `stop` is cancelled; a stopped command's process group is ended.
pub async fn execute(
    command: &str,
    cwd: &Path,
    timeout: Option<Duration>,
    stop: &CancellationToken,
) -> Ran {
    let mut running = match Running::start(command, cwd) {
        Ok(running) => running,
        Err(why) => {
            return Ran {
                output: String::new(),
                ended: Ended::NotStarted(why),
            };
        }
    };
    let ended = if let Some(ended) = running.wait(timeout, stop).await {
        ended
    } else {
        running.end().await;
        Ended::TimedOut
    };
    // What was read so far: all of it, for a command that ended by itself.
    Ran {
        output: running.printed(),
        ended,
    }
}

/// Starts `command` with `bash` in `cwd`, in a process group of its own.
fn spawn(command: &str, cwd: &Path) -> std::io::Result<Child> {
    let mut cmd = Command::new("bash");
    cmd.arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true);
    for (key, value) in [
        ("PAGER", "cat"),
        ("GIT_PAGER", "cat"),
        ("GH_PAGER", "cat"),
        ("MANPAGER", "cat"),
        ("SYSTEMD_PAGER", ""),
        ("GIT_EDITOR", "false"),
        ("GIT_SEQUENCE_EDITOR", "false"),
        ("GIT_TERMINAL_PROMPT", "0"),
    ] {
        cmd.env(key, value);
    }
    cmd.spawn()
}

/// The start and the end of what a stream gave, and how much it gave.
#[derive(Debug, Default)]
struct Kept {
    head: Vec<u8>,
    tail: VecDeque<u8>,
    total: usize,
}

impl Kept {
    /// Takes the stream's next bytes.
    fn push(&mut self, mut bytes: &[u8]) {
        self.total += bytes.len();
        let room = KEPT_BYTES - self.head.len();
        let (first, rest) = bytes.split_at(room.min(bytes.len()));
        self.head.extend_from_slice(first);
        bytes = rest;
        // Of the rest, the last `KEPT_BYTES` stay.
        if bytes.len() >= KEPT_BYTES {
            self.tail.clear();
            bytes = &bytes[bytes.len() - KEPT_BYTES..];
        }
        let over = (self.tail.len() + bytes.len()).saturating_sub(KEPT_BYTES);
        self.tail.drain(..over);
        self.tail.extend(bytes);
    }

    /// What was kept, as text, saying how much was left out between.
    fn text(&self) -> String {
        let mut text = String::from_utf8_lossy(&self.head).into_owned();
        let left_out = self.total - self.head.len() - self.tail.len();
        if left_out > 0 {
            if !text.ends_with('\n') {
                text.push('\n');
            }
            let _ = writeln!(text, "… ({left_out} bytes left out) …");
        }
        let (a, b) = self.tail.as_slices();
        text.push_str(&String::from_utf8_lossy(&[a, b].concat()));
        text
    }
}

/// Reads a pipe to its end into `kept`.
async fn read_into(pipe: Option<impl AsyncRead + Unpin + Send>, kept: &Mutex<Kept>) {
    let Some(mut pipe) = pipe else { return };
    let mut chunk = vec![0; 64 * 1024];
    // (A read error just ends what we show.)
    while let Ok(read @ 1..) = pipe.read(&mut chunk).await {
        kept.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(&chunk[..read]);
    }
}

/// Runs `command` in `cwd` for the model; never fails, since what went
/// wrong is output the model should read.
pub(super) async fn run(
    command: &str,
    cwd: &Path,
    timeout_ms: u64,
    stop: &CancellationToken,
) -> Output {
    let ran = execute(command, cwd, Some(Duration::from_millis(timeout_ms)), stop).await;
    shown(&ran, timeout_ms)
}

/// A command run for the model: ended, or still going.
#[derive(Debug)]
pub(crate) enum Within {
    /// It ended, or couldn't start: what the model reads of it.
    Ended(Output),
    /// It was still going when its time was up, and wasn't stopped.
    OutOfTime(Box<Running>),
    /// It was still going when a person asked not to wait for it.
    Asked(Box<Running>),
}

/// Runs `command` in `cwd` for the model, until `limit` passes or `asked`
/// resolves.
///
/// Like [`run`], except that a command still going then is handed back
/// as it is instead of being stopped.
pub(crate) async fn run_within(
    command: &str,
    cwd: &Path,
    limit: Duration,
    stop: &CancellationToken,
    asked: impl Future<Output = ()>,
) -> Within {
    let mut running = match Running::start(command, cwd) {
        Ok(running) => running,
        Err(why) => return Within::Ended(said(why, None)),
    };
    let waited = tokio::select! {
        ended = running.wait(Some(limit), stop) => Some(ended),
        () = asked => None,
    };
    match waited {
        Some(Some(ended)) => {
            let ran = Ran {
                output: running.printed(),
                ended,
            };
            Within::Ended(shown(&ran, 0))
        }
        Some(None) => Within::OutOfTime(Box::new(running)),
        None => Within::Asked(Box::new(running)),
    }
}

/// Stops `running`, which had `timeout_ms` to run and outran them; what
/// the model reads of it.
pub(crate) async fn out_of_time(mut running: Running, timeout_ms: u64) -> Output {
    running.end().await;
    let ran = Ran {
        output: running.printed(),
        ended: Ended::TimedOut,
    };
    shown(&ran, timeout_ms)
}

/// The last of `text`, for a look at where a command has got to: its
/// last [`LATEST_LINES`] lines, long ones cut short.
#[must_use]
pub(crate) fn latest(text: &str) -> String {
    let lines: Vec<&str> = text.trim_end().lines().collect();
    let from = lines.len().saturating_sub(LATEST_LINES);
    let mut out = String::new();
    if from > 0 {
        let _ = writeln!(out, "… ({from} lines before these) …");
    }
    for line in &lines[from..] {
        let line: String = line.chars().take(LATEST_LINE).collect();
        let _ = writeln!(out, "{line}");
    }
    out.trim_end().to_owned()
}

/// What the model reads of `ran`, a command that had `timeout_ms` to run.
fn shown(ran: &Ran, timeout_ms: u64) -> Output {
    // What it printed is shown however it ended: a build that ran out of
    // time got somewhere first.
    let printed = |ending: String| {
        let mut shown = capped(&ran.output);
        if !shown.is_empty() {
            shown.push('\n');
        }
        shown.push_str(&ending);
        shown
    };
    match &ran.ended {
        Ended::Exited(code) => {
            let mut shown = capped(&ran.output);
            if shown.is_empty() {
                shown.push_str("(no output)");
            }
            if let Some(code) = code.filter(|c| *c != 0) {
                let _ = write!(shown, "\n(exit code {code})");
            }
            said(shown, *code)
        }
        Ended::TimedOut => said(
            printed(format!(
                "Stopped after {} s: it took too long.",
                timeout_ms / 1000
            )),
            None,
        ),
        Ended::Stopped => said(printed("Stopped.".to_owned()), None),
        Ended::NotStarted(why) => said(why.clone(), None),
    }
}

/// Ends a process group: asks first, then kills.
async fn end_group(group: Option<i32>) {
    let Some(pid) = group else { return };
    // SAFETY: `kill` takes plain integers; a negative pid names the process
    // group the command was started in, which only holds its processes.
    unsafe { libc::kill(-pid, libc::SIGTERM) };
    tokio::time::sleep(GRACE).await;
    // SAFETY: as above.
    unsafe { libc::kill(-pid, libc::SIGKILL) };
}

/// `line`, cut to [`MAX_LINE`] characters with a note of what was left out.
pub(super) fn short_line(line: &str) -> Cow<'_, str> {
    // (Bytes first: no shorter line has more characters than bytes.)
    if line.len() <= MAX_LINE {
        return Cow::Borrowed(line);
    }
    let count = line.chars().count();
    if count <= MAX_LINE {
        return Cow::Borrowed(line);
    }
    let start: String = line.chars().take(MAX_LINE).collect();
    Cow::Owned(format!(
        "{start}… ({} more characters on this line)",
        count - MAX_LINE
    ))
}

/// `text` as a model reads a command's output: within the line and byte
/// caps, keeping its start and end; long lines are cut short first.
#[must_use]
pub fn capped(text: &str) -> String {
    let lines: Vec<Cow<'_, str>> = text.lines().map(short_line).collect();
    let bytes: usize = lines.iter().map(|l| l.len() + 1).sum();
    if lines.len() <= MAX_LINES && bytes <= MAX_BYTES {
        return lines.join("\n").trim_end().to_owned();
    }
    let half_lines = MAX_LINES / 2;
    let half_bytes = MAX_BYTES / 2;
    let mut head = String::new();
    let mut first = 0;
    for line in lines.iter().take(half_lines) {
        if head.len() + line.len() + 1 > half_bytes {
            break;
        }
        head.push_str(line);
        head.push('\n');
        first += 1;
    }
    let mut tail: Vec<&str> = Vec::new();
    let mut size = 0;
    for line in lines[first..].iter().rev().take(half_lines) {
        if size + line.len() + 1 > half_bytes {
            break;
        }
        size += line.len() + 1;
        tail.push(line);
    }
    tail.reverse();
    let left_out = lines.len() - first - tail.len();
    if left_out == 0 {
        return format!("{head}{}", tail.join("\n"));
    }
    format!("{head}… ({left_out} lines left out) …\n{}", tail.join("\n"))
}

fn said(text: String, code: Option<i32>) -> Output {
    let mut metadata = Map::new();
    metadata.insert("output".to_owned(), json!(text));
    metadata.insert("exit".to_owned(), code.map_or(Value::Null, |c| json!(c)));
    Output {
        text,
        metadata,
        changed: Vec::new(),
        pictures: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn output_exit_codes_and_timeouts() {
        let dir = tempfile::tempdir().expect("dir");
        let stop = CancellationToken::new();
        let out = run("echo hi; echo oops >&2; exit 3", dir.path(), 10_000, &stop).await;
        assert_eq!(out.text, "hi\noops\n(exit code 3)");
        let slow = run("sleep 30", dir.path(), 100, &stop).await;
        assert!(slow.text.starts_with("Stopped after"), "{}", slow.text);
    }

    #[tokio::test]
    async fn a_command_out_of_time_keeps_what_it_printed() {
        let dir = tempfile::tempdir().expect("dir");
        let stop = CancellationToken::new();
        let slow = run("echo compiled 3 of 9; sleep 30", dir.path(), 300, &stop).await;
        assert_eq!(
            slow.text,
            "compiled 3 of 9\nStopped after 0 s: it took too long."
        );
        // Stopped by the person, too.
        let stopping = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            stopping.cancel();
        });
        let stopped = run("echo halfway; sleep 30", dir.path(), 60_000, &stop).await;
        assert_eq!(stopped.text, "halfway\nStopped.");
    }

    #[tokio::test]
    async fn endless_output_takes_a_fixed_amount_of_memory() {
        let dir = tempfile::tempdir().expect("dir");
        let stop = CancellationToken::new();
        // 20 MB of numbered lines, then one last line.
        let command = "seq 1 2600000; echo the end";
        let ran = execute(command, dir.path(), Some(Duration::from_secs(60)), &stop).await;
        assert_eq!(ran.ended, Ended::Exited(Some(0)));
        assert!(
            ran.output.len() < 2 * KEPT_BYTES + 100,
            "{}",
            ran.output.len()
        );
        assert!(ran.output.starts_with("1\n2\n3\n"));
        assert!(ran.output.ends_with("2600000\nthe end\n"));
        assert!(ran.output.contains(" bytes left out) …\n"));

        let shown = run(command, dir.path(), 60_000, &stop).await.text;
        assert!(shown.len() <= MAX_BYTES + 100, "{}", shown.len());
        assert!(
            shown.starts_with("1\n") && shown.ends_with("the end"),
            "{shown}"
        );
    }

    #[test]
    fn what_is_kept_of_a_stream_is_its_start_and_end() {
        let mut kept = Kept::default();
        kept.push(b"abc");
        kept.push(b"def");
        assert_eq!((kept.text().as_str(), kept.total), ("abcdef", 6));
        // Past the start's share, only the newest bytes stay.
        let mut long = Kept::default();
        long.push(&vec![b'a'; KEPT_BYTES]);
        long.push(&vec![b'b'; KEPT_BYTES + 10]);
        long.push(b"cd");
        assert_eq!(long.tail.len(), KEPT_BYTES);
        assert_eq!(long.total, 2 * KEPT_BYTES + 12);
        let text = long.text();
        assert!(text.contains("\n… (12 bytes left out) …\n"), "no note");
        assert!(text.starts_with("aaa") && text.ends_with("bbcd"));
    }

    #[test]
    fn one_very_long_line_is_cut_not_dropped() {
        let line = "x".repeat(50_000);
        let shown = capped(&format!("before\n{line}\nafter\n"));
        assert!(shown.starts_with("before\nxxx"), "{}", &shown[..40]);
        assert!(shown.contains("… (48000 more characters on this line)"));
        assert!(shown.ends_with("\nafter"));
        assert!(shown.len() < 2 * MAX_LINE + 200);
        assert_eq!(short_line("short"), "short");
    }

    #[test]
    fn long_output_keeps_its_start_and_end() {
        let text: String = (0..2000)
            .map(|i| format!("line {i}\n"))
            .collect::<Vec<_>>()
            .concat();
        let shown = capped(&text);
        assert!(
            shown.starts_with("line 0\n") && shown.ends_with("line 1999"),
            "{shown}"
        );
        assert!(shown.contains("lines left out"));
        assert!(shown.len() <= MAX_BYTES + 100);
    }
}
