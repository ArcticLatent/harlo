//! Scripts and the project's files: a script reads and searches files
//! where the rules allow it without asking, and only what it returns
//! enters the conversation, unless `experimental.scriptFiles` is turned
//! off.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Client, Delivery, Message, Prompt};
use support::Sandbox;

/// Finds the text files that hold a word, and tries a file the rules ask
/// about.
const SCRIPT: &str = r#"const paths = await tools.files.glob({pattern: "*.txt"});
const hits = [];
for (const p of paths) { const text = await tools.files.read({path: p}); if (text.includes("needle")) hits.push(p); }
let secret = "read";
try { await tools.files.read({path: ".env"}); } catch (e) { secret = String(e); }
const lines = await tools.files.read({path: "a.txt", offset: 2, limit: 1});
return { hits: hits.sort(), count: paths.length, secret, lines };"#;

/// Changes the sandbox's own config as `change` says.
fn configure(sandbox: &Sandbox, change: impl FnOnce(&mut serde_json::Value)) {
    let path = sandbox.dir.path().join("config/harlo/harlo.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("config")).expect("JSON");
    change(&mut config);
    std::fs::write(&path, config.to_string()).expect("config");
}

/// Sends `script` to a new session and waits for its turn to end;
/// returns what its tool calls returned.
async fn run(sandbox: &Sandbox, client: &Client, script: &str) -> Vec<String> {
    let session = client
        .create_session(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .prompt(&session.id, &Prompt::new(script, Delivery::Queue))
        .await
        .expect("prompt");
    for _ in 0..400 {
        let messages = client.messages(&session.id).await.expect("messages");
        if messages.iter().any(|m| matches!(m, Message::Idle { .. })) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let listed: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{}/api/session/{}/message",
            sandbox.url(),
            session.id
        ))
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
        .filter(|part| part["type"] == "tool")
        .map(|part| part["state"].to_string())
        .collect()
}

/// The description of the `execute` tool in the first request a turn
/// sent.
fn execute_description(sandbox: &Sandbox) -> String {
    let requests = support::turn_requests(sandbox.dir.path());
    let first = requests.first().expect("a request");
    first["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["function"]["name"] == "execute")
        .map(|t| t["function"]["description"].to_string())
        .expect("the execute tool")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_script_reads_and_searches_what_the_rules_allow() {
    let sandbox = Sandbox::start().await;
    let project = sandbox.project();
    std::fs::create_dir_all(project.join("notes")).expect("folder");
    for (name, text) in [
        ("a.txt", "alpha needle\nsecond line\nthird line\n"),
        ("b.txt", "beta\n"),
        ("notes/c.txt", "gamma needle\n"),
        (".env", "SECRET=needle\n"),
    ] {
        std::fs::write(project.join(name), text).expect("file");
    }
    let client = sandbox.client();
    let script = format!(
        "tool execute {} ;; say Found them.",
        serde_json::json!({ "code": SCRIPT })
    );
    let outputs = run(&sandbox, &client, &script).await;
    let [ran] = outputs.as_slice() else {
        panic!("one call: {outputs:?}")
    };
    // Three files looked at, two named: what the others hold stays out.
    assert!(
        ran.contains(r#"\"hits\":[\"a.txt\",\"notes/c.txt\"]"#),
        "{ran}"
    );
    assert!(ran.contains(r#"\"count\":3"#), "{ran}");
    assert!(ran.contains(r#"\"lines\":\"second line\""#), "{ran}");
    assert!(!ran.contains("beta") && !ran.contains("gamma"), "{ran}");
    // What the rules would ask about, a script doesn't get.
    assert!(ran.contains("may not read .env"), "{ran}");
    assert!(!ran.contains("SECRET"), "{ran}");

    let told = execute_description(&sandbox);
    assert!(
        told.contains("tools.files.read") && told.contains("tools.files.glob"),
        "{told}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn scripts_reach_no_files_when_it_is_turned_off() {
    let sandbox = Sandbox::start().await;
    configure(&sandbox, |config| {
        config["experimental"]["scriptFiles"] = false.into();
    });
    std::fs::write(sandbox.project().join("a.txt"), "alpha\n").expect("file");
    let client = sandbox.client();
    let code = r#"return await tools.files.read({path: "a.txt"});"#;
    let script = format!(
        "tool execute {} ;; say Tried.",
        serde_json::json!({ "code": code })
    );
    let outputs = run(&sandbox, &client, &script).await;
    let [ran] = outputs.as_slice() else {
        panic!("one call: {outputs:?}")
    };
    assert!(
        ran.contains("\"status\":\"error\"") && !ran.contains("alpha"),
        "{ran}"
    );
    assert!(!execute_description(&sandbox).contains("tools.files"));
}
