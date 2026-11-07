//! Commit messages written by a model: the request and the diff go to
//! Harlo in one-shot print mode on stdin (`-p - --no-session --no-tools`),
//! or to Claude Code (`claude -p`, no tools, nothing saved) when Harlo
//! can't, and the printed answer is the message.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use harlo_harness::HarnessKind;

/// Larger diffs are cut here (the model only needs the gist).
const DIFF_LIMIT: usize = 120 * 1024;
/// Untracked files are shown in full up to this size, otherwise just named.
const UNTRACKED_LIMIT: u64 = 16 * 1024;
const TIMEOUT: Duration = Duration::from_secs(180);

fn git_out(root: &Path, args: &[&str]) -> String {
    crate::git::output_in(root, args)
}

/// A small text file's contents (here or on a remote host), else None.
fn small_text(path: &Path) -> Option<String> {
    let bytes = if let Some((name, f)) = crate::remote::split(path) {
        // One byte past the limit tells a big file.
        let host = crate::remote::host(&name)?;
        let out = crate::remote::run(&host, &format!("head -c {} -- {}", UNTRACKED_LIMIT + 1, crate::remote::quote(&f)), Duration::from_secs(20)).ok()?;
        out.status.success().then_some(out.stdout)?
    } else {
        if std::fs::metadata(path).ok()?.len() > UNTRACKED_LIMIT {
            return None;
        }
        std::fs::read(path).ok()?
    };
    (bytes.len() as u64 <= UNTRACKED_LIMIT && !bytes.contains(&0)).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

/// What the commit will contain, as text for the model: the staged diff, or
/// with `staged_only` false every change (tracked diff against HEAD plus
/// untracked files). Empty when there is nothing to commit.
pub fn changes(root: &Path, staged_only: bool) -> String {
    let mut out = if staged_only {
        git_out(root, &["diff", "--cached", "--no-color", "--no-ext-diff"])
    } else {
        let head = git_out(root, &["rev-parse", "--verify", "-q", "HEAD"]);
        let mut d = if head.trim().is_empty() {
            git_out(root, &["diff", "--cached", "--no-color", "--no-ext-diff"])
        } else {
            git_out(root, &["diff", "HEAD", "--no-color", "--no-ext-diff"])
        };
        for file in git_out(root, &["ls-files", "--others", "--exclude-standard"]).lines().filter(|l| !l.is_empty()).take(40) {
            if let Some(text) = small_text(&root.join(file)) {
                d.push_str(&format!("\nNew file {file}:\n{text}\n"));
            } else {
                d.push_str(&format!("\nNew file {file} (not shown)\n"));
            }
        }
        d
    };
    if out.len() > DIFF_LIMIT {
        let mut cut = DIFF_LIMIT;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
        out.push_str("\n[diff truncated]\n");
    }
    out
}

/// Subjects of the latest commits, newest first, so the message matches.
pub fn recent_subjects(root: &Path) -> Vec<String> {
    git_out(root, &["log", "-12", "--format=%s"]).lines().map(str::to_owned).filter(|l| !l.is_empty()).collect()
}

/// The request sent along with the diff.
pub fn instructions(recent: &[String]) -> String {
    let recent = if recent.is_empty() { "(none yet)".to_owned() } else { recent.iter().map(|s| format!("- {s}")).collect::<Vec<_>>().join("\n") };
    format!(
        "Write the git commit message for the changes in the diff at the end.\n\
         \n\
         Reply with the commit message only: no preamble, no quotes, no markdown fences, no explanation.\n\
         \n\
         - First line: a concise summary of what the change does and why it matters to someone reading the history, at most about 72 characters.\n\
         - Match the style of the recent commits below: their language, capitalization, whether they use a type prefix or scope, and their typical length. When they show no consistent style, write a plain imperative summary (\"Add …\", \"Fix …\").\n\
         - If the change needs it, add a blank line and a short body (a few lines or bullet points) explaining what changed and why. Leave the body out for small, self-explanatory changes.\n\
         - Describe the change itself; do not mention that you are an AI or how the message was written.\n\
         \n\
         Recent commits (newest first):\n{recent}\n"
    )
}

/// What the model is sent: the request, then the diff.
fn request(recent: &[String], diff: &str) -> String {
    format!("{}\nThe diff:\n\n{diff}", instructions(recent))
}

/// The model Claude Code is asked for: the setting when it names one of
/// Claude's, else a small one (a commit message needs no more).
fn claude_model(model: &str) -> &str {
    let m = model.trim();
    if m.starts_with("claude-") || ["opus", "sonnet", "haiku", "fable"].contains(&m) { m } else { "haiku" }
}

/// Tidies the model's answer: trims, drops code fences and wrapping quotes.
pub fn clean(answer: &str) -> String {
    let mut text = answer.trim();
    if text.starts_with("```") {
        text = text.trim_start_matches('`');
        // Drop a language tag on the fence line.
        text = text.split_once('\n').map_or("", |(_, rest)| rest);
        text = text.trim_end().trim_end_matches('`').trim();
    }
    for q in ['"', '\'', '`'] {
        if text.len() >= 2 && text.starts_with(q) && text.ends_with(q) && !text[1..text.len() - 1].contains(q) {
            text = &text[1..text.len() - 1];
        }
    }
    text.trim().to_owned()
}

/// Asks a model for a commit message: Harlo's (`harlo`: its program, from
/// Settings; else the one shipped with harlo), else Claude Code's. `model`
/// (may be empty) is Harlo's, and Claude's when it names one of Claude's.
/// A remote repository's are asked on its machine. Blocks for up to a few
/// minutes; run it off the UI thread.
pub fn generate(harlo: Option<PathBuf>, model: &str, root: &Path, staged_only: bool) -> Result<String> {
    let diff = changes(root, staged_only);
    if diff.trim().is_empty() {
        bail!("There are no changes to describe.");
    }
    let text = request(&recent_subjects(root), &diff);
    if let Some((host, _)) = crate::remote::split(root) {
        return finish(run_on(&host, model, &text));
    }
    let tried = match harlo.or_else(|| harlo_harness::resolve_program(HarnessKind::Harlo)) {
        Some(program) => match finish(run(Command::new(&program).args(harlo_args(model)).current_dir(root), &text)) {
            Ok(message) => return Ok(message),
            Err(e) => format!("Harlo: {e:#}"),
        },
        None => "Harlo isn't installed".to_owned(),
    };
    let Some(claude) = harlo_harness::resolve_program(HarnessKind::Claude) else { bail!("{tried}, and Claude Code isn't either.") };
    finish(run(Command::new(claude).args(claude_args(model)).current_dir(root), &text)).map_err(|e| anyhow::anyhow!("{tried}; Claude: {e:#}"))
}

fn harlo_args(model: &str) -> Vec<String> {
    let mut a: Vec<String> = ["-p", "-", "--no-session", "--no-tools", "--thinking", "off"].map(str::to_owned).to_vec();
    if !model.trim().is_empty() {
        a.extend(["--model".to_owned(), model.trim().to_owned()]);
    }
    a
}

/// (The request on stdin; `--tools ""` turns its tools off.)
fn claude_args(model: &str) -> Vec<String> {
    ["-p", "--no-session-persistence", "--tools", "", "--strict-mcp-config", "--model", claude_model(model)].map(str::to_owned).to_vec()
}

fn finish(answer: Result<String>) -> Result<String> {
    let message = clean(&answer?);
    if message.is_empty() {
        bail!("The model returned an empty message.");
    }
    Ok(message)
}

/// The same on the repository's machine: Harlo's copy there (put there
/// first when needed), else its Claude Code, with the request on stdin.
fn run_on(host: &str, model: &str, text: &str) -> Result<String> {
    let q = crate::remote::quote;
    let line = |program: &str, args: Vec<String>| format!("{} {}", q(program), args.iter().map(|a| q(a)).collect::<Vec<_>>().join(" "));
    let harlo = crate::agent_host::program_on(host).ok();
    let host = crate::remote::host(host).with_context(|| format!("unknown remote host {host}"))?;
    let claude = line("claude", claude_args(model));
    // (Harlo first; Claude Code when Harlo fails or isn't there.)
    let script = match &harlo {
        Some(p) => format!(r#"f=$(mktemp "${{TMPDIR:-/tmp}}/harlo-commit-XXXXXX") || exit 1; cat > "$f"; cd "${{TMPDIR:-/tmp}}"; {h} < "$f" 2>"$f.err"; rc=$?; if [ $rc -ne 0 ] && command -v claude >/dev/null 2>&1; then {claude} < "$f"; rc=$?; else cat "$f.err" >&2; fi; rm -f "$f" "$f.err"; exit $rc"#, h = line(p, harlo_args(model))),
        None => format!(r#"command -v claude >/dev/null 2>&1 || {{ echo "Neither Harlo nor Claude Code can run on {name}" >&2; exit 127; }}; cd "${{TMPDIR:-/tmp}}"; {claude}"#, name = host.name),
    };
    let out = crate::remote::run_with_input(&host, &script, Some(text.as_bytes()), TIMEOUT)?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail = err.trim().lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no error output").to_owned();
        bail!("On {}: {tail}", host.name);
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Runs `cmd` with `input` on stdin; its stdout.
fn run(cmd: &mut Command, input: &str) -> Result<String> {
    let program = cmd.get_program().to_owned();
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().with_context(|| format!("starting {}", program.display()))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let input = input.to_owned();
    let in_thread = std::thread::spawn(move || {
        let _ = std::io::Write::write_all(&mut stdin, input.as_bytes());
    });
    // Read both pipes on threads so a chatty stderr can't block the child.
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut stdout, &mut s);
        s
    });
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut stderr, &mut s);
        s
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            bail!("The model took too long (over {} s).", TIMEOUT.as_secs());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let _ = in_thread.join();
    let out = out_thread.join().unwrap_or_default();
    let err = err_thread.join().unwrap_or_default();
    if !status.success() {
        let tail = err.trim().lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no error output");
        bail!("{} failed: {tail}", Path::new(&program).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_answers() {
        assert_eq!(clean("  Add a cache\n"), "Add a cache");
        assert_eq!(clean("```text\nFix the build\n\nBody line\n```"), "Fix the build\n\nBody line");
        assert_eq!(clean("\"Fix typo\""), "Fix typo");
        assert_eq!(clean("Say \"hi\" politely"), "Say \"hi\" politely");
    }

    #[test]
    fn generates_with_a_stand_in_harlo() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("harlo-commitmsg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            assert!(Command::new("git").current_dir(&dir).args(args).env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t").env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t").output().unwrap().status.success());
        };
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "Start the project"]);
        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(dir.join("new.txt"), "fresh\n").unwrap();

        let all = changes(&dir, false);
        assert!(all.contains("+two") && all.contains("New file new.txt"), "{all}");
        assert!(changes(&dir, true).is_empty(), "nothing staged");

        // A stand-in for Harlo: checks its arguments and stdin, prints a
        // fenced answer.
        let fake = dir.join("fake-harlo");
        std::fs::write(&fake, "#!/bin/sh\n[ \"$*\" = '-p - --no-session --no-tools --thinking off --model m' ] || exit 3\ngrep -q '^+two' || exit 4\nprintf '```\\nAdd the second line\\n```\\n'\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let msg = generate(Some(fake.clone()), "m", &dir, false).unwrap();
        assert_eq!(msg, "Add the second line");
        assert!(generate(Some(fake), "m", &dir, true).is_err(), "no staged changes");
        assert_eq!(claude_args("")[..5], ["-p", "--no-session-persistence", "--tools", "", "--strict-mcp-config"]);
        assert_eq!((claude_model("codex/gpt-6.1-sol"), claude_model("sonnet"), claude_model("claude-opus-5-5")), ("haiku", "sonnet", "claude-opus-5-5"));
        assert!(instructions(&recent_subjects(&dir)).contains("- Start the project"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live (not run by default): a real repository's message by Harlo
    /// (HARLO_AGENT_BIN, HARLO_LIVE_MODEL), then by Claude Code when Harlo
    /// fails. Spends a little of both plans.
    #[test]
    #[ignore]
    fn live_harlo_then_claude() {
        let dir = std::env::temp_dir().join(format!("harlo-commitmsg-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| assert!(Command::new("git").current_dir(&dir).args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args).output().unwrap().status.success());
        git(&["init", "-q"]);
        std::fs::write(dir.join("greet.py"), "def greet(name):\n    return 'hi ' + name\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "Add greet"]);
        std::fs::write(dir.join("greet.py"), "def greet(name, loud=False):\n    text = 'hi ' + name\n    return text.upper() if loud else text\n").unwrap();
        let harlo = std::env::var_os("HARLO_AGENT_BIN").map(PathBuf::from);
        let model = std::env::var("HARLO_LIVE_MODEL").unwrap_or_default();
        let by_harlo = generate(harlo, &model, &dir, false).unwrap();
        eprintln!("Harlo: {by_harlo}");
        let by_claude = generate(Some(PathBuf::from("/bin/false")), &model, &dir, false).unwrap();
        eprintln!("Claude: {by_claude}");
        assert!(!by_harlo.is_empty() && !by_claude.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live on a machine over SSH (not run by default; the static agent
    /// built first): `HARLO_TEST_SSH="$(scripts/test-sshd.sh start /tmp/x)"
    /// HARLO_LIVE_MODEL=… cargo test -p harlo-core -- --ignored live_on_a_host`.
    /// Harlo's copy there writes it; with a model it doesn't have, Claude
    /// Code there does.
    #[test]
    #[ignore]
    fn live_on_a_host() {
        let dir = std::env::temp_dir().join(format!("harlo-commitmsg-host-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| assert!(Command::new("git").current_dir(&dir).args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args).output().unwrap().status.success());
        git(&["init", "-q"]);
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "Start"]);
        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        let ssh = crate::remote::parse_ssh_command(&std::env::var("HARLO_TEST_SSH").expect("HARLO_TEST_SSH")).unwrap();
        let host = crate::remote::Host { name: "t".into(), ssh };
        crate::remote::set_hosts(vec![host.clone()]);
        crate::remote::check(&host).unwrap();
        let root = crate::remote::join("t", &dir.to_string_lossy());
        let by_harlo = generate(None, &std::env::var("HARLO_LIVE_MODEL").unwrap_or_default(), &root, false).unwrap();
        eprintln!("Harlo there: {by_harlo}");
        let by_claude = generate(None, "nowhere/none", &root, false).unwrap();
        eprintln!("Claude there: {by_claude}");
        crate::remote::disconnect(&host);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
