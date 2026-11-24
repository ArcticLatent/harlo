//! Claude Agent on a stand-in Claude Code: once connected, its models are
//! offered; a session on one runs its turn through Claude Code, whose
//! request to run a command waits for the person, and the next turn
//! continues Claude Code's session.
//!
//! The stand-in sits in the scratch folder and is named by
//! `OPENCODE_CLAUDE_PATH`; the real Claude Code is never run.

// Rust guideline compliant 2026-10-09

mod support;

use std::os::unix::fs::PermissionsExt as _;
use std::time::Duration;

use opencode_client::{Delivery, Message, ModelRef, PermissionReply, Prompt};
use support::Sandbox;

/// A stand-in Claude Code: it lists Opus and Sonnet, and for a message
/// asks to run `ls`, then answers. Each run's arguments go to `args.log`.
const STAND_IN: &str = r#"#!/usr/bin/env python3
import json, os, sys
here = os.path.dirname(os.path.abspath(__file__))
open(os.path.join(here, "args.log"), "a").write(json.dumps(sys.argv[1:]) + "\n")
def say(v):
    print(json.dumps(v), flush=True)
efforts = ["low", "medium", "high"]
for line in sys.stdin:
    msg = json.loads(line)
    if msg.get("type") == "control_request":
        say({"type": "control_response", "response": {"subtype": "success",
             "request_id": msg["request_id"], "response": {"models": [
                 {"value": "opus", "displayName": "Opus", "supportedEffortLevels": efforts},
                 {"value": "sonnet", "displayName": "Sonnet", "supportedEffortLevels": efforts}]}}})
        continue
    if msg.get("type") != "user":
        continue
    text = msg["message"]["content"][0]["text"]
    say({"type": "system", "subtype": "init", "session_id": "cc-1"})
    say({"type": "assistant", "message": {"id": "m1", "content": [
        {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "ls"}}],
        "usage": {"input_tokens": 5, "output_tokens": 2}}})
    say({"type": "control_request", "request_id": "r1", "request": {
        "subtype": "can_use_tool", "tool_name": "Bash", "input": {"command": "ls"},
        "tool_use_id": "t1"}})
    answer = json.loads(sys.stdin.readline())["response"]["response"]
    allowed = answer["behavior"] == "allow"
    say({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "t1",
        "content": "a.txt" if allowed else answer.get("message", ""), "is_error": not allowed}]}})
    say({"type": "assistant", "message": {"id": "m2", "content": [
        {"type": "text", "text": "You said: " + text}]}})
    say({"type": "result", "subtype": "success", "is_error": False, "session_id": "cc-1",
         "result": "done"})
    break
"#;

/// Allows what session `id` asks until its messages hold `wanted`.
async fn allow_until(sandbox: &Sandbox, id: &str, wanted: &str) {
    let client = sandbox.client();
    for _ in 0..200 {
        if let Some(p) = client
            .permissions(id)
            .await
            .expect("permissions")
            .into_iter()
            .next()
        {
            client
                .reply_permission(id, &p.id, PermissionReply::Once, None)
                .await
                .expect("allowed");
        }
        if said(sandbox, id).await.contains(wanted) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("never said {wanted:?}: {}", said(sandbox, id).await);
}

/// The assistant text of session `id` so far.
async fn said(sandbox: &Sandbox, id: &str) -> String {
    let messages = sandbox.client().messages(id).await.expect("messages");
    format!("{messages:?}")
}

#[tokio::test(flavor = "multi_thread")]
async fn claude_code_runs_a_turn_that_asks_first_and_resumes() {
    let scratch = tempfile::tempdir().expect("scratch");
    let claude = scratch.path().join("claude");
    std::fs::write(&claude, STAND_IN).expect("stand-in");
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).expect("mode");
    let path = claude.to_str().expect("utf-8");
    let sandbox = Sandbox::start_with_env(&[("OPENCODE_CLAUDE_PATH", path)]).await;
    let client = sandbox.client();
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();

    assert!(
        !client
            .models(&folder)
            .await
            .expect("models")
            .iter()
            .any(|m| m.provider_id == "claude-agent"),
        "not before it's connected"
    );
    client
        .connect_external("claude-agent", "claude-login")
        .await
        .expect("connected");
    let mut opus = None;
    for _ in 0..200 {
        let models = client.models(&folder).await.expect("models");
        opus = models
            .into_iter()
            .find(|m| m.provider_id == "claude-agent" && m.id == "opus");
        if opus.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let opus = opus.expect("Claude Code's models are offered");
    assert_eq!(opus.variants, ["low", "medium", "high"]);

    let session = client.create_session(&folder).await.expect("session");
    let model = ModelRef {
        provider_id: "claude-agent".into(),
        id: "opus".into(),
        variant: Some("high".into()),
    };
    client.set_model(&session.id, &model).await.expect("model");
    client
        .prompt(&session.id, &Prompt::new("hello", Delivery::Queue))
        .await
        .expect("prompt");

    // Claude Code's request to run `ls` waits for the person.
    let mut asked = None;
    for _ in 0..200 {
        asked = client
            .permissions(&session.id)
            .await
            .expect("permissions")
            .into_iter()
            .next();
        if asked.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let asked = asked.expect("it asks");
    assert_eq!(
        (asked.action.as_str(), asked.resources.as_slice()),
        ("shell", ["ls".to_owned()].as_slice())
    );
    allow_until(&sandbox, &session.id, "You said: hello").await;
    let all = said(&sandbox, &session.id).await;
    assert!(all.contains("You said: hello"), "{all}");
    assert!(all.contains("status: Completed"), "the command ran: {all}");
    let messages = client.messages(&session.id).await.expect("messages");
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::Assistant { .. })),
        "{messages:?}"
    );

    // The next turn continues Claude Code's session, at the chosen effort.
    let args = || std::fs::read_to_string(scratch.path().join("args.log")).unwrap_or_default();
    client
        .prompt(&session.id, &Prompt::new("again", Delivery::Queue))
        .await
        .expect("prompt");
    allow_until(&sandbox, &session.id, "You said: again").await;
    let log = args();
    let turns: Vec<&str> = log.lines().filter(|l| l.contains("--model")).collect();
    assert_eq!(turns.len(), 2, "{log}");
    assert!(turns[0].contains(r#""--effort", "high""#), "{log}");
    assert!(!turns[0].contains("--resume"));
    assert!(turns[1].contains(r#""--resume", "cc-1""#), "{log}");
}

/// A stand-in Claude Code for web searches: it connects, and for a search
/// sends back `WebSearch`'s links and notes as Claude Code reports them.
const SEARCHER: &str = r#"#!/usr/bin/env python3
import json, os, sys
here = os.path.dirname(os.path.abspath(__file__))
open(os.path.join(here, "args.log"), "a").write(json.dumps(sys.argv[1:]) + "\n")
def say(v):
    print(json.dumps(v), flush=True)
if "--tools" not in sys.argv:
    for line in sys.stdin:
        msg = json.loads(line)
        if msg.get("type") == "control_request":
            say({"type": "control_response", "response": {"subtype": "success",
                 "request_id": msg["request_id"], "response": {"models": []}}})
    sys.exit(0)
query = sys.stdin.read()
say({"type": "system", "subtype": "init", "session_id": "cc-s"})
say({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "w1",
     "content": "..."}]}, "tool_use_result": {"query": query, "results": [
     {"tool_use_id": "w1", "content": [{"title": "Rust 9 notes", "url": "https://example.com/r9"}]},
     "Rust 9 is out, for " + query]}})
say({"type": "result", "subtype": "success", "is_error": False, "result": "done"})
"#;

#[tokio::test(flavor = "multi_thread")]
async fn claude_agent_searches_the_web_when_chosen() {
    let scratch = tempfile::tempdir().expect("scratch");
    let claude = scratch.path().join("claude");
    std::fs::write(&claude, SEARCHER).expect("stand-in");
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).expect("mode");
    let path = claude.to_str().expect("utf-8");
    let sandbox = Sandbox::start_with_env(&[("OPENCODE_CLAUDE_PATH", path)]).await;
    let client = sandbox.client();
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    let offered =
        |list: Vec<opencode_client::WebSearch>| list.iter().any(|s| s.id == "claude-agent");
    assert!(!offered(
        client.web_search(&folder).await.expect("services")
    ));
    client
        .connect_external("claude-agent", "claude-login")
        .await
        .expect("connected");
    assert!(offered(client.web_search(&folder).await.expect("services")));

    let session = client.create_session(&folder).await.expect("session");
    client
        .prompt(
            &session.id,
            &Prompt::new(
                r#"tool websearch {"query":"rust 9"} ;; say found"#,
                Delivery::Queue,
            ),
        )
        .await
        .expect("prompt");
    for (field, value) in [("choice", "choose"), ("provider", "claude-agent")] {
        let mut form = None;
        for _ in 0..200 {
            form = client
                .forms(&session.id)
                .await
                .expect("forms")
                .into_iter()
                .next();
            if form.as_ref().is_some_and(|f| f.fields[0].key == field) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let form = form.expect("asked");
        client
            .reply_form(
                &session.id,
                &form.id,
                &[(
                    field.to_owned(),
                    opencode_client::Answer::Text(value.into()),
                )],
            )
            .await
            .expect("answered");
    }
    // What the model was given back.
    let results = || {
        support::turn_requests(sandbox.dir.path())
            .iter()
            .flat_map(|r| r["messages"].as_array().cloned().unwrap_or_default())
            .filter(|m| m["role"] == "tool")
            .map(|m| m["content"].to_string())
            .collect::<String>()
    };
    for _ in 0..200 {
        if results().contains("Rust 9 notes") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let all = results();
    assert!(
        all.contains("[Rust 9 notes](https://example.com/r9)"),
        "{all}"
    );
    assert!(all.contains("Rust 9 is out, for rust 9"), "{all}");
    let log = std::fs::read_to_string(scratch.path().join("args.log")).expect("args");
    let search = log
        .lines()
        .find(|l| l.contains("--tools"))
        .expect("a search run");
    assert!(
        search.contains(r#""--tools", "WebSearch""#) && search.contains(r#""haiku""#),
        "{search}"
    );
}
