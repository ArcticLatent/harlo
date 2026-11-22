//! Checking Claude Code: installed or not, and which account it's signed
//! in to.
//!
//! Each test runs a stand-in `claude` script in a scratch folder, never the
//! real Claude Code or its sign-in.

// Rust guideline compliant 2026-10-09

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use harlo_core::claude::{Account, ClaudeCode, Login, check};

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
