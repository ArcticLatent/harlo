//! Running an engine's server on this computer, on the loopback address
//! only, and telling whether it answers.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use super::Engine;

/// How to start an engine's server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Serve {
    /// The engine.
    pub engine: Engine,
    /// Its program.
    pub program: PathBuf,
    /// The port it listens on.
    pub port: u16,
    /// For llama.cpp, the model file it serves.
    pub model: Option<PathBuf>,
    /// For llama.cpp, the context it keeps, in tokens.
    pub context: u32,
    /// For Ollama, where its models are kept; its own default otherwise.
    pub models: Option<PathBuf>,
    /// Where its output goes.
    pub log: PathBuf,
}

impl Serve {
    /// Starts the server; it stops when the returned [`Running`] is dropped.
    ///
    /// # Errors
    ///
    /// When the program can't be started, or llama.cpp has no model.
    pub fn start(&self) -> Result<Running, String> {
        let mut command = std::process::Command::new(&self.program);
        match self.engine {
            Engine::Ollama => {
                command
                    .arg("serve")
                    .env("OLLAMA_HOST", format!("127.0.0.1:{}", self.port));
                if let Some(models) = &self.models {
                    command.env("OLLAMA_MODELS", models);
                }
            }
            Engine::LlamaCpp => {
                let model = self
                    .model
                    .as_ref()
                    .ok_or("Choose a model for llama.cpp to serve first.")?;
                command
                    .arg("--model")
                    .arg(model)
                    .args(["--host", "127.0.0.1", "--port"])
                    .arg(self.port.to_string())
                    // Tool calls go through the model's chat template.
                    .arg("--jinja")
                    .args(["--n-gpu-layers", "999", "--ctx-size"])
                    .arg(self.context.max(4096).to_string());
            }
        }
        if let Some(folder) = self.log.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let log = std::fs::File::create(&self.log)
            .map_err(|e| format!("Couldn't write {}: {e}", self.log.display()))?;
        let errors = log
            .try_clone()
            .map_err(|e| format!("Couldn't write {}: {e}", self.log.display()))?;
        // The libraries it was built with sit beside it, or in the other
        // archives unpacked next to its folder (llama.cpp's CUDA runtime).
        command.env("LD_LIBRARY_PATH", library_path(&self.program));
        command
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(errors)
            .spawn()
            .map(|child| Running { child })
            .map_err(|e| format!("Couldn't start {}: {e}", self.engine.name()))
    }
}

/// A server Harlo started, which stops when this is dropped.
#[derive(Debug)]
pub struct Running {
    /// Its process.
    child: std::process::Child,
}

impl Running {
    /// Whether it has stopped by itself, and how it ended.
    pub fn ended(&mut self) -> Option<String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.to_string()),
            Ok(None) => None,
            Err(e) => Some(e.to_string()),
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `program`'s folder and the folders beside it, before whatever library
/// path there was.
fn library_path(program: &Path) -> std::ffi::OsString {
    let mut folders: Vec<PathBuf> = Vec::new();
    if let Some(own) = program.parent() {
        folders.push(own.to_owned());
        if let Some(Ok(entries)) = own.parent().map(std::fs::read_dir) {
            let mut beside: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir() && p != own)
                .collect();
            beside.sort();
            folders.extend(beside);
        }
    }
    let mut path = std::ffi::OsString::new();
    for folder in folders {
        if !path.is_empty() {
            path.push(":");
        }
        path.push(folder);
    }
    if let Some(old) = std::env::var_os("LD_LIBRARY_PATH").filter(|o| !o.is_empty()) {
        path.push(":");
        path.push(old);
    }
    path
}

/// The server's address on this computer.
#[must_use]
pub fn base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Whether `engine`'s server on `port` answers and is ready.
pub async fn answers(engine: Engine, port: u16) -> bool {
    let path = match engine {
        Engine::Ollama => "/api/version",
        // 503 while the model loads.
        Engine::LlamaCpp => "/health",
    };
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    client
        .get(format!("{}{path}", base_url(port)))
        .send()
        .await
        .is_ok_and(|r| r.status().is_success())
}
