//! Proposed notes: with notes on, a run that goes idle has its stretch
//! read in the background, and what the agent missed is proposed with the
//! session and message it comes from. Nothing proposed reaches a session
//! until the person accepts it; accepted, it is a note that replaces the
//! one it contradicts. A dismissed proposal doesn't come back when the
//! session is read again, and with notes off nothing is read at all.
//!
//! Each test runs its own `harlo-server` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Client, Delivery, Message, Prompt};
use serde_json::{Value, json};
use support::Sandbox;

/// How long a step of the test may take, in 50 ms looks.
const LOOKS: usize = 400;

async fn idles(client: &Client, session: &str) -> usize {
    client
        .messages(session)
        .await
        .expect("messages")
        .iter()
        .filter(|m| matches!(m, Message::Idle { .. }))
        .count()
}

/// Sends `text` and waits until the session is idle again.
async fn turn(client: &Client, session: &str, text: &str) {
    let before = idles(client, session).await;
    client
        .prompt(session, &Prompt::new(text, Delivery::Queue))
        .await
        .expect("prompt");
    for _ in 0..LOOKS {
        if idles(client, session).await > before {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the turn never ended");
}

/// Calls the sandbox's server; the JSON answer, if any.
async fn call(
    sandbox: &Sandbox,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Value {
    let mut request = reqwest::Client::new()
        .request(method, format!("{}{path}", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password));
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.expect("request");
    assert!(
        response.status().is_success(),
        "{path}: {}",
        response.status()
    );
    response.json::<Value>().await.unwrap_or(Value::Null)
}

async fn listed(sandbox: &Sandbox, what: &str, project: &str) -> Vec<Value> {
    call(
        sandbox,
        reqwest::Method::GET,
        &format!("/harlo/{what}?projectID={project}"),
        None,
    )
    .await["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Keeps `scripts` as the scripted model's answers from now on.
fn script(sandbox: &Sandbox, scripts: &Value) {
    std::fs::write(sandbox.dir.path().join("scripts.json"), scripts.to_string())
        .expect("the scripts");
}

/// The requests to propose notes the scripted model has answered.
fn looks(sandbox: &Sandbox) -> Vec<Value> {
    std::fs::read_to_string(sandbox.dir.path().join("model.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .map(|r| r["body"].clone())
        .filter(|b| {
            b["messages"][0]["content"]
                .as_str()
                .is_some_and(|s| s.starts_with(harlo_engine::memory::PROPOSE_SYSTEM))
        })
        .collect()
}

/// Waits until the scripted model has answered `count` requests to
/// propose notes, and the server has had time to store what came of the
/// last; returns the last request's words.
async fn looked(sandbox: &Sandbox, count: usize) -> String {
    for _ in 0..LOOKS {
        let seen = looks(sandbox);
        if seen.len() >= count {
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert_eq!(looks(sandbox).len(), count, "one look for each stretch");
            return seen[count - 1]["messages"][1]["content"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the session was never read for notes ({count})");
}

/// The system prompt of the model request that answered `prompt`.
fn system_prompt_for(sandbox: &Sandbox, prompt: &str) -> String {
    support::turn_requests(sandbox.dir.path())
        .into_iter()
        .find(|r| r["messages"].to_string().contains(prompt))
        .and_then(|r| r["messages"][0]["content"].as_str().map(str::to_owned))
        .unwrap_or_else(|| panic!("no request answered {prompt:?}"))
}

/// The ID of the person's message saying `text` in `session`.
async fn message_id(sandbox: &Sandbox, session: &str, text: &str) -> String {
    call(
        sandbox,
        reqwest::Method::GET,
        &format!("/api/session/{session}/message"),
        None,
    )
    .await["data"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["type"] == "user" && m["text"] == text)
        .and_then(|m| m["id"].as_str())
        .unwrap_or_else(|| panic!("no message {text:?}"))
        .to_owned()
}

#[tokio::test(flavor = "multi_thread")]
#[expect(
    clippy::too_many_lines,
    reason = "one story: missed, proposed, edited, kept, replacing a note"
)]
async fn a_correction_the_agent_missed_is_proposed_and_kept_only_when_accepted() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project_dir = sandbox.project();
    let folder = project_dir.to_str().expect("utf-8");
    call(
        &sandbox,
        reqwest::Method::PUT,
        "/harlo/memory",
        Some(json!({ "enabled": true })),
    )
    .await;

    // The agent keeps a note itself, as before; that stretch holds
    // nothing more to propose.
    let a = client.create_session(folder).await.expect("A");
    turn(
        &client,
        &a.id,
        r#"tool remember {"text":"Cache in Redis: it is already running."} ;; say Kept."#,
    )
    .await;
    let project = a.project_id.clone();
    let kept = listed(&sandbox, "notes", &project).await;
    assert_eq!(kept.len(), 1, "{kept:?}");
    let redis = kept[0]["id"].as_str().expect("id").to_owned();
    looked(&sandbox, 1).await;
    assert!(listed(&sandbox, "proposals", &project).await.is_empty());

    // The person corrects it and the agent carries on without keeping it.
    script(
        &sandbox,
        &json!({
            "No, cache in SQLite instead: nothing else to run.":
                r#"tool read {"path":"a.txt"} ;; say SQLite it is."#,
            "<notes>": format!(
                r#"[{{"text":"Cache in SQLite: nothing else to run.","kind":"correction","scope":"project","message":"{{person}}","replaces":"{redis}"}}]"#
            ),
        }),
    );
    turn(
        &client,
        &a.id,
        "No, cache in SQLite instead: nothing else to run.",
    )
    .await;
    let asked = looked(&sandbox, 2).await;
    // The model read the new stretch only, beside the note that holds
    // and what was said just before.
    assert!(
        asked.contains(&format!(
            "[{redis}] (project note) Cache in Redis: it is already running."
        )),
        "{asked}"
    );
    let (before, stretch) = asked.split_once("<transcript>").expect("a transcript");
    assert!(
        before.contains("<before>") && before.contains("Kept."),
        "{asked}"
    );
    assert!(
        stretch.contains("No, cache in SQLite instead") && !stretch.contains("Kept."),
        "{asked}"
    );

    let proposed = listed(&sandbox, "proposals", &project).await;
    assert_eq!(proposed.len(), 1, "{proposed:?}");
    let proposal = &proposed[0];
    let said = message_id(
        &sandbox,
        &a.id,
        "No, cache in SQLite instead: nothing else to run.",
    )
    .await;
    assert_eq!(proposal["text"], "Cache in SQLite: nothing else to run.");
    assert_eq!(proposal["kind"], "correction");
    assert_eq!(proposal["state"], "pending");
    assert_eq!(proposal["projectID"], project.as_str());
    assert_eq!(proposal["source"]["session"], a.id.as_str());
    assert_eq!(proposal["source"]["message"], said.as_str());
    assert_eq!(
        proposal["excerpt"],
        "No, cache in SQLite instead: nothing else to run."
    );
    assert_eq!(proposal["replaces"], redis.as_str());
    let id = proposal["id"].as_str().expect("id").to_owned();

    // Proposed isn't kept: the notes are as they were, and a new session
    // is told the old one.
    assert_eq!(listed(&sandbox, "notes", &project).await, kept);
    let b = client.create_session(folder).await.expect("B");
    turn(&client, &b.id, "say Hello from B.").await;
    let system = system_prompt_for(&sandbox, "say Hello from B.");
    assert!(
        system.contains("Cache in Redis") && !system.contains("Cache in SQLite"),
        "{system}"
    );

    // Accepted with its words changed, it is a note with the proposal's
    // source, and the note it contradicts is replaced by it.
    let note = call(
        &sandbox,
        reqwest::Method::POST,
        &format!("/harlo/proposals/{id}/accept"),
        Some(json!({ "text": "Cache in SQLite: one file, nothing else to run." })),
    )
    .await["data"]
        .clone();
    assert_eq!(
        note["text"],
        "Cache in SQLite: one file, nothing else to run."
    );
    assert_eq!(note["source"], proposal["source"]);
    let now = listed(&sandbox, "notes", &project).await;
    assert_eq!(now.len(), 2, "{now:?}");
    let old = now.iter().find(|n| n["id"] == redis.as_str()).expect("old");
    assert_eq!(old["supersededBy"], note["id"], "{now:?}");
    assert!(listed(&sandbox, "proposals", &project).await.is_empty());
    // Decided, it can't be decided again.
    let again = reqwest::Client::new()
        .delete(format!("{}/harlo/proposals/{id}", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password))
        .send()
        .await
        .expect("request");
    assert_eq!(again.status(), reqwest::StatusCode::BAD_REQUEST);

    let c = client.create_session(folder).await.expect("C");
    turn(&client, &c.id, "say Hello from C.").await;
    let system = system_prompt_for(&sandbox, "say Hello from C.");
    assert!(
        system.contains("Cache in SQLite: one file, nothing else to run.")
            && !system.contains(&redis),
        "{system}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dismissed_proposal_stays_away_and_notes_off_means_no_look() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project_dir = sandbox.project();
    let folder = project_dir.to_str().expect("utf-8");
    call(
        &sandbox,
        reqwest::Method::PUT,
        "/harlo/memory",
        Some(json!({ "enabled": true })),
    )
    .await;
    let nextest = r#"[{"text":"Tests run with nextest.","kind":"decision","message":"{person}"}]"#;
    let work = r#"tool read {"path":"a.txt"} ;; say Done."#;
    script(
        &sandbox,
        &json!({
            "Run the tests with nextest from now on.": work,
            "And again, with nextest.": work,
            "Format with rustfmt's defaults.": work,
            "One more thing, with notes off.": work,
            "<notes>": nextest,
        }),
    );
    let a = client.create_session(folder).await.expect("A");
    let project = a.project_id.clone();

    turn(&client, &a.id, "Run the tests with nextest from now on.").await;
    looked(&sandbox, 1).await;
    let proposed = listed(&sandbox, "proposals", &project).await;
    assert_eq!(proposed.len(), 1, "{proposed:?}");
    let id = proposed[0]["id"].as_str().expect("id");
    call(
        &sandbox,
        reqwest::Method::DELETE,
        &format!("/harlo/proposals/{id}"),
        None,
    )
    .await;
    assert!(listed(&sandbox, "proposals", &project).await.is_empty());
    assert!(listed(&sandbox, "notes", &project).await.is_empty());

    // The session goes on and is read again; the model proposes the same
    // thing from a later message. It stays dismissed.
    turn(&client, &a.id, "And again, with nextest.").await;
    let asked = looked(&sandbox, 2).await;
    assert!(asked.contains("And again, with nextest."), "{asked}");
    assert!(listed(&sandbox, "proposals", &project).await.is_empty());

    // Something else is still proposed.
    let mut scripts: Value = serde_json::from_str(
        &std::fs::read_to_string(sandbox.dir.path().join("scripts.json")).expect("scripts"),
    )
    .expect("json");
    scripts["<notes>"] = json!(
        r#"[{"text":"Format with rustfmt's defaults.","kind":"decision","message":"{person}"}]"#
    );
    script(&sandbox, &scripts);
    turn(&client, &a.id, "Format with rustfmt's defaults.").await;
    looked(&sandbox, 3).await;
    let proposed = listed(&sandbox, "proposals", &project).await;
    assert_eq!(proposed.len(), 1, "{proposed:?}");
    assert_eq!(proposed[0]["text"], "Format with rustfmt's defaults.");

    // With notes off no stretch is read.
    call(
        &sandbox,
        reqwest::Method::PUT,
        "/harlo/memory",
        Some(json!({ "enabled": false })),
    )
    .await;
    turn(&client, &a.id, "One more thing, with notes off.").await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(looks(&sandbox).len(), 3);
}
