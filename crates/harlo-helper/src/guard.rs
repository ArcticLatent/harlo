//! `harlo-helper sandbox describe|danger|run`: harlo's sandbox and its check
//! for commands that can't be undone, for pi's tools: harlo's extension for
//! pi runs pi's commands through it.
//!
//! - `describe --cwd <dir>`: the sandbox for that folder as JSON (`on`,
//!   `network`, `project`, `writable`, `hidden`, `shown`, `broken`, and
//!   `about`, what the model is told), or `{"on":false}`;
//! - `danger --cwd <dir> -- <command>`: `{"why": …}`, why the command can't
//!   be undone, or `{"why":null}`;
//! - `read|write|search --cwd <dir> -- <path>`: `{"error": …}`, why a file
//!   tool may not look at, change or search under that path, or
//!   `{"error":null}`;
//! - `run --cwd <dir> -- <command>`: runs `bash -c <command>` in the sandbox
//!   (as it is when the sandbox is off), its output and exit code its own.
//!
//! The policy is `HARLO_SANDBOX` (`on`, `off`, `no-network`; on when unset)
//! with `sandbox.json`'s extras.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::json;

/// Runs `harlo-helper sandbox …` (`args`: what follows `sandbox`).
///
/// # Errors
///
/// An unknown use, a missing `--cwd` or command, or a sandboxed command
/// that couldn't be started.
pub fn command(args: &[String]) -> Result<()> {
    let (what, cwd, line) = parse(args)?;
    crate::sandbox::configure(None)?;
    match what.as_str() {
        "describe" => {
            let out = match crate::sandbox::for_dir(&cwd) {
                None => json!({ "on": false }),
                Some(s) => json!({
                    "on": true,
                    "network": s.network,
                    "project": s.project,
                    "projectWritable": s.project_writable,
                    "writable": s.writable,
                    "hidden": s.hidden,
                    "shown": s.shown,
                    "broken": s.broken,
                    "about": s.describe_for(false),
                }),
            };
            println!("{out}");
            Ok(())
        }
        "danger" => {
            let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
            let why = crate::danger::check(&line.context("danger needs the command after --")?, &crate::sandbox::project_of(&cwd, &home), &home);
            println!("{}", json!({ "why": why }));
            Ok(())
        }
        "read" | "write" | "search" => {
            // (A file tool's path, relative to the folder.)
            let path = cwd.join(line.context("read, write and search need the path after --")?);
            let error = crate::sandbox::for_dir(&cwd).and_then(|s| match what.as_str() {
                "write" => s.may_write(&path).err().map(|e| e.to_string()),
                "search" => s.may_read(&path).err().map(|e| e.to_string()).or_else(|| search_refused(&path, &s.hidden_under(&path))),
                _ => s.may_read(&path).err().map(|e| e.to_string()),
            });
            println!("{}", json!({ "error": error }));
            Ok(())
        }
        "run" => run(&line.context("run needs the command after --")?, &cwd),
        other => bail!("sandbox takes describe, danger, read, write, search or run, not {other:?}"),
    }
}

/// Why a search from `root` is refused: it would look into `hidden`
/// folders under it (the other agent's search tools run outside the
/// sandbox, so they can't leave them out).
fn search_refused(root: &Path, hidden: &[PathBuf]) -> Option<String> {
    let first = hidden.first()?;
    let more = if hidden.len() > 1 { format!(" and {} more", hidden.len() - 1) } else { String::new() };
    Some(format!(
        "Searching {} would look into {}{more}, which Harlo's sandbox hides (it holds secrets: keys, sign-ins or other sessions). Search a folder below it, such as the project.",
        root.display(),
        first.display()
    ))
}

/// `<what> --cwd <dir> [-- <command>]`.
fn parse(args: &[String]) -> Result<(String, PathBuf, Option<String>)> {
    let what = args.first().cloned().context("sandbox describe|danger|run --cwd <dir> [-- <command>]")?;
    let (mut cwd, mut line) = (None, None);
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--cwd" => cwd = it.next().map(PathBuf::from),
            "--" => {
                line = Some(it.by_ref().cloned().collect::<Vec<_>>().join(" "));
            }
            other => bail!("unknown option {other}"),
        }
    }
    Ok((what, cwd.context("--cwd <dir> is needed")?, line))
}

/// Becomes the command: in the sandbox, or as it is when the sandbox is off.
/// A sandbox that can't start here refuses it (exit 126) and says why.
///
/// Input, output and the process group stay the caller's, so the agent
/// reads the output and stops the command as it would its own `bash -c`.
fn run(line: &str, cwd: &Path) -> Result<()> {
    use std::os::unix::process::CommandExt as _;
    let sandbox = crate::sandbox::for_dir(cwd);
    let mut cmd = match &sandbox {
        Some(sb) => {
            if let Some(why) = &sb.broken {
                eprintln!("Not run: Harlo's sandbox can't start on this machine ({why}).");
                std::process::exit(126);
            }
            let mut c = std::process::Command::new("bwrap");
            c.args(sb.args(line, cwd)).env_clear().envs(sb.env());
            c
        }
        None => {
            let mut c = std::process::Command::new("bash");
            c.arg("-c").arg(line);
            c
        }
    };
    let err = cmd.current_dir(cwd).exec();
    Err(err).with_context(|| format!("couldn't start {}", if sandbox.is_some() { "the sandbox (bwrap)" } else { "bash" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_its_uses() {
        let a = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let (what, cwd, line) = parse(&a(&["danger", "--cwd", "/w", "--", "git", "reset --hard"])).unwrap();
        assert_eq!((what.as_str(), cwd, line.as_deref()), ("danger", PathBuf::from("/w"), Some("git reset --hard")));
        assert_eq!(parse(&a(&["describe", "--cwd", "/w"])).unwrap().2, None);
        parse(&a(&["describe"])).unwrap_err();
        parse(&a(&["run", "--cwd", "/w", "--bogus"])).unwrap_err();
    }

    #[test]
    fn a_search_above_a_hidden_folder_is_refused() {
        assert_eq!(search_refused(Path::new("/home/u"), &[]), None);
        let why = search_refused(Path::new("/home/u"), &[PathBuf::from("/home/u/.ssh"), PathBuf::from("/home/u/.gnupg")]).unwrap();
        assert!(why.starts_with("Searching /home/u would look into /home/u/.ssh and 1 more,"), "{why}");
    }
}
