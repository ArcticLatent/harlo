//! Local models: engines installed from (fake) GitHub releases into scratch
//! folders, checked and unpacked; Ollama pulls and Hugging Face files from
//! local servers; the models that fit; and the provider entry for the server.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use futures_util::StreamExt;
use harlo_core::config::{self, LocalProvider, OPENAI_COMPATIBLE};
use harlo_core::download::Download;
use harlo_core::local_models::{
    Build, Engine, Engines, Fit, Install, ModelFiles, Suggestion, driver_major, newer,
    ollama_models, ollama_pull, suggest,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// A file the test server gives at a path: its type and bytes.
type Files = HashMap<String, (&'static str, Vec<u8>)>;

/// Serves `files` by path on a local port, 404 otherwise; gives the address.
fn serve(files: Files) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let address = format!("http://{}", listener.local_addr().expect("its address"));
    let files = Arc::new(files);
    std::thread::spawn(move || {
        for mut socket in listener.incoming().map_while(Result::ok) {
            let files = Arc::clone(&files);
            std::thread::spawn(move || {
                let mut request = [0_u8; 8192];
                let n = socket.read(&mut request).unwrap_or(0);
                let text = String::from_utf8_lossy(&request[..n]);
                let path = text.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let (status, kind, body) = match files.get(&path) {
                    Some((kind, body)) => ("200 OK", *kind, body.clone()),
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

/// A tar archive holding `files`, by path, as executables.
fn tar(files: &[(&str, &str)]) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    for (path, text) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(text.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, path, text.as_bytes())
            .expect("an entry");
    }
    tar.into_inner().expect("the archive")
}

/// A `.tar.gz` holding `files`, by path, as executables.
fn tar_gz(files: &[(&str, &str)]) -> Vec<u8> {
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&tar(files)).expect("the gzip");
    gz.finish().expect("the gzip")
}

/// The SHA-256 of `bytes`, in hex.
fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// A GitHub asset entry for `name` at `base`.
fn asset(base: &str, name: &str, bytes: &[u8], digest: Option<String>) -> Value {
    json!({
        "name": name,
        "browser_download_url": format!("{base}/download/{name}"),
        "size": bytes.len(),
        "digest": digest.map(|d| format!("sha256:{d}")),
    })
}

/// The real asset names of a llama.cpp build `tag`.
fn llama_assets(tag: &str) -> Vec<String> {
    [
        "cudart-llama-{t}-bin-ubuntu-cuda-12.8-x64.tar.gz",
        "cudart-llama-{t}-bin-ubuntu-cuda-13.5-x64.tar.gz",
        "llama-{t}-bin-ubuntu-cuda-12.8-x64.tar.gz",
        "llama-{t}-bin-ubuntu-cuda-13.5-x64.tar.gz",
        "llama-{t}-bin-ubuntu-rocm-10.0-x64.tar.gz",
        "llama-{t}-bin-ubuntu-vulkan-x64.tar.gz",
        "llama-{t}-bin-ubuntu-vulkan-arm64.tar.gz",
        "llama-{t}-bin-ubuntu-x64.tar.gz",
        "llama-{t}-bin-win-cpu-x64.zip",
    ]
    .iter()
    .map(|n| n.replace("{t}", tag))
    .collect()
}

#[tokio::test]
async fn the_right_build_is_chosen_for_each_computer() {
    let mut files = Files::new();
    let base_holder = std::sync::OnceLock::<String>::new();
    let _ = base_holder;
    // GitHub's answers name download URLs on the same fake server.
    let base = serve(Files::new());
    let llama: Vec<Value> = llama_assets("b200")
        .iter()
        .map(|n| asset(&base, n, b"x", Some(sha256(b"x"))))
        .collect();
    let builds = json!([
        { "tag_name": "v1.0.0", "assets": [] },
        { "tag_name": "b200", "assets": llama },
    ]);
    files.insert(
        "/repos/ggml-org/llama.cpp/releases?per_page=5".into(),
        ("application/json", builds.to_string().into_bytes()),
    );
    let ollama = json!({ "tag_name": "v0.40.17", "assets": [
        asset(&base, "ollama-linux-amd64.tar.zst", b"x", None),
        asset(&base, "ollama-linux-amd64-rocm.tar.zst", b"x", None),
        asset(&base, "ollama-linux-arm64.tar.zst", b"x", None),
    ]});
    files.insert(
        "/repos/ollama/ollama/releases/latest".into(),
        ("application/json", ollama.to_string().into_bytes()),
    );
    let api = serve(files);
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let engines = Engines::new(scratch.path(), &api);
    let names = |release: harlo_core::local_models::Release| -> Vec<String> {
        release.assets.into_iter().map(|a| a.name).collect()
    };

    let cuda13 = engines
        .latest(Engine::LlamaCpp, Build::Cuda { cuda13: true })
        .await
        .expect("a release");
    assert_eq!(cuda13.version, "b200");
    assert_eq!(
        cuda13.assets[0].sha256.as_deref(),
        Some(sha256(b"x").as_str())
    );
    assert_eq!(
        names(cuda13),
        [
            "llama-b200-bin-ubuntu-cuda-13.5-x64.tar.gz",
            "cudart-llama-b200-bin-ubuntu-cuda-13.5-x64.tar.gz"
        ]
    );
    let cuda12 = engines
        .latest(Engine::LlamaCpp, Build::Cuda { cuda13: false })
        .await
        .expect("a release");
    assert_eq!(
        names(cuda12)[0],
        "llama-b200-bin-ubuntu-cuda-12.8-x64.tar.gz"
    );
    for (build, name) in [
        (Build::Vulkan, "llama-b200-bin-ubuntu-vulkan-x64.tar.gz"),
        (Build::Rocm, "llama-b200-bin-ubuntu-rocm-10.0-x64.tar.gz"),
        (Build::Cpu, "llama-b200-bin-ubuntu-x64.tar.gz"),
    ] {
        let release = engines
            .latest(Engine::LlamaCpp, build)
            .await
            .expect("a release");
        assert_eq!(names(release), [name]);
    }

    let ollama = engines
        .latest(Engine::Ollama, Build::Cuda { cuda13: true })
        .await
        .expect("a release");
    assert_eq!(ollama.version, "0.40.17");
    assert_eq!(names(ollama), ["ollama-linux-amd64.tar.zst"]);
    let rocm = engines
        .latest(Engine::Ollama, Build::Rocm)
        .await
        .expect("a release");
    assert_eq!(
        names(rocm),
        [
            "ollama-linux-amd64.tar.zst",
            "ollama-linux-amd64-rocm.tar.zst"
        ]
    );
}

/// Installs `version` of llama.cpp from an archive served with `digest`.
async fn install(engines_at: &Path, version: &str, digest: Option<String>) -> Vec<Install> {
    let archive = tar_gz(&[(
        &format!("llama-{version}/llama-server"),
        "#!/bin/sh\necho server\n",
    )]);
    let name = format!("llama-{version}-bin-ubuntu-x64.tar.gz");
    let digest = digest.unwrap_or_else(|| sha256(&archive));
    let mut files = Files::new();
    files.insert(
        format!("/download/{name}"),
        ("application/gzip", archive.clone()),
    );
    let base = serve(files);
    let release = harlo_core::local_models::Release {
        version: version.to_owned(),
        assets: vec![harlo_core::local_models::Asset {
            name: name.clone(),
            url: format!("{base}/download/{name}"),
            size: archive.len() as u64,
            sha256: Some(digest),
        }],
    };
    Engines::new(engines_at, "http://unused")
        .install(Engine::LlamaCpp, release)
        .collect()
        .await
}

#[tokio::test]
async fn an_engine_installs_updates_and_refuses_a_damaged_download() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let engines = Engines::new(scratch.path(), "http://unused");
    assert!(engines.own(Engine::LlamaCpp).is_none());

    let steps = install(scratch.path(), "b100", None).await;
    assert!(steps.contains(&Install::Unpacking), "{steps:?}");
    let Some(Install::Done(Ok(installed))) = steps.last() else {
        panic!("installed, not {steps:?}");
    };
    assert_eq!(installed.version, "b100");
    assert!(installed.program.ends_with("b100/llama-b100/llama-server"));
    assert_eq!(engines.own(Engine::LlamaCpp).as_ref(), Some(installed));
    let output = std::process::Command::new(&installed.program)
        .output()
        .expect("the program runs");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "server\n");

    // An update replaces it, and the old version goes.
    let steps = install(scratch.path(), "b120", None).await;
    assert!(
        matches!(steps.last(), Some(Install::Done(Ok(_)))),
        "{steps:?}"
    );
    assert_eq!(
        engines.own(Engine::LlamaCpp).expect("installed").version,
        "b120"
    );
    let home = scratch.path().join("llamacpp");
    assert!(!home.join("b100").exists());
    assert!(!home.join("downloads").exists());

    // A download that doesn't match its checksum isn't installed.
    let steps = install(scratch.path(), "b130", Some("00".repeat(32))).await;
    let Some(Install::Done(Err(why))) = steps.last() else {
        panic!("refused, not {steps:?}");
    };
    assert!(why.contains("damaged"), "{why}");
    assert_eq!(
        engines.own(Engine::LlamaCpp).expect("installed").version,
        "b120"
    );
}

#[tokio::test]
async fn ollama_installs_from_its_zstd_archive() {
    let archive = ruzstd::encoding::compress_to_vec(
        tar(&[
            ("bin/ollama", "#!/bin/sh\necho ollama\n"),
            ("lib/ollama/libggml-base.so", "x"),
        ])
        .as_slice(),
        ruzstd::encoding::CompressionLevel::Fastest,
    );
    let name = "ollama-linux-amd64.tar.zst";
    let mut files = Files::new();
    files.insert(
        format!("/download/{name}"),
        ("application/zstd", archive.clone()),
    );
    let base = serve(files);
    let release = harlo_core::local_models::Release {
        version: "0.40.17".to_owned(),
        assets: vec![harlo_core::local_models::Asset {
            name: name.to_owned(),
            url: format!("{base}/download/{name}"),
            size: archive.len() as u64,
            sha256: Some(sha256(&archive)),
        }],
    };
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let engines = Engines::new(scratch.path(), "http://unused");
    let steps: Vec<Install> = engines.install(Engine::Ollama, release).collect().await;
    let Some(Install::Done(Ok(installed))) = steps.last() else {
        panic!("installed, not {steps:?}");
    };
    assert!(installed.program.ends_with("ollama/0.40.17/bin/ollama"));
    assert!(
        installed
            .program
            .with_file_name("../lib/ollama/libggml-base.so")
            .exists()
    );
    assert_eq!(engines.own(Engine::Ollama).as_ref(), Some(installed));
}

#[test]
fn versions_compare_by_their_numbers() {
    assert!(newer("b12648", "b12600"));
    assert!(newer("0.40.17", "0.40.9"));
    assert!(!newer("0.40.9", "0.40.17"));
    assert!(!newer("b100", "b100"));
    assert_eq!(
        driver_major(
            "NVRM version: NVIDIA UNIX Open Kernel Module for x86_64  615.78.08  Release Build"
        ),
        Some(615)
    );
    assert_eq!(driver_major("nothing"), None);
}

/// A suggestion for `repo` at `quant`.
fn suggestion(repo: &str, quant: &str) -> Suggestion {
    Suggestion {
        title: "Tiny-Coder".to_owned(),
        repo: repo.to_owned(),
        quant: quant.to_owned(),
        memory_gb: 1.0,
        context: 8192,
        fit: Fit::Well,
    }
}

#[tokio::test]
async fn a_gguf_file_is_found_and_downloaded_with_its_checksum() {
    let model = vec![9_u8; 50_000];
    let tree = json!([
        { "type": "file", "path": "README.md", "size": 10 },
        { "type": "file", "path": "Tiny-Coder-Q4_K_M.gguf", "size": 1 },
        { "type": "file", "path": "Tiny-Coder-UD-Q6_K_XL.gguf", "size": 1 },
        { "type": "file", "path": "Q8_0/Tiny-Coder-Q8_0-00001-of-00002.gguf", "size": 1 },
        { "type": "file", "path": "Tiny-Coder-Q6_K.gguf", "size": model.len(),
          "lfs": { "oid": sha256(&model) } },
    ]);
    let mut files = Files::new();
    files.insert(
        "/api/models/acme/Tiny-Coder-GGUF/tree/main".into(),
        ("application/json", tree.to_string().into_bytes()),
    );
    files.insert(
        "/acme/Tiny-Coder-GGUF/resolve/main/Tiny-Coder-Q6_K.gguf".into(),
        ("application/octet-stream", model),
    );
    let hub = serve(files);
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let store = ModelFiles::new(scratch.path(), &hub);
    let wanted = suggestion("acme/Tiny-Coder-GGUF", "Q6_K");
    assert!(store.downloaded(&wanted).is_none());

    let fetch = store.find(&wanted).await.expect("the file");
    assert!(fetch.url.ends_with("/resolve/main/Tiny-Coder-Q6_K.gguf"));
    let steps: Vec<Download> = fetch.download().collect().await;
    assert_eq!(steps.last(), Some(&Download::Done(Ok(()))));
    let path = store.downloaded(&wanted).expect("downloaded");
    assert!(path.ends_with("acme--Tiny-Coder-GGUF/Tiny-Coder-Q6_K.gguf"));

    // Split models aren't offered.
    let why = store
        .find(&suggestion("acme/Tiny-Coder-GGUF", "Q8_0"))
        .await
        .expect_err("no single file");
    assert!(why.contains("no single Q8_0 file"), "{why}");
}

#[tokio::test]
async fn ollama_pulls_with_progress_and_reports_failures() {
    let lines = [
        json!({ "status": "pulling manifest" }),
        json!({ "status": "pulling abc", "total": 100, "completed": 40 }),
        json!({ "status": "pulling abc", "total": 100, "completed": 100 }),
        json!({ "status": "success" }),
    ]
    .iter()
    .fold(String::new(), |mut all, line| {
        let _ = writeln!(all, "{line}");
        all
    });
    let mut files = Files::new();
    files.insert(
        "/api/pull".into(),
        ("application/x-ndjson", lines.into_bytes()),
    );
    files.insert(
        "/api/tags".into(),
        (
            "application/json",
            json!({ "models": [{ "name": "hf.co/acme/Tiny:Q6_K" }] })
                .to_string()
                .into_bytes(),
        ),
    );
    let ollama = serve(files);
    let steps: Vec<Download> = ollama_pull(&ollama, "hf.co/acme/Tiny:Q6_K").collect().await;
    assert_eq!(
        steps,
        [
            Download::Progress(0.4),
            Download::Progress(1.0),
            Download::Done(Ok(()))
        ]
    );
    assert_eq!(
        ollama_models(&ollama).await.expect("the models"),
        ["hf.co/acme/Tiny:Q6_K"]
    );

    let mut files = Files::new();
    files.insert(
        "/api/pull".into(),
        (
            "application/x-ndjson",
            format!(
                "{}\n",
                json!({ "error": "pull model manifest: file does not exist" })
            )
            .into_bytes(),
        ),
    );
    let failing = serve(files);
    let steps: Vec<Download> = ollama_pull(&failing, "nope").collect().await;
    assert!(
        matches!(steps.last(), Some(Download::Done(Err(why))) if why.contains("does not exist")),
        "{steps:?}"
    );
}

#[test]
fn suggestions_fit_the_memory_there_is() {
    let specs = llmfit_core::hardware::SystemSpecs::detect();
    let big = suggest(
        &specs
            .clone()
            .with_gpu_memory_override(32.0)
            .with_ram_override(96.0),
    );
    let small = suggest(&specs.with_gpu_memory_override(6.0).with_ram_override(8.0));
    assert!(!big.is_empty());
    assert!(big.len() > small.len(), "{} vs {}", big.len(), small.len());
    assert_eq!(big[0].title, "Qwen3.6-35B-A3B", "the most capable first");
    for s in big.iter().chain(&small) {
        assert!(s.repo.contains('/') && !s.quant.is_empty(), "{s:?}");
        assert!(s.ollama_tag().starts_with("hf.co/"));
    }
}

#[test]
fn a_local_provider_is_written_to_harlos_config() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    std::fs::write(
        scratch.path().join("harlo.jsonc"),
        "{\n  // mine\n  \"model\": \"x/y\"\n}\n",
    )
    .expect("a config");
    assert!(!config::has_provider(scratch.path(), "llamacpp").expect("read"));
    let provider = LocalProvider {
        id: "llamacpp".to_owned(),
        name: "llama.cpp on this computer".to_owned(),
        base_url: "http://127.0.0.1:8080/v1".to_owned(),
        models: vec![("tiny-coder".to_owned(), "Tiny-Coder".to_owned(), 8192)],
    };
    config::set_provider(scratch.path(), &provider).expect("written");
    assert!(config::has_provider(scratch.path(), "llamacpp").expect("read"));
    let text = std::fs::read_to_string(scratch.path().join("harlo.jsonc")).expect("the file");
    assert!(text.contains("// mine"), "comments stay: {text}");
    let plain: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect();
    let value: Value = serde_json::from_str(&plain).expect("valid JSON");
    let entry = &value["providers"]["llamacpp"];
    assert_eq!(entry["package"], OPENAI_COMPATIBLE);
    assert_eq!(entry["settings"]["baseURL"], "http://127.0.0.1:8080/v1");
    assert_eq!(entry["models"]["tiny-coder"]["limit"]["context"], 8192);
    config::remove_provider(scratch.path(), "llamacpp").expect("removed");
    assert!(!config::has_provider(scratch.path(), "llamacpp").expect("read"));
}
