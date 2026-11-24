//! Undo's acceptance tests, through the API with the scripted model:
//! uncommitted work from before the step survives; another session's later
//! edit survives (merged) or is reported (a clash); an edit another session
//! makes *during* a long step survives and is reported, as is a file nobody
//! can account for. Keeping an undo removes the undone messages; cancelling
//! it puts the files back.

// Rust guideline compliant 2026-10-09

mod support;

use std::path::Path;
use std::time::Duration;

use opencode_client::{Client, Delivery, Effect, Message, Prompt, Rule};
use serde_json::{Value, json};
use support::Sandbox;

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

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

async fn idles(client: &Client, session: &str) -> usize {
    client
        .messages(session)
        .await
        .expect("messages")
        .iter()
        .filter(|m| matches!(m, Message::Idle { .. }))
        .count()
}

/// The prompts of `session`, oldest first.
async fn prompts(client: &Client, session: &str) -> Vec<String> {
    client
        .messages(session)
        .await
        .expect("messages")
        .into_iter()
        .filter_map(|m| match m {
            Message::User { id, .. } => Some(id),
            _ => None,
        })
        .collect()
}

/// Stages an undo back to before `message` with its files; the server's
/// whole answer, report included.
async fn stage(sandbox: &Sandbox, session: &str, message: &str) -> Value {
    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/session/{session}/revert/stage",
            sandbox.url()
        ))
        .basic_auth("opencode", Some(&sandbox.password))
        .json(&json!({ "messageID": message, "files": true }))
        .send()
        .await
        .expect("stage");
    assert!(response.status().is_success(), "{}", response.status());
    response.json::<Value>().await.expect("json")["data"].clone()
}

fn kept(revert: &Value) -> Vec<(String, String)> {
    revert["report"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            (
                r["file"].as_str().unwrap_or_default().to_owned(),
                r["kept"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

async fn session(client: &Client, sandbox: &Sandbox, rules: &[Rule]) -> String {
    let s = client
        .create_session(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("session");
    client.set_permissions(&s.id, rules).await.expect("rules");
    s.id
}

#[tokio::test(flavor = "multi_thread")]
async fn uncommitted_work_survives_and_keeping_removes_the_messages() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    // Work of the person's, not committed, before the agent runs.
    std::fs::write(project.join("mine.txt"), "my draft\n").expect("mine");
    std::fs::write(project.join("a.txt"), "alpha\nmy uncommitted line\n").expect("a");
    let id = session(&client, &sandbox, &[]).await;
    turn(
        &client,
        &id,
        r#"tool edit {"path":"a.txt","oldString":"alpha","newString":"omega"} | tool write {"path":"new.txt","content":"made\n"} ;; say done"#,
    )
    .await;
    assert_eq!(read(&project.join("a.txt")), "omega\nmy uncommitted line\n");
    assert_eq!(read(&project.join("new.txt")), "made\n");

    let first = prompts(&client, &id).await[0].clone();
    let revert = stage(&sandbox, &id, &first).await;
    assert_eq!(read(&project.join("a.txt")), "alpha\nmy uncommitted line\n");
    assert!(!project.join("new.txt").exists());
    assert_eq!(read(&project.join("mine.txt")), "my draft\n", "untouched");
    let files: Vec<&str> = revert["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|f| f["file"].as_str())
        .collect();
    assert_eq!(files.len(), 2, "{revert}");
    assert!(kept(&revert).is_empty(), "{revert}");

    // Cancelled, the files come back; staged again and kept, the messages go.
    client.cancel_revert(&id).await.expect("cancel");
    assert_eq!(read(&project.join("a.txt")), "omega\nmy uncommitted line\n");
    assert_eq!(read(&project.join("new.txt")), "made\n");
    stage(&sandbox, &id, &first).await;
    client.commit_revert(&id).await.expect("keep");
    assert!(client.messages(&id).await.expect("messages").is_empty());
    assert_eq!(read(&project.join("a.txt")), "alpha\nmy uncommitted line\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_later_edit_by_another_session_survives_or_is_reported() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    std::fs::write(project.join("f.txt"), "one\ntwo\nthree\nfour\nfive\n").expect("f");
    std::fs::write(project.join("g.txt"), "g\n").expect("g");
    let a = session(&client, &sandbox, &[]).await;
    let b = session(&client, &sandbox, &[]).await;
    turn(
        &client,
        &a,
        r#"tool edit {"path":"f.txt","oldString":"two","newString":"TWO"} | tool edit {"path":"g.txt","oldString":"g","newString":"g by a"} ;; say done"#,
    )
    .await;
    // Afterwards B edits another line of f.txt, and the same line of g.txt.
    turn(
        &client,
        &b,
        r#"tool edit {"path":"f.txt","oldString":"five","newString":"FIVE"} | tool edit {"path":"g.txt","oldString":"g by a","newString":"g by b"} ;; say done"#,
    )
    .await;

    let first = prompts(&client, &a).await[0].clone();
    let revert = stage(&sandbox, &a, &first).await;
    assert_eq!(
        read(&project.join("f.txt")),
        "one\ntwo\nthree\nfour\nFIVE\n",
        "A's change undone, B's kept"
    );
    assert_eq!(
        read(&project.join("g.txt")),
        "g by b\n",
        "the clash is left"
    );
    assert_eq!(kept(&revert), [("g.txt".to_owned(), "conflict".to_owned())]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_made_during_a_long_step_survives_and_is_reported() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    std::fs::write(project.join("own.txt"), "own\n").expect("own");
    std::fs::write(project.join("b.txt"), "b\n").expect("b");
    std::fs::write(project.join("c.txt"), "c\n").expect("c");
    let commands = [Rule::any("shell", Effect::Allow)];
    let a = session(&client, &sandbox, &commands).await;
    let b = session(&client, &sandbox, &[]).await;

    // A edits its own file, then runs a long command.
    client
        .prompt(
            &a,
            &Prompt::new(
                r#"tool edit {"path":"own.txt","oldString":"own","newString":"own by a"} | tool shell {"command":"sleep 3","description":"wait"} ;; say done"#,
                Delivery::Queue,
            ),
        )
        .await
        .expect("prompt");
    for _ in 0..200 {
        if read(&project.join("own.txt")) == "own by a\n" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    // While it waits: B edits b.txt, and someone outside edits c.txt.
    turn(
        &client,
        &b,
        r#"tool edit {"path":"b.txt","oldString":"b","newString":"b by b"} ;; say done"#,
    )
    .await;
    std::fs::write(project.join("c.txt"), "c by someone\n").expect("c");
    for _ in 0..400 {
        if idles(&client, &a).await > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let first = prompts(&client, &a).await[0].clone();
    let revert = stage(&sandbox, &a, &first).await;
    assert_eq!(
        read(&project.join("own.txt")),
        "own\n",
        "A's own edit undone"
    );
    assert_eq!(
        read(&project.join("b.txt")),
        "b by b\n",
        "B's edit survives"
    );
    assert_eq!(
        read(&project.join("c.txt")),
        "c by someone\n",
        "the outside edit survives"
    );
    let mut report = kept(&revert);
    report.sort();
    assert_eq!(
        report,
        [
            ("b.txt".to_owned(), "another session".to_owned()),
            ("c.txt".to_owned(), "changed while a command ran".to_owned()),
        ],
        "{revert}"
    );
}
