//! Settings → Local models: llama.cpp installs from a (fake) GitHub
//! release, a model downloads from a (fake) Hugging Face, the server Harlo
//! starts answers and is added to the server's config; Ollama's server,
//! already running, pulls a model. Everything is local: scratch folders,
//! loopback servers, and a stand-in `llama-server`.

// Rust guideline compliant 2026-10-09

mod support;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use harlo::app::{App, LocalEvent, Message, Section, ServerState, SettingChoice, SheetEvent};
use harlo_core::local_models::{Computer, Engine, Engines, Fit, ModelFiles, Suggestion};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// How long a step may take.
const WAIT: Duration = Duration::from_secs(30);

/// Answers a request for a path with a type and a body; `None` is a 404.
type Answer = dyn Fn(&str) -> Option<(&'static str, Vec<u8>)> + Send + Sync;

/// Serves `answer` on a local port; gives the address.
fn serve(answer: Arc<Answer>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let address = format!("http://{}", listener.local_addr().expect("its address"));
    std::thread::spawn(move || {
        for mut socket in listener.incoming().map_while(Result::ok) {
            let answer = Arc::clone(&answer);
            std::thread::spawn(move || {
                let mut request = [0_u8; 8192];
                let n = socket.read(&mut request).unwrap_or(0);
                let text = String::from_utf8_lossy(&request[..n]);
                let path = text.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let (status, kind, body) = match answer(&path) {
                    Some((kind, body)) => ("200 OK", kind, body),
                    None => ("404 Not Found", "text/plain", Vec::new()),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(head.as_bytes());
                let _ = socket.write_all(&body);
            });
        }
    });
    address
}

/// Serves fixed `files` by path.
fn serve_files(files: HashMap<String, (&'static str, Vec<u8>)>) -> String {
    serve(Arc::new(move |path| files.get(path).cloned()))
}

/// A free port on the loopback address.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("a port")
}

/// A stand-in `llama-server`: answers every request on its `--port`.
const LLAMA_SERVER: &str = r#"#!/bin/sh
port=""
while [ $# -gt 0 ]; do [ "$1" = --port ] && port="$2"; shift; done
exec python3 -c '
import http.server, sys
class Answer(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200); self.end_headers(); self.wfile.write(b"{}")
    def log_message(self, *args): pass
http.server.HTTPServer(("127.0.0.1", int(sys.argv[1])), Answer).serve_forever()
' "$port"
"#;

/// A `.tar.gz` holding the stand-in server as `llama-b100/llama-server`.
fn llama_archive() -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(LLAMA_SERVER.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    tar.append_data(
        &mut header,
        "llama-b100/llama-server",
        LLAMA_SERVER.as_bytes(),
    )
    .expect("an entry");
    let tar = tar.into_inner().expect("the archive");
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&tar).expect("the gzip");
    gz.finish().expect("the gzip")
}

/// The one model the tests offer.
fn tiny() -> Suggestion {
    Suggestion {
        title: "Tiny-Coder".to_owned(),
        repo: "acme/Tiny-Coder-GGUF".to_owned(),
        quant: "Q6_K".to_owned(),
        memory_gb: 1.2,
        context: 8192,
        fit: Fit::Well,
    }
}

/// The app with engines and models in `scratch`, from fake GitHub and
/// Hugging Face servers, and OpenCode's config in `scratch/opencode`.
fn app(scratch: &Path, github: &str, hub: &str, ports: [u16; 2]) -> App {
    let mut app = demo::app();
    app.opencode_folder = Some(scratch.join("opencode"));
    app.local.engines = Some(Engines::new(scratch.join("engines"), github));
    app.local.files = Some(ModelFiles::new(scratch.join("models"), hub));
    app.local.ports = ports;
    let specs = llmfit_core::hardware::SystemSpecs::detect()
        .with_gpu_memory_override(8.0)
        .with_ram_override(16.0);
    let mut computer = Computer::from_specs(specs, None);
    computer.build = harlo_core::local_models::Build::Cpu;
    app.local.computer = Some(computer);
    app.local.suggestions = vec![tiny()];
    app
}

/// A fake GitHub with llama.cpp build b100 for CPUs, served from `base`
/// (the files) and listed at the returned address.
fn github(archive: &[u8]) -> String {
    let name = "llama-b100-bin-ubuntu-x64.tar.gz";
    let mut files = HashMap::new();
    files.insert(
        format!("/download/{name}"),
        ("application/gzip", archive.to_vec()),
    );
    let downloads = serve_files(files);
    let builds = json!([{ "tag_name": "b100", "assets": [{
        "name": name,
        "browser_download_url": format!("{downloads}/download/{name}"),
        "size": archive.len(),
        "digest": format!("sha256:{}", hex::encode(Sha256::digest(archive))),
    }]}]);
    let mut files = HashMap::new();
    files.insert(
        "/repos/ggml-org/llama.cpp/releases?per_page=5".to_owned(),
        ("application/json", builds.to_string().into_bytes()),
    );
    serve_files(files)
}

/// A fake Hugging Face with Tiny-Coder's `Q6_K` file.
fn hub() -> String {
    let model = vec![7_u8; 40_000];
    let tree = json!([{ "type": "file", "path": "Tiny-Coder-Q6_K.gguf", "size": model.len(),
                        "lfs": { "oid": hex::encode(Sha256::digest(&model)) } }]);
    let mut files = HashMap::new();
    files.insert(
        "/api/models/acme/Tiny-Coder-GGUF/tree/main".to_owned(),
        ("application/json", tree.to_string().into_bytes()),
    );
    files.insert(
        "/acme/Tiny-Coder-GGUF/resolve/main/Tiny-Coder-Q6_K.gguf".to_owned(),
        ("application/octet-stream", model),
    );
    serve_files(files)
}

/// Opens Settings → Local models.
fn open(ui: &Ui) {
    ui.send(Message::Sheet(SheetEvent::Open(Section::LocalModels)));
}

#[test]
fn llama_cpp_installs_serves_a_model_and_joins_opencode() {
    let mut tasks = Tasks::new();
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let llama_port = free_port();
    let ui = ui(app(
        scratch.path(),
        &github(&llama_archive()),
        &hub(),
        [free_port(), llama_port],
    ));
    open(&ui);
    assert!(ui.shows("This computer"));
    assert!(
        tasks.until(&ui, WAIT, |ui| !ui.find("Install 1 MB").is_empty()),
        "llama.cpp offered: {:?}",
        ui.app().local.state(Engine::LlamaCpp).latest
    );
    // Ollama's fake GitHub has no release: it says so instead.
    assert!(
        tasks.until(&ui, WAIT, |ui| matches!(
            ui.app().local.state(Engine::Ollama).latest,
            Some(Err(_))
        )),
        "Ollama's release missing"
    );
    assert!(ui.shows("No release"));

    ui.press("Install 1 MB");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("b100")),
        "installed: {:?}",
        ui.app().local.state(Engine::LlamaCpp).error
    );
    assert!(ui.shows("Stopped; get a model below and press Serve"));

    ui.send(Message::Sheet(SheetEvent::Choose(
        SettingChoice::LocalEngine,
        1,
    )));
    assert_eq!(ui.app().preferences.local_engine, "llamacpp");
    ui.press("Get");
    assert!(
        tasks.until(&ui, WAIT, |ui| !ui.find("Serve").is_empty()),
        "downloaded: {:?}",
        ui.app().local.get_error
    );
    assert!(
        scratch
            .path()
            .join("models/acme--Tiny-Coder-GGUF/Tiny-Coder-Q6_K.gguf")
            .exists()
    );

    ui.press("Serve");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui
            .shows(&format!("Running at 127.0.0.1:{llama_port}"))),
        "served: {:?}",
        ui.app().local.state(Engine::LlamaCpp).server
    );
    assert!(ui.shows("Tiny-Coder"));
    assert_eq!(ui.app().preferences.local_servers, ["llamacpp"]);

    ui.press("Add to the model menu");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Added")),
        "{:?}",
        ui.app().local.error
    );
    assert!(ui.shows("Tiny-Coder is in the model menu."));
    let file = harlo_core::config::file(&scratch.path().join("opencode"));
    let text = std::fs::read_to_string(file).expect("OpenCode's config");
    let config: Value = serde_json::from_str(&text).expect("JSON");
    let provider = &config["providers"]["llamacpp"];
    assert_eq!(
        provider["settings"]["baseURL"],
        format!("http://127.0.0.1:{llama_port}/v1")
    );
    assert_eq!(provider["models"]["tiny-coder"]["name"], "Tiny-Coder");

    ui.press("Stop");
    assert_eq!(
        ui.app().local.state(Engine::LlamaCpp).server,
        ServerState::Stopped
    );
    assert!(ui.app().preferences.local_servers.is_empty());
    // The stand-in stopped with it.
    assert!(
        tasks.until(&ui, WAIT, |_| std::net::TcpStream::connect((
            "127.0.0.1",
            llama_port
        ))
        .is_err()),
        "the server stopped"
    );
}

#[test]
fn a_running_ollama_pulls_a_model() {
    let mut tasks = Tasks::new();
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let pulled = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&pulled);
    let ollama = serve(Arc::new(move |path| match path {
        "/api/version" => Some(("application/json", br#"{"version":"0.40.0"}"#.to_vec())),
        "/api/tags" => {
            let models = if seen.load(Ordering::SeqCst) {
                json!({ "models": [{ "name": "hf.co/acme/Tiny-Coder-GGUF:Q6_K" }] })
            } else {
                json!({ "models": [] })
            };
            Some(("application/json", models.to_string().into_bytes()))
        }
        "/api/pull" => {
            seen.store(true, Ordering::SeqCst);
            let lines = [
                json!({ "status": "pulling", "total": 10, "completed": 5 }),
                json!({ "status": "success" }),
            ]
            .iter()
            .fold(String::new(), |mut all, line| {
                let _ = writeln!(all, "{line}");
                all
            });
            Some(("application/x-ndjson", lines.into_bytes()))
        }
        _ => None,
    }));
    let port: u16 = ollama
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .expect("its port");
    let ui = ui(app(
        scratch.path(),
        "http://127.0.0.1:9",
        "http://127.0.0.1:9",
        [port, free_port()],
    ));
    open(&ui);
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows(&format!(
            "Running at 127.0.0.1:{port}, started outside Harlo"
        ))),
        "Ollama found running: {:?}",
        ui.app().local.state(Engine::Ollama).server
    );
    assert!(ui.shows("Not installed"), "Harlo didn't install it");

    ui.press("Get");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui
            .shows("Q6_K · 1.2 GB · fits well · Ready")),
        "pulled: {:?}",
        ui.app().local.get_error
    );
    assert!(pulled.load(Ordering::SeqCst));
}

#[test]
fn another_computers_server_has_no_local_models() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let mut app = app(
        scratch.path(),
        "http://127.0.0.1:9",
        "http://127.0.0.1:9",
        [1, 2],
    );
    app.location = harlo_core::server::Location::Server {
        url: "http://10.0.0.2:4096".to_owned(),
        password: None,
    };
    let ui = ui(app);
    ui.send(Message::Local(LocalEvent::Look));
    open(&ui);
    assert!(ui.shows(
        "Harlo is following a server on another computer, which can't reach models run \
         here. Follow this computer's server to use them."
    ));
    assert!(ui.find("Get").is_empty());
}
