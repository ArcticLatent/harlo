//! A session's messages a page at a time: the newest first for a window
//! that shows the end, the pages before as the reader goes back, and the
//! whole in pages for whoever wants it all.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Client, Delivery, Message, Prompt};
use serde_json::Value;
use support::Sandbox;

/// Sends `script` and waits until the session is idle again.
async fn turn(client: &Client, session: &str, script: &str) {
    let idles = |messages: Vec<Message>| {
        messages
            .iter()
            .filter(|m| matches!(m, Message::Idle { .. }))
            .count()
    };
    let before = idles(client.messages(session).await.expect("messages"));
    client
        .prompt(session, &Prompt::new(script, Delivery::Queue))
        .await
        .expect("prompt");
    for _ in 0..400 {
        if idles(client.messages(session).await.expect("messages")) > before {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the turn never ended");
}

/// One request for a session's messages; the status and the answer.
async fn list(sandbox: &Sandbox, session: &str, query: &[(&str, &str)]) -> (u16, Value) {
    let response = reqwest::Client::new()
        .get(format!("{}/api/session/{session}/message", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password))
        .query(query)
        .send()
        .await
        .expect("request");
    let status = response.status().as_u16();
    (
        status,
        response.json::<Value>().await.unwrap_or(Value::Null),
    )
}

fn ids(messages: &[Message]) -> Vec<String> {
    messages.iter().map(|m| m.id().to_owned()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_long_session_is_read_a_page_at_a_time() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    for word in ["One", "Two", "Three", "Four"] {
        turn(&client, &session.id, &format!("say {word}.")).await;
    }
    let whole = ids(&client.messages(&session.id).await.expect("messages"));
    assert!(whole.len() >= 8, "prompts and answers: {whole:?}");

    // The end first, then back a page at a time to the start.
    let mut page = client
        .recent_messages(&session.id, 3)
        .await
        .expect("the end");
    assert_eq!(ids(&page.messages), whole[whole.len() - 3..]);
    let mut read = ids(&page.messages);
    let mut pages = 1;
    while let Some(earlier) = page.earlier.take() {
        page = client
            .earlier_messages(&session.id, &earlier, 3)
            .await
            .expect("the page before");
        assert!(page.messages.len() <= 3, "{:?}", ids(&page.messages));
        let mut joined = ids(&page.messages);
        joined.append(&mut read);
        read = joined;
        pages += 1;
    }
    assert_eq!(read, whole);
    assert_eq!(pages, whole.len().div_ceil(3));
    // A session shorter than what's asked for comes whole, with nothing before.
    let all = client
        .recent_messages(&session.id, 500)
        .await
        .expect("all of it");
    assert_eq!(ids(&all.messages), whole);
    assert_eq!(all.earlier, None);

    // Oldest first, a cursor leads through the whole as well.
    let mut read: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut query = vec![("limit", "3")];
        if let Some(cursor) = &cursor {
            query.push(("cursor", cursor.as_str()));
        }
        let (status, answer) = list(&sandbox, &session.id, &query).await;
        assert_eq!(status, 200, "{answer}");
        let data = answer["data"].as_array().expect("data");
        read.extend(
            data.iter()
                .filter_map(|m| m["id"].as_str().map(str::to_owned)),
        );
        cursor = answer["cursor"]["next"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(read, whole);

    // Without a limit everything comes at once, as it always did.
    let (_, answer) = list(&sandbox, &session.id, &[]).await;
    assert_eq!(answer["data"].as_array().map(Vec::len), Some(whole.len()));
    assert_eq!(answer["cursor"]["next"], Value::Null);
    // A cursor that isn't the server's is refused, not guessed at.
    let (status, _) = list(
        &sandbox,
        &session.id,
        &[("limit", "3"), ("cursor", "nonsense")],
    )
    .await;
    assert_eq!(status, 400);
}
