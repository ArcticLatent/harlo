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

    // The request for the summary is the step's, with one message more:
    // the same system prompt and tools, so the provider reads the
    // conversation from its cache.
    let sent = support::turn_requests(sandbox.dir.path());
    let (step, asked) = (&sent[0], &sent[1]);
    let request = asked["messages"]
        .as_array()
        .and_then(|m| m.last())
        .map(|m| m["content"].to_string())
        .unwrap_or_default();
    assert!(request.contains("Summarize the conversation"), "{request}");
    assert!(step["tools"].as_array().is_some_and(|t| !t.is_empty()));
    assert_eq!(asked["tools"], step["tools"]);
    assert_eq!(asked["messages"][0], step["messages"][0]);

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

/// A file of about 90,000 characters, every line saying `word`.
fn big(word: &str) -> String {
    format!(
        "{} filler to make a long line of text\n",
        format!("{word} ").repeat(6)
    )
    .repeat(1_200)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_taken_up_cold_has_its_old_tool_results_cleared() {
    let sandbox = Sandbox::start().await;
    // Any pause counts as cold here: `prune` is the seconds of rest it
    // takes (`true` for the five minutes a provider's cache lasts).
    let path = sandbox.dir.path().join("config/harlo/harlo.json");
    let config = std::fs::read_to_string(&path).expect("config");
    let mut config: serde_json::Value = serde_json::from_str(&config).expect("json");
    config["compaction"] = serde_json::json!({ "prune": 0 });
    std::fs::write(&path, config.to_string()).expect("config");
    for word in ["oneone", "twotwo", "threethree"] {
        std::fs::write(sandbox.project().join(format!("{word}.txt")), big(word)).expect("file");
    }
    let client = sandbox.client();
    let directory = sandbox.project().to_str().expect("utf-8").to_owned();
    let mut events = client.events().await.expect("events");
    let session = client.create_session(&directory).await.expect("session");
    let mut turn = async |script: &str| {
        client
            .prompt(&session.id, &Prompt::new(script, Delivery::Queue))
            .await
            .expect("prompt");
        until(&mut events, |kind| match kind {
            EventKind::Done { .. } => Some(()),
            _ => None,
        })
        .await;
    };
    let last_sent = || {
        support::turn_requests(sandbox.dir.path())
            .pop()
            .map(|request| request["messages"].clone())
            .expect("a request")
    };

    turn(
        r#"tool read {"path":"oneone.txt"} ;; tool read {"path":"twotwo.txt"} ;; tool read {"path":"threethree.txt"} ;; say Read them."#,
    )
    .await;
    // While the turn ran, every step read the whole of what came before.
    let sent = last_sent().to_string();
    assert!(sent.contains("oneone") && sent.contains("threethree"));

    // Taken up again, the oldest result is cleared; the newest two stay.
    turn("say Still here.").await;
    let second = last_sent();
    let sent = second.to_string();
    assert!(!sent.contains("oneone oneone"), "the oldest is cleared");
    assert!(sent.contains("This output was cleared"), "{}", &sent[..400]);
    assert!(sent.contains("twotwo twotwo") && sent.contains("threethree threethree"));
    // People still see what the call returned.
    let stored = reqwest::Client::new()
        .get(format!(
            "{}/api/session/{}/message",
            sandbox.url(),
            session.id
        ))
        .basic_auth("harlo", Some(&sandbox.password))
        .send()
        .await
        .expect("request")
        .text()
        .await
        .expect("messages");
    assert!(stored.contains("oneone oneone"));
    assert!(stored.contains(r#""cleared":"#), "and that it was cleared");

    // The next turn reads the same conversation, to the letter: what was
    // cleared stays cleared, and nothing more goes.
    turn("say And again.").await;
    let third = last_sent();
    let (second, third) = (
        second.as_array().expect("messages"),
        third.as_array().expect("messages"),
    );
    assert!(third.len() > second.len());
    assert_eq!(&third[..second.len()], &**second);
}
