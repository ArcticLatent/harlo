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
/// Kept in memory at most; past that the output streams to its file, with
/// its last TAIL_BYTES kept for showing the end.
const KEEP_BYTES: usize = 16 * 1024 * 1024;
const TAIL_BYTES: usize = 256 * 1024;

/// A command's output as it comes: all of it while it's small; past
/// KEEP_BYTES its start, the whole of it in a file, and its end.
struct Collected {
    all: Vec<u8>,
    total: usize,
    lines: usize,
    spill: Option<(std::fs::File, String)>,
    tail: std::collections::VecDeque<u8>,
    /// The file couldn't be made: what's past `all` is lost.
    lost: bool,
    /// Writing the file failed part way (a full disk): it has only the start.
    broke: bool,
}

impl Collected {
    fn new() -> Self {
        Collected { all: Vec::new(), total: 0, lines: 0, spill: None, tail: std::collections::VecDeque::new(), lost: false, broke: false }
    }

    fn take(&mut self, b: &[u8], dir: &Path) {
        use std::io::Write;
        self.total += b.len();
        self.lines += b.iter().filter(|c| **c == b'\n').count();
        if self.spill.is_none() && !self.lost && self.all.len() + b.len() <= KEEP_BYTES {
            self.all.extend_from_slice(b);
            return;
        }
        if self.spill.is_none() && !self.lost {
            match open_spill(dir).and_then(|(mut f, path)| f.write_all(&self.all).ok().map(|_| (f, path))) {
                Some(spill) => {
                    // (The end so far, for output only just past the limit.)
                    self.tail.extend(&self.all[self.all.len().saturating_sub(TAIL_BYTES)..]);
                    self.spill = Some(spill);
                }
                None => self.lost = true,
            }
        }
        match &mut self.spill {
            Some((f, _)) => {
                if !self.broke && f.write_all(b).is_err() {
                    self.broke = true;
                }
                self.tail.extend(b);
                let over = self.tail.len().saturating_sub(TAIL_BYTES);
                self.tail.drain(..over);
            }
            None => {}
        }
    }

    /// What the model sees of it.
    fn shown(self, spill_dir: &Path) -> String {
        let head = String::from_utf8_lossy(&self.all).into_owned();
        match &self.spill {
            None => shorten(&head, self.total, spill_dir),
            Some((_, path)) => {
                let tail_bytes: Vec<u8> = self.tail.iter().copied().collect();
                let tail = String::from_utf8_lossy(&tail_bytes).into_owned();
                // (The first line of the end may be cut part way: left out.)
                let tail_lines: Vec<&str> = tail.lines().skip(1).collect();
                let head_lines: Vec<&str> = head.lines().collect();
                let count = self.lines + usize::from(!head.is_empty() && !tail.ends_with('\n'));
                let saved = if self.broke { Saved::Part(path.clone()) } else { Saved::Whole(path.clone()) };
                cut_view(&head_lines, &tail_lines, false, count, saved, String::new())
            }
        }
    }
}

pub(crate) fn open_spill(dir: &Path) -> Option<(std::fs::File, String)> {
    std::fs::create_dir_all(dir).ok()?;
    let mut b = [0u8; 6];
    getrandom::fill(&mut b).ok()?;
    let name: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let path = dir.join(format!("output-{name}.log"));
    let f = std::fs::File::create(&path).ok()?;
    Some((f, path.to_string_lossy().into_owned()))
}

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
pub struct Group(Option<i32>);

impl Group {
    /// The group a spawned command leads (it was started in its own).
    pub fn of(child: &tokio::process::Child) -> Group {
        Group(child.id().map(|p| p as i32))
    }

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

/// The command, ready to spawn: in the sandbox when there's one (None: as
/// it is, with all of Harlo's variables), in its own process group, with
/// no input and nothing that waits for a person.
pub fn command(command: &str, cwd: &Path, sandbox: Option<&Sandbox>) -> Command {
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
    cmd
}

/// `sandbox`: run it in that (None: as it is, with all of Harlo's
/// variables).
/// `stop`: the user stopping the turn ends it too, and what it printed so
/// far comes back.
pub async fn run(command: &str, cwd: &Path, timeout: Option<u64>, spill_dir: &Path, sandbox: Option<&Sandbox>, stop: Option<&tokio_util::sync::CancellationToken>) -> (String, bool) {
    let secs = timeout.filter(|t| *t > 0).unwrap_or(DEFAULT_TIMEOUT);
    let mut cmd = self::command(command, cwd, sandbox);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (format!("Couldn't start {}: {e}", if sandbox.is_some() { "the sandbox (bwrap)" } else { "bash" }), true),
    };
    let mut group = Group(child.id().map(|p| p as i32));
    let (mut out, mut err) = (child.stdout.take().expect("piped"), child.stderr.take().expect("piped"));
    // Both streams into one, as they come (kept outside the work, so a
    // timeout still has what it printed).
    let mut got = Collected::new();
    let collect = async {
        let (mut b1, mut b2) = ([0u8; 8192], [0u8; 8192]);
        let (mut o_open, mut e_open) = (true, true);
        while o_open || e_open {
            tokio::select! {
                n = out.read(&mut b1), if o_open => match n { Ok(0) | Err(_) => o_open = false, Ok(n) => got.take(&b1[..n], spill_dir) },
                n = err.read(&mut b2), if e_open => match n { Ok(0) | Err(_) => e_open = false, Ok(n) => got.take(&b2[..n], spill_dir) },
            }
        }
    };
    let work = async {
        collect.await;
        child.wait().await
    };
    let stopped = async {
        match stop {
            Some(s) => s.cancelled().await,
            None => std::future::pending().await,
        }
    };
    let (status, timed_out, by_user) = tokio::select! {
        biased;
        _ = stopped => {
            group.kill(libc::SIGTERM);
            tokio::time::sleep(Duration::from_millis(300)).await;
            group.kill(libc::SIGKILL);
            (None, false, true)
        }
        r = tokio::time::timeout(Duration::from_secs(secs), work) => match r {
            Ok(s) => (s.ok(), false, false),
            Err(_) => {
                group.kill(libc::SIGTERM);
                tokio::time::sleep(Duration::from_millis(1500)).await;
                group.kill(libc::SIGKILL);
                (None, true, false)
            }
        },
    };
    // Finished: anything it left in the background goes too.
    group.kill(libc::SIGKILL);
    group.0 = None;
    if by_user {
        let printed = if got.total == 0 { "It printed nothing.".to_owned() } else { format!("What it printed:\n{}", got.shown(spill_dir)) };
        return (format!("Interrupted: the user stopped the turn while this command ran, and it was ended with everything it started. It may have partly run. {printed}"), true);
    }
    if timed_out {
        let printed = if got.total == 0 { "It printed nothing.".to_owned() } else { format!("What it printed:\n{}", got.shown(spill_dir)) };
        return (format!("The command was stopped after {secs} seconds, with everything it started. If it needs longer and isn't waiting for input, run it again with a larger timeout. {printed}"), true);
    }
    let mut shown = got.shown(spill_dir);
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
    let saved = save(text, spill_dir).map_or(Saved::Not, Saved::Whole);
    let more = if total > text.len() { format!(" (and {} more bytes not kept)", total - text.len()) } else { String::new() };
    cut_view(&lines, &lines, true, lines.len(), saved, more)
}

/// The start of `head` and the end of `tail` (one text: `same`, so they
/// don't overlap), with notices at the top and at the cut.
/// Where the whole output went.
enum Saved {
    Whole(String),
    /// Writing it failed part way: the file has only its start.
    Part(String),
    Not,
}

fn cut_view(head_src: &[&str], tail_src: &[&str], same: bool, count: usize, saved: Saved, more: String) -> String {
    // A quarter for the start, the rest for the end (errors are usually last).
    let (head_lines, head_bytes) = (MAX_LINES / 4, MAX_BYTES / 4);
    let (tail_lines, tail_bytes) = (MAX_LINES - head_lines, MAX_BYTES - head_bytes);
    let mut head = Vec::new();
    let mut size = 0;
    for l in head_src {
        if head.len() >= head_lines || size + l.len() + 1 > head_bytes {
            break;
        }
        size += l.len() + 1;
        head.push(*l);
    }
    let mut tail = Vec::new();
    size = 0;
    for l in tail_src.iter().skip(if same { head.len() } else { 0 }).rev() {
        if tail.len() >= tail_lines || size + l.len() + 1 > tail_bytes {
            break;
        }
        size += l.len() + 1;
        tail.push(*l);
    }
    tail.reverse();
    let cut = count.saturating_sub(head.len() + tail.len());
    let saved = match saved {
        Saved::Whole(p) => format!("The whole output is in {p} (read it with offset and limit, or grep it)."),
        Saved::Part(p) => format!("Only the start of the output is in {p}: writing the rest failed (is the disk full?)."),
        Saved::Not => "The whole output couldn't be saved.".into(),
    };
    format!(
        "[Output shortened: {count} lines{more}; showing the first {} and the last {}. {saved}]\n{}\n[… {cut} lines cut here …]\n{}",
        head.len(),
        tail.len(),
        head.join("\n"),
        tail.join("\n")
    )
}

pub(crate) fn save(text: &str, dir: &Path) -> Option<String> {
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
        let (out, failed) = run("echo hi; echo err >&2; echo \"$PAGER $GIT_EDITOR $GIT_TERMINAL_PROMPT\"", &dir, None, &dir, None, None).await;
        assert!(!failed && out.contains("hi") && out.contains("err") && out.contains("cat false 0"), "{out}");
        // Reading stdin gets end-of-input, not a hang.
        let (out, _) = run("read x; echo \"got:$x\"", &dir, Some(5), &dir, None, None).await;
        assert!(out.contains("got:"), "{out}");
        let (out, failed) = run("exit 3", &dir, None, &dir, None, None).await;
        assert!(failed && out.contains("[exit code 3]"));
    }

    #[tokio::test]
    async fn a_timeout_ends_the_whole_group() {
        let dir = std::env::temp_dir();
        let marker = dir.join(format!("harlo-agent-bg-{}", std::process::id()));
        let cmd = format!("(sleep 3; touch {}) & sleep 30", marker.display());
        let (out, failed) = run(&cmd, &dir, Some(1), &dir, None, None).await;
        assert!(failed && out.contains("stopped after 1 seconds"), "{out}");
        tokio::time::sleep(Duration::from_secs(4)).await;
        assert!(!marker.exists(), "the background child was left running");
    }

    #[tokio::test]
    async fn a_timeout_keeps_what_was_printed() {
        let dir = std::env::temp_dir();
        let (out, failed) = run("echo 'error: E0308 mismatched types'; sleep 30", &dir, Some(1), &dir, None, None).await;
        assert!(failed && out.contains("stopped after 1 seconds") && out.contains("What it printed:") && out.contains("error: E0308"), "{out}");
        let (out, _) = run("sleep 30", &dir, Some(1), &dir, None, None).await;
        assert!(out.contains("It printed nothing."), "{out}");
    }

    #[tokio::test]
    async fn stopped_by_the_user_it_says_what_it_printed() {
        let dir = std::env::temp_dir();
        let marker = dir.join(format!("harlo-agent-stopped-cmd-{}", std::process::id()));
        let stop = tokio_util::sync::CancellationToken::new();
        let later = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(700)).await;
            later.cancel();
        });
        let started = std::time::Instant::now();
        let cmd = format!("echo 'step 1 of 3 done'; (sleep 3; touch {}) & sleep 30", marker.display());
        let (out, failed) = run(&cmd, &dir, None, &dir, None, Some(&stop)).await;
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        assert!(failed && out.contains("the user stopped the turn") && out.contains("What it printed:") && out.contains("step 1 of 3 done"), "{out}");
        tokio::time::sleep(Duration::from_secs(4)).await;
        assert!(!marker.exists(), "the background child was left running");
    }

    #[test]
    fn a_spill_that_fails_part_way_says_so() {
        // (/dev/full takes the file's opening, then refuses each write.)
        let Ok(full) = std::fs::OpenOptions::new().write(true).open("/dev/full") else { return };
        let mut got = Collected::new();
        got.spill = Some((full, "/tmp/output-x.log".into()));
        got.take(&vec![b'a'; 100][..], Path::new("/tmp"));
        got.take(b"\n", Path::new("/tmp"));
        let shown = got.shown(Path::new("/tmp"));
        assert!(shown.contains("Only the start of the output is in /tmp/output-x.log") && !shown.contains("The whole output is in"), "{shown}");
    }

    #[tokio::test]
    async fn output_past_the_memory_limit_keeps_its_end() {
        let spill = std::env::temp_dir().join(format!("harlo-agent-spill-{}", std::process::id()));
        // 20 MiB of lines, then the line that matters.
        let cmd = "yes 0123456789012345678901234567890123456789012345678901234567890123 | head -c 20971520; echo; echo 'FINAL: test x failed'";
        let (out, failed) = run(cmd, &std::env::temp_dir(), Some(60), &spill, None, None).await;
        assert!(!failed && out.contains("FINAL: test x failed") && !out.contains("not kept"), "the real end shown: {}", &out[out.len().saturating_sub(300)..]);
        let file = out.split("The whole output is in ").nth(1).and_then(|r| r.split(' ').next()).expect("a file");
        let saved = std::fs::read(file).unwrap();
        assert!(saved.len() > 20 * 1024 * 1024 && String::from_utf8_lossy(&saved[saved.len() - 64..]).contains("FINAL: test x failed"), "the whole output in the file");
        let _ = std::fs::remove_dir_all(&spill);
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
        let (out, failed) = run("echo a > made && cat made && echo t > /tmp/harlo-agent-sb-t && rm /tmp/harlo-agent-sb-t", &proj, None, &root, Some(&sb), None).await;
        assert!(!failed && out.contains('a'), "{out}");
        let (out, failed) = run("echo x > ../other/f", &proj, None, &root, Some(&sb), None).await;
        assert!(failed && out.contains("Read-only"), "{out}");
        let (out, _) = run("cat ../.ssh/id; ls -A ../.ssh | wc -l", &proj, None, &root, Some(&sb), None).await;
        assert!(!out.contains("secret-key") && out.ends_with('0'), "{out}");
        // SAFETY: a variable for this test only (tests in this binary don't read it).
        unsafe { std::env::set_var("HARLO_TEST_API_KEY", "k") };
        let (out, _) = run("echo \"[$HARLO_TEST_API_KEY][$PAGER]\"", &proj, None, &root, Some(&sb), None).await;
        assert!(out.contains("[][cat]"), "{out}");
        let marker = std::env::temp_dir().join(format!("harlo-agent-sb-bg-{}", std::process::id()));
        let (out, failed) = run(&format!("(sleep 3; touch {}) & sleep 30", marker.display()), &proj, Some(1), &root, Some(&sb), None).await;
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
