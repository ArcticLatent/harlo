//! M0 acceptance tests: attach and detach, state across a disconnect, retries and undo.
//!
//! Each test runs its own private OpenCode server (see `support`). "Another
//! client" is the real `opencode` CLI pointed at that server, as a terminal
//! user would be.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use opencode_client::{
    Client, Delivery, EventKind, Message, Part, PermissionReply, Prompt, ToolStatus, message_id,
};
use support::{Sandbox, wait_for};

/// Counts finished runs in a session's timeline.
async fn idles(client: &Client, session_id: &str) -> usize {
    let messages = client.messages(session_id).await.expect("messages");
    messages
        .iter()
        .filter(|m| matches!(m, Message::Idle { .. }))
        .count()
}

/// Waits until the session has finished `count` runs and isn't working.
async fn wait_runs(client: &Client, session_id: &str, count: usize) {
    wait_for("the run to finish", || async {
        let active = client.active().await.ok()?;
        let done = idles(client, session_id).await >= count;
        (done && !active.iter().any(|id| id == session_id)).then_some(())
    })
    .await;
}

fn texts(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|m| match m {
            Message::Assistant { parts, .. } => Some(parts.iter().filter_map(|p| match p {
                Part::Text(t) => Some(t.clone()),
                _ => None,
            })),
            _ => None,
        })
        .flatten()
        .collect()
}

/// A session started in the terminal shows in Harlo and continues there; closing Harlo leaves it all running.
#[tokio::test(flavor = "multi_thread")]
async fn attach_and_detach() {
    let sandbox = Sandbox::start().await;
    let out = sandbox.cli(&["run", "--format", "json", "say started in the terminal"]);
    let session_id = out
        .lines()
        .find_map(|l| {
            serde_json::from_str::<serde_json::Value>(l).ok()?["sessionID"]
                .as_str()
                .map(str::to_owned)
        })
        .expect("the CLI printed its session");

    let harlo = sandbox.client();
    // The session list is a projection the server updates just after the run,
    // so the terminal's session can take a moment to appear.
    let project = sandbox.project_str();
    wait_for("the terminal's session in the list", || async {
        let sessions = harlo.sessions(Some(&project)).await.ok()?;
        sessions.iter().any(|s| s.id == session_id).then_some(())
    })
    .await;
    let before = harlo.messages(&session_id).await.expect("messages");
    assert_eq!(texts(&before), ["started in the terminal"]);

    harlo
        .prompt(
            &session_id,
            &Prompt::new("say continued in Harlo", Delivery::Steer),
        )
        .await
        .expect("prompt");
    wait_runs(&harlo, &session_id, 2).await;
    let after = harlo.messages(&session_id).await.expect("messages");
    assert_eq!(
        texts(&after),
        ["started in the terminal", "continued in Harlo"]
    );

    // Closing Harlo drops its client; the server and the session stay.
    drop(harlo);
    let other = sandbox.client();
    other.info().await.expect("the server still answers");
    assert_eq!(
        texts(&other.messages(&session_id).await.expect("messages")),
        ["started in the terminal", "continued in Harlo"]
    );
}

/// An approval answered in the terminal while Harlo was away disappears on reconnect, and runs once.
#[tokio::test(flavor = "multi_thread")]
async fn approval_across_disconnect() {
    let sandbox = Sandbox::start().await;
    let harlo = sandbox.client();
    let session = harlo
        .create_session(&sandbox.project_str())
        .await
        .expect("session");

    let mut events = harlo.events().await.expect("events");
    let script = r#"tool shell {"command":"echo hi >> c.txt"} ;; say ran it"#;
    harlo
        .prompt(&session.id, &Prompt::new(script, Delivery::Steer))
        .await
        .expect("prompt");
    let asked = tokio::time::timeout(support::WAIT, async {
        while let Some(event) = events.next().await {
            if let EventKind::PermissionAsked(p) = event.expect("event").kind {
                return p;
            }
        }
        panic!("event stream ended");
    })
    .await
    .expect("an approval was asked");
    assert_eq!(asked.action, "shell");

    // Harlo goes away; the terminal answers.
    drop(events);
    let path = format!("/api/session/{}/permission/{}/reply", session.id, asked.id);
    sandbox.cli(&["api", "POST", &path, "--data", r#"{"decision":"once"}"#]);

    // Harlo reconnects and reloads what the user can act on.
    let _events = harlo.events().await.expect("events again");
    wait_runs(&harlo, &session.id, 1).await;
    assert!(
        harlo
            .permissions(&session.id)
            .await
            .expect("permissions")
            .is_empty(),
        "no stale approval"
    );
    assert!(
        harlo
            .permissions_in(&sandbox.project_str())
            .await
            .expect("permissions")
            .is_empty()
    );
    let messages = harlo.messages(&session.id).await.expect("messages");
    let shell = messages.iter().find_map(|m| match m {
        Message::Assistant { parts, .. } => parts.iter().find_map(|p| match p {
            Part::Tool(t) if t.name == "shell" => Some(t.status),
            _ => None,
        }),
        _ => None,
    });
    assert_eq!(
        shell,
        Some(ToolStatus::Completed),
        "the transcript caught up"
    );
    assert_eq!(texts(&messages), ["ran it"]);

    // The stale card's button is harmless and the command ran once.
    let stale = harlo
        .reply_permission(&session.id, &asked.id, PermissionReply::Once)
        .await
        .expect_err("already answered");
    assert!(stale.is_settled(), "{stale}");
    assert_eq!(
        std::fs::read_to_string(sandbox.project().join("c.txt")).expect("c.txt"),
        "hi\n"
    );
}

/// A question answered in the terminal while Harlo was away disappears on reconnect.
#[tokio::test(flavor = "multi_thread")]
async fn question_across_disconnect() {
    let sandbox = Sandbox::start().await;
    let harlo = sandbox.client();
    let session = harlo
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let question = serde_json::json!({ "questions": [{
        "question": "Which colour?", "header": "Colour",
        "options": [{ "label": "Red", "description": "warm" }, { "label": "Blue", "description": "cool" }]
    }]});
    let script = format!("tool question {question} ;; say got it");
    harlo
        .prompt(&session.id, &Prompt::new(script, Delivery::Steer))
        .await
        .expect("prompt");
    let form = wait_for("a question", || async {
        harlo.forms(&session.id).await.ok()?.into_iter().next()
    })
    .await;
    let key = form.fields[0]["key"]
        .as_str()
        .expect("field key")
        .to_owned();

    let path = format!("/api/session/{}/form/{}/reply", session.id, form.id);
    let answer = serde_json::json!({ "answer": { key.clone(): "Blue" } }).to_string();
    sandbox.cli(&["api", "POST", &path, "--data", &answer]);

    wait_runs(&harlo, &session.id, 1).await;
    assert!(
        harlo.forms(&session.id).await.expect("forms").is_empty(),
        "no stale question"
    );
    assert!(
        harlo
            .forms_in(&sandbox.project_str())
            .await
            .expect("forms")
            .is_empty()
    );
    assert_eq!(
        texts(&harlo.messages(&session.id).await.expect("messages")),
        ["got it"]
    );
    let stale = harlo
        .reply_form(&session.id, &form.id, serde_json::json!({ key: "Red" }))
        .await
        .expect_err("already answered");
    assert!(stale.is_settled(), "{stale}");
}

/// A queued prompt cancelled in the terminal while Harlo was away is gone on reconnect and never sent.
#[tokio::test(flavor = "multi_thread")]
async fn queued_prompt_across_disconnect() {
    let sandbox = Sandbox::start().await;
    let harlo = sandbox.client();
    let session = harlo
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    harlo
        .prompt(
            &session.id,
            &Prompt::new("pace 60 | long 4", Delivery::Steer),
        )
        .await
        .expect("slow turn");
    let queued = message_id();
    harlo
        .prompt(
            &session.id,
            &Prompt {
                id: queued.clone(),
                ..Prompt::new("say the queued one", Delivery::Queue)
            },
        )
        .await
        .expect("queued prompt");
    let inbox = harlo.inbox(&session.id).await.expect("inbox");
    assert_eq!(
        inbox
            .iter()
            .map(|i| (i.id.as_str(), i.delivery))
            .collect::<Vec<_>>(),
        [(queued.as_str(), Delivery::Queue)]
    );

    sandbox.cli(&[
        "api",
        "DELETE",
        &format!("/api/session/{}/inbox/{queued}", session.id),
    ]);

    assert!(
        harlo.inbox(&session.id).await.expect("inbox").is_empty(),
        "no stale queue entry"
    );
    wait_runs(&harlo, &session.id, 1).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let messages = harlo.messages(&session.id).await.expect("messages");
    assert!(
        !messages.iter().any(|m| m.id() == queued),
        "the cancelled prompt was never delivered"
    );
    harlo
        .cancel_inbox(&session.id, &queued)
        .await
        .expect("cancelling again is harmless");
}

/// Sending a prompt again with the same ID stores it and answers it once.
#[tokio::test(flavor = "multi_thread")]
async fn retry_with_same_id_runs_once() {
    let sandbox = Sandbox::start().await;
    let harlo = sandbox.client();
    let session = harlo
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let id = message_id();
    harlo
        .prompt(
            &session.id,
            &Prompt {
                id: id.clone(),
                ..Prompt::new("say once", Delivery::Steer)
            },
        )
        .await
        .expect("first send");
    harlo
        .prompt(
            &session.id,
            &Prompt {
                id: id.clone(),
                ..Prompt::new("say once", Delivery::Steer)
            },
        )
        .await
        .expect("retry");
    wait_runs(&harlo, &session.id, 1).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let messages = harlo.messages(&session.id).await.expect("messages");
    let users = messages
        .iter()
        .filter(|m| matches!(m, Message::User { .. }))
        .count();
    assert_eq!(users, 1, "one user message: {messages:#?}");
    assert_eq!(texts(&messages), ["once"]);
}

/// Undo restores the agent's files, dropping the user's later edits to them, and Cancel brings everything back.
#[tokio::test(flavor = "multi_thread")]
async fn undo_with_user_edit() {
    let sandbox = Sandbox::start().await;
    let harlo = sandbox.client();
    let project = sandbox.project();
    let read = |f: &str| std::fs::read_to_string(project.join(f)).expect("file");
    let session = harlo
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let prompt_id = message_id();
    let script =
        r#"tool edit {"path":"a.txt","oldString":"alpha","newString":"ALPHA"} ;; say edited"#;
    harlo
        .prompt(
            &session.id,
            &Prompt {
                id: prompt_id.clone(),
                ..Prompt::new(script, Delivery::Steer)
            },
        )
        .await
        .expect("prompt");
    wait_runs(&harlo, &session.id, 1).await;
    assert_eq!(read("a.txt"), "ALPHA\n");
    let turn = harlo.diff(&session.id).await.expect("diff");
    assert_eq!(
        turn.iter().map(|d| d.file.as_str()).collect::<Vec<_>>(),
        ["a.txt"]
    );

    // The user edits the agent's file and another one.
    std::fs::write(project.join("a.txt"), "ALPHA\nuser line\n").expect("user edit");
    std::fs::write(project.join("b.txt"), "beta user\n").expect("user edit");

    // Staged without files: nothing changes, and there is no list to preview.
    let dry = harlo
        .stage_revert(&session.id, &prompt_id, false)
        .await
        .expect("stage");
    assert!(dry.files.is_empty());
    assert_eq!(
        (read("a.txt"), read("b.txt")),
        ("ALPHA\nuser line\n".into(), "beta user\n".into())
    );
    harlo.cancel_revert(&session.id).await.expect("cancel");

    // Staged with files: the agent's file goes back, the user's line in it is
    // lost, and the result's patch shows that line being removed.
    let applied = harlo
        .stage_revert(&session.id, &prompt_id, true)
        .await
        .expect("stage");
    assert_eq!(
        applied
            .files
            .iter()
            .map(|d| d.file.as_str())
            .collect::<Vec<_>>(),
        ["a.txt"]
    );
    assert!(
        applied.files[0].patch.contains("-user line"),
        "{}",
        applied.files[0].patch
    );
    assert_eq!(
        (read("a.txt"), read("b.txt")),
        ("alpha\n".into(), "beta user\n".into())
    );

    // Cancel puts every file back as it was, the user's line included.
    harlo.cancel_revert(&session.id).await.expect("cancel");
    assert_eq!(
        (read("a.txt"), read("b.txt")),
        ("ALPHA\nuser line\n".into(), "beta user\n".into())
    );

    // Keep makes it final: files restored, the turn's messages gone.
    harlo
        .stage_revert(&session.id, &prompt_id, true)
        .await
        .expect("stage");
    harlo.commit_revert(&session.id).await.expect("commit");
    assert_eq!(
        (read("a.txt"), read("b.txt")),
        ("alpha\n".into(), "beta user\n".into())
    );
    let left = harlo.messages(&session.id).await.expect("messages");
    assert!(
        !left.iter().any(|m| m.id() == prompt_id),
        "the undone prompt is gone"
    );
}
