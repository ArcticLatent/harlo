//! M1 at the client level: `opencode-client`, unchanged, drives one whole
//! turn on `harlo-server` with the scripted model. An edit asks, is
//! approved and shows its diff. The server restarts and the session is
//! still there, and a client that names the last event it saw gets the
//! rest, once and in order.
//!
//! Each test runs its own server and model with a scratch `HOME` and XDG
//! folders and a random password.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use opencode_client::{
    Delivery, Effect, EventKind, Message, Part, PermissionReply, Prompt, Rule, ToolStatus,
};
use support::{Sandbox, replay, until};

const EDIT: &str = r#"tool edit {"path":"a.txt","oldString":"alpha","newString":"omega"} ;; say Changed alpha to omega."#;

#[tokio::test]
async fn one_approved_edit_then_a_restart_keeps_the_session() {
    let mut sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let mut events = client.events().await.expect("events");

    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .set_permissions(&session.id, &[Rule::any("edit", Effect::Ask)])
        .await
        .expect("careful");
    client
        .prompt(&session.id, &Prompt::new(EDIT, Delivery::Queue))
        .await
        .expect("prompt");

    let asked = until(&mut events, |kind| match kind {
        EventKind::PermissionAsked(p) => Some(*p),
        _ => None,
    })
    .await;
    assert_eq!(
        (asked.action.as_str(), asked.resources.as_slice()),
        ("edit", ["a.txt".to_owned()].as_slice())
    );
    assert!(asked.files[0].patch.contains("+omega"), "{:?}", asked.files);
    assert!(
        asked
            .message
            .as_deref()
            .is_some_and(|m| m.contains("session rule")),
        "{:?}",
        asked.message
    );
    assert_eq!(
        client
            .permissions(&session.id)
            .await
            .expect("pending")
            .len(),
        1
    );
    assert_eq!(
        std::fs::read_to_string(project.join("a.txt")).expect("read"),
        "alpha\n",
        "not before approval"
    );

    client
        .reply_permission(&session.id, &asked.id, PermissionReply::Once, None)
        .await
        .expect("approve");
    let outcome = until(&mut events, |kind| match kind {
        EventKind::Done {
            session_id,
            outcome,
        } if session_id == session.id => Some(outcome),
        _ => None,
    })
    .await;
    assert_eq!(outcome, "succeeded");
    assert_eq!(
        std::fs::read_to_string(project.join("a.txt")).expect("read"),
        "omega\n"
    );

    let diff = client.diff(&session.id).await.expect("diff");
    assert_eq!(diff.len(), 1);
    assert_eq!(
        (diff[0].file.as_str(), diff[0].additions, diff[0].deletions),
        ("a.txt", 1, 1)
    );
    assert!(
        diff[0].patch.contains("-alpha\n+omega"),
        "{}",
        diff[0].patch
    );

    let before = client.messages(&session.id).await.expect("messages");
    check_timeline(&before);

    sandbox.restart().await;
    let client = sandbox.client();
    let again = client
        .session(&session.id)
        .await
        .expect("the session survives a restart");
    assert_eq!(again.id, session.id);
    assert_eq!(
        client.messages(&session.id).await.expect("messages"),
        before
    );
    let listed = client.root_sessions().await.expect("sessions");
    assert!(listed.iter().any(|s| s.id == session.id));
}

/// User prompt, the edit step with its completed call, the answer, the end.
fn check_timeline(messages: &[Message]) {
    assert_eq!(messages.len(), 4, "{messages:#?}");
    assert!(matches!(&messages[0], Message::User { text, .. } if text == EDIT));
    let Message::Assistant { parts, .. } = &messages[1] else {
        panic!("a step: {:?}", messages[1])
    };
    let Part::Tool(call) = &parts[0] else {
        panic!("a call: {parts:?}")
    };
    assert_eq!(
        (call.name.as_str(), call.status),
        ("edit", ToolStatus::Completed)
    );
    assert_eq!(call.files.len(), 1, "the call carries its diff");
    assert!(
        matches!(&messages[2], Message::Assistant { parts, .. } if parts == &[Part::Text("Changed alpha to omega.".into())])
    );
    assert!(matches!(&messages[3], Message::Idle { outcome, .. } if outcome == "succeeded"));
}

#[tokio::test]
async fn a_reconnecting_client_gets_the_events_it_missed_once_and_in_order() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    std::fs::write(project.join("AGENTS.md"), "Answer in lowercase.\n").expect("AGENTS.md");
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .prompt(&session.id, &Prompt::new("say hello", Delivery::Queue))
        .await
        .expect("prompt");
    for _ in 0..200 {
        let messages = client.messages(&session.id).await.expect("messages");
        if messages.iter().any(|m| matches!(m, Message::Idle { .. })) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // The project's AGENTS.md reached the model, under its path.
    let log = std::fs::read_to_string(sandbox.dir.path().join("model.jsonl")).expect("model log");
    let first: serde_json::Value =
        serde_json::from_str(log.lines().next().expect("a request")).expect("json");
    let system = first["body"]["messages"][0]["content"]
        .as_str()
        .unwrap_or_default();
    assert!(system.contains("Answer in lowercase."), "{system}");
    let source = format!("Instructions from: {}", project.join("AGENTS.md").display());
    assert!(system.contains(&source), "{system}");

    let all = replay(&sandbox, 0).await;
    assert!(all.len() >= 6, "{all:?}");
    let seqs: Vec<u64> = all.iter().map(|(seq, _)| *seq).collect();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "in order: {seqs:?}");
    let types: Vec<&str> = all.iter().map(|(_, kind)| kind.as_str()).collect();
    assert_eq!(types.first(), Some(&"session.created"));
    assert_eq!(types.last(), Some(&"session.execution.succeeded"));

    // Having seen the third, the client gets exactly the rest.
    let rest = replay(&sandbox, seqs[2]).await;
    assert_eq!(rest, all[3..]);
}
