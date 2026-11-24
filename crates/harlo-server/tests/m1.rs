//! M1 at the client level: `opencode-client`, unchanged, drives one whole
//! turn on `harlo-server` with the scripted model. An edit asks, is
//! approved and shows its diff. The server restarts and the session is
//! still there, and a client that names the last event it saw gets the
//! rest, once and in order.
//!
//! Each test runs its own server and model with a scratch `HOME` and XDG
//! folders and a random password.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use futures_util::StreamExt;
use opencode_client::{
    Client, Delivery, Effect, Endpoint, EventKind, Message, Part, PermissionReply, Prompt, Rule,
    ToolStatus,
};

/// A scratch folder with the scripted model, a git project and a server.
struct Sandbox {
    dir: tempfile::TempDir,
    model: Child,
    server: Child,
    port: u16,
    password: String,
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .expect("a free port")
        .port()
}

fn run(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("git")
        .success();
    assert!(ok, "git {args:?}");
}

impl Sandbox {
    async fn start() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("harlo-server-m1.")
            .tempdir()
            .expect("scratch");
        let root = dir.path();
        for sub in [
            "home",
            "config/opencode",
            "data",
            "state",
            "cache",
            "project",
        ] {
            std::fs::create_dir_all(root.join(sub)).expect("folder");
        }
        let model_port = free_port();
        let config = format!(
            r#"{{
  "model": "fake/fake-coder",
  "permissions": [{{ "action": "shell", "resource": "*", "effect": "ask" }}],
  "providers": {{ "fake": {{
    "name": "Fake",
    "settings": {{ "baseURL": "http://127.0.0.1:{model_port}/v1", "apiKey": "fake-key" }},
    "models": {{ "fake-coder": {{ "name": "Fake Coder", "limit": {{ "context": 128000, "output": 8192 }} }} }}
  }} }}
}}"#
        );
        std::fs::write(root.join("config/opencode/opencode.json"), config).expect("config");
        let project = root.join("project");
        run(&project, &["init", "-q"]);
        std::fs::write(project.join("a.txt"), "alpha\n").expect("a.txt");
        run(&project, &["add", "."]);
        run(
            &project,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-qm",
                "init",
            ],
        );

        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/fake-model.py");
        let model = Command::new("python3")
            .arg("-I")
            .arg(script)
            .arg(model_port.to_string())
            .arg(root.join("model.jsonl"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the scripted model");
        for _ in 0..200 {
            if tokio::net::TcpStream::connect(("127.0.0.1", model_port))
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let port = free_port();
        let password = format!("test-{}", opencode_client::message_id());
        let server = spawn_server(root, port, &password);
        let sandbox = Self {
            dir,
            model,
            server,
            port,
            password,
        };
        sandbox.wait_until_up().await;
        sandbox
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn client(&self) -> Client {
        Client::new(Endpoint::new(self.url(), Some(self.password.clone())))
    }

    fn project(&self) -> PathBuf {
        self.dir.path().join("project")
    }

    async fn wait_until_up(&self) {
        let client = self.client();
        for _ in 0..200 {
            if client.info().await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("harlo-server didn't come up");
    }

    async fn restart(&mut self) {
        let _ = self.server.kill();
        let _ = self.server.wait();
        self.server = spawn_server(self.dir.path(), self.port, &self.password);
        self.wait_until_up().await;
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        for child in [&mut self.server, &mut self.model] {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn spawn_server(root: &Path, port: u16, password: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_harlo-server"))
        .args([
            "serve",
            "--hostname",
            "127.0.0.1",
            "--port",
            &port.to_string(),
        ])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("OPENCODE_PASSWORD", password)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", root.join("no-bus").display()),
        )
        .current_dir(root.join("project"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start harlo-server")
}

const EDIT: &str = r#"tool edit {"path":"a.txt","oldString":"alpha","newString":"omega"} ;; say Changed alpha to omega."#;

#[tokio::test]
async fn one_approved_edit_then_a_restart_keeps_the_session() {
    let mut sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let mut events = client.events().await.expect("events");

    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .set_permissions(&session.id, &[Rule::any("edit", Effect::Ask)])
        .await
        .expect("careful");
    client
        .prompt(&session.id, &Prompt::new(EDIT, Delivery::Queue))
        .await
        .expect("prompt");

    let asked = until(&mut events, |kind| match kind {
        EventKind::PermissionAsked(p) => Some(*p),
        _ => None,
    })
    .await;
    assert_eq!(
        (asked.action.as_str(), asked.resources.as_slice()),
        ("edit", ["a.txt".to_owned()].as_slice())
    );
    assert!(asked.files[0].patch.contains("+omega"), "{:?}", asked.files);
    assert!(
        asked
            .message
            .as_deref()
            .is_some_and(|m| m.contains("session rule")),
        "{:?}",
        asked.message
    );
    assert_eq!(
        client
            .permissions(&session.id)
            .await
            .expect("pending")
            .len(),
        1
    );
    assert_eq!(
        std::fs::read_to_string(project.join("a.txt")).expect("read"),
        "alpha\n",
        "not before approval"
    );

    client
        .reply_permission(&session.id, &asked.id, PermissionReply::Once, None)
        .await
        .expect("approve");
    let outcome = until(&mut events, |kind| match kind {
        EventKind::Done {
            session_id,
            outcome,
        } if session_id == session.id => Some(outcome),
        _ => None,
    })
    .await;
    assert_eq!(outcome, "succeeded");
    assert_eq!(
        std::fs::read_to_string(project.join("a.txt")).expect("read"),
        "omega\n"
    );

    let diff = client.diff(&session.id).await.expect("diff");
    assert_eq!(diff.len(), 1);
    assert_eq!(
        (diff[0].file.as_str(), diff[0].additions, diff[0].deletions),
        ("a.txt", 1, 1)
    );
    assert!(
        diff[0].patch.contains("-alpha\n+omega"),
        "{}",
        diff[0].patch
    );

    let before = client.messages(&session.id).await.expect("messages");
    check_timeline(&before);

    sandbox.restart().await;
    let client = sandbox.client();
    let again = client
        .session(&session.id)
        .await
        .expect("the session survives a restart");
    assert_eq!(again.id, session.id);
    assert_eq!(
        client.messages(&session.id).await.expect("messages"),
        before
    );
    let listed = client.root_sessions().await.expect("sessions");
    assert!(listed.iter().any(|s| s.id == session.id));
}

/// The first event `pick` takes, within 20 seconds.
async fn until<T>(
    events: &mut opencode_client::Events,
    mut pick: impl FnMut(EventKind) -> Option<T>,
) -> T {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let event = events.next().await.expect("stream open").expect("event");
            if let Some(found) = pick(event.kind) {
                return found;
            }
        }
    })
    .await
    .expect("the event within 20 s")
}

/// User prompt, the edit step with its completed call, the answer, the end.
fn check_timeline(messages: &[Message]) {
    assert_eq!(messages.len(), 4, "{messages:#?}");
    assert!(matches!(&messages[0], Message::User { text, .. } if text == EDIT));
    let Message::Assistant { parts, .. } = &messages[1] else {
        panic!("a step: {:?}", messages[1])
    };
    let Part::Tool(call) = &parts[0] else {
        panic!("a call: {parts:?}")
    };
    assert_eq!(
        (call.name.as_str(), call.status),
        ("edit", ToolStatus::Completed)
    );
    assert_eq!(call.files.len(), 1, "the call carries its diff");
    assert!(
        matches!(&messages[2], Message::Assistant { parts, .. } if parts == &[Part::Text("Changed alpha to omega.".into())])
    );
    assert!(matches!(&messages[3], Message::Idle { outcome, .. } if outcome == "succeeded"));
}

#[tokio::test]
async fn a_reconnecting_client_gets_the_events_it_missed_once_and_in_order() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .prompt(&session.id, &Prompt::new("say hello", Delivery::Queue))
        .await
        .expect("prompt");
    for _ in 0..200 {
        let messages = client.messages(&session.id).await.expect("messages");
        if messages.iter().any(|m| matches!(m, Message::Idle { .. })) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let all = replay(&sandbox, 0).await;
    assert!(all.len() >= 6, "{all:?}");
    let seqs: Vec<u64> = all.iter().map(|(seq, _)| *seq).collect();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "in order: {seqs:?}");
    let types: Vec<&str> = all.iter().map(|(_, kind)| kind.as_str()).collect();
    assert_eq!(types.first(), Some(&"session.created"));
    assert_eq!(types.last(), Some(&"session.execution.succeeded"));

    // Having seen the third, the client gets exactly the rest.
    let rest = replay(&sandbox, seqs[2]).await;
    assert_eq!(rest, all[3..]);
}

/// The stored events after `after`, read from the stream with
/// `Last-Event-ID` until it goes quiet.
async fn replay(sandbox: &Sandbox, after: u64) -> Vec<(u64, String)> {
    let response = reqwest::Client::new()
        .get(format!("{}/api/event", sandbox.url()))
        .basic_auth("opencode", Some(&sandbox.password))
        .header("Last-Event-ID", after.to_string())
        .send()
        .await
        .expect("events");
    let mut body = response.bytes_stream();
    let mut text = String::new();
    while let Ok(Some(Ok(chunk))) =
        tokio::time::timeout(Duration::from_millis(500), body.next()).await
    {
        text.push_str(&String::from_utf8_lossy(&chunk));
    }
    let mut out = Vec::new();
    for block in text.split("\n\n") {
        let id = block
            .lines()
            .find_map(|l| l.strip_prefix("id: ").or_else(|| l.strip_prefix("id:")));
        let data = block
            .lines()
            .find_map(|l| l.strip_prefix("data: ").or_else(|| l.strip_prefix("data:")));
        if let (Some(id), Some(data)) = (id, data) {
            let v: serde_json::Value = serde_json::from_str(data).expect("json");
            out.push((
                id.trim().parse().expect("a number"),
                v["type"].as_str().unwrap_or_default().to_owned(),
            ));
        }
    }
    out
}
