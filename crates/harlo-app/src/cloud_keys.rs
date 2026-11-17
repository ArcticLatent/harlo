//! Harlo's cloud model providers, for Settings → Models: the built-in
//! ones harlo-agent knows (`providers --json`) and their API keys, kept by
//! the agent in its private `keys.json` (`key set|test|remove`), here or on
//! another machine. A key goes to the agent on its input, never in a
//! command line.

// Rust guideline compliant 2026-10-09

use std::io::Write as _;
use std::path::Path;
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use harlo_core::remote::{self, Host};
use serde_json::Value;

/// How long the agent has for a listing or a key's change.
const QUICK: Duration = Duration::from_secs(30);
/// How long a key's check has (it asks the provider).
const CHECK: Duration = Duration::from_secs(45);

/// Where the agent runs: its program here, or a machine (whose copy of it
/// harlo puts there).
pub enum Agent<'a> {
    Here(&'a Path),
    There(&'a Host, String),
}

/// The built-in providers, as the agent lists them: `[{ id, name, api,
/// keyOf, baseUrl, key: "saved" | "environment" | "none", keyEnv, keyPage,
/// pricePage, replaced, models: [{ id, name, contextWindow, price: {
/// input, output, cacheRead, cacheWrite } }] }]`.
///
/// # Errors
///
/// When the agent can't be run or its answer isn't a list.
pub fn list(agent: &Agent<'_>) -> Result<Value> {
    let out = run(agent, &["providers", "--json"], None, QUICK)?;
    let text = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(text.trim()).ok().filter(Value::is_array).with_context(|| failure(&out, "Harlo didn't list its providers."))?;
    Ok(v)
}

/// Saves `key` for `provider`.
///
/// # Errors
///
/// What the agent said when it refused it (an empty key, one with spaces),
/// or why it couldn't be run.
pub fn set(agent: &Agent<'_>, provider: &str, key: &str) -> Result<()> {
    let out = run(agent, &["key", "set", provider], Some(key), QUICK)?;
    if !out.status.success() {
        bail!("{}", failure(&out, "The key wasn't saved."));
    }
    Ok(())
}

/// Checks the key saved for `provider` with the provider: Ok with what the
/// check found ("12 models"), Err with why it failed.
///
/// # Errors
///
/// The provider's answer to a key that doesn't work, or why the agent
/// couldn't ask.
pub fn test(agent: &Agent<'_>, provider: &str) -> Result<String> {
    let out = run(agent, &["key", "test", provider, "--json"], None, CHECK)?;
    let text = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(text.trim()).ok().with_context(|| failure(&out, "The key couldn't be checked."))?;
    let message = v["message"].as_str().unwrap_or_default().to_owned();
    if v["ok"] == true { Ok(message) } else { bail!("{message}") }
}

/// Forgets the key saved for `provider`.
///
/// # Errors
///
/// When the agent couldn't be run or its file couldn't be written.
pub fn remove(agent: &Agent<'_>, provider: &str) -> Result<()> {
    let out = run(agent, &["key", "remove", provider], None, QUICK)?;
    if !out.status.success() {
        bail!("{}", failure(&out, "The key wasn't removed."));
    }
    Ok(())
}

/// The agent's last error line, else `fallback`.
fn failure(out: &Output, fallback: &str) -> String {
    let err = String::from_utf8_lossy(&out.stderr);
    harlo_core::signin::plain(&err).lines().map(str::trim).rfind(|l| !l.is_empty()).map_or_else(|| fallback.to_owned(), |l| l.trim_start_matches("Error: ").to_owned())
}

/// Runs the agent with `args`, `input` on its input, for at most `limit`.
fn run(agent: &Agent<'_>, args: &[&str], input: Option<&str>, limit: Duration) -> Result<Output> {
    match agent {
        Agent::There(host, program) => {
            let words: Vec<String> = std::iter::once(program.as_str()).chain(args.iter().copied()).map(remote::quote).collect();
            let script = if input.is_some() { words.join(" ") } else { format!("{} </dev/null", words.join(" ")) };
            remote::run_with_input(host, &script, input.map(str::as_bytes), limit).map_err(|e| anyhow::anyhow!("{e:#}"))
        }
        Agent::Here(program) => {
            let mut child = std::process::Command::new(program)
                .args(args)
                .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .with_context(|| format!("{}", program.display()))?;
            if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
                // (Closed after it, so the agent sees its end.)
                stdin.write_all(text.as_bytes()).context("the key couldn't be handed to Harlo")?;
            }
            let deadline = Instant::now() + limit;
            while child.try_wait().ok().flatten().is_none() {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    bail!("Harlo's agent didn't answer in time.");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(child.wait_with_output()?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in agent (a shell script) that answers as harlo-agent does
    /// and records what it was given.
    fn stand_in(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("agent");
        let script = r#"#!/bin/sh
case "$1 $2" in
  "providers --json") echo '[{"id":"anthropic","name":"Anthropic","key":"none","models":[]}]' ;;
  "key set") cat > "$(dirname "$0")/got-$3"; echo "Saved." ;;
  "key test") if [ -s "$(dirname "$0")/got-$3" ]; then echo '{"ok":true,"message":"Works: 4 models"}'; else echo '{"ok":false,"message":"401: invalid x-api-key"}'; exit 1; fi ;;
  "key remove") rm -f "$(dirname "$0")/got-$3"; echo "Removed." ;;
  *) echo "Error: no such thing" >&2; exit 1 ;;
esac
"#;
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(&path, <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn keys_go_to_the_agent_on_its_input() {
        let dir = std::env::temp_dir().join(format!("harlo-cloud-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // (Removed at the end, passed or failed.)
        struct Gone<'a>(&'a Path);
        impl Drop for Gone<'_> {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(self.0);
            }
        }
        let _gone = Gone(&dir);
        let program = stand_in(&dir);
        let agent = Agent::Here(&program);
        assert_eq!(list(&agent).unwrap()[0]["id"], "anthropic");
        assert_eq!(test(&agent, "anthropic").unwrap_err().to_string(), "401: invalid x-api-key", "the provider's own words");
        set(&agent, "anthropic", "sk-ant-1").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("got-anthropic")).unwrap(), "sk-ant-1", "on its input");
        assert_eq!(test(&agent, "anthropic").unwrap(), "Works: 4 models");
        remove(&agent, "anthropic").unwrap();
        assert!(!dir.join("got-anthropic").exists());
        assert_eq!(run(&agent, &["nothing"], None, QUICK).map(|o| failure(&o, "x")).unwrap(), "no such thing");
    }
}
