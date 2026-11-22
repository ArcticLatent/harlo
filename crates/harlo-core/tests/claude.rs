//! Checking Claude Code: installed or not, which account it's signed in to,
//! and how much of the plan's limits is used.
//!
//! Each test runs a stand-in `claude` script in a scratch folder, never the
//! real Claude Code or its sign-in.

// Rust guideline compliant 2026-10-09

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use harlo_core::claude::{Account, Alert, ClaudeCode, Limit, Login, Severity, Watch, check, usage};

/// Writes a stand-in `claude` that prints `status` for `auth status` and
/// exits with `code`.
fn stand_in(folder: &Path, status: &str, code: i32) -> OsString {
    let path = folder.join("claude");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\n\
             case \"$1\" in\n\
             --version) echo '2.1.330 (Claude Code)' ;;\n\
             auth) cat <<'JSON'\n{status}\nJSON\nexit {code} ;;\n\
             esac\n"
        ),
    )
    .expect("the stand-in");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("executable");
    path.into_os_string()
}

#[tokio::test]
async fn a_missing_claude_is_missing() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let found = check(scratch.path().join("no-claude").into_os_string()).await;
    assert_eq!(found, ClaudeCode::Missing);
}

#[tokio::test]
async fn signed_out_is_read_though_claude_exits_with_an_error() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let claude = stand_in(
        scratch.path(),
        r#"{"loggedIn": false, "authMethod": "none", "apiProvider": "firstParty"}"#,
        1,
    );
    assert_eq!(
        check(claude).await,
        ClaudeCode::Installed {
            version: "2.1.330".into(),
            login: Login::SignedOut,
        }
    );
}

#[tokio::test]
async fn a_subscription_shows_its_account_and_plan() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let claude = stand_in(
        scratch.path(),
        r#"{"loggedIn": true, "authMethod": "claude.ai", "email": "a@example.com",
            "orgName": "Example", "subscriptionType": "max", "loginExpiresAt": "2027-01-01"}"#,
        0,
    );
    assert_eq!(
        check(claude).await,
        ClaudeCode::Installed {
            version: "2.1.330".into(),
            login: Login::Subscription(Account {
                email: Some("a@example.com".into()),
                organization: Some("Example".into()),
                plan: Some("max".into()),
            }),
        }
    );
}

#[tokio::test]
async fn another_sign_in_isnt_a_subscription() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let claude = stand_in(
        scratch.path(),
        r#"{"loggedIn": true, "authMethod": "console"}"#,
        0,
    );
    let ClaudeCode::Installed { login, .. } = check(claude).await else {
        panic!("installed");
    };
    assert_eq!(
        login,
        Login::Other {
            method: "console".into()
        }
    );
}

#[tokio::test]
async fn unreadable_status_fails() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let claude = stand_in(scratch.path(), "not json", 0);
    assert!(matches!(check(claude).await, ClaudeCode::Failed(_)));
}

/// What the usage stand-in runs: it answers `initialize`, and answers
/// `get_usage` with the JSON in `answer.json` beside it.
const USAGE_STAND_IN: &str = r#"#!/usr/bin/env python3
import json, os, sys
answer = json.load(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "answer.json")))
for line in sys.stdin:
    request = json.loads(line)
    reply = answer if request["request"]["subtype"] == "get_usage" else {"subtype": "success", "response": {}}
    reply = dict(reply, request_id=request["request_id"])
    print(json.dumps({"type": "control_response", "response": reply}), flush=True)
"#;

/// Writes the usage stand-in in its own folder under `folder`, answering
/// `get_usage` with `answer`.
fn usage_stand_in(folder: &Path, name: &str, answer: &str) -> OsString {
    let folder = folder.join(name);
    std::fs::create_dir_all(&folder).expect("the folder");
    std::fs::write(folder.join("answer.json"), answer).expect("the answer");
    let path = folder.join("claude");
    std::fs::write(&path, USAGE_STAND_IN).expect("the stand-in");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("executable");
    path.into_os_string()
}

#[tokio::test]
async fn a_plans_limits_are_read() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let claude = usage_stand_in(
        scratch.path(),
        "plan",
        r#"{"subtype": "success", "response": {"rate_limits_available": true, "rate_limits": {
            "limits": [
              {"kind": "session", "group": "session", "percent": 37, "severity": "normal",
               "resets_at": "2026-11-22T18:00:00Z", "is_active": false, "scope": {}},
              {"kind": "weekly_scoped", "group": "weekly", "percent": 91.5,
               "severity": "critical", "resets_at": null, "is_active": true,
               "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null}}
            ]}}}"#,
    );
    let limits = usage(claude).await.expect("an answer").expect("limits");
    assert_eq!(
        limits,
        vec![
            Limit {
                kind: "session".into(),
                group: "session".into(),
                model: None,
                percent: 37.0,
                resets_at: Some(1_795_370_400),
                severity: Severity::Normal,
                active: false,
            },
            Limit {
                kind: "weekly_scoped".into(),
                group: "weekly".into(),
                model: Some("Fable".into()),
                percent: 91.5,
                resets_at: None,
                severity: Severity::Critical,
                active: true,
            },
        ]
    );
}

#[tokio::test]
async fn no_plan_has_no_limits_and_errors_say_why() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let none = usage_stand_in(
        scratch.path(),
        "none",
        r#"{"subtype": "success", "response": {"rate_limits_available": false, "rate_limits": null}}"#,
    );
    assert_eq!(usage(none).await, Ok(None));

    let refused = usage_stand_in(
        scratch.path(),
        "refused",
        r#"{"subtype": "error", "error": "not signed in"}"#,
    );
    assert_eq!(usage(refused).await, Err("not signed in".to_owned()));
}

/// A weekly limit `percent` used that resets at `resets_at`.
fn weekly(percent: f64, resets_at: i64) -> Limit {
    Limit {
        kind: "weekly_all".into(),
        group: "weekly".into(),
        model: None,
        percent,
        resets_at: Some(resets_at),
        severity: Severity::Normal,
        active: true,
    }
}

#[test]
fn a_used_up_limit_is_told_once() {
    let mut watch = Watch::default();
    assert_eq!(watch.read(&[weekly(80.0, 1_000)], 0), vec![]);
    assert_eq!(
        watch.read(&[weekly(100.0, 1_000)], 10),
        vec![Alert::UsedUp(weekly(100.0, 1_000))]
    );
    assert_eq!(watch.read(&[weekly(100.0, 1_000)], 20), vec![], "once");
    // Even when the reported reset time moves a little.
    assert_eq!(watch.read(&[weekly(100.0, 1_001)], 30), vec![]);
    // Used up again after its reset: told again.
    assert_eq!(
        watch.read(&[weekly(100.0, 605_800)], 1_100),
        vec![
            Alert::Reset(weekly(100.0, 1_001)),
            Alert::UsedUp(weekly(100.0, 605_800))
        ]
    );
}

#[test]
fn a_nearly_used_limit_is_told_when_it_resets() {
    let mut watch = Watch::default();
    watch.read(&[weekly(93.0, 1_000)], 0);
    assert_eq!(watch.tick(999), vec![]);
    assert_eq!(watch.tick(1_000), vec![Alert::Reset(weekly(93.0, 1_000))]);
    assert_eq!(watch.tick(1_030), vec![], "once");
}

#[test]
fn a_reset_between_readings_is_told_with_the_next_one() {
    let mut watch = Watch::default();
    watch.read(&[weekly(100.0, 1_000)], 0);
    assert_eq!(
        watch.read(&[weekly(3.0, 605_800)], 1_200),
        vec![Alert::Reset(weekly(100.0, 1_000))]
    );
    assert_eq!(watch.tick(2_000), vec![]);
}

#[test]
fn a_limit_less_used_is_not_told() {
    let mut watch = Watch::default();
    watch.read(&[weekly(89.0, 1_000)], 0);
    assert_eq!(watch.tick(1_000), vec![]);
    // Nearly used, then less used before its reset: nothing to tell.
    watch.read(&[weekly(95.0, 5_000)], 1_000);
    watch.read(&[weekly(40.0, 5_000)], 2_000);
    assert_eq!(watch.tick(5_000), vec![]);
}
