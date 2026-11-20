//! Provider plan limits, per account its plan and each limit window's use.
//!
//! The Claude account's behind pi's Claude subscription (Claude Code's
//! `/usage`, which Claude Code reads with its own sign-in).

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

/// Claude Code's plan limits, as Claude Code itself reports them.
///
/// Its `/usage` data, asked over its stream-json control channel (`get_usage`).
/// No prompt is sent, so nothing is spent; its hooks stay off for the run.
/// Claude Code asks Anthropic with its own sign-in: harlo never reads it
/// (Anthropic doesn't let other apps use a Claude subscription's tokens). None:
/// no plan limits (an API key, Bedrock, Vertex).
///
/// # Errors
///
/// When Claude Code can't be started or doesn't answer in time.
pub fn claude(program: &Path, config_dir: &Path) -> Result<Option<Value>> {
    let mut cmd = Command::new(program);
    cmd.args(CLAUDE_USAGE_ARGS).current_dir(std::env::temp_dir());
    let Some(usage) = claude_usage(cmd, Duration::from_secs(45))? else { return Ok(None) };
    let plan = claude_plan(usage["subscription_type"].as_str().unwrap_or_default());
    Ok(Some(parse_claude(&usage["rate_limits"], &plan, &claude_account(config_dir))))
}

/// Claude Code with no prompt: its control channel only, no hooks, no
/// session saved, only the user's settings.
const CLAUDE_USAGE_ARGS: [&str; 10] = ["--output-format", "stream-json", "--verbose", "--input-format", "stream-json", "--no-session-persistence", "--strict-mcp-config",
    "--setting-sources=user", "--settings", r#"{"disableAllHooks":true}"#];

/// Asks the Claude Code that `cmd` starts for its usage (`get_usage`);
/// None when it has no plan limits.
fn claude_usage(mut cmd: Command, wait: Duration) -> Result<Option<Value>> {
    use std::io::{BufRead, Write};
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("running Claude Code")?;
    let mut stdin = child.stdin.take().context("no input")?;
    for (id, request) in [("harlo-init", json!({ "subtype": "initialize" })), ("harlo-usage", json!({ "subtype": "get_usage", "skip_behaviors": true }))] {
        writeln!(stdin, "{}", json!({ "type": "control_request", "request_id": id, "request": request }))?;
    }
    stdin.flush()?;
    let stdout = child.stdout.take().context("no output")?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(m) = serde_json::from_str::<Value>(&line) else { continue };
            if m["type"] == "control_response" && m["response"]["request_id"] == "harlo-usage" {
                let _ = tx.send(m["response"].clone());
                return;
            }
        }
    });
    let answer = rx.recv_timeout(wait);
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    let r = answer.ok().ok_or_else(|| anyhow::anyhow!("Claude Code didn't say its limits in time"))?;
    if r["subtype"] != "success" {
        bail!("Claude Code couldn't read its limits: {}", r["error"].as_str().unwrap_or("it said no"));
    }
    let usage = &r["response"];
    if usage["rate_limits_available"].as_bool() != Some(true) || !usage["rate_limits"].is_object() {
        return Ok(None);
    }
    Ok(Some(usage.clone()))
}

/// "pro" → "Pro", "max" → "Max".
fn claude_plan(subscription: &str) -> String {
    let mut c = subscription.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// The signed-in address, from Claude Code's `.claude.json` (in the config
/// folder when it's set, else the home folder).
fn claude_account(config_dir: &Path) -> String {
    let file = if std::env::var_os("CLAUDE_CONFIG_DIR").is_some() {
        config_dir.join(".claude.json")
    } else {
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".claude.json")
    };
    std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["oauthAccount"]["emailAddress"].as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Claude's usage answer → an account as `parse` gives: the session (5 h)
/// and weekly windows, then the weekly ones per model.
pub fn parse_claude(body: &Value, plan: &str, account: &str) -> Value {
    let resets = |v: &Value| v.as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp_millis());
    let window = |label: String, used: Option<f64>, reset: Option<i64>| {
        let f = used.unwrap_or(0.0) / 100.0;
        json!({ "label": label, "usedFraction": f, "used": used, "limit": 100, "unit": "percent", "resetsAt": reset,
                "status": if f >= 1.0 { "exhausted" } else { "ok" } })
    };
    let mut windows = Vec::new();
    for (key, label) in [("five_hour", "Session (5 h)"), ("seven_day", "Weekly")] {
        let w = &body[key];
        if w.is_object() {
            // Whole percents, rounded: 100 can be 99.6, still allowed. Used up
            // only when it says it's locked (a running session's own report
            // says it as soon as a message is refused).
            let used = w["utilization"].as_f64().map(|u| if u >= 100.0 && w["locked_reason"].is_null() { 99.5 } else { u });
            windows.push(window(label.to_owned(), used, resets(&w["resets_at"])));
        }
    }
    // Per model: `limits[]` (weekly, scoped), or the older `seven_day_<model>` keys.
    let mut scoped: Vec<(String, Option<f64>, Option<i64>)> = Vec::new();
    for l in body["limits"].as_array().into_iter().flatten().filter(|l| l["kind"] == "weekly_scoped") {
        let scope = &l["scope"];
        let name = ["model", "surface"].iter().find_map(|d| scope[d]["display_name"].as_str().or_else(|| scope[d]["id"].as_str()).filter(|n| !n.trim().is_empty()));
        if let Some(name) = name {
            scoped.push((name.trim().to_owned(), l["percent"].as_f64(), resets(&l["resets_at"])));
        }
    }
    // Claude Code's `/usage`: `model_scoped`.
    for m in body["model_scoped"].as_array().into_iter().flatten() {
        if let Some(name) = m["display_name"].as_str().filter(|n| !n.trim().is_empty())
            && !scoped.iter().any(|(n, ..)| n.eq_ignore_ascii_case(name.trim()))
        {
            scoped.push((name.trim().to_owned(), m["utilization"].as_f64(), resets(&m["resets_at"])));
        }
    }
    for (key, name) in [("seven_day_opus", "Opus"), ("seven_day_sonnet", "Sonnet")] {
        let w = &body[key];
        if w.is_object() && !scoped.iter().any(|(n, ..)| n.eq_ignore_ascii_case(name)) {
            scoped.push((name.to_owned(), w["utilization"].as_f64(), resets(&w["resets_at"])));
        }
    }
    windows.extend(scoped.into_iter().map(|(name, used, reset)| window(format!("Weekly · {name}"), used, reset)));
    let reached = windows.iter().any(|w| w["usedFraction"].as_f64().unwrap_or(0.0) >= 1.0);
    json!({ "provider": "Claude", "plan": plan, "account": account, "limitReached": reached, "windows": windows })
}

/// From this much of a window used, harlo warns, and says when it resets.
pub const NEARLY: f64 = 0.9;

/// A limit window's name for its length ("5 hours", "7 days").
#[expect(clippy::cast_possible_truncation, reason = "whole pixels, sizes and times from floats: `as` rounds toward zero and saturates, as wanted here")]
pub fn window_label(minutes: i64) -> String {
    let secs = minutes.max(0) * 60;
    let (n, unit) = if secs >= 86_400 { ((secs as f64 / 86_400.0).round() as i64, "day") } else { (((secs as f64 / 3600.0).round() as i64).max(1), "hour") };
    format!("{n} {unit}{}", if n == 1 { "" } else { "s" })
}

/// The windows (provider, label, when) that were nearly or fully used and
/// have reset by `now`, at a time after `since` (unix ms).
pub fn resets_due(accounts: &[Value], since: i64, now: i64) -> Vec<(String, String, i64)> {
    let mut out = Vec::new();
    for a in accounts {
        for w in a["windows"].as_array().into_iter().flatten() {
            let Some(at) = w["resetsAt"].as_i64() else { continue };
            let used = w["usedFraction"].as_f64().unwrap_or(0.0);
            if at > since && at <= now && (used >= NEARLY || w["status"] == "exhausted") {
                out.push((a["provider"].as_str().unwrap_or_default().to_owned(), w["label"].as_str().unwrap_or_default().to_owned(), at));
            }
        }
    }
    out
}

/// Whose limits a session draws on: Claude's sign-in; "" for one without
/// plan limits.
pub fn session_provider(harness: &str, model_id: &str) -> String {
    // pi: Claude's models through the Claude subscription extension
    // (Claude Code, on the plan); pi's own `anthropic/…` say none.
    if harness == "pi" && model_id.split('/').next() == Some("claude-bridge") {
        return "Claude".to_owned();
    }
    String::new()
}

/// A session's plan limit as the line above the composer says it.
///
/// None: under 90% everywhere. Else its fullest window of 90% or more (`label`,
/// `usedFraction`, `resetsAt`), whether one is used up (`out`), and when work
/// can go on (`resetAt`: when the last used-up window resets, 0 when none is
/// used up). `accounts` are its machine's.
pub fn session_limit(accounts: &[Value], harness: &str, model_id: &str) -> Option<Value> {
    let provider = session_provider(harness, model_id);
    if provider.is_empty() {
        return None;
    }
    let windows: Vec<&Value> = accounts.iter().filter(|a| a["provider"] == provider.as_str()).flat_map(|a| a["windows"].as_array().into_iter().flatten()).collect();
    let used = |w: &Value| w["usedFraction"].as_f64().unwrap_or(0.0);
    let gone = |w: &Value| w["status"] == "exhausted" || used(w) >= 1.0;
    let worst = windows.iter().copied().filter(|w| used(w) >= NEARLY || gone(w)).fold(None::<&Value>, |best, w| match best {
        Some(b) if used(b) >= used(w) => Some(b),
        _ => Some(w),
    })?;
    let out = gone(worst);
    let reset_at = if out { windows.iter().copied().filter(|w| gone(w)).filter_map(|w| w["resetsAt"].as_i64()).max().unwrap_or(0) } else { 0 };
    Some(json!({ "provider": provider, "label": worst["label"], "usedFraction": used(worst), "resetsAt": worst["resetsAt"], "out": out, "resetAt": reset_at }))
}

/// A window that has reset: nothing used, until fresh numbers come.
pub fn mark_reset(accounts: &mut [Value], provider: &str, label: &str) {
    for a in accounts.iter_mut().filter(|a| a["provider"] == provider) {
        for w in a["windows"].as_array_mut().into_iter().flatten().filter(|w| w["label"] == label) {
            w["usedFraction"] = json!(0.0);
            w["used"] = json!(0);
            w["status"] = json!("ok");
            w["resetsAt"] = Value::Null;
        }
        let reached = a["windows"].as_array().into_iter().flatten().any(|w| w["status"] == "exhausted");
        a["limitReached"] = json!(reached);
    }
}

/// How a provider's limit reads in a notification: "Claude's 5-hour limit".
pub fn limit_name(provider: &str, label: &str) -> String {
    let who = match provider {
        "Claude" | "anthropic" => "Claude",
        p => p,
    };
    let what = match label {
        "Session (5 h)" | "5 hours" => "5-hour limit".to_owned(),
        "Weekly" | "7 days" => "weekly limit".to_owned(),
        l if l.starts_with("Weekly · ") => format!("weekly {} limit", &l["Weekly · ".len()..]),
        l => format!("{l} limit"),
    };
    format!("{who}'s {what}")
}

/// The limits shown after a refresh, from what each source answered.
///
/// `previous` is what was shown. Claude not asked (`None`) adds nothing,
/// except its last accounts when `keep_claude`. When it failed, it keeps its
/// last accounts and adds its error. The time of the update is `now_ms` when
/// nothing failed, else the previous one.
pub fn refreshed(previous: &Value, claude: Option<Result<Option<Value>>>, keep_claude: bool, now_ms: i64) -> Value {
    let kept = || -> Vec<Value> { previous["accounts"].as_array().into_iter().flatten().filter(|a| a["provider"] == "Claude").cloned().collect() };
    let (mut accounts, mut errors) = (Vec::new(), Vec::new());
    match claude {
        Some(Ok(Some(a))) => accounts.push(a),
        None if keep_claude => accounts.extend(kept()),
        Some(Ok(None)) | None => {}
        Some(Err(e)) => {
            accounts.extend(kept());
            errors.push(format!("Claude: {e:#}"));
        }
    }
    let updated = if errors.is_empty() { json!(now_ms) } else { previous["updatedAt"].clone() };
    json!({ "accounts": accounts, "updatedAt": updated, "error": errors.join("\n") })
}

/// Sessions waiting for a plan limit to reset: session file → reset time.
///
/// The times are milliseconds since the epoch, as saved. Text that isn't
/// such a map reads as none.
pub fn resumes(text: &str) -> serde_json::Map<String, Value> {
    serde_json::from_str(text).unwrap_or_default()
}

/// Which sessions' limits have reset by `now`, and whether to give up on each.
///
/// A minute's grace lets the limit lift; a day past it, continuing has no
/// point any more.
pub fn due_resumes(resumes: &serde_json::Map<String, Value>, now: i64) -> Vec<(String, bool)> {
    resumes
        .iter()
        .filter_map(|(id, at)| {
            let at = at.as_i64()?;
            (now >= at + 60_000).then(|| (id.clone(), now > at + 24 * 3600 * 1000))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_windows_and_limits() {
        use super::*;
        assert_eq!((window_label(300), window_label(10080), window_label(1440), window_label(30), window_label(60)), ("5 hours".into(), "7 days".into(), "1 day".into(), "1 hour".into(), "1 hour".into()));
        assert_eq!(session_provider("claude", "anthropic/claude-sonnet-5-5"), "", "Claude Code's old sessions draw on nothing harlo shows");
        assert_eq!(session_provider("harlo", "codex/gpt-6"), "", "nor do the old agent's");
        assert_eq!(limit_name("Claude", "7 days"), "Claude's weekly limit");
    }

    #[test]
    fn a_sessions_limit() {
        let accounts = vec![
            json!({ "provider": "Claude", "windows": [
                { "label": "Session (5 h)", "usedFraction": 1.0, "status": "exhausted", "resetsAt": 2000 },
                { "label": "Weekly", "usedFraction": 0.95, "status": "ok", "resetsAt": 9000 },
            ] }),
        ];
        let claude = session_limit(&accounts, "pi", "claude-bridge/claude-opus-5-5").unwrap();
        assert_eq!((claude["label"].as_str(), claude["out"].as_bool(), claude["resetAt"].as_i64()), (Some("Session (5 h)"), Some(true), Some(2000)));
        // A model without plan limits.
        assert!(session_limit(&accounts, "pi", "llama/x").is_none());
        assert_eq!((session_provider("pi", "claude-bridge/claude-opus-5-5"), session_provider("pi", "anthropic/claude-opus-5-5")), ("Claude".into(), String::new()), "pi on the plan through Claude Code; pi's own Anthropic isn't");
    }

    use super::*;

    #[test]
    fn a_used_up_window_resets() {
        let at = 1793653800000_i64;
        let mut accounts = vec![
            json!({ "provider": "Claude", "limitReached": true, "windows": [
                { "label": "Session (5 h)", "usedFraction": 1.01, "status": "exhausted", "resetsAt": at },
                { "label": "Weekly", "usedFraction": 0.06, "status": "ok", "resetsAt": 1794229200000_i64 },
                { "label": "Weekly · Opus", "usedFraction": 0.3, "status": "ok" },
            ] }),
        ];
        // It resets: said once it's past, then nothing's used until fresh numbers.
        assert!(resets_due(&accounts, 0, at - 1).is_empty());
        assert_eq!(resets_due(&accounts, 0, at), [("Claude".to_owned(), "Session (5 h)".to_owned(), at)]);
        assert!(resets_due(&accounts, at, at + 10).is_empty(), "not one that reset before harlo started");
        mark_reset(&mut accounts, "Claude", "Session (5 h)");
        assert_eq!(accounts[0]["windows"][0]["usedFraction"], 0.0);
        assert_eq!(accounts[0]["limitReached"], false);
        assert!(resets_due(&accounts, 0, at + 10).is_empty());
        // (A window hardly used resets without a word.)
        assert_eq!(limit_name("Claude", "Session (5 h)"), "Claude's 5-hour limit");
        assert_eq!(limit_name("Claude", "Weekly · Opus"), "Claude's weekly Opus limit");
    }

    #[test]
    fn parses_claude_usage() {
        let body = json!({
            "five_hour": { "utilization": 42.0, "resets_at": "2026-10-13T20:00:00.000000+00:00" },
            "seven_day": { "utilization": 92.0, "resets_at": "2026-10-16T09:00:00+00:00" },
            "seven_day_opus": { "utilization": 10.0, "resets_at": null },
            "limits": [
                { "kind": "weekly_scoped", "percent": 15.0, "resets_at": null, "scope": { "model": { "display_name": "Opus" } } },
                { "kind": "weekly_scoped", "percent": 100.0, "scope": { "surface": { "id": "fable" } } },
                { "kind": "other" }
            ],
        });
        // Rounded up to 100 but not locked: nearly, not used up.
        let nearly = parse_claude(&json!({ "five_hour": { "utilization": 100, "locked_reason": null } }), "Pro", "");
        assert_eq!((nearly["windows"][0]["status"].as_str(), nearly["limitReached"].as_bool()), (Some("ok"), Some(false)));
        let locked = parse_claude(&json!({ "five_hour": { "utilization": 100, "locked_reason": "rate_limited" } }), "Pro", "");
        assert_eq!(locked["windows"][0]["status"], "exhausted");
        let a = parse_claude(&body, "Pro", "me@example.com");
        assert_eq!((a["provider"].as_str(), a["plan"].as_str(), a["account"].as_str()), (Some("Claude"), Some("Pro"), Some("me@example.com")));
        let labels: Vec<&str> = a["windows"].as_array().unwrap().iter().map(|w| w["label"].as_str().unwrap()).collect();
        assert_eq!(labels, ["Session (5 h)", "Weekly", "Weekly · Opus", "Weekly · fable"]);
        assert_eq!(a["windows"][0]["usedFraction"], 0.42);
        assert_eq!(a["windows"][0]["resetsAt"], 1791921600000_i64);
        // limits[] wins over the older key.
        assert_eq!(a["windows"][2]["usedFraction"], 0.15);
        assert_eq!(a["limitReached"], true);
        assert_eq!(claude_plan("max"), "Max");
    }

    /// This computer's Claude Code, asked for its limits (run by hand:
    /// `cargo test -p harlo-core claude_here -- --ignored --nocapture`).
    #[test]
    #[ignore = "asks this machine's own Claude Code"]
    fn claude_here() {
        let dir = crate::claude::config_dir().unwrap();
        let program = crate::claude::program().unwrap();
        let a = claude(&program, &dir).unwrap().expect("signed in with claude.ai");
        for w in a["windows"].as_array().unwrap() {
            println!("{} {}: {:.0}% (resets {})", a["plan"], w["label"], w["usedFraction"].as_f64().unwrap() * 100.0, w["resetsAt"]);
        }
    }


    #[test]
    fn a_session_continues_a_minute_after_its_limit_resets() {
        let resumes = super::resumes(r#"{"/a.jsonl": 1000000, "/b.jsonl": 2000000}"#);
        assert!(super::due_resumes(&resumes, 1_000_000).is_empty());
        assert_eq!(super::due_resumes(&resumes, 1_060_000), [("/a.jsonl".to_owned(), false)]);
        assert_eq!(super::due_resumes(&resumes, 90_000_000).iter().filter(|(_, give_up)| *give_up).count(), 2);
        assert!(super::resumes("").is_empty());
    }

    #[test]
    fn a_failed_source_keeps_its_last_limits() {
        use serde_json::json;
        let previous = json!({ "accounts": [{ "provider": "Claude", "plan": "max" }], "updatedAt": 5 });
        let fresh = json!({ "provider": "Claude", "plan": "pro" });
        let got = super::refreshed(&previous, Some(Ok(Some(fresh.clone()))), false, 9);
        assert_eq!(got, json!({ "accounts": [fresh], "updatedAt": 9, "error": "" }));
        let kept = super::refreshed(&previous, None, true, 9);
        assert_eq!(kept, json!({ "accounts": [{ "provider": "Claude", "plan": "max" }], "updatedAt": 9, "error": "" }));
        let failed = super::refreshed(&previous, Some(Err(anyhow::anyhow!("no sign-in"))), false, 9);
        assert_eq!(failed, json!({ "accounts": [{ "provider": "Claude", "plan": "max" }], "updatedAt": 5, "error": "Claude: no sign-in" }));
    }
}
