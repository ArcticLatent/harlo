//! Commit messages written by the agent's model: the diff goes to pi or omp
//! in one-shot print mode (`-p --no-session --no-tools`, attached as an
//! `@file`), and the printed answer is the message.

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
        "Write the git commit message for the changes in the attached diff.\n\
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

/// Asks the harness's model for a commit message. `program` overrides the
/// binary (Settings), `model` the model; both may be empty. Blocks for up
/// to a few minutes; run it off the UI thread.
pub fn generate(kind: HarnessKind, program: Option<PathBuf>, model: &str, root: &Path, staged_only: bool) -> Result<String> {
    let diff = changes(root, staged_only);
    if diff.trim().is_empty() {
        bail!("There are no changes to describe.");
    }
    let program = match program {
        Some(p) => p,
        None => harlo_harness::resolve_program(kind).with_context(|| format!("could not find `{}`", kind.as_str()))?,
    };
    let file = std::env::temp_dir().join(format!("harlo-commit-{}-{}.diff", std::process::id(), unique()));
    std::fs::write(&file, &diff).context("writing the diff")?;
    let result = run(&program, model, root, &file, &instructions(&recent_subjects(root)));
    let _ = std::fs::remove_file(&file);
    let message = clean(&result?);
    if message.is_empty() {
        bail!("The model returned an empty message.");
    }
    Ok(message)
}

fn unique() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default()
}

fn run(program: &Path, model: &str, root: &Path, diff_file: &Path, prompt: &str) -> Result<String> {
    let mut cmd = Command::new(program);
    cmd.args(["-p", "--no-session", "--no-tools", "--thinking", "off"]);
    if !model.trim().is_empty() {
        cmd.args(["--model", model.trim()]);
    }
    cmd.arg(format!("@{}", diff_file.display()))
        .arg(prompt)
        // (A remote repository's diff is already in the file.)
        .current_dir(if crate::remote::is_remote(root) { std::env::temp_dir() } else { root.to_owned() })
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().with_context(|| format!("starting {}", program.display()))?;
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
    let out = out_thread.join().unwrap_or_default();
    let err = err_thread.join().unwrap_or_default();
    if !status.success() {
        let tail = err.trim().lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no error output");
        bail!("{} failed: {tail}", program.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
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
    fn generates_with_a_stand_in_harness() {
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

        // A stand-in for pi: checks its arguments and prints a fenced answer.
        let fake = dir.join("fake-pi");
        std::fs::write(&fake, "#!/bin/sh\n[ \"$1\" = -p ] || exit 3\nfor a; do case $a in @*) grep -q '+two' \"${a#@}\" || exit 4;; esac; done\nprintf '```\\nAdd the second line\\n```\\n'\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let msg = generate(HarnessKind::Pi, Some(fake.clone()), "", &dir, false).unwrap();
        assert_eq!(msg, "Add the second line");
        assert!(generate(HarnessKind::Pi, Some(fake), "", &dir, true).is_err(), "no staged changes");
        assert!(instructions(&recent_subjects(&dir)).contains("- Start the project"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
