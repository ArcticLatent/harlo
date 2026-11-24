//! Notes between sessions, the plan's acceptance test: a decision kept in
//! worktree A reaches a fresh session in worktree B, under its ID; the
//! decision is reversed in B; a later session gets only the replacement,
//! and searching still finds the old conversation and says what replaced
//! the old note. People can correct and forget notes; off, there are none.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Client, Delivery, Message, PermissionReply, Prompt};
use serde_json::{Value, json};
use support::Sandbox;

/// Sends `script` and waits until the session is idle again.
async fn turn(client: &Client, session: &str, script: &str) {
    let before = idles(client, session).await;
    client
        .prompt(session, &Prompt::new(script, Delivery::Queue))
        .await
        .expect("prompt");
    for _ in 0..400 {
        if idles(client, session).await > before {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the turn never ended");
}

/// Sends `script`, allows once the one thing it asks about (which must be
/// `action`), and waits until the session is idle again.
async fn turn_allowing(client: &Client, session: &str, script: &str, action: &str) {
    let before = idles(client, session).await;
    client
        .prompt(session, &Prompt::new(script, Delivery::Queue))
        .await
        .expect("prompt");
    let mut allowed = false;
    for _ in 0..400 {
        if idles(client, session).await > before {
            assert!(allowed, "the turn ended without asking");
            return;
        }
        for asked in client.permissions(session).await.expect("permissions") {
            assert_eq!(asked.action, action, "{asked:?}");
            client
                .reply_permission(session, &asked.id, PermissionReply::Once, None)
                .await
                .expect("allow");
            allowed = true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the turn never ended");
}

async fn idles(client: &Client, session: &str) -> usize {
    client
        .messages(session)
        .await
        .expect("messages")
        .iter()
        .filter(|m| matches!(m, Message::Idle { .. }))
        .count()
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

/// The system prompt of the model request that answered `prompt`.
fn system_prompt_for(sandbox: &Sandbox, prompt: &str) -> String {
    support::turn_requests(sandbox.dir.path())
        .into_iter()
        .find(|r| r["messages"].to_string().contains(prompt))
        .and_then(|r| r["messages"][0]["content"].as_str().map(str::to_owned))
        .unwrap_or_else(|| panic!("no request answered {prompt:?}"))
}

/// What the session's tool calls returned, in order.
async fn tool_outputs(sandbox: &Sandbox, session: &str) -> Vec<String> {
    let messages = call(
        sandbox,
        reqwest::Method::GET,
        &format!("/api/session/{session}/message"),
        None,
    )
    .await;
    messages["data"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
        .filter(|p| p["type"] == "tool")
        .map(|p| {
            p["state"]["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}

async fn notes(sandbox: &Sandbox, project: &str) -> Vec<Value> {
    call(
        sandbox,
        reqwest::Method::GET,
        &format!("/harlo/notes?projectID={project}"),
        None,
    )
    .await["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_decision_crosses_worktrees_and_its_reversal_is_followed() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let a_dir = sandbox.project();
    let b_dir = sandbox.dir.path().join("worktree-b");
    support::run(
        &a_dir,
        &[
            "worktree",
            "add",
            "-q",
            b_dir.to_str().expect("utf-8"),
            "-b",
            "b",
        ],
    );
    call(
        &sandbox,
        reqwest::Method::PUT,
        "/harlo/memory",
        Some(json!({ "enabled": true })),
    )
    .await;

    // Worktree A: the decision is kept.
    let a = client
        .create_session(a_dir.to_str().expect("utf-8"))
        .await
        .expect("A");
    turn(
        &client,
        &a.id,
        r#"tool remember {"text":"Cache in SQLite: one file, no server to run."} ;; say Kept."#,
    )
    .await;
    let project = a.project_id.clone();
    let kept = notes(&sandbox, &project).await;
    assert_eq!(kept.len(), 1, "{kept:?}");
    let old = kept[0]["id"].as_str().expect("id").to_owned();
    assert_eq!(kept[0]["source"]["session"], a.id.as_str());

    // Worktree B, a fresh session: the note is in its prompt, citable by ID.
    let b = client
        .create_session(b_dir.to_str().expect("utf-8"))
        .await
        .expect("B");
    assert_eq!(b.project_id, project, "worktrees share the project");
    turn(&client, &b.id, "say Hello from B.").await;
    let system = system_prompt_for(&sandbox, "say Hello from B.");
    assert!(
        system.contains(&format!("[{old}] (project note")) && system.contains("Cache in SQLite"),
        "{system}"
    );

    // B reverses it.
    let reverse = format!(
        r#"tool remember {{"text":"Cache in Postgres: several servers must share it.","replaces":"{old}"}} ;; say Changed."#
    );
    turn(&client, &b.id, &reverse).await;
    let both = notes(&sandbox, &project).await;
    let new = both
        .iter()
        .find(|n| n["id"] != old.as_str())
        .and_then(|n| n["id"].as_str())
        .expect("the replacement")
        .to_owned();
    let replaced = both
        .iter()
        .find(|n| n["id"] == old.as_str())
        .expect("the old note");
    assert_eq!(replaced["supersededBy"], new.as_str());

    // Another session follows the replacement only…
    let c = client
        .create_session(a_dir.to_str().expect("utf-8"))
        .await
        .expect("C");
    turn(&client, &c.id, "say Hello from C.").await;
    let system = system_prompt_for(&sandbox, "say Hello from C.");
    assert!(
        system.contains(&format!("[{new}]")) && system.contains("Cache in Postgres"),
        "{system}"
    );
    assert!(!system.contains("Cache in SQLite"), "{system}");

    // …and searching still explains the history.
    turn(
        &client,
        &c.id,
        r#"tool search_sessions {"query":"SQLite cache"} ;; say Found."#,
    )
    .await;
    let found = tool_outputs(&sandbox, &c.id).await.join("\n");
    assert!(
        found.contains(&format!("[{old}] (replaced by {new}) Cache in SQLite")),
        "{found}"
    );
    assert!(
        found.contains(&format!("({})", a.id)),
        "A's conversation: {found}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn people_correct_and_forget_notes_and_off_means_none() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let a_dir = sandbox.project();
    call(
        &sandbox,
        reqwest::Method::PUT,
        "/harlo/memory",
        Some(json!({ "enabled": true })),
    )
    .await;
    let a = client
        .create_session(a_dir.to_str().expect("utf-8"))
        .await
        .expect("A");
    // A note for the project is kept unasked; one for every project asks.
    turn_allowing(
        &client,
        &a.id,
        r#"tool remember {"text":"Cache in SQLite."} | tool remember {"text":"Cache in Postgres.","scope":"user"} ;; say Kept."#,
        "remember_user",
    )
    .await;
    let kept = notes(&sandbox, &a.project_id).await;
    assert_eq!(kept.len(), 2, "{kept:?}");
    let id = |text: &str| {
        kept.iter()
            .find(|n| n["text"] == text)
            .and_then(|n| n["id"].as_str())
            .expect("note")
            .to_owned()
    };
    let (old, new) = (id("Cache in SQLite."), id("Cache in Postgres."));

    // People correct and forget notes.
    call(
        &sandbox,
        reqwest::Method::PATCH,
        &format!("/harlo/notes/{new}"),
        Some(json!({ "text": "Cache in Postgres, shared by every server." })),
    )
    .await;
    call(
        &sandbox,
        reqwest::Method::DELETE,
        &format!("/harlo/notes/{old}"),
        None,
    )
    .await;
    let left = notes(&sandbox, &a.project_id).await;
    assert_eq!(left.len(), 1, "{left:?}");
    assert_eq!(
        left[0]["text"],
        "Cache in Postgres, shared by every server."
    );

    // Off, sessions get no notes and no memory tools.
    call(
        &sandbox,
        reqwest::Method::PUT,
        "/harlo/memory",
        Some(json!({ "enabled": false })),
    )
    .await;
    let d = client
        .create_session(a_dir.to_str().expect("utf-8"))
        .await
        .expect("D");
    turn(&client, &d.id, "say Hello from D.").await;
    let system = system_prompt_for(&sandbox, "say Hello from D.");
    assert!(!system.contains("Cache in Postgres"), "{system}");
}
