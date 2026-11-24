//! The real thing, by hand: installs this computer's llama.cpp build from
//! GitHub, downloads a small model from Hugging Face, serves it, adds it to
//! a private server (scratch `HOME`/`XDG_*`) as a provider, and
//! has the server answer with it.
//!
//! `HARLO_TEST_LOCAL_MODELS=<scratch folder, kept between runs>
//! cargo test -p harlo-core --test local_models_real -- --ignored --nocapture`

// Rust guideline compliant 2026-10-09

#[path = "../../harlo-client/tests/support/mod.rs"]
mod support;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use harlo_client::ModelRef;
use harlo_core::config::{self, LocalProvider};
use harlo_core::download::Download;
use harlo_core::local_models::{
    Computer, Engine, Engines, Fit, Install, ModelFiles, Serve, Suggestion, answers, base_url,
};
use support::{Sandbox, wait_for};

/// A free port on the loopback address.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("a port")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads llama.cpp and a model; HARLO_TEST_LOCAL_MODELS names a scratch folder"]
async fn a_real_llama_server_answers_through_opencode() {
    let scratch =
        PathBuf::from(std::env::var("HARLO_TEST_LOCAL_MODELS").expect("HARLO_TEST_LOCAL_MODELS"));
    let computer = tokio::task::spawn_blocking(Computer::detect)
        .await
        .expect("the computer");
    println!("{} · build {:?}", computer.gpu_words(), computer.build);

    let engines = Engines::new(scratch.join("engines"), "https://api.github.com");
    let installed = if let Some(installed) = engines.own(Engine::LlamaCpp) {
        installed
    } else {
        let release = engines
            .latest(Engine::LlamaCpp, computer.build)
            .await
            .expect("a release");
        println!("installing {} {:?}", release.version, release.assets);
        let steps: Vec<Install> = engines.install(Engine::LlamaCpp, release).collect().await;
        match steps.last() {
            Some(Install::Done(Ok(installed))) => installed.clone(),
            other => panic!("not installed: {other:?}"),
        }
    };
    println!(
        "llama-server {} at {}",
        installed.version,
        installed.program.display()
    );

    let small = Suggestion {
        // Small and without a thinking phase, so it answers at once.
        title: "Qwen2.5-Coder-0.5B-Instruct".to_owned(),
        repo: "Qwen/Qwen2.5-Coder-0.5B-Instruct-GGUF".to_owned(),
        quant: "Q4_K_M".to_owned(),
        memory_gb: 1.0,
        context: 8192,
        fit: Fit::Well,
    };
    let files = ModelFiles::new(scratch.join("models"), "https://huggingface.co");
    let model = if let Some(model) = files.downloaded(&small) {
        model
    } else {
        let fetch = files.find(&small).await.expect("the file");
        let steps: Vec<Download> = fetch.download().collect().await;
        assert_eq!(steps.last(), Some(&Download::Done(Ok(()))));
        files.downloaded(&small).expect("downloaded")
    };

    let port = free_port();
    let _server = Serve {
        engine: Engine::LlamaCpp,
        program: installed.program.clone(),
        port,
        model: Some(model),
        context: small.context,
        models: None,
        log: scratch.join("llama-server.log"),
    }
    .start()
    .expect("started");
    let started = Instant::now();
    while !answers(Engine::LlamaCpp, port).await {
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "llama-server never got ready: {}",
            std::fs::read_to_string(scratch.join("llama-server.log")).unwrap_or_default()
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    println!("ready in {:?}", started.elapsed());

    let sandbox = Sandbox::start().await;
    let provider = LocalProvider {
        id: "llamacpp".to_owned(),
        name: "llama.cpp on this computer".to_owned(),
        base_url: format!("{}/v1", base_url(port)),
        models: vec![(small.id(), small.title.clone(), small.context)],
    };
    config::set_provider(&sandbox.root().join("config/opencode"), &provider).expect("added");
    let client = sandbox.client();
    let project = sandbox.project_str();
    wait_for("OpenCode lists the model", || {
        let client = client.clone();
        let project = project.clone();
        async move {
            let models = client.models(&project).await.ok()?;
            models
                .iter()
                .any(|m| m.provider_id == "llamacpp")
                .then_some(())
        }
    })
    .await;
    let text = client
        .generate_text(
            "Answer with one word: what colour is the sky on a clear day?",
            Some(&ModelRef {
                provider_id: "llamacpp".to_owned(),
                id: small.id(),
                variant: None,
            }),
        )
        .await
        .expect("an answer");
    println!("answer: {text:?}");
    assert!(!text.trim().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Ollama's port, 11434, free on this computer"]
async fn opencode_finds_a_server_on_ollamas_port_by_itself() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:11434").expect("Ollama's port is free");
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let log = std::sync::Arc::clone(&asked);
    std::thread::spawn(move || {
        for mut socket in listener.incoming().map_while(Result::ok) {
            let mut request = [0_u8; 8192];
            let n = socket.read(&mut request).unwrap_or(0);
            let text = String::from_utf8_lossy(&request[..n]).to_string();
            let line = text.lines().next().unwrap_or_default().to_owned();
            log.lock().expect("the log").push(line.clone());
            let body = if line.contains("/api/tags") {
                r#"{"models":[{"name":"tiny-coder:latest","model":"tiny-coder:latest","modified_at":"2026-11-01T00:00:00Z","size":400000000,"digest":"abc123","details":{"format":"gguf","family":"qwen2","families":["qwen2"],"parameter_size":"0.5B","quantization_level":"Q4_K_M"}}]}"#
            } else if line.contains("/api/show") {
                r#"{"capabilities":["completion","tools"],"details":{"format":"gguf","family":"qwen2","parameter_size":"0.5B","quantization_level":"Q4_K_M"},"model_info":{"general.architecture":"qwen2","qwen2.context_length":32768}}"#
            } else if line.contains("/v1/models") {
                r#"{"object":"list","data":[{"id":"tiny-coder:latest","object":"model","owned_by":"library"}]}"#
            } else if line.contains("/api/version") {
                r#"{"version":"0.40.0"}"#
            } else {
                "{}"
            };
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = socket.write_all(head.as_bytes());
            let _ = socket.write_all(body.as_bytes());
        }
    });
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project_str();
    let started = Instant::now();
    let mut found = Vec::new();
    while started.elapsed() < Duration::from_secs(20) {
        found = client
            .models(&project)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|m| format!("{}/{}", m.provider_id, m.id))
            .filter(|m| m.contains("tiny-coder"))
            .collect();
        if !found.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    println!("asked: {:?}", asked.lock().expect("the log"));
    println!("models: {found:?}");
    assert!(found.iter().any(|m| m.contains("tiny-coder")), "{found:?}");
}
