//! Commands, in bash. Nothing can wait for input: stdin is closed, and
//! pagers, editors and git's prompts are turned off. Each command runs in
//! its own process group, so a timeout or a stop ends it and everything
//! it started. Long output keeps its start and its end; the whole of it
//! goes to a file the model can read.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::sandbox::Sandbox;

/// Shown to the model at most: lines and bytes (start and end together).
/// Small, because what's shown stays in every later request; the whole
/// output is in a file to grep or read.
pub const MAX_LINES: usize = 600;
pub const MAX_BYTES: usize = 16 * 1024;
const DEFAULT_TIMEOUT: u64 = 120;
/// Kept in memory at most (the rest still goes to the file).
const KEEP_BYTES: usize = 16 * 1024 * 1024;

const ENV: &[(&str, &str)] = &[
    ("PAGER", "cat"),
    ("GIT_PAGER", "cat"),
    ("GH_PAGER", "cat"),
    ("MANPAGER", "cat"),
    ("SYSTEMD_PAGER", ""),
    ("GIT_EDITOR", "false"),
    ("EDITOR", "false"),
    ("VISUAL", "false"),
    ("GIT_TERMINAL_PROMPT", "0"),
    ("GIT_SEQUENCE_EDITOR", "false"),
    ("DEBIAN_FRONTEND", "noninteractive"),
    ("NO_COLOR", "1"),
    ("TERM", "dumb"),
    ("HARLO_AGENT", "1"),
];

/// Ends the process group when dropped (a stopped turn drops the future).
struct Group(Option<i32>);

impl Group {
    fn kill(&mut self, signal: i32) {
        if let Some(pgid) = self.0 {
            // SAFETY: kill(2) on a process group this command created.
            unsafe {
                libc::kill(-pgid, signal);
            }
        }
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        self.kill(libc::SIGKILL);
    }
}

/// `sandbox`: run it in that (None: as it is, with all of Harlo's
/// variables).
pub async fn run(command: &str, cwd: &Path, timeout: Option<u64>, spill_dir: &Path, sandbox: Option<&Sandbox>) -> (String, bool) {
    let secs = timeout.filter(|t| *t > 0).unwrap_or(DEFAULT_TIMEOUT);
    let mut cmd = match sandbox {
        Some(sb) => {
            let mut c = Command::new("bwrap");
            c.args(sb.args(command, cwd)).env_clear().envs(sb.env());
            c
        }
        None => {
            let mut c = Command::new("bash");
            c.arg("-c").arg(command);
            c
        }
    };
    // (The process group reaches bwrap; what it runs ends with it.)
    cmd.current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
    for (k, v) in ENV {
        cmd.env(k, v);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (format!("Couldn't start {}: {e}", if sandbox.is_some() { "the sandbox (bwrap)" } else { "bash" }), true),
    };
    let mut group = Group(child.id().map(|p| p as i32));
    let (mut out, mut err) = (child.stdout.take().expect("piped"), child.stderr.take().expect("piped"));
    // Both streams into one, as they come.
    let collect = async {
        let mut all: Vec<u8> = Vec::new();
        let mut total = 0usize;
        let (mut b1, mut b2) = ([0u8; 8192], [0u8; 8192]);
        let (mut o_open, mut e_open) = (true, true);
        while o_open || e_open {
            tokio::select! {
                n = out.read(&mut b1), if o_open => match n { Ok(0) | Err(_) => o_open = false, Ok(n) => { total += n; if all.len() < KEEP_BYTES { all.extend_from_slice(&b1[..n]); } } },
                n = err.read(&mut b2), if e_open => match n { Ok(0) | Err(_) => e_open = false, Ok(n) => { total += n; if all.len() < KEEP_BYTES { all.extend_from_slice(&b2[..n]); } } },
            }
        }
        (all, total)
    };
    let work = async {
        let (bytes, total) = collect.await;
        let status = child.wait().await;
        (bytes, total, status)
    };
    let (bytes, total, status, timed_out) = match tokio::time::timeout(Duration::from_secs(secs), work).await {
        Ok((b, t, s)) => (b, t, s.ok(), false),
        Err(_) => {
            group.kill(libc::SIGTERM);
            tokio::time::sleep(Duration::from_millis(1500)).await;
            group.kill(libc::SIGKILL);
            (Vec::new(), 0, None, true)
        }
    };
    // Finished: anything it left in the background goes too.
    group.kill(libc::SIGKILL);
    group.0 = None;
    if timed_out {
        return (format!("The command was stopped after {secs} seconds, with everything it started. If it needs longer and isn't waiting for input, run it again with a larger timeout."), true);
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let mut shown = shorten(&text, total, spill_dir);
    let code = status.and_then(|s| s.code());
    let failed = code != Some(0);
    match code {
        Some(0) => {}
        Some(c) => shown.push_str(&format!("\n[exit code {c}]")),
        None => shown.push_str("\n[ended by a signal]"),
    }
    if shown.trim().is_empty() {
        shown = "(no output)".into();
    }
    (shown, failed)
}

/// Long output: the start and the end, with notices at the top and at the
/// cut, and the whole output in a file.
pub fn shorten(text: &str, total: usize, spill_dir: &Path) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= MAX_LINES && text.len() <= MAX_BYTES && total == text.len() {
        return text.trim_end_matches('\n').to_owned();
    }
    let file = save(text, spill_dir);
    // A quarter for the start, the rest for the end (errors are usually last).
    let (head_lines, head_bytes) = (MAX_LINES / 4, MAX_BYTES / 4);
    let (tail_lines, tail_bytes) = (MAX_LINES - head_lines, MAX_BYTES - head_bytes);
    let mut head = Vec::new();
    let mut size = 0;
    for l in &lines {
        if head.len() >= head_lines || size + l.len() + 1 > head_bytes {
            break;
        }
        size += l.len() + 1;
        head.push(*l);
    }
    let mut tail = Vec::new();
    size = 0;
    for l in lines.iter().skip(head.len()).rev() {
        if tail.len() >= tail_lines || size + l.len() + 1 > tail_bytes {
            break;
        }
        size += l.len() + 1;
        tail.push(*l);
    }
    tail.reverse();
    let cut = lines.len() - head.len() - tail.len();
    let saved = match &file {
        Some(p) => format!("The whole output is in {p} (read it with offset and limit, or grep it)."),
        None => "The whole output couldn't be saved.".into(),
    };
    let more = if total > text.len() { format!(" (and {} more bytes not kept)", total - text.len()) } else { String::new() };
    format!(
        "[Output shortened: {} lines{more}; showing the first {} and the last {}. {saved}]\n{}\n[… {cut} lines cut here …]\n{}",
        lines.len(),
        head.len(),
        tail.len(),
        head.join("\n"),
        tail.join("\n")
    )
}

fn save(text: &str, dir: &Path) -> Option<String> {
    std::fs::create_dir_all(dir).ok()?;
    let mut b = [0u8; 6];
    getrandom::fill(&mut b).ok()?;
    let name: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let path = dir.join(format!("output-{name}.log"));
    std::fs::write(&path, text).ok()?;
    Some(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_without_waiting_for_input() {
        let dir = std::env::temp_dir();
        let (out, failed) = run("echo hi; echo err >&2; echo \"$PAGER $GIT_EDITOR $GIT_TERMINAL_PROMPT\"", &dir, None, &dir, None).await;
        assert!(!failed && out.contains("hi") && out.contains("err") && out.contains("cat false 0"), "{out}");
        // Reading stdin gets end-of-input, not a hang.
        let (out, _) = run("read x; echo \"got:$x\"", &dir, Some(5), &dir, None).await;
        assert!(out.contains("got:"), "{out}");
        let (out, failed) = run("exit 3", &dir, None, &dir, None).await;
        assert!(failed && out.contains("[exit code 3]"));
    }

    #[tokio::test]
    async fn a_timeout_ends_the_whole_group() {
        let dir = std::env::temp_dir();
        let marker = dir.join(format!("harlo-agent-bg-{}", std::process::id()));
        let cmd = format!("(sleep 3; touch {}) & sleep 30", marker.display());
        let (out, failed) = run(&cmd, &dir, Some(1), &dir, None).await;
        assert!(failed && out.contains("stopped after 1 seconds"), "{out}");
        tokio::time::sleep(Duration::from_secs(4)).await;
        assert!(!marker.exists(), "the background child was left running");
    }

    /// In the sandbox (where bubblewrap works here): what it can change and
    /// see, and a timeout still ends what it started.
    #[tokio::test]
    async fn in_the_sandbox() {
        if crate::sandbox::unavailable().is_some() {
            eprintln!("no sandbox here: skipped");
            return;
        }
        let scratch = crate::sandbox::tests::home_outside_tmp("bash");
        let root = scratch.0.clone();
        let (home, proj) = (root.join("home"), root.join("home/proj"));
        for d in [&proj, &home.join(".ssh"), &home.join("other")] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(home.join(".ssh/id"), "secret-key").unwrap();
        let p = crate::sandbox::Policy { on: true, network: false, writable: Vec::new(), hidden: Vec::new(), env: Vec::new() };
        let sb = Sandbox::new(&p, &proj, &home, &home.join("none"), None);
        let (out, failed) = run("echo a > made && cat made && echo t > /tmp/harlo-agent-sb-t && rm /tmp/harlo-agent-sb-t", &proj, None, &root, Some(&sb)).await;
        assert!(!failed && out.contains('a'), "{out}");
        let (out, failed) = run("echo x > ../other/f", &proj, None, &root, Some(&sb)).await;
        assert!(failed && out.contains("Read-only"), "{out}");
        let (out, _) = run("cat ../.ssh/id; ls -A ../.ssh | wc -l", &proj, None, &root, Some(&sb)).await;
        assert!(!out.contains("secret-key") && out.ends_with('0'), "{out}");
        // SAFETY: a variable for this test only (tests in this binary don't read it).
        unsafe { std::env::set_var("HARLO_TEST_API_KEY", "k") };
        let (out, _) = run("echo \"[$HARLO_TEST_API_KEY][$PAGER]\"", &proj, None, &root, Some(&sb)).await;
        assert!(out.contains("[][cat]"), "{out}");
        let marker = std::env::temp_dir().join(format!("harlo-agent-sb-bg-{}", std::process::id()));
        let (out, failed) = run(&format!("(sleep 3; touch {}) & sleep 30", marker.display()), &proj, Some(1), &root, Some(&sb)).await;
        assert!(failed && out.contains("stopped after 1 seconds"), "{out}");
        tokio::time::sleep(Duration::from_secs(4)).await;
        assert!(!marker.exists(), "the background child was left running");
    }

    #[test]
    fn long_output_keeps_both_ends() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-spill-{}", std::process::id()));
        let text: String = (1..=5000).map(|i| format!("line {i}\n")).collect();
        let out = shorten(&text, text.len(), &dir);
        assert!(out.starts_with("[Output shortened: 5000 lines") && out.contains("\nline 1\n") && out.ends_with("line 5000") && out.contains("lines cut here"), "{}", &out[..200]);
        let path = out.split("The whole output is in ").nth(1).unwrap().split(' ').next().unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), text);
        let _ = std::fs::remove_dir_all(dir);
    }
}
