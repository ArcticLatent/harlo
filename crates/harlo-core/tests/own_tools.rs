//! Harlo's own tools reach agents over OpenCode's client MCP connection:
//! the scripted model finds an earlier session by its title and reads it,
//! through Code Mode, and the tools go away when the connection drops.
//!
//! Runs against a private `opencode serve` with scratch `HOME`/`XDG_*`
//! folders, never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod support;

use std::sync::Arc;

use harlo_core::own_tools::{NAME, OwnTools, shorten};
use harlo_core::server::{Client, Delivery, Error, Prompt};
use serde_json::Value;
use support::{Sandbox, wait_for, wait_idle};

/// The requests the scripted model got, as JSON.
fn requests(sandbox: &Sandbox) -> Vec<Value> {
    std::fs::read_to_string(sandbox.root().join("model.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// The tool results the latest request carried back to the model.
fn tool_results(sandbox: &Sandbox) -> Vec<String> {
    requests(sandbox)
        .last()
        .map(|request| {
            request["body"]["messages"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|m| m["role"] == "tool")
                .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Whether the latest request's system prompt lists Harlo's tools.
fn offered(sandbox: &Sandbox) -> bool {
    requests(sandbox).last().is_some_and(|request| {
        request["body"]["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|m| m["role"] == "system" && m["content"].to_string().contains("tools.harlo."))
    })
}

/// A prompt that has the scripted model run `code` in Code Mode, then stop.
fn execute(code: &str) -> Prompt {
    let input = serde_json::json!({ "code": code });
    Prompt::new(format!("tool execute {input} ;; say done"), Delivery::Queue)
}

/// Has session `id`'s agent run `code`, and returns what the tool gave back.
async fn run(client: &Client, sandbox: &Sandbox, id: &str, code: &str) -> String {
    client.prompt(id, &execute(code)).await.expect("prompt");
    wait_idle(client, id).await;
    tool_results(sandbox).last().cloned().unwrap_or_default()
}

/// Runs `code` in session `id` until what the tool gives back holds `want`,
/// and returns it: the tools can take a moment to be offered.
async fn run_until(client: &Client, sandbox: &Sandbox, id: &str, code: &str, want: &str) -> String {
    wait_for(want, || async {
        let result = run(client, sandbox, id, code).await;
        result.contains(want).then_some(result)
    })
    .await
}

/// Offers Harlo's tools for `folder` until the task is stopped.
fn serve(client: &Client, folder: String) -> tokio::task::JoinHandle<Result<(), Error>> {
    let client = client.clone();
    tokio::spawn(async move {
        let host = Arc::new(OwnTools::new(client.clone()));
        client.host_tools(&folder, NAME, host).await
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn agents_find_and_read_past_sessions_through_harlos_tools() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project_str();

    // An earlier session worth finding.
    let earlier = client.create_session(&project).await.expect("session");
    let say = Prompt::new("say The parser fix is in lexer.rs.", Delivery::Queue);
    client.prompt(&earlier.id, &say).await.expect("prompt");
    wait_idle(&client, &earlier.id).await;
    client
        .set_title(&earlier.id, "Fix the parser crash")
        .await
        .expect("title");

    // Offered once the folder's agents can call them; the asking session is
    // left out of what it finds.
    let serving = serve(&client, project.clone());
    let find = "return await tools.harlo.past_sessions({query: 'parser crash'})";
    run_until(
        &client,
        &sandbox,
        &earlier.id,
        find,
        "No session titles match",
    )
    .await;

    // Another folder's sessions get them once Harlo attaches for it too:
    // OpenCode gives a client's tools to the folder it attached for.
    let other = sandbox.root().join("other");
    std::fs::create_dir_all(&other).expect("other folder");
    let other = other.display().to_string();
    let now = client.create_session(&other).await.expect("session");
    let before = run(&client, &sandbox, &now.id, find).await;
    assert!(
        before.contains("Unknown tool"),
        "not attached there yet: {before}"
    );
    let serving_other = serve(&client, other);
    let found = run_until(&client, &sandbox, &now.id, find, "Fix the parser crash").await;
    assert!(found.contains(&earlier.id), "{found}");
    assert!(
        !found.contains(&now.id),
        "the asking session is left out: {found}"
    );

    let read = format!(
        "return await tools.harlo.read_session({{sessionID: '{}'}})",
        earlier.id
    );
    let text = run(&client, &sandbox, &now.id, &read).await;
    assert!(text.contains("# Fix the parser crash"), "{text}");
    assert!(text.contains("The parser fix is in lexer.rs."), "{text}");

    // Dropping the connections takes them away.
    serving.abort();
    serving_other.abort();
    let _ = serving.await;
    let _ = serving_other.await;
    wait_for("the tools to go away", || async {
        let hello = Prompt::new("say hello", Delivery::Queue);
        client.prompt(&now.id, &hello).await.ok()?;
        wait_idle(&client, &now.id).await;
        (!offered(&sandbox)).then_some(())
    })
    .await;
}

#[test]
fn long_sessions_keep_their_start_and_end() {
    let text = format!("start{}end", "x".repeat(1_000));
    let short = shorten(&text, 100);
    assert!(short.starts_with("start"));
    assert!(short.ends_with("end"));
    assert!(short.contains("characters left out"));
    assert_eq!(shorten("brief", 100), "brief");
}
