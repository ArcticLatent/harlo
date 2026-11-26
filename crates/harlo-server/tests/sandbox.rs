//! The sandbox an agent's commands run in: they change the project and
//! nothing beside it, don't read where secrets are kept, and leave it
//! only when a person says so.
//!
//! The scratch server here lives outside the temporary folders, which
//! the sandbox leaves open. Where the kernel has no Landlock, or the only
//! place for a scratch folder is a temporary one, there is no sandbox to
//! test, and the tests say so and pass.

// Rust guideline compliant 2026-10-09

mod support;

use std::path::Path;
use std::time::Duration;

use harlo_client::{Client, Delivery, Effect, EventKind, Message, PermissionReply, Prompt, Rule};
use support::{Sandbox, turn_requests, until};

/// The folders the sandbox leaves open to every command.
const OPEN_TO_ALL: [&str; 3] = ["/tmp", "/var/tmp", "/dev/shm"];

/// A scratch server whose commands the sandbox holds; none, with a word
/// on why, where this system can't confine commands or the scratch
/// folder landed in a temporary folder, where nothing is closed to them.
async fn confined() -> Option<Sandbox> {
    if !harlo_engine::sandbox::supported() {
        eprintln!("no Landlock on this system: nothing to test");
        return None;
    }
    let sandbox = Sandbox::start_outside_tmp().await;
    let root = std::fs::canonicalize(sandbox.dir.path()).expect("the scratch folder");
    if OPEN_TO_ALL.iter().any(|open| root.starts_with(open)) {
        eprintln!(
            "the scratch folder {} is in a temporary folder: nothing to test",
            root.display()
        );
        return None;
    }
    Some(sandbox)
}

async fn idles(client: &Client, session: &str) -> usize {
    let messages = client.messages(session).await.expect("messages");
    messages
        .iter()
        .filter(|m| matches!(m, Message::Idle { .. }))
        .count()
}

/// Sends `script` to `session` and waits until its run is over.
async fn run(client: &Client, session: &str, script: &str) {
    let before = idles(client, session).await;
    client
        .prompt(session, &Prompt::new(script, Delivery::Steer))
        .await
        .expect("prompt");
    for _ in 0..400 {
        if idles(client, session).await > before {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{session} never finished {script}");
}

/// What the model was last told a tool call gave.
fn last_result(root: &Path) -> String {
    let requests = turn_requests(root);
    let last = requests.last().expect("a request");
    let messages = last["messages"].as_array().expect("messages");
    messages
        .iter()
        .rev()
        .find(|m| m["role"] == "tool")
        .and_then(|m| m["content"].as_str())
        .unwrap_or_default()
        .to_owned()
}

/// A shell call of `command`, then "done".
fn shell(command: &str) -> String {
    let call = serde_json::json!({ "command": command });
    format!("tool shell {call} ;; say done")
}

/// Whether the model was last offered the shell tool's way out.
fn offered_leaving(root: &Path) -> bool {
    let requests = turn_requests(root);
    let last = requests.last().expect("a request");
    last["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|tool| tool["function"]["name"] == "shell")
        .any(|tool| !tool["function"]["parameters"]["properties"]["outside_sandbox"].is_null())
}

/// The system prompt the model was last sent.
fn system_prompt(root: &Path) -> String {
    let requests = turn_requests(root);
    let last = requests.last().expect("a request");
    last["messages"][0]["content"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_changes_its_project_and_reads_no_secrets() {
    let Some(sandbox) = confined().await else {
        return;
    };
    let (client, project, root) = (sandbox.client(), sandbox.project(), sandbox.dir.path());
    std::fs::create_dir_all(root.join("home/.ssh")).expect("keys");
    std::fs::write(root.join("home/.ssh/id_ed25519"), "PRIVATE KEY\n").expect("key");
    std::fs::write(root.join("home/notes.txt"), "plain notes\n").expect("notes");
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    // Nothing asks: the rules allow everything. The sandbox still holds.
    client
        .set_permissions(&session.id, &[Rule::any("*", Effect::Allow)])
        .await
        .expect("trusted");

    // The project is its to change; what lies beside it isn't.
    run(
        &client,
        &session.id,
        &shell(
            "echo in > inside.txt; echo out > ../outside.txt; mktemp > /dev/null && echo made-one",
        ),
    )
    .await;
    let result = last_result(root);
    assert_eq!(result.matches("Permission denied").count(), 1, "{result}");
    assert!(result.contains("made-one"), "{result}");
    assert_eq!(
        std::fs::read_to_string(project.join("inside.txt")).expect("inside.txt"),
        "in\n"
    );
    assert!(!root.join("outside.txt").exists());

    // It reads what is on the computer, but not keys, the server's
    // configuration with its provider keys, or the server's own files,
    // however their paths are spelled.
    run(
        &client,
        &session.id,
        &shell(
            "cat ~/notes.txt; cat ~/.ssh/id_ed25519; ls ~/.ssh; \
             cd ../config/harlo && cat harlo.json; \
             cd ../../data/harlo && cat server.d? > /dev/null",
        ),
    )
    .await;
    let result = last_result(root);
    assert!(result.contains("plain notes"), "{result}");
    assert!(
        result.contains("id_ed25519"),
        "names can be listed: {result}"
    );
    assert!(!result.contains("PRIVATE KEY"), "{result}");
    assert!(!result.contains("fake-key"), "{result}");
    assert_eq!(result.matches("Permission denied").count(), 3, "{result}");

    // A command in the background is held to the same.
    let call = serde_json::json!({
        "command": "echo bg > bg.txt; echo bg > ../bg-outside.txt", "background": true
    });
    run(
        &client,
        &session.id,
        &format!("tool shell {call} ;; say done"),
    )
    .await;
    for _ in 0..200 {
        if project.join("bg.txt").exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(project.join("bg.txt").exists());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!root.join("bg-outside.txt").exists());

    // The agent is told, and offered the way out.
    assert!(system_prompt(root).contains("Shell commands run in a sandbox"));
    assert!(offered_leaving(root));
}

#[tokio::test(flavor = "multi_thread")]
async fn leaving_the_sandbox_waits_for_a_person_even_when_nothing_else_asks() {
    let Some(sandbox) = confined().await else {
        return;
    };
    let (client, project, root) = (sandbox.client(), sandbox.project(), sandbox.dir.path());
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .set_permissions(&session.id, &[Rule::any("*", Effect::Allow)])
        .await
        .expect("trusted");
    let mut events = client.events().await.expect("events");
    let call = serde_json::json!({
        "command": "echo out > ../outside.txt", "outside_sandbox": true
    });
    client
        .prompt(
            &session.id,
            &Prompt::new(format!("tool shell {call} ;; say done"), Delivery::Steer),
        )
        .await
        .expect("prompt");
    let asked = until(&mut events, |kind| match kind {
        EventKind::PermissionAsked(p) => Some(*p),
        _ => None,
    })
    .await;
    assert_eq!(asked.action, "outside_sandbox");
    assert_eq!(asked.resources, ["echo out > ../outside.txt"]);
    assert_eq!(asked.save, ["echo *", "> ../outside.txt"]);
    assert!(!root.join("outside.txt").exists());
    client
        .reply_permission(&session.id, &asked.id, PermissionReply::Once, None)
        .await
        .expect("allowed");
    for _ in 0..200 {
        if root.join("outside.txt").exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(
        std::fs::read_to_string(root.join("outside.txt")).expect("outside.txt"),
        "out\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_users_configuration_turns_the_sandbox_off() {
    let Some(sandbox) = confined().await else {
        return;
    };
    let (client, project, root) = (sandbox.client(), sandbox.project(), sandbox.dir.path());
    let config = root.join("config/harlo/harlo.json");
    let text = std::fs::read_to_string(&config).expect("config");
    let off = text.replacen('{', "{\n  \"sandbox\": false,", 1);
    std::fs::write(&config, off).expect("config");
    // A project's own configuration can't turn it back on or off: the
    // sandbox is the user's to set.
    std::fs::write(project.join("harlo.json"), r#"{ "sandbox": true }"#).expect("project config");
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .set_permissions(&session.id, &[Rule::any("*", Effect::Allow)])
        .await
        .expect("trusted");
    run(&client, &session.id, &shell("echo out > ../outside.txt")).await;
    assert_eq!(
        std::fs::read_to_string(root.join("outside.txt")).expect("outside.txt"),
        "out\n"
    );
    assert!(!system_prompt(root).contains("sandbox"));
    assert!(!offered_leaving(root));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_temporary_folder_takes_new_files_though_something_hidden_lies_in_it() {
    let Some(sandbox) = confined().await else {
        return;
    };
    let (client, project, root) = (sandbox.client(), sandbox.project(), sandbox.dir.path());
    // Something to hide where every command may write, as an SSH agent's
    // socket often is.
    let elsewhere = tempfile::tempdir_in("/tmp").expect("a folder in /tmp");
    let socket = elsewhere.path().join("agent.sock");
    std::fs::write(&socket, "").expect("the hidden thing");
    let config = root.join("config/harlo/harlo.json");
    let text = std::fs::read_to_string(&config).expect("config");
    let hides = serde_json::json!({ "hidden": [socket] }).to_string();
    let hidden = text.replacen('{', &format!("{{\n  \"sandbox\": {hides},"), 1);
    std::fs::write(&config, hidden).expect("config");
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .set_permissions(&session.id, &[Rule::any("*", Effect::Allow)])
        .await
        .expect("trusted");

    run(
        &client,
        &session.id,
        &shell("made=$(mktemp) && echo made-one && rm \"$made\"; echo in > inside.txt"),
    )
    .await;
    let result = last_result(root);
    assert!(result.contains("made-one"), "{result}");
    assert!(!result.contains("Permission denied"), "{result}");
    assert!(project.join("inside.txt").exists());
}
