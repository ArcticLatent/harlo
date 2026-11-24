//! A session made on OpenCode comes over to Harlo's server whole: exported
//! from a private OpenCode (`HARLO_IMPORT_OPENCODE`, else the last release
//! checked, where the dev setup keeps it) and imported into a private
//! `harlo-server`, its history reads the same and its next turn carries on
//! from it. Importing it again is refused, so nothing is doubled, and
//! copying several brings over only the new ones.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod oc;
mod support;

use std::path::PathBuf;
use std::sync::LazyLock;

use opencode_client::{Delivery, Message, Prompt};

/// Where the dev setup keeps the last OpenCode release the import was
/// checked against.
static OPENCODE: LazyLock<PathBuf> = LazyLock::new(|| {
    std::env::var_os("HARLO_IMPORT_OPENCODE").map_or_else(
        || {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join(".local/share/harlo-dev/opencode/2.4.1/opencode")
        },
        PathBuf::from,
    )
});

/// The OpenCode program to export from, when it's there.
fn opencode() -> Option<PathBuf> {
    OPENCODE.is_file().then(|| OPENCODE.clone())
}

/// Sends `script` to session `id` on `client` and waits for its turn.
async fn turn(client: &opencode_client::Client, id: &str, script: &str) {
    client
        .prompt(id, &Prompt::new(script, Delivery::Queue))
        .await
        .expect("prompt");
    oc::wait_idle(client, id).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_opencode_session_imports_whole_and_carries_on() {
    let Some(program) = opencode() else {
        eprintln!("skipped: no OpenCode at {}", OPENCODE.display());
        return;
    };
    let opencode = oc::Sandbox::start_program(&program).await;
    std::fs::write(opencode.project().join("a.txt"), "alpha\n").expect("file");
    let from = opencode.client();
    let made = from
        .create_session(&opencode.project_str())
        .await
        .expect("session");
    turn(
        &from,
        &made.id,
        r#"think Looking first. | tool read {"path":"a.txt"} ;; say It says alpha."#,
    )
    .await;
    let before = from.messages(&made.id).await.expect("messages");
    let transcript = from.export_session(&made.id).await.expect("exported");

    let harlo = support::Sandbox::start().await;
    let to = harlo.client();
    let folder = harlo.project().to_str().expect("utf-8").to_owned();
    let imported = to
        .import_session(&transcript, &folder)
        .await
        .expect("imported");
    assert_eq!(imported.id, made.id, "the same session");
    let after = to.messages(&made.id).await.expect("messages");
    let shown = |messages: &[Message]| {
        messages
            .iter()
            .filter(|m| !matches!(m, Message::Idle { .. }))
            .map(|m| format!("{m:?}"))
            .collect::<Vec<_>>()
    };
    let (before, after) = (shown(&before), shown(&after));
    assert_eq!(before.len(), after.len(), "{before:#?}\n{after:#?}");
    let all = after.join("\n");
    for wanted in [
        "Looking first.",
        "It says alpha.",
        "name: \"read\"",
        "status: Completed",
    ] {
        assert!(all.contains(wanted), "{wanted}: {all}");
    }

    // The next turn on Harlo's server sends the model what came before.
    turn(&to, &made.id, "say Carried on.").await;
    let sent = support::turn_requests(harlo.dir.path());
    let last = sent.last().expect("a request").to_string();
    for wanted in ["It says alpha.", "1: alpha", "tool_call_id"] {
        assert!(last.contains(wanted), "{wanted}: {last}");
    }
    let done = format!("{:?}", to.messages(&made.id).await.expect("messages"));
    assert!(done.contains("Carried on."), "{done}");

    let again = to.import_session(&transcript, &folder).await;
    assert!(again.is_err(), "imported once");

    // Copying several brings over only what isn't there yet, each into the
    // folder it was in.
    let second = from
        .create_session(&opencode.project_str())
        .await
        .expect("session");
    turn(&from, &second.id, "say Second.").await;
    let copied =
        harlo_core::import::copy_sessions(&from, &to, &[made.id.clone(), second.id.clone()]).await;
    assert_eq!(
        copied,
        harlo_core::import::Imported {
            copied: 1,
            already: 1,
            failed: Vec::new(),
        }
    );
    let there = to.session(&second.id).await.expect("copied");
    assert_eq!(there.directory, opencode.project_str());
    let said = format!("{:?}", to.messages(&second.id).await.expect("messages"));
    assert!(said.contains("Second."), "{said}");

    // After Harlo stops talking to OpenCode, a session left there still
    // comes over: OpenCode's own `session export` writes the file Harlo's
    // "Import a session…" reads.
    let left = from
        .create_session(&opencode.project_str())
        .await
        .expect("session");
    turn(&from, &left.id, "say Left behind.").await;
    let file = opencode.cli(&["session", "export", &left.id]);
    let brought = to.import_session(&file, &folder).await.expect("imported");
    assert_eq!(brought.id, left.id);
    let said = format!("{:?}", to.messages(&left.id).await.expect("messages"));
    assert!(said.contains("Left behind."), "{said}");
}
