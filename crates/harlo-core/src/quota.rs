//! Provider plan limits: from `omp usage --json` (every account omp is
//! signed in to), and Claude Code's (its `/usage`, which Claude Code reads
//! with its own sign-in). Per account its plan and each limit window's use.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

/// One account's limits, flattened for display:
/// `{ provider, plan, account, limitReached, windows: [{ label, usedFraction,
/// used, limit, unit, resetsAt (ms), status }] }`.
pub fn parse(report: &Value) -> Vec<Value> {
    report["reports"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            let meta = &r["metadata"];
            let windows: Vec<Value> = r["limits"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|l| {
                    let a = &l["amount"];
                    json!({
                        "label": l["label"].as_str().or_else(|| l["window"]["label"].as_str()).unwrap_or("limit"),
                        "usedFraction": a["usedFraction"].as_f64().unwrap_or(0.0),
                        "used": a["used"], "limit": a["limit"], "unit": a["unit"],
                        "resetsAt": l["window"]["resetsAt"],
                        "status": l["status"].as_str().unwrap_or("ok"),
                    })
                })
                .collect();
            json!({
                "provider": r["provider"].as_str().unwrap_or("provider"),
                "plan": meta["planType"].as_str().unwrap_or(""),
                "account": meta["email"].as_str().or_else(|| meta["accountId"].as_str()).unwrap_or(""),
                "limitReached": meta["limitReached"].as_bool().unwrap_or(false),
                "windows": windows,
            })
        })
        .collect()
}

/// Runs `<omp> usage --json` (up to a minute) and parses it.
pub fn fetch(omp: &Path) -> Result<Vec<Value>> {
    let mut child = Command::new(omp)
        .args(["usage", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("running {} usage", omp.display()))?;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            bail!("omp usage took too long");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("omp usage failed: {}", err.lines().last().unwrap_or("").trim());
    }
    let report: Value = serde_json::from_slice(&out.stdout).context("reading omp usage output")?;
    Ok(parse(&report))
}

/// Claude Code's plan limits, as Claude Code itself reports them: its
/// `/usage` data, asked over its stream-json control channel (`get_usage`).
/// No prompt is sent, so nothing is spent; its hooks stay off for the run.
/// Claude Code asks Anthropic with its own sign-in: harlo never reads it
/// (Anthropic doesn't let other apps use a Claude subscription's tokens).
/// None: no plan limits (an API key, Bedrock, Vertex).
pub fn claude(program: &Path, config_dir: &Path) -> Result<Option<Value>> {
    use std::io::{BufRead, Write};
    let mut child = Command::new(program)
        .args(["--output-format", "stream-json", "--verbose", "--input-format", "stream-json", "--no-session-persistence", "--strict-mcp-config",
               "--setting-sources=user", "--settings", r#"{"disableAllHooks":true}"#])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("running {}", program.display()))?;
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
    let answer = rx.recv_timeout(Duration::from_secs(45));
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    let r = answer.map_err(|_| anyhow::anyhow!("Claude Code didn't say its limits in time"))?;
    if r["subtype"] != "success" {
        bail!("Claude Code couldn't read its limits: {}", r["error"].as_str().unwrap_or("it said no"));
    }
    let usage = &r["response"];
    if usage["rate_limits_available"].as_bool() != Some(true) || !usage["rate_limits"].is_object() {
        return Ok(None);
    }
    let plan = claude_plan(usage["subscription_type"].as_str().unwrap_or_default());
    Ok(Some(parse_claude(&usage["rate_limits"], &plan, &claude_account(config_dir))))
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
            windows.push(window(label.to_owned(), w["utilization"].as_f64(), resets(&w["resets_at"])));
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

/// A running Claude session's word on its limits (`rate_limit_info`, after
/// each request) into its account in `accounts` (added when there's none
/// yet): the 5-hour and weekly windows, the rest as they were. Returns the
/// windows that have just run out.
pub fn apply_claude_live(accounts: &mut Vec<Value>, info: &Value) -> Vec<String> {
    let mut seen: Vec<(&str, &Value)> = Vec::new();
    for (key, label) in [("five_hour", "Session (5 h)"), ("seven_day", "Weekly")] {
        let w = &info["unifiedWindows"][key];
        if w["utilization"].is_number() {
            seen.push((label, w));
        } else if info["rateLimitType"] == key && info["utilization"].is_number() {
            seen.push((label, info));
        }
    }
    if seen.is_empty() {
        return Vec::new();
    }
    let at = match accounts.iter().position(|a| a["provider"] == "Claude") {
        Some(i) => i,
        None => {
            accounts.insert(0, json!({ "provider": "Claude", "plan": "", "account": "", "limitReached": false, "windows": [] }));
            0
        }
    };
    let account = &mut accounts[at];
    if !account["windows"].is_array() {
        account["windows"] = json!([]);
    }
    let windows = account["windows"].as_array_mut().expect("an array");
    let mut ran_out = Vec::new();
    for (label, w) in seen {
        let used = w["utilization"].as_f64().unwrap_or(0.0);
        let out = used >= 1.0 || w["status"] == "rejected";
        let reset = w["resetsAt"].as_i64().map(|s| s * 1000);
        let window = json!({ "label": label, "usedFraction": used, "used": (used * 100.0).round(), "limit": 100, "unit": "percent",
                             "resetsAt": reset, "status": if out { "exhausted" } else { "ok" } });
        match windows.iter_mut().find(|x| x["label"] == label) {
            Some(x) => {
                if out && x["status"] != "exhausted" {
                    ran_out.push(label.to_owned());
                }
                *x = window;
            }
            None => {
                if out {
                    ran_out.push(label.to_owned());
                }
                let place = if label == "Weekly" { windows.len().min(1) } else { 0 };
                windows.insert(place, window);
            }
        }
    }
    let reached = windows.iter().any(|w| w["status"] == "exhausted" || w["usedFraction"].as_f64().unwrap_or(0.0) >= 1.0);
    account["limitReached"] = json!(reached);
    ran_out
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

/// When a limit resets (unix ms), as a notification says it: "18:30", or
/// "Thu 09:00" further off than a day.
pub fn reset_time(at: i64) -> String {
    use chrono::TimeZone;
    let Some(t) = chrono::Local.timestamp_millis_opt(at).single() else { return String::new() };
    if t - chrono::Local::now() > chrono::Duration::hours(20) { t.format("%a %H:%M").to_string() } else { t.format("%H:%M").to_string() }
}

/// How a provider's limit reads in a notification: "Claude's 5-hour limit".
pub fn limit_name(provider: &str, label: &str) -> String {
    let who = match provider {
        "Claude" => "Claude",
        "openai-codex" => "ChatGPT",
        "anthropic" => "Claude",
        "google-gemini-cli" | "google-antigravity" => "Gemini",
        p => p,
    };
    let what = match label {
        "Session (5 h)" => "5-hour limit".to_owned(),
        "Weekly" => "weekly limit".to_owned(),
        l if l.starts_with("Weekly · ") => format!("weekly {} limit", &l["Weekly · ".len()..]),
        l => format!("{l} limit"),
    };
    format!("{who}'s {what}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_session_says_its_limits() {
        let info = |five: f64, status: &str| json!({ "status": status, "rateLimitType": "five_hour", "unifiedWindows": {
            "five_hour": { "utilization": five, "resetsAt": 1793653800, "status": status },
            "seven_day": { "utilization": 0.06, "resetsAt": 1794229200, "status": "allowed" } } });
        // None asked for yet: the account comes from it.
        let mut accounts = vec![json!({ "provider": "openai-codex", "windows": [] })];
        assert!(apply_claude_live(&mut accounts, &info(0.92, "allowed_warning")).is_empty());
        let a = &accounts[0];
        assert_eq!(a["provider"], "Claude");
        assert_eq!(a["windows"][0]["label"], "Session (5 h)");
        assert_eq!(a["windows"][0]["usedFraction"], 0.92);
        assert_eq!(a["windows"][0]["resetsAt"], 1793653800000_i64);
        assert_eq!(a["windows"][1]["label"], "Weekly");
        assert_eq!(a["limitReached"], false);
        // Asked for earlier: its per-model windows stay.
        accounts[0]["windows"].as_array_mut().unwrap().push(json!({ "label": "Weekly · Opus", "usedFraction": 0.3, "status": "ok" }));
        assert_eq!(apply_claude_live(&mut accounts, &info(1.01, "rejected")), ["Session (5 h)"]);
        assert_eq!(accounts[0]["windows"].as_array().unwrap().len(), 3);
        assert_eq!(accounts[0]["windows"][0]["status"], "exhausted");
        assert_eq!(accounts[0]["limitReached"], true);
        assert!(apply_claude_live(&mut accounts, &info(1.02, "rejected")).is_empty(), "said once");
        assert!(apply_claude_live(&mut accounts, &json!({ "status": "allowed" })).is_empty());
        assert_eq!(accounts.len(), 2);

        // It resets: said once it's past, then nothing's used until fresh numbers.
        let at = 1793653800000_i64;
        assert!(resets_due(&accounts, 0, at - 1).is_empty());
        assert_eq!(resets_due(&accounts, 0, at), [("Claude".to_owned(), "Session (5 h)".to_owned(), at)]);
        assert!(resets_due(&accounts, at, at + 10).is_empty(), "not one that reset before harlo started");
        mark_reset(&mut accounts, "Claude", "Session (5 h)");
        assert_eq!(accounts[0]["windows"][0]["usedFraction"], 0.0);
        assert_eq!(accounts[0]["limitReached"], false);
        assert!(resets_due(&accounts, 0, at + 10).is_empty());
        // (A window hardly used resets without a word.)
        assert_eq!(limit_name("Claude", "Session (5 h)"), "Claude's 5-hour limit");
        assert_eq!(limit_name("openai-codex", "7 days"), "ChatGPT's 7 days limit");
        assert_eq!(limit_name("Claude", "Weekly · Opus"), "Claude's weekly Opus limit");
    }

    #[test]
    fn parses_omp_usage() {
        let report = json!({ "generatedAt": 1, "reports": [{
            "provider": "openai-codex", "fetchedAt": 1,
            "limits": [{ "id": "openai-codex:primary", "label": "7 days", "scope": { "provider": "openai-codex", "windowId": "7d", "shared": true },
                "window": { "id": "7d", "label": "7 days", "durationMs": 604800000, "resetsAt": 1792300303000_i64 },
                "amount": { "used": 2, "limit": 100, "remaining": 98, "usedFraction": 0.02, "remainingFraction": 0.98, "unit": "percent" },
                "status": "ok" }],
            "metadata": { "planType": "prolite", "allowed": true, "limitReached": false, "email": "me@example.com" } }],
            "accountsWithoutUsage": [], "disabledCredentials": [] });
        let accounts = parse(&report);
        assert_eq!(accounts.len(), 1);
        let a = &accounts[0];
        assert_eq!(a["provider"], "openai-codex");
        assert_eq!(a["plan"], "prolite");
        assert_eq!(a["account"], "me@example.com");
        assert_eq!(a["windows"][0]["label"], "7 days");
        assert_eq!(a["windows"][0]["usedFraction"], 0.02);
        assert_eq!(a["windows"][0]["resetsAt"], 1792300303000_i64);
        assert!(parse(&json!({})).is_empty());
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
    #[ignore]
    fn claude_here() {
        let dir = harlo_harness::claude::config_dir().unwrap();
        let program = harlo_harness::resolve_program(harlo_harness::HarnessKind::Claude).unwrap();
        let a = claude(&program, &dir).unwrap().expect("signed in with claude.ai");
        for w in a["windows"].as_array().unwrap() {
            println!("{} {}: {:.0}% (resets {})", a["plan"], w["label"], w["usedFraction"].as_f64().unwrap() * 100.0, w["resetsAt"]);
        }
    }
}
