//! `/plugin …` in either agent's session: Claude Code's plugins.
//!
//! Its plugins (and their marketplaces) are installed and managed by its own
//! `claude plugin …` command, which harlo runs in the session's folder, on the
//! session's machine, instead of sending the words to the agent. Both agents
//! use what it installs: Claude Code loads its plugins, and Harlo reads their
//! skills (harlo-agent's `skills::plugin_folders`). The result shows in the
//! conversation as a notice ([`notice`], drawn from [`parse_notice`]).

// Rust guideline compliant 2026-10-09

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a plugin command may take (a marketplace is a git clone).
const TIMEOUT: Duration = Duration::from_secs(300);
/// Of its output, the last lines shown.
const SHOWN_LINES: usize = 80;
const SHOWN_BYTES: usize = 16 * 1024;

/// The words after `/plugin`, when `text` is one.
///
/// `/plugins` and `/marketplace …` (which is `/plugin marketplace …`) count
/// too; `/plugin` alone lists what's installed. None: not one (or its quotes
/// don't close: then it's for the agent, as typed).
pub fn parse(text: &str) -> Option<Vec<String>> {
    let t = text.trim();
    let (name, rest) = t.split_once(char::is_whitespace).unwrap_or((t, ""));
    let mut args = match name {
        "/plugin" | "/plugins" => Vec::new(),
        "/marketplace" => vec!["marketplace".to_owned()],
        _ => return None,
    };
    args.extend(crate::remote::shell_words(rest).ok()?);
    if args.is_empty() || args == ["marketplace"] {
        args.push("list".to_owned());
    }
    Some(args)
}

/// The `/` list's entry for it.
pub fn command() -> crate::session::SlashCommand {
    crate::session::SlashCommand {
        name: "plugin".into(),
        description: "Install and manage Claude Code plugins; Claude and Harlo both use their skills".into(),
        hint: "install <plugin>@<marketplace> | marketplace add <source> | list".into(),
    }
}

/// What a run does, in words: while it runs, once it worked, once it
/// didn't.
fn words(args: &[String]) -> (&'static str, &'static str, &'static str) {
    let a: Vec<&str> = args.iter().map(String::as_str).filter(|a| !a.starts_with('-')).collect();
    match a.as_slice() {
        ["install" | "i", ..] => ("Installing the plugin…", "Plugin installed", "Plugin not installed"),
        ["uninstall" | "remove" | "rm", ..] => ("Removing the plugin…", "Plugin removed", "Plugin not removed"),
        ["enable", ..] => ("Turning the plugin on…", "Plugin turned on", "Plugin not turned on"),
        ["disable", ..] => ("Turning the plugin off…", "Plugin turned off", "Plugin not turned off"),
        ["update", ..] => ("Updating the plugin…", "Plugin updated", "Plugin not updated"),
        ["marketplace", "add", ..] => ("Adding the marketplace…", "Marketplace added", "Marketplace not added"),
        ["marketplace", "remove" | "rm", ..] => ("Removing the marketplace…", "Marketplace removed", "Marketplace not removed"),
        ["marketplace", "update", ..] => ("Updating marketplaces…", "Marketplaces updated", "Marketplaces not updated"),
        ["marketplace", "list", ..] => ("Listing marketplaces…", "Marketplaces", "Marketplaces couldn't be listed"),
        ["list", ..] => ("Listing plugins…", "Plugins", "Plugins couldn't be listed"),
        _ => ("Running the plugin command…", "Plugin command finished", "Plugin command failed"),
    }
}

/// Whether a run that worked changed what's installed (so the session's
/// agent should start again to load it).
pub fn changes(args: &[String]) -> bool {
    let a: Vec<&str> = args.iter().map(String::as_str).filter(|a| !a.starts_with('-')).collect();
    !matches!(a.as_slice(), ["list" | "details" | "configure" | "eval" | "help" | "validate", ..] |
["marketplace", "list" | "help", ..]) && !args.iter().any(|a| a == "--help" || a == "-h")
}

/// The command as shown.
pub fn shown(args: &[String]) -> String {
    let mut s = "claude plugin".to_owned();
    for a in args {
        s.push(' ');
        s.push_str(&crate::remote::quote(a));
    }
    s
}

/// How a run went.
#[derive(Debug, Clone, PartialEq)]
pub struct Ran {
    pub ok: bool,
    /// Why it failed, briefly ("exit code 1"), or "".
    pub detail: String,
    /// Its output (the last lines), or why it couldn't run.
    pub output: String,
}

/// The notice for a run: its headline (and detail), the command, and its
/// output; `ran` None while it runs. Read back by [`parse_notice`]; plain
/// text for anything that doesn't draw it.
pub fn notice(args: &[String], ran: Option<&Ran>) -> String {
    let (running, worked, failed) = words(args);
    let head = match ran {
        None => running.to_owned(),
        Some(r) if r.ok => worked.to_owned(),
        Some(r) if r.detail.is_empty() => failed.to_owned(),
        Some(r) => format!("{failed} · {}", r.detail),
    };
    let mut s = format!("{head}\n$ {}", shown(args));
    if let Some(r) = ran.filter(|r| !r.output.is_empty()) {
        s.push('\n');
        s.push_str(&r.output);
    }
    s
}

/// A notice [`notice`] wrote: (headline, detail, command, output, state:
/// running, done or error).
pub fn parse_notice(text: &str, failed: bool) -> Option<(String, String, String, String, &'static str)> {
    let mut lines = text.splitn(3, '\n');
    let head = lines.next()?;
    let command = lines.next()?.strip_prefix("$ ")?;
    if !command.starts_with("claude plugin") {
        return None;
    }
    let output = lines.next().unwrap_or_default().to_owned();
    let (headline, detail) = head.split_once(" · ").unwrap_or((head, ""));
    let state = if failed {
        "error"
    } else if headline.ends_with('…') {
        "running"
    } else {
        "done"
    };
    Some((headline.to_owned(), detail.to_owned(), command.to_owned(), output, state))
}

/// Runs `claude plugin <args>` in `cwd`: here with `program` (Settings'
/// Claude Code, else the one found), or on `host`. Blocks (at most
/// [`TIMEOUT`]).
pub fn run(args: &[String], cwd: &Path, host: Option<&crate::remote::Host>, program: Option<&Path>) -> Ran {
    let missing = |r#where: &str| Ran {
        ok: false,
        detail: "Claude Code isn't installed".into(),
        output: format!(
            "Plugins are installed through Claude Code, which isn't {where}. Install it (Settings → General → Open the setup guide{}), then send this again.",
            if r#where == "on this computer" { "" } else { ", or the machine's row in Settings → Other machines" }
        ),
    };
    let out = match host {
        Some(h) => {
            let script = format!(
                "command -v claude >/dev/null 2>&1 || exit 127; cd {} && claude plugin {} </dev/null",
                crate::remote::quote_path(&cwd.to_string_lossy()),
                args.iter().map(|a| crate::remote::quote(a)).collect::<Vec<_>>().join(" ")
            );
            match crate::remote::run(h, &script, TIMEOUT) {
                Ok(o) if o.status.code() == Some(127) && o.stdout.is_empty() => return missing(&format!("on {}", h.name)),
                Ok(o) => o,
                Err(e) => return Ran { ok: false, detail: String::new(), output: e.to_string() },
            }
        }
        None => {
            let Some(program) = program.map(Path::to_path_buf).or_else(|| harlo_harness::resolve_program(harlo_harness::HarnessKind::Claude)) else {
                return missing("on this computer");
            };
            let mut cmd = Command::new(&program);
            cmd.arg("plugin").args(args).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
            // (Plain output: no colours, no spinner.)
            cmd.env("NO_COLOR", "1").env("CI", "1");
            match run_local(cmd) {
                Ok(o) => o,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return missing("on this computer"),
                Err(e) => return Ran { ok: false, detail: String::new(), output: format!("{} couldn't run: {e}", program.display()) },
            }
        }
    };
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        if !text.trim().is_empty() {
            text.push('\n');
        }
        text.push_str(&err);
    }
    let ok = out.status.success();
    let detail = match out.status.code() {
        _ if ok => String::new(),
        Some(c) => format!("exit code {c}"),
        None => "it was stopped".into(),
    };
    Ran { ok, detail, output: last_lines(&plain(&text)) }
}

/// Runs a local command and waits, at most [`TIMEOUT`].
fn run_local(mut cmd: Command) -> std::io::Result<std::process::Output> {
    let mut child = cmd.spawn()?;
    let (mut out, mut err) = (child.stdout.take(), child.stderr.take());
    let read = |p: Option<&mut dyn std::io::Read>| {
        let mut v = Vec::new();
        if let Some(p) = p {
            let _ = p.read_to_end(&mut v);
        }
        v
    };
    let out_reader = std::thread::spawn(move || read(out.as_mut().map(|p| p as &mut dyn std::io::Read)));
    let err_reader = std::thread::spawn(move || read(err.as_mut().map(|p| p as &mut dyn std::io::Read)));
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            break child.wait()?;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Ok(std::process::Output { status, stdout: out_reader.join().unwrap_or_default(), stderr: err_reader.join().unwrap_or_default() })
}

/// Without terminal colours and cursor moves.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // (Up to the final letter.)
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        if c == '\r' {
            continue;
        }
        out.push(c);
    }
    out
}

/// The last lines of an output, trimmed.
fn last_lines(text: &str) -> String {
    let lines: Vec<&str> = text.trim_end().lines().collect();
    let mut s = lines[lines.len().saturating_sub(SHOWN_LINES)..].join("\n").trim_start_matches('\n').to_owned();
    if s.len() > SHOWN_BYTES {
        let mut cut = s.len() - SHOWN_BYTES;
        while !s.is_char_boundary(cut) {
            cut += 1;
        }
        s = format!("…{}", &s[cut..]);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(std::string::ToString::to_string).collect()
    }

    #[test]
    fn what_counts_as_one() {
        assert_eq!(parse("/plugin install superpowers@obra"), Some(v(&["install", "superpowers@obra"])));
        assert_eq!(parse("  /plugins  marketplace add 'my repo/x' "), Some(v(&["marketplace", "add", "my repo/x"])));
        assert_eq!(parse("/marketplace add obra/superpowers-marketplace"), Some(v(&["marketplace", "add", "obra/superpowers-marketplace"])));
        assert_eq!(parse("/plugin"), Some(v(&["list"])));
        assert_eq!(parse("/marketplace"), Some(v(&["marketplace", "list"])));
        // Not one: for the agent, as typed.
        assert_eq!(parse("/pluginx"), None);
        assert_eq!(parse("tell me about /plugin"), None);
        assert_eq!(parse("/plugin install 'oops"), None);
    }

    #[test]
    fn its_notice_reads_back() {
        let args = v(&["install", "x@y"]);
        let running = notice(&args, None);
        assert_eq!(running, "Installing the plugin…\n$ claude plugin install x@y");
        assert_eq!(parse_notice(&running, false), Some(("Installing the plugin…".into(), String::new(), "claude plugin install x@y".into(), String::new(), "running")));
        let failed = notice(&args, Some(&Ran { ok: false, detail: "exit code 1".into(), output: "✘ Plugin \"x\" not found\nin any marketplace".into() }));
        assert_eq!(
            parse_notice(&failed, true),
            Some(("Plugin not installed".into(), "exit code 1".into(), "claude plugin install x@y".into(), "✘ Plugin \"x\" not found\nin any marketplace".into(), "error"))
        );
        let done = notice(&v(&["marketplace", "add", "a b"]), Some(&Ran { ok: true, detail: String::new(), output: "✔ Added".into() }));
        assert_eq!(parse_notice(&done, false).map(|n| (n.0, n.2, n.4)), Some(("Marketplace added".into(), "claude plugin marketplace add 'a b'".into(), "done")));
        // Anything else stays a plain notice.
        assert_eq!(parse_notice("Disconnected from box.", false), None);
        assert_eq!(parse_notice("Something\n$ rm -rf /", false), None);
    }

    #[test]
    fn what_changes_what_is_installed() {
        assert!(changes(&v(&["install", "x"])) && changes(&v(&["marketplace", "add", "o/r"])) && changes(&v(&["disable", "x"])));
        assert!(!changes(&v(&["list"])) && !changes(&v(&["marketplace", "list"])) && !changes(&v(&["install", "--help"])));
    }

    #[test]
    fn output_is_plain_and_bounded() {
        assert_eq!(plain("\u{1b}[32m✔\u{1b}[0m done\r\n"), "✔ done\n");
        let long: String = (0..500).map(|i| format!("line {i}\n")).collect();
        let s = last_lines(&long);
        assert!(s.starts_with("line 420") && s.ends_with("line 499"));
    }

    /// A stand-in `claude` (here and on a host it's the same script):
    /// its arguments, folder and output come back; a missing one says how
    /// to get it.
    #[test]
    fn runs_claude_plugin_here() {
        let dir = std::env::temp_dir().join(format!("harlo-plugins-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("claude");
        std::fs::write(&fake, "#!/bin/sh\necho \"args: $*\"\necho \"in: $(pwd)\"\n[ \"$3\" = fail ] && { echo nope >&2; exit 3; }\nexit 0\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let r = run(&v(&["install", "x@y"]), &dir, None, Some(&fake));
        assert!(r.ok, "{r:?}");
        assert_eq!(r.output, format!("args: plugin install x@y\nin: {}", dir.canonicalize().unwrap().display()));
        let r = run(&v(&["install", "fail"]), &dir, None, Some(&fake));
        assert_eq!((r.ok, r.detail.as_str(), r.output.ends_with("nope")), (false, "exit code 3", true));
        let r = run(&v(&["list"]), &dir, None, Some(&dir.join("no-claude")));
        assert_eq!((r.ok, r.detail.as_str()), (false, "Claude Code isn't installed"));
        assert!(r.output.contains("setup guide"), "{}", r.output);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
