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
        /// The setup's full output (its "Show output" panel).
        #[qproperty(QString, output)]
        /// The engines' latest versions: `{ <engine id>: { latest, available } }`.
        #[qproperty(QString, updates)]
        /// The machine it all happens on: "" this computer, else another
        /// machine's name (its engines, models and servers, there).
        #[qproperty(QString, host)]
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
        fn find_models(self: Pin<&mut ServerSetup>, search: &QString, engine: &QString);
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
        /// Looks for newer versions of the engines installed here, at most
        /// once an hour unless `force` (`updates` follows).
        #[qinvokable]
        fn check_updates(self: Pin<&mut ServerSetup>, force: bool);
    }
}

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::engine_setup::{Event, Report};
use harlo_core::engines;
use serde_json::{Value, json};

use qobject::ServerSetup;

#[derive(Default)]
pub struct ServerSetupRust {
    catalog: QString,
    output: QString,
    hf_models: QString,
    ggufs: QString,
    job: QString,
    updates: QString,
    host: QString,
    /// When the updates were last looked for, and where.
    checked: Option<(std::time::Instant, String)>,
    stop: Arc<AtomicBool>,
    /// A setup going on on another machine: (machine, its job).
    remote_job: Option<(String, String)>,
}

impl ServerSetupRust {
    fn on(&self) -> Option<String> {
        Some(self.host.to_string()).filter(|h| !h.is_empty())
    }

    /// The hardware the models are for: the machine's, from its catalog.
    fn hardware(&self) -> impl FnOnce() -> engines::Hardware + Send + 'static {
        let there = self.on().map(|_| serde_json::from_str::<Value>(&self.catalog.to_string()).unwrap_or_default()["hardware"].clone());
        move || match there {
            Some(hw) => engines::Hardware::from_json(&hw),
            None => engines::detect(),
        }
    }
}



























impl ServerSetup {
    pub fn refresh(mut self: Pin<&mut Self>) {
        let mut v: Value = serde_json::from_str(&self.catalog.to_string()).unwrap_or_else(|_| json!({}));
        // (Another machine's: none of this one's meanwhile.)
        if v["host"].as_str().unwrap_or_default() != self.host.to_string() {
            v = json!({ "engines": [] });
        }
        v["state"] = json!("loading");
        v["host"] = json!(self.host.to_string());
        self.as_mut().set_catalog(QString::from(&v.to_string()));
        let thread = self.qt_thread();
        let host = self.on();
        crate::runtime().spawn_blocking(move || {
            let v = match &host {
                None => harlo_core::engine_setup::catalog(),
                Some(h) => match harlo_core::engine_host::json(h, &["catalog"], Duration::from_secs(120)) {
                    Ok(mut v) => {
                        v["host"] = json!(h);
                        v
                    }
                    Err(e) => json!({ "state": "failed", "host": h, "error": format!("{e:#}"), "engines": [] }),
                },
            };
            let _ = thread.queue(move |mut this| this.as_mut().set_catalog(QString::from(&v.to_string())));
        });
    }

    pub fn find_models(mut self: Pin<&mut Self>, search: &QString, engine: &QString) {
        let search = search.to_string().trim().to_owned();
        // vLLM's models are in Hugging Face's own format, NInfer's its own, the
        // others' GGUF.
        let vllm = engine.to_string() == "vllm";
        let ninfer = engine.to_string() == "ninfer";
        let mut v: Value = serde_json::from_str(&self.hf_models.to_string()).unwrap_or_else(|_| json!({ "models": [] }));
        v["state"] = json!("finding");
        v["search"] = json!(search);
        self.as_mut().set_hf_models(QString::from(&v.to_string()));
        let thread = self.qt_thread();
        let hardware = self.hardware();
        crate::runtime().spawn_blocking(move || {
            let hw = hardware();
            let found = if ninfer { engines::hf_ninfer_models(&search) } else if vllm { engines::hf_vllm_models(&search) } else { engines::hf_models(&search) };
            let v = match found {
                Ok(models) => {
                    let list: Vec<Value> = models
                        .iter()
                        .map(|m| {
                            // (NInfer's: on the card, or its experts in RAM as well.)
                            if ninfer {
                                let mb = m.bytes / (1024 * 1024);
                                let fit = if m.bytes == 0 { "" } else if mb <= hw.vram_mb() * 8 / 10 { "gpu" } else if mb <= (hw.vram_mb() + hw.ram_mb) * 9 / 10 { "runs" } else { "no" };
                                return json!({ "repo": m.repo, "name": engines::ninfer_name(&m.repo), "params": 0, "context": 0, "downloads": m.downloads, "q4Size": m.bytes, "fit": fit });
                            }
                            // (vLLM: its weights as they are, all on the card, with room
                            // for its context.)
                            if vllm {
                                let name = m.repo.rsplit('/').next().unwrap_or(&m.repo);
                                let fit = if m.bytes == 0 { "" } else if m.bytes / (1024 * 1024) <= hw.vram_mb() * 8 / 10 { "gpu" } else { "no" };
                                return json!({ "repo": m.repo, "name": name, "params": m.params, "context": 0, "downloads": m.downloads, "q4Size": m.bytes, "fit": fit, "modelType": m.model_type });
                            }
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
        let hardware = self.hardware();
        crate::runtime().spawn_blocking(move || {
            let hw = hardware();
            let v = match engines::hf_ggufs(&repo) {
                Ok(files) => {
                    let list: Vec<Value> = files
                        .iter()
                        .map(|(f, s)| {
                            json!({
                                "file": f, "size": s, "quant": engines::quant_of(f),
                                // Ollama takes neither a split model nor one in a folder.
                                "split": engines::gguf_parts(f).len() > 1 || f.contains('/'),
                                // (Basalt's: the card and RAM together, by design.)
                                "fit": if f.ends_with(".basalt") { if *s / (1 << 20) <= (hw.vram_mb() + hw.ram_mb) * 9 / 10 { "runs" } else { "no" } } else { engines::fit(*s, hw.vram_mb(), hw.ram_mb) },
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
        // (There: asked there.)
        if let Some(h) = self.on() {
            return harlo_core::engine_host::json(&h, &["free-port", &from.to_string()], Duration::from_secs(30)).ok().and_then(|v| v.as_i64()).map_or(from, |p| p as i32);
        }
        // (Free now, and not a port of a server harlo knows, stopped or not.)
        let taken: Vec<u16> = harlo_core::servers::ports_in_use().into_iter().map(|(p, _)| p).collect();
        let mut port = from.clamp(1024, 65000) as u16;
        loop {
            port = engines::free_port(port);
            if !taken.contains(&port) || port >= 65000 {
                return port as i32;
            }
            port += 1;
        }
    }

    pub fn start(mut self: Pin<&mut Self>, request: &QString) {
        if serde_json::from_str::<Value>(&self.job.to_string()).is_ok_and(|v| v["state"] == "running") {
            return;
        }
        let req: Value = serde_json::from_str(&request.to_string()).unwrap_or_default();
        let stop = Arc::new(AtomicBool::new(false));
        self.as_mut().rust_mut().stop = stop.clone();
        let first = json!({ "state": "running", "step": "Starting", "progress": -1, "detail": "", "result": null, "update": req["update"].is_string() });
        self.as_mut().set_job(QString::from(&first.to_string()));
        self.as_mut().set_output(QString::default());
        let thread = self.qt_thread();
        // Another machine's: its helper does it there and says how it goes.
        if let Some(host) = self.on() {
            let job = format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis()));
            self.as_mut().rust_mut().remote_job = Some((host.clone(), job.clone()));
            crate::runtime().spawn_blocking(move || {
                let mut last = first;
                let outcome = harlo_core::engine_host::setup(&host, &job, &req, &mut |line| {
                    if line["job"].is_object() {
                        last = line["job"].clone();
                        let text = last.to_string();
                        let _ = thread.queue(move |mut this| this.as_mut().set_job(QString::from(&text)));
                    } else if let Some(text) = line["output"].as_str().map(str::to_owned) {
                        let _ = thread.queue(move |mut this| this.as_mut().set_output(QString::from(&text)));
                    }
                });
                if let Err(e) = outcome {
                    last["state"] = json!(if stop.load(Ordering::Relaxed) { "cancelled" } else { "failed" });
                    last["detail"] = json!(format!("{e:#}"));
                    last["progress"] = json!(-1);
                    let text = last.to_string();
                    let _ = thread.queue(move |mut this| this.as_mut().set_job(QString::from(&text)));
                }
            });
            return;
        }
        self.as_mut().rust_mut().remote_job = None;
        let report = Report::new(first, Duration::from_millis(250), move |event| {
            let _ = match event {
                Event::Job(text) => thread.queue(move |mut this| this.as_mut().set_job(QString::from(&text))),
                Event::Output(text) => thread.queue(move |mut this| this.as_mut().set_output(QString::from(&text))),
            };
        });
        crate::runtime().spawn_blocking(move || harlo_core::engine_setup::run(&req, &report, &stop));
    }

    pub fn cancel(self: Pin<&mut Self>) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some((host, job)) = self.remote_job.clone() {
            crate::runtime().spawn_blocking(move || {
                if let Err(e) = harlo_core::engine_host::cancel(&host, &job) {
                    tracing::warn!("cancelling the setup on {host}: {e:#}");
                }
            });
        }
    }

    pub fn check_updates(mut self: Pin<&mut Self>, force: bool) {
        let host = self.on();
        let at = host.clone().unwrap_or_default();
        if !force && self.checked.as_ref().is_some_and(|(t, h)| *h == at && t.elapsed() < Duration::from_secs(3600)) {
            return;
        }
        self.as_mut().rust_mut().checked = Some((std::time::Instant::now(), at));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let text = match &host {
                None => harlo_core::engine_setup::updates().to_string(),
                Some(h) => harlo_core::engine_host::json(h, &["updates"], Duration::from_secs(120)).map(|v| v.to_string()).unwrap_or_else(|_| "{}".into()),
            };
            let _ = thread.queue(move |mut this| this.as_mut().set_updates(QString::from(&text)));
        });
    }
}

