//! A scratch sandbox for `harlo-server` tests: the scripted model, a git
//! project, and the server, with its own `HOME`, XDG folders and password.

// Rust guideline compliant 2026-10-09

#![allow(dead_code, reason = "each test file uses part of it")]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use opencode_client::{Client, Endpoint, EventKind, Events};

/// A scratch folder with the scripted model, a git project and a server.
pub struct Sandbox {
    pub dir: tempfile::TempDir,
    model: Child,
    server: Child,
    pub port: u16,
    pub password: String,
    /// What the server's environment adds.
    env: Vec<(String, String)>,
}

pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .expect("a free port")
        .port()
}

pub fn run(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("git")
        .success();
    assert!(ok, "git {args:?}");
}

impl Sandbox {
    pub async fn start() -> Self {
        Self::start_with_env(&[]).await
    }

    /// A sandbox whose server also gets `env`.
    pub async fn start_with_env(env: &[(&str, &str)]) -> Self {
        let env: Vec<(String, String)> = env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
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
        let server = spawn_server(root, port, &password, &env);
        let sandbox = Self {
            dir,
            model,
            server,
            port,
            password,
            env,
        };
        sandbox.wait_until_up().await;
        sandbox
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn client(&self) -> Client {
        Client::new(Endpoint::new(self.url(), Some(self.password.clone())))
    }

    pub fn project(&self) -> PathBuf {
        self.dir.path().join("project")
    }

    pub async fn wait_until_up(&self) {
        let client = self.client();
        for _ in 0..200 {
            if client.info().await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("harlo-server didn't come up");
    }

    pub async fn restart(&mut self) {
        let _ = self.server.kill();
        let _ = self.server.wait();
        self.server = spawn_server(self.dir.path(), self.port, &self.password, &self.env);
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

pub fn spawn_server(root: &Path, port: u16, password: &str, env: &[(String, String)]) -> Child {
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
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
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

/// The first event `pick` takes, within 20 seconds.
pub async fn until<T>(events: &mut Events, mut pick: impl FnMut(EventKind) -> Option<T>) -> T {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let event = futures_util::StreamExt::next(events)
                .await
                .expect("stream open")
                .expect("event");
            if let Some(found) = pick(event.kind) {
                return found;
            }
        }
    })
    .await
    .expect("the event within 20 s")
}

/// The stored events after `after`, read from the stream with
/// `Last-Event-ID` until it goes quiet.
pub async fn replay(sandbox: &Sandbox, after: u64) -> Vec<(u64, String)> {
    let response = reqwest::Client::new()
        .get(format!("{}/api/event", sandbox.url()))
        .basic_auth("opencode", Some(&sandbox.password))
        .header("Last-Event-ID", after.to_string())
        .send()
        .await
        .expect("events");
    let mut body = response.bytes_stream();
    let mut text = String::new();
    while let Ok(Some(Ok(chunk))) = tokio::time::timeout(
        Duration::from_millis(500),
        futures_util::StreamExt::next(&mut body),
    )
    .await
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

/// The bodies of the requests turns sent the scripted model, oldest
/// first: titles asked for on the side are left out.
#[must_use]
pub fn turn_requests(root: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(root.join("model.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .map(|r| r["body"].clone())
        .filter(|b| {
            !b["messages"][0]["content"]
                .as_str()
                .is_some_and(|s| s.starts_with(harlo_engine::generate::TITLE_SYSTEM))
        })
        .collect()
}
