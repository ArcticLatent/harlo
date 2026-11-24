//! Subagents: a `subagent` call starts a child session in the background;
//! when the child finishes, its answer comes back to the parent, which
//! runs on it. A follow-up with the child's ID goes to the same child.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Delivery, EventKind, Message, Prompt};
use support::{Sandbox, until};

#[tokio::test(flavor = "multi_thread")]
async fn a_subagent_works_in_a_child_and_reports_back() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    let mut events = client.events().await.expect("events");
    let session = client.create_session(&folder).await.expect("session");
    let call = r#"{"agent":"explore","description":"Find the file","prompt":"say Found a.txt."}"#;
    client
        .prompt(
            &session.id,
            &Prompt::new(
                format!("tool subagent {call} ;; say Parent done."),
                Delivery::Queue,
            ),
        )
        .await
        .expect("prompt");

    let child = until(&mut events, |kind| match kind {
        EventKind::JobUpdated(job) => match job.kind {
            harlo_client::JobKind::Subagent { child, .. } => Some(child),
            _ => None,
        },
        _ => None,
    })
    .await;

    // The parent gets the child's answer and answers it in turn.
    let mut report = None;
    for _ in 0..100 {
        let messages = client.messages(&session.id).await.expect("messages");
        report = messages.iter().find_map(|m| match m {
            Message::Report { report, .. } => Some(report.clone()),
            _ => None,
        });
        let answered = messages
            .iter()
            .rev()
            .take_while(|m| !matches!(m, Message::Report { .. }))
            .any(|m| matches!(m, Message::Assistant { .. }));
        if report.is_some() && answered {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let report = report.expect("the child's answer reached the parent");
    assert_eq!(report.child, child);
    assert_eq!(report.state, "completed");
    assert_eq!(report.agent, "Explore");
    assert_eq!(report.task, "Find the file");
    assert!(report.text.contains("Found a.txt."), "{}", report.text);

    let made = client.session(&child).await.expect("the child");
    assert_eq!(made.parent_id.as_deref(), Some(session.id.as_str()));
    assert_eq!(made.title.as_deref(), Some("Find the file"));

    // The child ran as Explore, which may not start subagents of its own.
    let requests = support::turn_requests(sandbox.dir.path());
    let child_request = requests
        .iter()
        .find(|r| {
            r["messages"]
                .to_string()
                .contains("another session handed you")
        })
        .expect("the child's request");
    let tools = child_request["tools"].to_string();
    assert!(!tools.contains("\"subagent\""), "{tools}");
    assert!(
        child_request["messages"][0]["content"]
            .as_str()
            .is_some_and(|s| s.contains("You search and explain code"))
    );

    // A follow-up goes to the same child.
    let follow = format!(
        r#"{{"agent":"explore","description":"Again","prompt":"say Still here.","sessionID":"{child}"}}"#
    );
    client
        .prompt(
            &session.id,
            &Prompt::new(format!("tool subagent {follow}"), Delivery::Queue),
        )
        .await
        .expect("prompt");
    for _ in 0..100 {
        let messages = client.messages(&child).await.expect("child messages");
        if messages
            .iter()
            .any(|m| matches!(m, Message::User { text, .. } if text == "say Still here."))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the follow-up never reached the child");
}
