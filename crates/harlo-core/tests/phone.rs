//! The phone bridge against a private server: pairing, the sealed
//! channel, the session list, a session's rows as they stream, messages,
//! approvals and new sessions, as the phone app sees them.
//!
//! Each test runs its own `harlo-server` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service. The phone is a
//! stand-in that speaks the phone app's protocol.

// Rust guideline compliant 2026-10-09

#[path = "../../harlo-client/tests/support/mod.rs"]
mod support;
mod test_phone;

use std::time::Duration;

use harlo_client::{Effect, Rule};
use harlo_core::phone::{Identity, Offer, Phone, Settings};
use harlo_core::server::Location;
use serde_json::{Value, json};
use support::Sandbox;
use test_phone::{TestPhone, connect};

/// Starts phone access for the sandbox's server, keeping its files in `folder`.
async fn phone_for(sandbox: &Sandbox, folder: &std::path::Path) -> Phone {
    Phone::start(Settings {
        theme: serde_json::Value::Null,
        folder: folder.to_owned(),
        name: "Test computer".to_owned(),
        location: Location::Server {
            url: sandbox.url.clone(),
            password: Some(sandbox.password.clone()),
        },
        hidden: Vec::new(),
        listen: "127.0.0.1:0".parse().expect("an address"),
        paused: false,
    })
    .await
    .expect("phone access starts")
}

/// Starts phone access and pairs a stand-in phone with it.
async fn paired(sandbox: &Sandbox, folder: &std::path::Path) -> (Phone, TestPhone) {
    let phone = phone_for(sandbox, folder).await;
    let code = phone.offer().code;
    let p = connect(
        phone.address(),
        &Identity::generate(),
        &phone.public_key(),
        &code,
    )
    .await
    .expect("pairs");
    (phone, p)
}

/// Asks for the session list until it has `key` (Harlo may still be loading
/// the server), and returns its row.
async fn listed(p: &mut TestPhone, key: &str) -> Value {
    for _ in 0..100 {
        let list = p.call(json!({ "op": "sessions" })).await;
        if let Some(row) = list["sessions"]
            .as_array()
            .and_then(|l| l.iter().find(|s| s["key"] == key))
        {
            return row.clone();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the session {key} was never listed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_pairs_once_and_is_turned_away_when_removed() {
    let sandbox = Sandbox::start().await;
    let folder = tempfile::tempdir().expect("scratch folder");
    let phone = phone_for(&sandbox, folder.path()).await;
    let harlo = phone.public_key();
    let key = Identity::generate();

    // An unpaired phone without a code is turned away.
    let denied = connect(phone.address(), &key, &harlo, "")
        .await
        .err()
        .expect("denied");
    assert_eq!(denied["type"], "denied");

    // The QR code's link leads to Harlo's own page, with the offer after `#p=`.
    let address = phone.address();
    phone.set_endpoints(vec![format!("ws://{address}/ws")]);
    let offer = phone.offer();
    let link = offer.link();
    assert!(link.starts_with(&format!("http://{address}/#p=")), "{link}");
    assert_eq!(Offer::decode(&link).expect("the link reads"), offer);
    let page = reqwest::get(format!("http://{address}/"))
        .await
        .expect("the page")
        .text()
        .await
        .expect("its text");
    assert!(page.contains("Harlo app"), "{page}");

    // With the offer's code it pairs, and pings through the sealed channel.
    let mut paired = connect(phone.address(), &key, &harlo, &offer.code)
        .await
        .expect("pairs");
    assert_eq!(paired.call(json!({ "op": "ping" })).await["pong"], true);
    let devices = phone.devices();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].name, "Test phone");

    // The code is spent: another phone can't use it.
    let stranger = Identity::generate();
    let reused = connect(phone.address(), &stranger, &harlo, &offer.code).await;
    assert_eq!(reused.err().expect("denied")["type"], "denied");

    // Paired, the phone comes back without a code.
    let mut again = connect(phone.address(), &key, &harlo, "")
        .await
        .expect("back in");
    assert_eq!(again.call(json!({ "op": "ping" })).await["pong"], true);
    assert_eq!(phone.connected().len(), 1, "one phone, connected twice");

    // The key and the phone are kept for next time.
    let reopened = harlo_core::phone::saved_devices(folder.path());
    assert_eq!(reopened.len(), 1);

    // Removed in Harlo: each connection is told and closed.
    assert!(phone.remove(&devices[0].id).expect("removed"));
    assert_eq!(
        paired
            .until("revoked", |_, m| (m["ev"] == "revoked").then_some(()))
            .await,
        ()
    );
    assert!(paired.next().await.is_null(), "Harlo hangs up");
    assert_eq!(
        again
            .until("revoked", |_, m| (m["ev"] == "revoked").then_some(()))
            .await,
        ()
    );
    let gone = connect(phone.address(), &key, &harlo, "").await;
    assert_eq!(gone.err().expect("denied")["type"], "denied");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_follows_a_session_sends_and_approves() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let folder = tempfile::tempdir().expect("scratch folder");
    let (_phone, mut p) = paired(&sandbox, folder.path()).await;

    // The list has the session, under its project.
    let row = listed(&mut p, &session.id).await;
    assert_eq!(row["project"], "project");
    assert_eq!(row["status"], "idle");
    assert_eq!(row["harness"], "harlo");

    // Opening it: the reply, then its (empty) rows and status.
    let opened = p.call(json!({ "op": "open", "key": session.id })).await;
    assert_eq!(opened["key"], session.id.as_str());
    p.until("its rows", |_, m| (m["ev"] == "reset").then_some(()))
        .await;

    // A message from the phone: the agent's answer streams into its rows.
    let sent = p
        .call(json!({ "op": "send", "key": session.id, "text": "say hello from the phone" }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    p.until("the answer", |phone, _| {
        phone
            .text()
            .contains("markdown hello from the phone")
            .then_some(())
    })
    .await;
    assert!(
        p.text().contains("user say hello from the phone"),
        "{}",
        p.text()
    );
    p.until("idle", |_, m| {
        (m["ev"] == "status" && m["status"] == "idle").then_some(())
    })
    .await;

    // An edit that needs approval: the phone is asked, allows it once, and
    // the agent goes on.
    client
        .set_permissions(&session.id, &[Rule::any("edit", Effect::Ask)])
        .await
        .expect("rules");
    std::fs::write(sandbox.project().join("e.txt"), "one\ntwo\n").expect("e.txt");
    let script =
        r#"tool edit {"path":"e.txt","oldString":"two","newString":"three"} ;; say edited"#;
    let sent = p
        .call(json!({ "op": "send", "key": session.id, "text": script }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    let request = p
        .until("the approval", |_, m| {
            (m["ev"] == "request").then(|| m["request"].clone())
        })
        .await;
    assert_eq!(request["approval"], true);
    assert!(
        request["title"]
            .as_str()
            .unwrap_or_default()
            .starts_with("Allow tool: edit"),
        "{request}"
    );
    let answered = p
        .call(
            json!({ "op": "answer", "key": session.id, "requestId": request["id"],
                      "answer": { "value": "Allow once" } }),
        )
        .await;
    assert_eq!(answered["ok"], true, "{answered}");
    p.until("cleared", |_, m| {
        (m["ev"] == "cleared" && m["id"] == request["id"]).then_some(())
    })
    .await;
    p.until("edited", |phone, _| {
        phone.text().contains("markdown edited").then_some(())
    })
    .await;
    assert_eq!(
        std::fs::read_to_string(sandbox.project().join("e.txt")).expect("e.txt"),
        "one\nthree\n"
    );
    let tool = p
        .rows
        .iter()
        .find(|r| r["kind"] == "tool")
        .expect("a tool row")
        .clone();
    assert_eq!(tool["state"], "done");
    assert!(
        tool["diff"].as_str().unwrap_or_default().contains("+three"),
        "{tool}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_starts_and_renames_sessions() {
    let sandbox = Sandbox::start().await;
    let session = sandbox
        .client()
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let folder = tempfile::tempdir().expect("scratch folder");
    let (_phone, mut p) = paired(&sandbox, folder.path()).await;
    listed(&mut p, &session.id).await;

    // A new session from the phone, in the first project, with a first message.
    let choices = p.call(json!({ "op": "projects" })).await;
    assert_eq!(choices["projects"][0]["name"], "project", "{choices}");
    let started = p
        .call(json!({ "op": "new", "project": 0, "agent": "", "model": "", "text": "say started on the phone" }))
        .await;
    let new_key = started["key"].as_str().expect("a new session").to_owned();
    assert_ne!(new_key, session.id);
    p.until("the new session listed", |_, m| {
        (m["ev"] == "sessions"
            && m["sessions"]
                .as_array()
                .is_some_and(|l| l.iter().any(|s| s["key"] == new_key.as_str())))
        .then_some(())
    })
    .await;

    // Renamed from the phone: the list follows.
    let renamed = p
        .call(json!({ "op": "rename", "key": session.id, "name": "From the phone" }))
        .await;
    assert_eq!(renamed["ok"], true, "{renamed}");
    p.until("the new title", |_, m| {
        (m["ev"] == "sessions"
            && m["sessions"].as_array().is_some_and(|l| {
                l.iter()
                    .any(|s| s["key"] == session.id.as_str() && s["title"] == "From the phone")
            }))
        .then_some(())
    })
    .await;

    // Something Harlo doesn't do from a phone says so.
    let unknown = p.call(json!({ "op": "wiki", "key": session.id })).await;
    assert!(unknown["error"].is_string(), "{unknown}");
}

/// The first row of the open session's `jobs` event that `pick` accepts.
async fn job_row(p: &mut TestPhone, what: &str, pick: impl Fn(&Value) -> bool) -> Value {
    p.until(what, |_, m| {
        (m["ev"] == "jobs")
            .then(|| m["items"].as_array()?.iter().find(|row| pick(row)).cloned())
            .flatten()
    })
    .await
}

/// Waits until the open session's `jobs` event has no rows.
async fn no_job_rows(p: &mut TestPhone, what: &str) {
    p.until(what, |_, m| {
        (m["ev"] == "jobs" && m["items"] == json!([])).then_some(())
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_moves_a_command_to_the_background_and_follows_a_goal() {
    let sandbox = Sandbox::start().await;
    // The goal's reviewer finds it blocked, so it pauses with a reason.
    std::fs::write(
        sandbox.root().join("scripts.json"),
        json!({ "<reviewer>": r#"{"verdict":"blocked","reason":"It needs the release date."}"# })
            .to_string(),
    )
    .expect("the reviewer's verdict");
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    client
        .set_permissions(&session.id, &[Rule::any("shell", Effect::Allow)])
        .await
        .expect("rules");
    let folder = tempfile::tempdir().expect("scratch folder");
    let (_phone, mut p) = paired(&sandbox, folder.path()).await;
    listed(&mut p, &session.id).await;
    let key = session.id.as_str();

    // Opened: nothing in the background, and what can be typed after "/".
    let opened = p.call(json!({ "op": "open", "key": key })).await;
    assert_eq!(opened["key"], key);
    no_job_rows(&mut p, "nothing in the background").await;
    let commands = p
        .until("the commands", |_, m| {
            (m["ev"] == "commands").then(|| m["commands"].clone())
        })
        .await;
    let names: Vec<&str> = commands
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert!(names.contains(&"compact"), "{names:?}");
    assert!(names.contains(&"goal"), "{names:?}");

    // A command the agent waits for: the phone is told its call, sends it
    // to the background, and the turn goes on without it.
    let script = r#"tool shell {"command":"sleep 30"} ;; say moved on"#;
    let sent = p
        .call(json!({ "op": "send", "key": key, "text": script }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    let call = p
        .until("a command under way", |_, m| {
            (m["ev"] == "jobs")
                .then(|| {
                    m["background"]
                        .as_str()
                        .filter(|c| !c.is_empty())
                        .map(str::to_owned)
                })
                .flatten()
        })
        .await;
    let moved = p
        .call(json!({ "op": "background", "key": key, "call": call }))
        .await;
    assert_eq!(moved["ok"], true, "{moved}");
    let row = job_row(&mut p, "the command in the background", |row| {
        row["title"] == "Command"
    })
    .await;
    assert_eq!(row["text"], "sleep 30");
    assert_eq!(row["note"], "In the background");
    assert_eq!(row["action"], "Stop");
    p.until("the turn going on", |phone, _| {
        phone.text().contains("markdown moved on").then_some(())
    })
    .await;

    // Stop, from its row.
    let stopped = p
        .call(json!({ "op": "jobAction", "key": key, "row": row["id"] }))
        .await;
    assert_eq!(stopped["ok"], true, "{stopped}");
    no_job_rows(&mut p, "the command stopped").await;
    let refused = p
        .call(json!({ "op": "jobAction", "key": key, "row": "something else" }))
        .await;
    assert!(refused["error"].is_string(), "{refused}");

    // A goal typed as the command it is: its row says where it stands and
    // offers the one thing to do next.
    let sent = p
        .call(json!({ "op": "send", "key": key, "text": "/goal Ship it" }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    let paused = job_row(&mut p, "the goal paused", |row| row["note"] == "Paused").await;
    assert_eq!(paused["title"], "Goal");
    assert_eq!(paused["text"], "Ship it");
    assert_eq!(paused["detail"], "It needs the release date.");
    assert_eq!(paused["action"], "Resume");
    std::fs::remove_file(sandbox.root().join("scripts.json")).expect("met from now on");
    let resumed = p
        .call(json!({ "op": "jobAction", "key": key, "row": paused["id"] }))
        .await;
    assert_eq!(resumed["ok"], true, "{resumed}");
    let reached = job_row(&mut p, "the goal reached", |row| row["note"] == "Reached").await;
    assert_eq!(reached["action"], "Dismiss");
    let cleared = p
        .call(json!({ "op": "jobAction", "key": key, "row": reached["id"] }))
        .await;
    assert_eq!(cleared["ok"], true, "{cleared}");
    no_job_rows(&mut p, "the goal dismissed").await;
}

/// A small PNG, as a phone sends a photo: base64.
fn photo() -> String {
    use base64::Engine as _;
    // A 1x1 red pixel.
    const PNG: [u8; 69] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8,
        0xcf, 0xc0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    base64::engine::general_purpose::STANDARD.encode(PNG)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_reads_an_edit_before_refusing_it_with_a_reason() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    client
        .set_permissions(&session.id, &[Rule::any("edit", Effect::Ask)])
        .await
        .expect("rules");
    std::fs::write(sandbox.project().join("e.txt"), "one\ntwo\n").expect("e.txt");
    let folder = tempfile::tempdir().expect("scratch folder");
    let (_phone, mut p) = paired(&sandbox, folder.path()).await;
    listed(&mut p, &session.id).await;
    let key = session.id.as_str();
    p.call(json!({ "op": "open", "key": key })).await;

    // The approval says what the agent wants, shows the change, and what
    // Allow always would cover.
    let script =
        r#"tool edit {"path":"e.txt","oldString":"two","newString":"three"} ;; say I left it"#;
    let sent = p
        .call(json!({ "op": "send", "key": key, "text": script }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    let request = p
        .until("the approval", |_, m| {
            (m["ev"] == "request").then(|| m["request"].clone())
        })
        .await;
    assert_eq!(request["wants"], "edit a file");
    assert_eq!(request["detail"], "e.txt");
    let diff = request["diff"].as_str().unwrap_or_default();
    assert!(
        diff.contains("-two") && diff.contains("+three"),
        "{request}"
    );
    assert!(
        request["always"]
            .as_str()
            .unwrap_or_default()
            .starts_with("Allow always applies to "),
        "{request}"
    );

    // Refused with a reason: the agent is told it and carries on.
    let answered = p
        .call(
            json!({ "op": "answer", "key": key, "requestId": request["id"],
                      "answer": { "value": "Deny", "reason": "Keep it at two." } }),
        )
        .await;
    assert_eq!(answered["ok"], true, "{answered}");
    p.until("the agent's answer", |phone, _| {
        phone.text().contains("markdown I left it").then_some(())
    })
    .await;
    let refused = p
        .rows
        .iter()
        .find(|r| r["kind"] == "tool")
        .expect("the edit's row")
        .clone();
    assert_eq!(refused["state"], "error");
    assert!(
        refused["result"]
            .as_str()
            .unwrap_or_default()
            .ends_with("said: Keep it at two."),
        "{refused}"
    );
    assert_eq!(
        std::fs::read_to_string(sandbox.project().join("e.txt")).expect("e.txt"),
        "one\ntwo\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_reads_a_script_and_gets_a_sessions_pictures() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    client
        .set_permissions(
            &session.id,
            &[
                Rule::any("execute", Effect::Allow),
                Rule::any("read", Effect::Allow),
            ],
        )
        .await
        .expect("rules");
    std::fs::write(sandbox.project().join("e.txt"), "one\ntwo\n").expect("e.txt");
    let folder = tempfile::tempdir().expect("scratch folder");
    let (_phone, mut p) = paired(&sandbox, folder.path()).await;
    listed(&mut p, &session.id).await;
    let key = session.id.as_str();
    p.call(json!({ "op": "open", "key": key })).await;

    // A script: its row carries the code and the tools it called.
    let script = r#"tool execute {"code":"// Read the file\nconst t = await tools.project.read({path: 'e.txt'});\nreturn t.length;"} ;; say counted"#;
    let sent = p
        .call(json!({ "op": "send", "key": key, "text": script }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    p.until("the script's answer", |phone, _| {
        phone.text().contains("markdown counted").then_some(())
    })
    .await;
    let ran = p
        .rows
        .iter()
        .find(|r| r["tool"] == "execute")
        .expect("the script's row")
        .clone();
    assert_eq!(ran["subtitle"], "Read the file");
    assert_eq!(ran["result"], "1 call");
    assert_eq!(
        ran["code"]["lines"][0],
        json!({ "f": "Called project.read" })
    );
    assert_eq!(
        ran["code"]["lines"][1],
        json!({ "n": "1", "h": "// Read the file" })
    );

    // A photo sent from the phone shows under its message, and is asked
    // for by the name its row gives it.
    let sent = p
        .call(json!({ "op": "send", "key": key, "text": "say seen",
                      "images": [{ "data": photo(), "mimeType": "image/png" }] }))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    p.until("the answer about the photo", |phone, _| {
        phone.text().contains("markdown seen").then_some(())
    })
    .await;
    let named = p
        .rows
        .iter()
        .filter(|r| r["kind"] == "user")
        .find_map(|r| r["images"][0].as_str().map(str::to_owned))
        .expect("a picture under the message");
    for size in ["thumb", "screen"] {
        let picture = p
            .call(json!({ "op": "picture", "key": key, "path": named, "size": size }))
            .await;
        assert_eq!(picture["width"], 1, "{picture}");
        assert_eq!(picture["type"], "image/jpeg");
        assert!(!picture["data"].as_str().unwrap_or_default().is_empty());
    }
    let missing = p
        .call(
            json!({ "op": "picture", "key": key, "path": "00000000000000aa.png", "size": "thumb" }),
        )
        .await;
    assert!(missing["error"].is_string(), "{missing}");
}
