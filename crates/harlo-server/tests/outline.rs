//! A file's outline: `read` can list what a source file declares, and the
//! lines each part spans, instead of its text, unless
//! `experimental.outline` is turned off.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Delivery, Message, Prompt};
use support::Sandbox;

/// A source file with a type, a method written over three lines, and a
/// function.
const SOURCE: &str = "\
pub struct Counter {
    seen: u32,
}

impl Counter {
    pub fn add(
        &mut self,
    ) {
        self.seen += 1;
    }
}

fn main() {
    let secret_body_text = 1;
}
";

/// Sets whether outlines are on in the sandbox's own config.
fn configure(sandbox: &Sandbox, outline: bool) {
    let path = sandbox.dir.path().join("config/harlo/harlo.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("config")).expect("JSON");
    config["experimental"]["outline"] = outline.into();
    std::fs::write(&path, config.to_string()).expect("config");
}

/// Asks the scripted model to read `file` for its outline in a new
/// session; returns what the read returned.
async fn outline_of(sandbox: &Sandbox, file: &str) -> String {
    let client = sandbox.client();
    let session = client
        .create_session(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("session");
    let script = format!(
        "tool read {} ;; say Seen.",
        serde_json::json!({ "path": file, "outline": true })
    );
    client
        .prompt(&session.id, &Prompt::new(&script, Delivery::Queue))
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
        .find(|part| part["type"] == "tool")
        .and_then(|part| {
            part["state"]["content"][0]["text"]
                .as_str()
                .map(str::to_owned)
        })
        .expect("the read's result")
}

/// The `read` tool as the first request of a turn offered it.
fn read_tool(sandbox: &Sandbox) -> serde_json::Value {
    let requests = support::turn_requests(sandbox.dir.path());
    let first = requests.first().expect("a request");
    first["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["function"]["name"] == "read")
        .map(|t| t["function"].clone())
        .expect("the read tool")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_lists_what_a_file_declares() {
    let sandbox = Sandbox::start().await;
    std::fs::write(sandbox.project().join("counter.rs"), SOURCE).expect("file");
    std::fs::write(sandbox.project().join("notes.md"), "# Notes\n").expect("file");

    let listed = outline_of(&sandbox, "counter.rs").await;
    assert_eq!(
        listed,
        "counter.rs: 15 lines. What it declares, after the lines each spans:\n\
         1-3\tpub struct Counter\n\
         5-11\timpl Counter\n\
         6-10\t  pub fn add( &mut self, )\n\
         13-15\tfn main()\n"
    );
    let tool = read_tool(&sandbox);
    assert!(
        tool["parameters"]["properties"]["outline"].is_object(),
        "{tool}"
    );
    assert!(
        tool["description"].to_string().contains("outline"),
        "{tool}"
    );

    // A file in no language read here says so, and what to do instead.
    let none = outline_of(&sandbox, "notes.md").await;
    assert!(none.starts_with("notes.md has no outline"), "{none}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_gives_the_file_when_outlines_are_off() {
    let sandbox = Sandbox::start().await;
    configure(&sandbox, false);
    std::fs::write(sandbox.project().join("counter.rs"), SOURCE).expect("file");

    // Not offered, so an `outline` the model sends anyway changes nothing.
    let read = outline_of(&sandbox, "counter.rs").await;
    assert!(read.contains("secret_body_text"), "{read}");
    assert!(read.starts_with("1\tpub struct Counter {"), "{read}");
    let tool = read_tool(&sandbox);
    assert!(
        tool["parameters"]["properties"]["outline"].is_null(),
        "{tool}"
    );
    assert!(
        !tool["description"].to_string().contains("outline"),
        "{tool}"
    );
}
