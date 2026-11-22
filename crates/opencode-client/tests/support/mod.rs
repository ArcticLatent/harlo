//! A private OpenCode server and scripted model for integration tests.
//!
//! Each [`Sandbox`] gets its own scratch folder with `HOME` and every XDG folder
//! inside it, a git project, a random password and a free port. It never
//! touches the user's OpenCode config, sessions or shared background service.
//! Dropping it stops both processes and removes the folder.

// Rust guideline compliant 2026-10-09

#![allow(
    dead_code,
    reason = "each test file uses a different part of the harness"
)]

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use opencode_client::{Client, Endpoint};

/// How long to wait for the server to answer after starting it.
///
/// OpenCode needs a few seconds on first start to set up its database.
const START_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a test waits for something it expects to happen.
pub const WAIT: Duration = Duration::from_secs(20);

/// A running private OpenCode server with the scripted model.
#[derive(Debug)]
pub struct Sandbox {
    dir: tempfile::TempDir,
    server: Child,
    model: Child,
    pub url: String,
    pub password: String,
}

/// Returns the pinned OpenCode binary for tests.
fn binary() -> PathBuf {
    if let Some(bin) = std::env::var_os("HARLO_OPENCODE_BIN") {
        return bin.into();
    }
    let home = std::env::var_os("HOME").expect("HOME is set");
    Path::new(&home)
        .join(".local/share/harlo-dev/opencode")
        .join(opencode_client::OPENCODE_VERSION)
        .join("opencode")
}

/// Starts `opencode serve` for the sandbox in `root`.
fn spawn_server(root: &Path, port: u16, password: &str) -> Child {
    let project = root.join("project");
    let mut server = Command::new(binary());
    scratch_env(&mut server, root, password);
    server
        .args([
            "serve",
            "--hostname",
            "127.0.0.1",
            "--port",
            &port.to_string(),
        ])
        .env("PWD", &project)
        .current_dir(&project)
        .stdout(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(root.join("opencode.log"))
                .expect("log"),
        )
        .stderr(Stdio::null())
        .spawn()
        .expect("start opencode serve")
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("a free local port")
        .local_addr()
        .expect("bound address")
        .port()
}

impl Sandbox {
    /// Starts the scripted model and a private `opencode serve`.
    pub async fn start() -> Self {
        let bin = binary();
        assert!(
            bin.exists(),
            "pinned OpenCode binary missing: {}",
            bin.display()
        );
        let dir = tempfile::Builder::new()
            .prefix("harlo-oc.")
            .tempdir()
            .expect("scratch folder");
        let root = dir.path();
        for sub in [
            "home",
            "config/opencode",
            "data",
            "state",
            "cache",
            "project",
        ] {
            std::fs::create_dir_all(root.join(sub)).expect("scratch subfolder");
        }
        let model_port = free_port();
        let port = free_port();
        let password = format!("test-{}", opencode_client::message_id());
        prepare(root, model_port);

        let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/fake-model.py");
        let model = Command::new("python3")
            .arg("-I")
            .arg(&tools)
            .arg(model_port.to_string())
            .arg(root.join("model.jsonl"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the scripted model");

        let server = spawn_server(root, port, &password);
        let url = format!("http://127.0.0.1:{port}");
        let sandbox = Self {
            dir,
            server,
            model,
            url,
            password,
        };
        sandbox.wait_until_up().await;
        sandbox
    }

    async fn wait_until_up(&self) {
        let client = self.client();
        let started = Instant::now();
        while client.info().await.is_err() {
            assert!(started.elapsed() < START_TIMEOUT, "OpenCode did not start");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Stops this private server and starts it again on the same port.
    ///
    /// Clients see their connection drop, as when a server restarts.
    pub async fn restart_server(&mut self) {
        let _ = self.server.kill();
        let _ = self.server.wait();
        let port: u16 = self
            .url
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .expect("the URL ends in a port");
        self.server = spawn_server(self.dir.path(), port, &self.password);
        self.wait_until_up().await;
    }

    /// A new client for this server.
    pub fn client(&self) -> Client {
        Client::new(Endpoint::new(self.url.clone(), Some(self.password.clone())))
    }

    /// The git project sessions work in.
    pub fn project(&self) -> PathBuf {
        self.dir.path().join("project")
    }

    /// The project folder as a string, as the API takes it.
    pub fn project_str(&self) -> String {
        self.project().to_string_lossy().into_owned()
    }

    /// Runs the real `opencode` CLI against this server, as a second client would.
    pub fn cli(&self, args: &[&str]) -> String {
        let mut cmd = Command::new(binary());
        scratch_env(&mut cmd, self.dir.path(), &self.password);
        let out = cmd
            .args(args)
            .args(["--server", &self.url])
            // The CLI takes its folder from `PWD`, not only the working directory.
            .env("PWD", self.project())
            .current_dir(self.project())
            .stdin(Stdio::null())
            .output()
            .expect("run the opencode CLI");
        assert!(
            out.status.success(),
            "opencode {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// The sandbox's scratch folder, which holds the model's log (`model.jsonl`).
    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// How many requests the scripted model has answered.
    pub fn model_requests(&self) -> usize {
        std::fs::read_to_string(self.dir.path().join("model.jsonl"))
            .map_or(0, |s| s.lines().count())
    }
}

/// Writes the scratch OpenCode config and a git project with two committed files.
fn prepare(root: &Path, model_port: u16) {
    let config = serde_json::json!({
        "model": "fake/fake-coder",
        "providers": { "fake": {
            "name": "Fake",
            "package": "@opencode/ai/providers/openai-compatible",
            "settings": { "baseURL": format!("http://127.0.0.1:{model_port}/v1"), "apiKey": "fake-key" },
            "models": { "fake-coder": { "name": "Fake Coder", "limit": { "context": 128_000, "output": 8_192 } } }
        }},
        "permissions": [ { "action": "shell", "resource": "*", "effect": "ask" } ]
    });
    std::fs::write(
        root.join("config/opencode/opencode.json"),
        config.to_string(),
    )
    .expect("config");

    let project = root.join("project");
    std::fs::write(project.join("a.txt"), "alpha\n").expect("a.txt");
    std::fs::write(project.join("b.txt"), "beta\n").expect("b.txt");
    for args in [
        &["init", "-q"][..],
        &["add", "."],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "init",
        ],
    ] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(&project)
            .status()
            .expect("git")
            .success();
        assert!(ok, "git {args:?} failed");
    }
}

fn scratch_env(cmd: &mut Command, root: &Path, password: &str) {
    cmd.env("HOME", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("OPENCODE_PASSWORD", password);
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // Errors are ignored: the processes may already have exited.
        let _ = self.server.kill();
        let _ = self.model.kill();
        let _ = self.server.wait();
        let _ = self.model.wait();
    }
}

/// Polls `check` until it returns `Some`, or panics after [`WAIT`].
pub async fn wait_for<T, F, Fut>(what: &str, mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    let started = Instant::now();
    loop {
        if let Some(value) = check().await {
            return value;
        }
        assert!(started.elapsed() < WAIT, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Waits until the session is no longer working.
pub async fn wait_idle(client: &Client, session_id: &str) {
    wait_for("the session to finish", || async {
        let active = client.active().await.ok()?;
        (!active.iter().any(|id| id == session_id)).then_some(())
    })
    .await;
}
