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

/// Every `subagent` call in session `id`'s steps: whether it started a
/// child, and what it said.
async fn subagent_calls(sandbox: &Sandbox, id: &str) -> Vec<(bool, String)> {
    let listed: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/api/session/{id}/message", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password))
        .send()
        .await
        .expect("messages")
        .json()
        .await
        .expect("JSON");
    listed["data"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
        .filter(|part| part["type"] == "tool" && part["name"] == "subagent")
        .filter(|part| part["state"]["status"] != "running")
        .map(|part| {
            (
                part["state"]["status"] == "completed",
                part["state"].to_string(),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_runs_so_many_subagents_at_once_and_no_more() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    let session = client.create_session(&folder).await.expect("session");
    // Nine at once. Each asks to run a command and waits for leave, so
    // all are still running when the last is asked for.
    let call = serde_json::json!({
        "agent": "general", "description": "Look",
        "prompt": r#"tool shell {"command":"true"}"#,
    });
    let step = vec![format!("tool subagent {call}"); 9].join(" | ");
    client
        .prompt(
            &session.id,
            &Prompt::new(format!("{step} ;; say Parent done."), Delivery::Queue),
        )
        .await
        .expect("prompt");
    let mut calls = Vec::new();
    for _ in 0..200 {
        calls = subagent_calls(&sandbox, &session.id).await;
        if calls.len() == 9 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let started = calls.iter().filter(|(started, _)| *started).count();
    assert_eq!((calls.len(), started), (9, 8), "{calls:?}");
    let (_, refused) = calls.iter().find(|(started, _)| !started).expect("refused");
    assert!(refused.contains("the most at once"), "{refused}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_subagents_subagent_starts_none_of_its_own() {
    let sandbox = Sandbox::start().await;
    // An agent from the configuration, which nothing stops from handing
    // tasks on.
    let agents = sandbox.project().join(".harlo/agents");
    std::fs::create_dir_all(&agents).expect("agents");
    std::fs::write(
        agents.join("nester.md"),
        "---\ndescription: Hands tasks on\nmode: subagent\n---\nHand the task on.\n",
    )
    .expect("agent");
    let client = sandbox.client();
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    let mut events = client.events().await.expect("events");
    let session = client.create_session(&folder).await.expect("session");
    // Each hands the same task on: the session, its subagent, and that
    // one's subagent, which is refused.
    let hand_on = |prompt: &str| {
        let call = serde_json::json!({ "agent": "nester", "description": "On", "prompt": prompt });
        format!("tool subagent {call}")
    };
    let script = hand_on(&hand_on(&hand_on("say Never reached.")));
    client
        .prompt(&session.id, &Prompt::new(script, Delivery::Queue))
        .await
        .expect("prompt");

    let mut children = Vec::new();
    while children.len() < 2 {
        let child = until(&mut events, |kind| match kind {
            EventKind::JobUpdated(job) => match job.kind {
                harlo_client::JobKind::Subagent { child, .. } => Some(child),
                _ => None,
            },
            _ => None,
        })
        .await;
        if !children.contains(&child) {
            children.push(child);
        }
    }
    let mut calls = Vec::new();
    for _ in 0..200 {
        calls = subagent_calls(&sandbox, &children[1]).await;
        if !calls.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let [(started, said)] = calls.as_slice() else {
        panic!("one call by the subagent's subagent: {calls:?}");
    };
    assert!(!started, "{said}");
    assert!(said.contains("start none of their own"), "{said}");
}
