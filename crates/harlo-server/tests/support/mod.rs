//! A scratch sandbox for `harlo-server` tests: the scripted model, a git
//! project, and the server, with its own `HOME`, XDG folders and password.

// Rust guideline compliant 2026-10-09

#![allow(dead_code, reason = "each test file uses part of it")]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use harlo_client::{Client, Endpoint, EventKind, Events};

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

/// A free port on this computer, handed to no other test of this program.
///
/// Tests run side by side, and the system can give the same port twice
/// before either server has taken it.
pub fn free_port() -> u16 {
    static HANDED: std::sync::Mutex<Vec<u16>> = std::sync::Mutex::new(Vec::new());
    loop {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("a free port")
            .port();
        let mut handed = HANDED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !handed.contains(&port) {
            handed.push(port);
            return port;
        }
    }
}

/// Starts the scripted model, logging its requests to `log`; returns it
/// and its port.
///
/// The model takes a free port itself and says which: one found free here
/// first could be handed to another test before the model took it.
pub fn start_model(log: &Path) -> (Child, u16) {
    use std::io::BufRead as _;

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/fake-model.py");
    let mut model = Command::new("python3")
        .arg("-I")
        .arg(script)
        .arg("0")
        .arg(log)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the scripted model");
    let said = model.stdout.take().expect("the model's output");
    let mut line = String::new();
    std::io::BufReader::new(said)
        .read_line(&mut line)
        .expect("the model's port");
    let port = line.trim().parse().expect("the scripted model's port");
    (model, port)
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
        Self::start_in(&std::env::temp_dir(), env).await
    }

    /// A sandbox away from the temporary folders, where the sandbox for
    /// agents' commands holds: everything may write to the temporary
    /// folders, so nothing in them is closed to a command.
    ///
    /// Its scratch folder is made in a folder of its own under the cache
    /// folder, and removed with it.
    pub async fn start_outside_tmp() -> Self {
        let caches = std::env::var_os("XDG_CACHE_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::home_dir().map(|home| home.join(".cache")))
            .expect("a cache folder");
        let base = caches.join("harlo-tests");
        std::fs::create_dir_all(&base).expect("scratch base");
        Self::start_in(&base, &[]).await
    }

    /// A sandbox in a new folder under `base`, whose server also gets `env`.
    async fn start_in(base: &Path, env: &[(&str, &str)]) -> Self {
        let env: Vec<(String, String)> = env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let dir = tempfile::Builder::new()
            .prefix("harlo-server-m1.")
            .tempdir_in(base)
            .expect("scratch");
        let root = dir.path();
        for sub in ["home", "config/harlo", "data", "state", "cache", "project"] {
            std::fs::create_dir_all(root.join(sub)).expect("folder");
        }
        let (model, model_port) = start_model(&root.join("model.jsonl"));
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
        std::fs::write(root.join("config/harlo/harlo.json"), config).expect("config");
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

        let port = free_port();
        let password = format!("test-{}", harlo_client::message_id());
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
        self.restart_after(|_| {}).await;
    }

    /// Stops the server, lets `change` work on the sandbox's folder while
    /// nothing runs there, and starts the server again.
    pub async fn restart_after(&mut self, change: impl FnOnce(&Path)) {
        let _ = self.server.kill();
        let _ = self.server.wait();
        change(self.dir.path());
        self.server = spawn_server(self.dir.path(), self.port, &self.password, &self.env);
        self.wait_until_up().await;
    }

    /// Where the sandbox's server keeps its store.
    pub fn database(&self) -> PathBuf {
        self.dir.path().join("data/harlo/server.db")
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
        .env("HARLO_SERVER_PASSWORD", password)
        // Key checks go to an address nothing answers at, never a real
        // provider; a test that checks keys sets its own stand-in.
        .env("HARLO_KEY_CHECK_BASE", "http://127.0.0.1:9")
        // So is the list of published plugins, never GitHub.
        .env("HARLO_PLUGIN_GALLERY_BASE", "http://127.0.0.1:9")
        // So is the phone relay, unless a test sets its own.
        .env("HARLO_RELAY", "http://127.0.0.1:9")
        // Never the real Claude Code: nothing is there unless a test puts
        // a stand-in in its place, or names its own.
        .env("HARLO_CLAUDE_PATH", root.join("bin/claude"))
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

/// Allows the settings `folder`'s own config asks for, as a person would:
/// the first run there asks, and is answered "always".
pub async fn allow_project_settings(client: &Client, folder: &str) {
    let mut events = client.events().await.expect("events");
    let session = client.create_session(folder).await.expect("session");
    client
        .prompt(
            &session.id,
            &harlo_client::Prompt::new("say settled", harlo_client::Delivery::Queue),
        )
        .await
        .expect("prompt");
    let asked = until(&mut events, |kind| match kind {
        EventKind::PermissionAsked(p) => Some(*p),
        _ => None,
    })
    .await;
    assert_eq!(asked.action, "project_config", "{asked:?}");
    client
        .reply_permission(
            &session.id,
            &asked.id,
            harlo_client::PermissionReply::Always,
            None,
        )
        .await
        .expect("allowed");
    until(&mut events, |kind| match kind {
        EventKind::Done { .. } => Some(()),
        _ => None,
    })
    .await;
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
        .basic_auth("harlo", Some(&sandbox.password))
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
/// first: titles and proposed notes, asked for on the side, are left out.
#[must_use]
pub fn turn_requests(root: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(root.join("model.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .map(|r| r["body"].clone())
        .filter(|b| {
            !b["messages"][0]["content"].as_str().is_some_and(|s| {
                s.starts_with(harlo_engine::generate::TITLE_SYSTEM)
                    || s.starts_with(harlo_engine::memory::PROPOSE_SYSTEM)
            })
        })
        .collect()
}
