//! Compaction: asked for, the session is summarized as one of its
//! messages, and the next prompt's request starts from the summary.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{Delivery, EventKind, Message, Prompt};
use support::{Sandbox, until};

#[tokio::test(flavor = "multi_thread")]
async fn a_compacted_session_carries_on_from_its_summary() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let directory = sandbox.project().to_str().expect("utf-8").to_owned();
    let mut events = client.events().await.expect("events");
    let session = client.create_session(&directory).await.expect("session");
    let done = async |events: &mut harlo_client::Events| {
        until(events, |kind| match kind {
            EventKind::Done { .. } => Some(()),
            _ => None,
        })
        .await;
    };

    client
        .prompt(
            &session.id,
            &Prompt::new("say First answer.", Delivery::Queue),
        )
        .await
        .expect("prompt");
    done(&mut events).await;
    client.compact(&session.id).await.expect("compact");
    let summary = until(&mut events, |kind| match kind {
        EventKind::CompactionEnded { summary, .. } => Some(summary),
        _ => None,
    })
    .await;
    assert!(!summary.is_empty());
    done(&mut events).await;

    let messages = client.messages(&session.id).await.expect("messages");
    let kept = messages.iter().find_map(|m| match m {
        Message::Compaction {
            status, summary, ..
        } => Some((status.clone(), summary.clone())),
        _ => None,
    });
    assert_eq!(kept, Some(("completed".to_owned(), Some(summary))));

    client
        .prompt(
            &session.id,
            &Prompt::new("say Second answer.", Delivery::Queue),
        )
        .await
        .expect("prompt");
    done(&mut events).await;
    let last = support::turn_requests(sandbox.dir.path())
        .pop()
        .expect("a request");
    let sent = last["messages"].to_string();
    assert!(
        sent.contains("Summary of the conversation so far"),
        "{sent}"
    );
    assert!(!sent.contains("First answer."), "{sent}");
    assert!(sent.contains("Second answer."), "{sent}");
}
