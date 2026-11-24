//! The `shell` tool: one command, without a terminal or input.
//!
//! The command runs in its own process group, so stopping it ends
//! everything it started. Pagers and editors are turned off so nothing
//! waits for a person. Long output keeps its start and end.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Map, Value, json};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use super::Output;

/// Lines the model sees of a long output.
const MAX_LINES: usize = 600;
/// Bytes the model sees of a long output.
const MAX_BYTES: usize = 16 * 1024;
/// How long a stopped command gets to end before it's killed.
const GRACE: Duration = Duration::from_secs(2);

/// Runs `command` in `cwd`; never fails, since what went wrong is output
/// the model should read.
pub(super) async fn run(
    command: &str,
    cwd: &Path,
    timeout_ms: u64,
    stop: &CancellationToken,
) -> Output {
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
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => return said(format!("Couldn't start bash: {e}"), None),
    };
    let group = child.id().and_then(|id| i32::try_from(id).ok());
    let (out, err) = (child.stdout.take(), child.stderr.take());
    let reading = async { tokio::join!(read_all(out), read_all(err)) };
    let waiting = async {
        let (out, err) = reading.await;
        (child.wait().await, out, err)
    };
    tokio::pin!(waiting);
    let ended = tokio::select! {
        r = &mut waiting => Ok(r),
        () = tokio::time::sleep(Duration::from_millis(timeout_ms)) => Err(format!("Stopped after {} s: it took too long.", timeout_ms / 1000)),
        () = stop.cancelled() => Err("Stopped.".to_owned()),
    };
    match ended {
        Ok((status, stdout, stderr)) => {
            let code = status.ok().and_then(|s| s.code());
            let mut text = stdout;
            if !stderr.is_empty() {
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&stderr);
            }
            let mut shown = capped(&text);
            if shown.is_empty() {
                shown.push_str("(no output)");
            }
            if let Some(code) = code.filter(|c| *c != 0) {
                let _ = write!(shown, "\n(exit code {code})");
            }
            said(shown, code)
        }
        Err(why) => {
            end_group(group).await;
            said(why, None)
        }
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

/// Everything a pipe gives, as text.
async fn read_all(pipe: Option<impl AsyncRead + Unpin>) -> String {
    let mut bytes = Vec::new();
    if let Some(mut p) = pipe {
        // (A read error just ends what we show.)
        let _ = p.read_to_end(&mut bytes).await;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// `text` within the line and byte caps, keeping its start and end.
fn capped(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= MAX_LINES && text.len() <= MAX_BYTES {
        return text.trim_end().to_owned();
    }
    let half_lines = MAX_LINES / 2;
    let half_bytes = MAX_BYTES / 2;
    let mut head = String::new();
    for line in lines.iter().take(half_lines) {
        if head.len() + line.len() + 1 > half_bytes {
            break;
        }
        head.push_str(line);
        head.push('\n');
    }
    let mut tail: Vec<&str> = Vec::new();
    let mut size = 0;
    for line in lines.iter().rev().take(half_lines) {
        if size + line.len() + 1 > half_bytes {
            break;
        }
        size += line.len() + 1;
        tail.push(line);
    }
    tail.reverse();
    let left_out = lines
        .len()
        .saturating_sub(head.lines().count() + tail.len());
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
