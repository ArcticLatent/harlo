//! `ServerSetup` (QML singleton): setting up a model server from harlo
//! (Settings → Model servers → Set up a model server): this computer's
//! hardware, the engines that suit it (`harlo_core::engines`), and one
//! setup at a time in the background: installing the engine (or using the
//! one installed), getting the model, starting the server, a test message.
//! The server's start and stop commands are saved, so Start and Stop on
//! the Model servers page work afterwards; adding it to the agents is the
//! page's (Accounts.addServer).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// `{ state: "loading" | "ready", hardware: { summary, … },
        /// engines: [{ id, name, tagline, fit: "ok" | "warn" | "no", why,
        /// installed: { program, version, harlo } | null }] }`.
        #[qproperty(QString, catalog)]
        /// Models on Hugging Face to pick from: `{ state: "finding" |
        /// "ready" | "failed", search, models: [{ repo, name, params,
        /// context, downloads, q4Size, fit: "gpu" | "ram" | "no" }], error }`.
        #[qproperty(QString, hf_models, cxx_name = "hfModels")]
        /// A repository's GGUF files: `{ state: "listing" | "ready" |
        /// "failed", repo, files: [{ file, size, quant, split, fit }], pick
        /// (the file to start with), error }`.
        #[qproperty(QString, ggufs)]
        /// The setup going on (or the last): `{ state: "running" | "done" |
        /// "failed" | "cancelled", step, progress (0–1, or -1), detail,
        /// result: { baseUrl, name, models: [{ id, context }], answer,
        /// speed } }`; "" before one.
        #[qproperty(QString, job)]
        type ServerSetup = super::ServerSetupRust;
    }

    impl cxx_qt::Threading for ServerSetup {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// Looks at this computer and the engines again (`catalog` follows).
        #[qinvokable]
        fn refresh(self: Pin<&mut ServerSetup>);
        /// Finds GGUF models on Hugging Face: the most downloaded, or those
        /// matching `search` (`hfModels` follows).
        #[qinvokable]
        fn find_models(self: Pin<&mut ServerSetup>, search: &QString);
        /// Lists a Hugging Face repository's GGUF files (`ggufs` follows).
        #[qinvokable]
        fn list_ggufs(self: Pin<&mut ServerSetup>, repo: &QString);
        /// A free port on this computer, from `from` up.
        #[qinvokable]
        fn free_port(self: &ServerSetup, from: i32) -> i32;
        /// Starts a setup: JSON `{ engine, useInstalled, model (an Ollama
        /// tag like hf.co/<repo>:<quant>, a GGUF file, or a Strata size), repo (llama.cpp), size
        /// (bytes, llama.cpp), port, context (tokens; 0: as much as fits),
        /// name }` (`job` follows).
        #[qinvokable]
        fn start(self: Pin<&mut ServerSetup>, request: &QString);
        /// Stops the setup going on (a server it started too).
        #[qinvokable]
        fn cancel(self: Pin<&mut ServerSetup>);
    }
}

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::engines::{self, Engine};
use serde_json::{Value, json};

use qobject::ServerSetup;

#[derive(Default)]
pub struct ServerSetupRust {
    catalog: QString,
    hf_models: QString,
    ggufs: QString,
    job: QString,
    stop: Arc<AtomicBool>,
}

/// How a setup tells the dialog where it is.
#[derive(Clone)]
struct Report {
    thread: CxxQtThread<ServerSetup>,
    state: Arc<std::sync::Mutex<Value>>,
}

impl Report {
    fn update(&self, f: impl FnOnce(&mut Value)) {
        let text = {
            let mut v = self.state.lock().unwrap();
            f(&mut v);
            v.to_string()
        };
        let _ = self.thread.queue(move |mut this| this.as_mut().set_job(QString::from(&text)));
    }

    fn step(&self, step: &str) {
        let step = step.to_owned();
        self.update(|v| {
            v["step"] = json!(step);
            v["progress"] = json!(-1);
            v["detail"] = json!("");
        });
    }

    fn progress(&self, done: u64, total: u64) {
        let p = if total > 0 { done as f64 / total as f64 } else { -1.0 };
        let detail = if total > 0 { format!("{:.1} of {:.1} GB", done as f64 / 1e9, total as f64 / 1e9) } else { format!("{:.1} GB", done as f64 / 1e9) };
        self.update(|v| {
            v["progress"] = json!(p);
            v["detail"] = json!(detail);
        });
    }

    fn detail(&self, line: &str) {
        let line: String = line.chars().take(160).collect();
        self.update(|v| v["detail"] = json!(line));
    }
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The folder holding each `.so` under `dir` (a llama.cpp build and its
/// CUDA runtime), for `LD_LIBRARY_PATH`.
fn library_dirs(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() && depth > 0 {
            library_dirs(&p, out, depth - 1);
        } else if e.file_name().to_string_lossy().contains(".so") && !out.contains(&dir.to_path_buf()) {
            out.push(dir.to_path_buf());
        }
    }
}

/// The models a llama.cpp server serves now: its preset's, or (one set up
/// before presets) the one its command loads, with its name and context.
fn llamacpp_models(base: &str, preset: &Path) -> Vec<engines::PresetModel> {
    let start = crate::accounts::start_command(base);
    if start.contains("--models-preset") {
        return engines::read_preset(preset);
    }
    let after = |flag: &str| start.split(&format!(" {flag} ")).nth(1).map(crate::accounts::shell_word).filter(|v| !v.is_empty());
    let Some(path) = after("-m") else { return Vec::new() };
    let name = after("--alias").unwrap_or_else(|| Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
    vec![engines::PresetModel { name, path: PathBuf::from(path), context: after("-c").and_then(|c| c.parse().ok()) }]
}

/// Installs an engine from its release (blocking): its program.
fn install(engine: Engine, hw: &engines::Hardware, report: &Report, stop: &AtomicBool) -> Result<PathBuf, String> {
    report.step(&format!("Finding {}'s latest release", engine.name()));
    let (tag, assets) = engines::plan(engine, hw)?;
    let dir = engines::engines_dir().join(engine.id()).join(&tag);
    for (i, a) in assets.iter().enumerate() {
        report.step(&format!("Downloading {} {tag} ({}/{})", engine.name(), i + 1, assets.len()));
        let file = dir.join(&a.name);
        let mut checking = false;
        let mut progress = |phase: &str, d: u64, t: u64| {
            if phase == "check" && !checking {
                checking = true;
                report.step(&format!("Checking {} against its published checksum", a.name));
            }
            report.progress(d, t);
        };
        engines::download(&a.url, &file, Some(&a.sha256), &mut progress, &|| stop.load(Ordering::Relaxed))?;
        if engine != Engine::Strata {
            report.step(&format!("Unpacking {}", a.name));
            engines::unpack(&file, &dir)?;
            let _ = std::fs::remove_file(&file);
        }
    }
    match engine {
        Engine::Strata => {
            // Its own installer (checked above): ~/.local/share/strata, ~/.local/bin/strata.
            report.step("Running Strata's installer");
            let out = std::process::Command::new("sh").arg(dir.join("install.sh")).stdin(std::process::Stdio::null()).output().map_err(|e| e.to_string())?;
            let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            if !out.status.success() {
                return Err(said.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("Strata's installer failed.").trim().to_owned());
            }
            engines::installed(Engine::Strata).map(|i| i.program).ok_or_else(|| "Strata's installer finished, but there's no strata program.".into())
        }
        _ => engines::installed(engine).filter(|i| i.harlo).map(|i| i.program).ok_or_else(|| format!("{} unpacked, but harlo can't find its program.", engine.name())),
    }
}

/// One setup (blocking): the server's address, name, models and test.
fn setup(req: &Value, report: &Report, stop: &AtomicBool) -> Result<Value, String> {
    let engine = Engine::parse(req["engine"].as_str().unwrap_or_default()).ok_or("Pick an engine.")?;
    let model = req["model"].as_str().unwrap_or_default().trim().to_owned();
    if model.is_empty() {
        return Err("Pick a model.".into());
    }
    let port = req["port"].as_u64().filter(|p| *p > 0 && *p < 65536).unwrap_or(8080) as u16;
    let base = format!("http://127.0.0.1:{port}/v1");
    let name = req["name"].as_str().map(str::trim).filter(|n| !n.is_empty()).unwrap_or(engine.id()).to_owned();
    let cancelled = || stop.load(Ordering::Relaxed);
    // A model for a server set up before (Ollama's "Add a model…"): it runs
    // as it is (its command, port and name), started if it's stopped.
    let adding = req["addModel"] == true;

    report.step("Checking this computer");
    let hw = engines::detect();
    if engine.fit(&hw).0 == "no" {
        return Err(engine.fit(&hw).1);
    }
    if !adding && engines::free_port(port) != port {
        return Err(format!("Port {port} is in use: pick another."));
    }
    let program = match engines::installed(engine).filter(|_| req["useInstalled"] == true) {
        Some(i) => i.program,
        None => install(engine, &hw, report, stop)?,
    };
    let prog = quote(&program.to_string_lossy());

    // The start (and stop) command, as Start on the page runs it.
    let (start, stop_cmd, model_id) = match engine {
        // A context picked: Ollama's for every model (else its own choice
        // by graphics memory).
        Engine::Ollama => {
            let context = req["context"].as_u64().filter(|c| *c >= 2048).map(|c| format!("OLLAMA_CONTEXT_LENGTH={c} ")).unwrap_or_default();
            (format!("OLLAMA_HOST=127.0.0.1:{port} {context}exec {prog} serve"), String::new(), model.clone())
        }
        Engine::LlamaCpp => {
            let repo = req["repo"].as_str().unwrap_or_default().trim().to_owned();
            let folder = engines::engines_dir().parent().unwrap_or(Path::new(".")).join("models").join(repo.replace('/', "__"));
            let need = req["size"].as_u64().unwrap_or(0) / (1024 * 1024);
            if need > hw.disk_free_mb {
                return Err(format!("It's {} GB and only {} GB are free.", need / 1024, hw.disk_free_mb / 1024));
            }
            let parts = engines::gguf_parts(&model);
            for (i, part) in parts.iter().enumerate() {
                let of = if parts.len() > 1 { format!(" ({}/{})", i + 1, parts.len()) } else { String::new() };
                report.step(&format!("Downloading the model{of}"));
                let dest = folder.join(part);
                if !dest.exists() {
                    // (8 connections at once; checked against Hugging Face's SHA-256.)
                    let mut checking = false;
                    let mut progress = |phase: &str, d: u64, t: u64| {
                        if phase == "check" && !checking {
                            checking = true;
                            report.step(&format!("Checking the model{of} against Hugging Face's checksum"));
                        }
                        report.progress(d, t);
                    };
                    engines::download_parallel(&engines::hf_url(&repo, part), &dest, None, 8, &mut progress, &cancelled)?;
                }
            }
            let alias = Path::new(&model).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(model.clone());
            let alias = alias.split("-0000").next().unwrap_or(&alias).to_owned();
            let mut libs = Vec::new();
            library_dirs(program.parent().and_then(Path::parent).unwrap_or(Path::new("/")), &mut libs, 4);
            let libs = libs.iter().map(|d| d.to_string_lossy().into_owned()).collect::<Vec<_>>().join(":");
            // Its models in a preset (llama.cpp's router mode: one server,
            // several models, one loaded at a time, switched as asked for);
            // adding one: those it serves, and this. A context per model
            // (none: llama.cpp takes the model's own and lowers it to fit,
            // not below 16k).
            let preset = engines::preset_path(port);
            let mut models = if adding { llamacpp_models(&base, &preset) } else { Vec::new() };
            models.retain(|m| m.name != alias);
            models.push(engines::PresetModel { name: alias.clone(), path: folder.join(&model), context: req["context"].as_u64().filter(|c| *c >= 2048) });
            engines::write_preset(&preset, &models).map_err(|e| format!("Couldn't write {}: {e}", preset.display()))?;
            let start = format!(
                "LD_LIBRARY_PATH={}${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}} exec {prog} --models-preset {} --models-max 1 --host 127.0.0.1 --port {port}",
                quote(&libs),
                quote(&preset.to_string_lossy())
            );
            (start, String::new(), alias)
        }
        Engine::Strata => (
            format!("{prog} launch --family qwen --model {} --port {port} --access local --no-api-key --yes --no-tui", quote(&model)),
            format!("{prog} launch --stop"),
            String::new(),
        ),
    };
    // (Adding to a llama.cpp server: its command now names its preset.)
    let before = crate::accounts::start_command(&base);
    if !adding || engine == Engine::LlamaCpp {
        crate::accounts::save_commands(&base, &start, &stop_cmd);
    }

    report.step(match engine {
        // (Strata's launch downloads the model first.)
        Engine::Strata => "Starting Strata (it downloads the model the first time: that takes a while)",
        _ => "Starting the server",
    });
    if adding {
        // A llama.cpp server running: its models read again (router), or it
        // starts again as one (it served a single model).
        if engine == Engine::LlamaCpp && harlo_core::model_servers::answers(&base) {
            if before.contains("--models-preset") {
                engines::reload_models(&base);
            } else {
                report.step("Restarting the server with its models");
                crate::accounts::stop_any(&base);
            }
        }
        crate::accounts::ensure_running(&base)?;
    } else {
        crate::accounts::launch(&base, &start)?;
        let limit = Duration::from_secs(if engine == Engine::Strata { 12 * 3600 } else { 20 * 60 });
        let waited = crate::accounts::wait_up(&base, limit, &mut |line| report.detail(line), &cancelled);
        if let Err(e) = waited {
            crate::accounts::stop_launched(&base);
            return Err(e);
        }
    }

    // Ollama: its model, pulled (its API: bytes of each part as they come).
    if engine == Engine::Ollama {
        let shown = model.strip_prefix("hf.co/").unwrap_or(&model).to_owned();
        report.step(&format!("Downloading {shown}"));
        let mut phase = String::new();
        engines::ollama_pull(&base, &model, &mut |status, done, total| {
            // (Its steps between the downloads: "verifying sha256 digest"…)
            if total == 0 && status != phase {
                phase = status.to_owned();
                report.detail(status);
            } else if total > 0 {
                report.progress(done, total);
            }
        }, &cancelled)?;
    }

    report.step("Sending a test message");
    let listing = harlo_core::model_servers::find(&base, "")?;
    let id = if model_id.is_empty() { listing.models.first().map(|m| m.id.clone()).unwrap_or_default() } else { model_id };
    let (answer, speed, thinks) = engines::test_chat(&base, &id)?;
    // The context it set (llama.cpp's /props, Ollama's loaded model): not
    // in /models.
    let served = match engine {
        Engine::Strata => None,
        Engine::LlamaCpp => engines::served_context_of(&base, &id),
        Engine::Ollama => engines::served_context(&base),
    };
    // Its other models (Ollama serves several): what harlo knew of them, else
    // the server's context up to each model's own.
    let before = crate::accounts::saved_models_of(&base);
    let models: Vec<Value> = listing
        .models
        .iter()
        .map(|m| {
            if m.id == id {
                // (The model it tested thinks: the agents are told.)
                return json!({ "id": m.id, "context": served.or(m.context), "reasoning": thinks });
            }
            let known = before.iter().find(|b| b.id == m.id);
            let own = || engines::model_context(&base, &m.id);
            let context = known.and_then(|k| k.context).or(m.context).or_else(|| match (served, own()) {
                (Some(s), Some(o)) => Some(s.min(o)),
                (s, o) => s.or(o),
            });
            json!({ "id": m.id, "context": context, "reasoning": known.is_some_and(|k| k.reasoning) })
        })
        .collect();
    // (Listed from now on, with or without agents.)
    crate::accounts::save_server(&base, &name, &models);
    Ok(json!({ "baseUrl": base, "name": name, "models": models, "answer": answer, "speed": speed, "context": served }))
}

impl ServerSetup {
    pub fn refresh(mut self: Pin<&mut Self>) {
        let mut v: Value = serde_json::from_str(&self.catalog.to_string()).unwrap_or_else(|_| json!({}));
        v["state"] = json!("loading");
        self.as_mut().set_catalog(QString::from(&v.to_string()));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let hw = engines::detect();
            let list: Vec<Value> = engines::ENGINES
                .iter()
                .map(|e| {
                    let (fit, why) = e.fit(&hw);
                    let installed = engines::installed(*e).map(|i| json!({ "program": i.program.to_string_lossy(), "version": i.version, "harlo": i.harlo }));
                    json!({ "id": e.id(), "name": e.name(), "tagline": e.tagline(), "fit": fit, "why": why, "installed": installed })
                })
                .collect();
            let strata: Vec<Value> = engines::STRATA_MODELS.iter().map(|(id, what, gb)| json!({ "id": id, "what": what, "needGb": gb })).collect();
            let v = json!({ "state": "ready", "hardware": hw.json(), "engines": list, "strataModels": strata });
            let _ = thread.queue(move |mut this| this.as_mut().set_catalog(QString::from(&v.to_string())));
        });
    }

    pub fn find_models(mut self: Pin<&mut Self>, search: &QString) {
        let search = search.to_string().trim().to_owned();
        let mut v: Value = serde_json::from_str(&self.hf_models.to_string()).unwrap_or_else(|_| json!({ "models": [] }));
        v["state"] = json!("finding");
        v["search"] = json!(search);
        self.as_mut().set_hf_models(QString::from(&v.to_string()));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let hw = engines::detect();
            let v = match engines::hf_models(&search) {
                Ok(models) => {
                    let list: Vec<Value> = models
                        .iter()
                        .map(|m| {
                            let q4 = engines::q4_bytes(m.params);
                            let name = m.repo.rsplit('/').next().unwrap_or(&m.repo).trim_end_matches("-GGUF").trim_end_matches("-gguf");
                            json!({
                                "repo": m.repo, "name": name, "params": m.params, "context": m.context, "downloads": m.downloads,
                                "q4Size": q4, "fit": if m.params == 0 { "" } else { engines::fit(q4, hw.vram_mb(), hw.ram_mb) },
                            })
                        })
                        .collect();
                    json!({ "state": "ready", "search": search, "models": list, "error": "" })
                }
                Err(e) => json!({ "state": "failed", "search": search, "models": [], "error": e }),
            };
            let _ = thread.queue(move |mut this| {
                // (Only the latest search.)
                if serde_json::from_str::<Value>(&this.hf_models.to_string()).ok().is_some_and(|c| c["search"] == v["search"]) {
                    this.as_mut().set_hf_models(QString::from(&v.to_string()));
                }
            });
        });
    }

    pub fn list_ggufs(mut self: Pin<&mut Self>, repo: &QString) {
        let repo = repo.to_string().trim().to_owned();
        self.as_mut().set_ggufs(QString::from(&json!({ "state": "listing", "repo": repo, "files": [] }).to_string()));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let hw = engines::detect();
            let v = match engines::hf_ggufs(&repo) {
                Ok(files) => {
                    let list: Vec<Value> = files
                        .iter()
                        .map(|(f, s)| {
                            json!({
                                "file": f, "size": s, "quant": engines::quant_of(f),
                                // Ollama takes neither a split model nor one in a folder.
                                "split": engines::gguf_parts(f).len() > 1 || f.contains('/'),
                                "fit": engines::fit(*s, hw.vram_mb(), hw.ram_mb),
                            })
                        })
                        .collect();
                    let pick = engines::pick_file(&files, hw.vram_mb(), hw.ram_mb);
                    let whole: Vec<(String, u64)> = files.iter().filter(|(f, _)| engines::gguf_parts(f).len() == 1 && !f.contains('/')).cloned().collect();
                    let pick_whole = engines::pick_file(&whole, hw.vram_mb(), hw.ram_mb);
                    json!({ "state": "ready", "repo": repo, "files": list, "pick": pick, "pickWhole": pick_whole, "error": "" })
                }
                Err(e) => json!({ "state": "failed", "repo": repo, "files": [], "error": e }),
            };
            let _ = thread.queue(move |mut this| {
                // (Only the latest listing.)
                if serde_json::from_str::<Value>(&this.ggufs.to_string()).ok().is_some_and(|c| c["repo"] == v["repo"]) {
                    this.as_mut().set_ggufs(QString::from(&v.to_string()));
                }
            });
        });
    }

    pub fn free_port(&self, from: i32) -> i32 {
        engines::free_port(from.clamp(1024, 65000) as u16) as i32
    }

    pub fn start(mut self: Pin<&mut Self>, request: &QString) {
        if serde_json::from_str::<Value>(&self.job.to_string()).is_ok_and(|v| v["state"] == "running") {
            return;
        }
        let req: Value = serde_json::from_str(&request.to_string()).unwrap_or_default();
        let stop = Arc::new(AtomicBool::new(false));
        self.as_mut().rust_mut().stop = stop.clone();
        let first = json!({ "state": "running", "step": "Starting", "progress": -1, "detail": "", "result": null });
        self.as_mut().set_job(QString::from(&first.to_string()));
        let report = Report { thread: self.qt_thread(), state: Arc::new(std::sync::Mutex::new(first)) };
        crate::runtime().spawn_blocking(move || {
            let outcome = setup(&req, &report, &stop);
            report.update(|v| {
                match outcome {
                    Ok(result) => {
                        v["state"] = json!("done");
                        v["result"] = result;
                    }
                    Err(e) if stop.load(Ordering::Relaxed) => {
                        v["state"] = json!("cancelled");
                        v["detail"] = json!(e);
                    }
                    Err(e) => {
                        v["state"] = json!("failed");
                        v["detail"] = json!(e);
                    }
                }
                v["progress"] = json!(-1);
            });
        });
    }

    pub fn cancel(self: Pin<&mut Self>) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
