//! Scripts and the session's own tools: a script reads, searches, changes
//! files and runs commands as the agent's own calls would, under the same
//! rules and with the same asking, and only what it returns enters the
//! conversation.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Client, Delivery, Effect, Message, Permission, PermissionReply, Prompt, Rule};
use support::Sandbox;

/// Finds the text files that hold a word, changes the word in each, writes
/// a file and runs a command.
const CHANGE: &str = r#"const paths = await tools.project.glob({pattern: "*.txt"});
const hits = [];
for (const p of paths) { const text = await tools.project.read({path: p}); if (text.includes("needle")) hits.push(p); }
for (const p of hits) await tools.project.edit({path: p, oldString: "needle", newString: "thread"});
await tools.project.write({path: "made.md", content: "made by a script\n"});
const said = await tools.project.shell({command: "cat made.md"});
const lines = await tools.project.read({path: "a.txt", offset: 2, limit: 1});
return { hits: hits.sort(), count: paths.length, said, lines };"#;

/// Changes two files and reads a third, saying how each call went.
const ASKING: &str = r#"const went = [];
for (const path of ["a.txt", "b.txt"]) {
  try { await tools.project.edit({path, oldString: "old", newString: "new"}); went.push(path + " changed"); }
  catch (e) { went.push(path + ": " + String(e)); }
}
try { went.push(await tools.project.read({path: ".env"})); } catch (e) { went.push(".env: " + String(e)); }
return went;"#;

/// Changes one file, then another.
const TWO: &str = r#"await tools.project.edit({path: "a.txt", oldString: "old", newString: "new"});
await tools.project.write({path: "b.txt", content: "written\n"});
return "both";"#;

/// The project's text files, by name and content.
fn fill(sandbox: &Sandbox, files: &[(&str, &str)]) {
    let project = sandbox.project();
    for (name, text) in files {
        let path = project.join(name);
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("folder");
        std::fs::write(path, text).expect("file");
    }
}

/// What the project's file `name` holds.
fn held(sandbox: &Sandbox, name: &str) -> String {
    std::fs::read_to_string(sandbox.project().join(name)).unwrap_or_default()
}

/// Sets `change` in the sandbox's own config.
fn configure(sandbox: &Sandbox, change: impl FnOnce(&mut serde_json::Value)) {
    let path = sandbox.dir.path().join("config/harlo/harlo.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("config")).expect("JSON");
    change(&mut config);
    std::fs::write(&path, config.to_string()).expect("config");
}

/// Starts a session under `rules` whose scripted model runs `code` as a
/// script, then says "Done."; returns the session's ID.
async fn start(sandbox: &Sandbox, client: &Client, rules: &[Rule], code: &str) -> String {
    let session = client
        .create_session(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .set_permissions(&session.id, rules)
        .await
        .expect("rules");
    let script = format!(
        "tool execute {} ;; say Done.",
        serde_json::json!({ "code": code })
    );
    client
        .prompt(&session.id, &Prompt::new(&script, Delivery::Queue))
        .await
        .expect("prompt");
    session.id
}

/// Waits for the session to ask permission; returns the request.
async fn asked(client: &Client, session: &str) -> Permission {
    for _ in 0..400 {
        let pending = client.permissions(session).await.expect("permissions");
        if let Some(first) = pending.into_iter().next() {
            return first;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("nothing was asked");
}

/// Waits for the session's turn to end; returns its messages as the API
/// lists them.
async fn ended(sandbox: &Sandbox, client: &Client, session: &str) -> Vec<serde_json::Value> {
    for _ in 0..400 {
        let messages = client.messages(session).await.expect("messages");
        if messages.iter().any(|m| matches!(m, Message::Idle { .. })) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let listed: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/api/session/{session}/message", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password))
        .send()
        .await
        .expect("messages")
        .json()
        .await
        .expect("JSON");
    listed["data"].as_array().cloned().unwrap_or_default()
}

/// The state of the one `execute` call among `messages`.
fn script_state(messages: &[serde_json::Value]) -> serde_json::Value {
    let mut calls = messages
        .iter()
        .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
        .filter(|part| part["type"] == "tool");
    let call = calls.next().expect("a call");
    assert!(calls.next().is_none(), "one call");
    call["state"].clone()
}

/// Whether the model's "Done." came after the script.
fn went_on(messages: &[serde_json::Value]) -> bool {
    messages.iter().any(|m| {
        m["type"] == "assistant"
            && m["content"]
                .as_array()
                .is_some_and(|c| c.iter().any(|p| p["text"] == "Done."))
    })
}

/// The description of the `execute` tool in the first request a turn sent.
fn execute_description(sandbox: &Sandbox) -> String {
    let requests = support::turn_requests(sandbox.dir.path());
    let first = requests.first().expect("a request");
    first["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["function"]["name"] == "execute")
        .map(|t| t["function"]["description"].to_string())
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_script_reads_searches_and_changes_files_in_one_step() {
    let sandbox = Sandbox::start().await;
    fill(
        &sandbox,
        &[
            ("a.txt", "alpha needle\nsecond line\nthird line\n"),
            ("b.txt", "beta\n"),
            ("notes/c.txt", "gamma needle\n"),
        ],
    );
    let client = sandbox.client();
    let rules = [
        Rule::any("edit", Effect::Allow),
        Rule::any("shell", Effect::Allow),
    ];
    let session = start(&sandbox, &client, &rules, CHANGE).await;
    let messages = ended(&sandbox, &client, &session).await;
    let state = script_state(&messages);
    let ran = state.to_string();
    assert_eq!(state["status"], "completed", "{ran}");

    // Three files looked at, two named: what the third holds stays out.
    assert!(
        ran.contains(r#"\"hits\":[\"a.txt\",\"notes/c.txt\"]"#),
        "{ran}"
    );
    assert!(ran.contains(r#"\"count\":3"#), "{ran}");
    assert!(ran.contains(r#"\"lines\":\"second line\""#), "{ran}");
    assert!(ran.contains("made by a script"), "{ran}");
    assert!(!ran.contains("beta") && !ran.contains("gamma"), "{ran}");

    // The changes were made, and the step says which files it changed.
    assert_eq!(
        held(&sandbox, "a.txt"),
        "alpha thread\nsecond line\nthird line\n"
    );
    assert_eq!(held(&sandbox, "notes/c.txt"), "gamma thread\n");
    assert_eq!(held(&sandbox, "b.txt"), "beta\n");
    assert_eq!(held(&sandbox, "made.md"), "made by a script\n");
    let changed = messages
        .iter()
        .find_map(|m| m["snapshot"]["files"].as_array().filter(|f| !f.is_empty()))
        .map(|files| serde_json::Value::Array(files.clone()).to_string())
        .unwrap_or_default();
    for file in ["a.txt", "notes/c.txt", "made.md"] {
        assert!(changed.contains(file), "{file} in {changed}");
    }
    assert!(went_on(&messages));

    let told = execute_description(&sandbox);
    for tool in ["read", "grep", "glob", "edit", "write", "shell"] {
        assert!(
            told.contains(&format!("tools.project.{tool}")),
            "{tool}: {told}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scripts_call_asks_where_the_tool_would() {
    let sandbox = Sandbox::start().await;
    fill(
        &sandbox,
        &[
            ("a.txt", "old a\n"),
            ("b.txt", "old b\n"),
            (".env", "SECRET=1\n"),
        ],
    );
    let client = sandbox.client();
    let session = start(&sandbox, &client, &[Rule::any("edit", Effect::Ask)], ASKING).await;

    // The first change is refused with a reason: the script hears it and
    // goes on.
    let first = asked(&client, &session).await;
    assert_eq!(first.action, "edit");
    assert_eq!(first.resources, ["a.txt"]);
    assert!(!first.files.is_empty(), "the change is shown");
    client
        .reply_permission(
            &session,
            &first.id,
            PermissionReply::Reject,
            Some("leave a alone"),
        )
        .await
        .expect("refused");

    // The second is allowed.
    let second = asked(&client, &session).await;
    assert_eq!(second.resources, ["b.txt"]);
    client
        .reply_permission(&session, &second.id, PermissionReply::Once, None)
        .await
        .expect("allowed");

    // Reading `.env` asks too, as the read tool does.
    let third = asked(&client, &session).await;
    assert_eq!(third.action, "read");
    assert_eq!(third.resources, [".env"]);
    client
        .reply_permission(
            &session,
            &third.id,
            PermissionReply::Reject,
            Some("not that file"),
        )
        .await
        .expect("refused");

    let messages = ended(&sandbox, &client, &session).await;
    let ran = script_state(&messages).to_string();
    assert!(ran.contains("leave a alone"), "{ran}");
    assert!(ran.contains("b.txt changed"), "{ran}");
    assert!(
        ran.contains("not that file") && !ran.contains("SECRET"),
        "{ran}"
    );
    assert_eq!(held(&sandbox, "a.txt"), "old a\n");
    assert_eq!(held(&sandbox, "b.txt"), "new b\n");
    assert!(went_on(&messages));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refusal_without_a_reason_stops_the_script_and_the_turn() {
    let sandbox = Sandbox::start().await;
    fill(&sandbox, &[("a.txt", "old a\n")]);
    let client = sandbox.client();
    let session = start(&sandbox, &client, &[Rule::any("edit", Effect::Ask)], TWO).await;
    let first = asked(&client, &session).await;
    client
        .reply_permission(&session, &first.id, PermissionReply::Reject, None)
        .await
        .expect("refused");

    let messages = ended(&sandbox, &client, &session).await;
    let state = script_state(&messages);
    assert_eq!(state["status"], "error", "{state}");
    assert!(
        state
            .to_string()
            .contains("declined a call this script made"),
        "{state}"
    );
    // Nothing after the refused call ran, and the model wasn't asked again.
    assert_eq!(held(&sandbox, "a.txt"), "old a\n");
    assert!(!sandbox.project().join("b.txt").exists());
    assert!(!went_on(&messages));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_script_has_only_the_tools_the_rules_leave() {
    let sandbox = Sandbox::start().await;
    fill(&sandbox, &[("a.txt", "old a\n")]);
    let client = sandbox.client();
    let rules = [
        Rule::any("edit", Effect::Deny),
        Rule::any("shell", Effect::Deny),
    ];
    let code = r#"const text = await tools.project.read({path: "a.txt"});
try { await tools.project.edit({path: "a.txt", oldString: "old", newString: "new"}); return "changed"; }
catch (e) { return text + String(e); }"#;
    let session = start(&sandbox, &client, &rules, code).await;
    let messages = ended(&sandbox, &client, &session).await;
    let ran = script_state(&messages).to_string();
    assert!(
        ran.contains("old a") && ran.contains("no tool edit"),
        "{ran}"
    );
    assert_eq!(held(&sandbox, "a.txt"), "old a\n");

    let told = execute_description(&sandbox);
    assert!(told.contains("tools.project.read"), "{told}");
    for tool in ["edit", "write", "shell"] {
        assert!(
            !told.contains(&format!("tools.project.{tool}")),
            "{tool}: {told}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn scripts_have_none_of_it_when_it_is_turned_off() {
    let sandbox = Sandbox::start().await;
    configure(&sandbox, |config| {
        config["experimental"]["scriptTools"] = false.into();
    });
    fill(&sandbox, &[("a.txt", "alpha\n")]);
    let client = sandbox.client();
    let code = r#"return await tools.project.read({path: "a.txt"});"#;
    let session = start(&sandbox, &client, &[], code).await;
    let messages = ended(&sandbox, &client, &session).await;
    let state = script_state(&messages);
    assert_eq!(state["status"], "error", "{state}");
    assert!(!state.to_string().contains("alpha"), "{state}");
    assert!(!execute_description(&sandbox).contains("tools.project"));
}
