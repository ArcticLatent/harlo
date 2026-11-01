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
    output: QString,
    hf_models: QString,
    ggufs: QString,
    job: QString,
    updates: QString,
    checked: Option<std::time::Instant>,
    stop: Arc<AtomicBool>,
}

/// How a setup tells the dialog where it is.
#[derive(Clone)]
struct Report {
    thread: CxxQtThread<ServerSetup>,
    state: Arc<std::sync::Mutex<Value>>,
    output: Arc<std::sync::Mutex<Output>>,
}

/// The setup's full output, for its "Show output" panel: what it ran and
/// said, then the server's own log while it starts.
#[derive(Default)]
struct Output {
    lines: Vec<String>,
    /// How far a download is: its own line, rewritten as it moves.
    live: String,
    server: String,
    shown: Option<std::time::Instant>,
    /// A later update is already on its way.
    pending: bool,
}

impl Report {
    /// A line of output (kept: the last few thousand).
    fn line(&self, text: &str) {
        let mut o = self.output.lock().unwrap();
        let live = std::mem::take(&mut o.live);
        if !live.is_empty() {
            o.lines.push(live);
        }
        o.lines.push(text.to_owned());
        let excess = o.lines.len().saturating_sub(3000);
        o.lines.drain(..excess);
        drop(o);
        self.show_output(false);
    }

    /// The server's log as it is now (its last lines).
    fn server_log(&self, path: &Path) {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let lines: Vec<&str> = text.lines().flat_map(|l| l.rsplit('\r').next()).collect();
        let tail = lines[lines.len().saturating_sub(400)..].join("\n");
        let mut o = self.output.lock().unwrap();
        if o.server == tail {
            return;
        }
        o.server = tail;
        drop(o);
        self.show_output(false);
    }

    /// To the dialog, a few times a second at most (or now, at the end).
    fn show_output(&self, now: bool) {
        let text = {
            let mut o = self.output.lock().unwrap();
            if !now && o.shown.is_some_and(|t| t.elapsed() < Duration::from_millis(250)) {
                // (Shown a moment later: nothing may wait for a next line.)
                if !o.pending {
                    o.pending = true;
                    let report = self.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(250));
                        report.show_output(true);
                    });
                }
                return;
            }
            o.pending = false;
            o.shown = Some(std::time::Instant::now());
            let mut t = o.lines[o.lines.len().saturating_sub(800)..].join("\n");
            if !o.live.is_empty() {
                t.push('\n');
                t.push_str(&o.live);
            }
            if !o.server.is_empty() {
                t.push_str("\n── the server ──\n");
                t.push_str(&o.server);
            }
            t
        };
        let _ = self.thread.queue(move |mut this| this.as_mut().set_output(QString::from(&text)));
    }

    fn update(&self, f: impl FnOnce(&mut Value)) {
        let text = {
            let mut v = self.state.lock().unwrap();
            f(&mut v);
            v.to_string()
        };
        let _ = self.thread.queue(move |mut this| this.as_mut().set_job(QString::from(&text)));
    }

    fn step(&self, step: &str) {
        self.line(&format!("▸ {step}"));
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
        self.output.lock().unwrap().live = if total > 0 { format!("{detail} ({:.0}%)", p * 100.0) } else { detail.clone() };
        self.show_output(false);
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

/// Where a llama.cpp build's libraries are (its `LD_LIBRARY_PATH`).
fn llamacpp_libs(program: &Path) -> String {
    let mut libs = Vec::new();
    library_dirs(program.parent().and_then(Path::parent).unwrap_or(Path::new("/")), &mut libs, 4);
    libs.iter().map(|d| d.to_string_lossy().into_owned()).collect::<Vec<_>>().join(":")
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

/// The format of the Basalt pack a setup will use: the copy here, or Hugging
/// Face's. (Unknown: the engine says, when it starts.)
fn basalt_pack_format(repo: &str, model: &str) -> Option<u32> {
    let folder = engines::engines_dir().parent()?.join("models").join(repo.replace('/', "__"));
    let here = folder.join(model);
    if here.exists() { engines::basalt_format(&here) } else { engines::basalt_format_online(repo, model) }
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
        // (A file with no published checksum, Basalt's source: as it comes.)
        report.line(&format!("{} from {}", a.name, a.url));
        engines::download(&a.url, &file, Some(a.sha256.as_str()).filter(|s| !s.is_empty()), &mut progress, &|| stop.load(Ordering::Relaxed))?;
        report.line(if a.sha256.is_empty() { "Downloaded (no published checksum to check it against)." } else { "Downloaded; it matches its published SHA-256." });
        if engine != Engine::Strata {
            report.step(&format!("Unpacking {}", a.name));
            engines::unpack(&file, &dir)?;
            let _ = std::fs::remove_file(&file);
        }
    }
    match engine {
        Engine::Vllm => install_vllm(&dir, report, stop),
        Engine::Basalt => install_basalt(&dir, report, stop),
        Engine::Ninfer => install_ninfer(&dir, report, stop),
        Engine::Strata => {
            // Its own installer (checked above): ~/.local/share/strata, ~/.local/bin/strata.
            report.step("Running Strata's installer");
            let out = std::process::Command::new("sh").arg(dir.join("install.sh")).stdin(std::process::Stdio::null()).output().map_err(|e| e.to_string())?;
            let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            for l in said.lines() {
                report.line(&harlo_core::signin::plain(l));
            }
            if !out.status.success() {
                return Err(said.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("Strata's installer failed.").trim().to_owned());
            }
            engines::installed(Engine::Strata).map(|i| i.program).ok_or_else(|| "Strata's installer finished, but there's no strata program.".into())
        }
        _ => engines::installed(engine).filter(|i| i.harlo).map(|i| i.program).ok_or_else(|| format!("{} unpacked, but harlo can't find its program.", engine.name())),
    }
}

/// Updates an engine to its latest version (blocking). A new version goes in
/// a folder of its own and its servers move to it: those running stop only
/// when it's ready, and start again on it; if one doesn't, they all stay on
/// the version they had. Strata and vLLM update in place: their servers stop
/// first.
fn update(req: &Value, report: &Report, stop: &AtomicBool) -> Result<Value, String> {
    let engine = engines::ENGINES.iter().copied().find(|e| req["update"] == e.id()).ok_or("harlo doesn't know that engine.")?;
    let name = engine.name();
    let hw = engines::detect();
    let before = engines::installed(engine).ok_or_else(|| format!("{name} isn't installed."))?;
    let servers = crate::accounts::servers_of(name);
    let running: Vec<String> = servers.iter().filter(|u| harlo_core::model_servers::answers(u)).cloned().collect();
    let in_place = matches!(engine, Engine::Strata | Engine::Vllm);
    let stop_them = |report: &Report| {
        for url in &running {
            report.step(&format!("Stopping the server at {url}"));
            crate::accounts::stop_any(url);
        }
    };
    let start_them = |report: &Report| -> Result<(), String> {
        for url in &running {
            report.step(&format!("Starting the server at {url} again"));
            crate::accounts::ensure_running(url)?;
        }
        Ok(())
    };
    let root = engines::engines_dir().join(engine.id());
    let folder_of = |program: &Path| program.strip_prefix(&root).ok().and_then(|rest| rest.components().next()).map(|c| root.join(c));
    let old_folder = folder_of(&before.program);

    if in_place {
        stop_them(report);
    }
    let program = match install(engine, &hw, report, stop) {
        Ok(p) => p,
        Err(e) => {
            if in_place {
                let _ = start_them(report);
            } else if let Ok(tag) = engines::latest_version(engine)
                && Some(root.join(&tag)) != old_folder
            {
                // (What it got of the new version.)
                let _ = std::fs::remove_dir_all(root.join(&tag));
            }
            return Err(e);
        }
    };
    let now = engines::installed(engine).map(|i| i.version).unwrap_or_default();
    let new_folder = folder_of(&program);
    let drop_new = || {
        if let Some(f) = new_folder.as_ref().filter(|f| Some(*f) != old_folder.as_ref()) {
            let _ = std::fs::remove_dir_all(f);
        }
    };

    if in_place {
        start_them(report).map_err(|e| format!("{name} is updated to {now}, but its server didn't start: {e}"))?;
    } else {
        // Basalt reads one format of pack: the models it serves must suit.
        if engine == Engine::Basalt
            && let Some(reads) = engines::basalt_build_format(&program)
        {
            for url in &servers {
                let pack = crate::accounts::model_files(url).first().and_then(|f| engines::basalt_format(f));
                if pack.is_some_and(|p| p != reads) {
                    drop_new();
                    return Err(format!("Basalt {now} reads format {reads}, and the model at {url} is in format {}: it stays on {}.", pack.unwrap_or_default(), before.version));
                }
            }
        }
        // Its servers on the new version (a llama.cpp's libraries too).
        let libs = (engine == Engine::LlamaCpp).then(|| (llamacpp_libs(&before.program), llamacpp_libs(&program)));
        let (old_program, new_program) = (before.program.to_string_lossy().into_owned(), program.to_string_lossy().into_owned());
        let moved = |text: &str| {
            let text = text.replace(&old_program, &new_program);
            match &libs {
                Some((old, new)) if !old.is_empty() => text.replace(old, new),
                _ => text,
            }
        };
        let saved: Vec<(String, String, String)> = servers.iter().map(|u| { let (start, stop) = crate::accounts::commands(u); (u.clone(), start, stop) }).collect();
        stop_them(report);
        for (url, start, stop) in &saved {
            crate::accounts::save_commands(url, &moved(start), &moved(stop));
        }
        if let Err(e) = start_them(report) {
            report.line(&format!("{name} {now} didn't start: {e}"));
            stop_them(report);
            for (url, start, stop) in &saved {
                crate::accounts::save_commands(url, start, stop);
            }
            drop_new();
            let _ = start_them(report);
            return Err(format!("{name} {now} didn't start, so its servers stay on {}: {e}", before.version));
        }
        // (The old version: nothing uses it now.)
        if let Some(old) = old_folder.as_ref().filter(|f| Some(*f) != new_folder.as_ref()) {
            report.line(&format!("Removing {}", old.display()));
            let _ = std::fs::remove_dir_all(old);
        }
    }
    let again = if running.is_empty() { String::new() } else { " Its running servers started again.".to_owned() };
    Ok(json!({ "message": format!("{name} is updated: {} → {now}.{again}", before.version), "models": [] }))
}

/// vLLM with uv (blocking): a Python of its own and vLLM with PyTorch for
/// this computer's CUDA, in harlo's engines folder (several GB). Its
/// program, `vllm`.
fn install_vllm(dir: &Path, report: &Report, stop: &AtomicBool) -> Result<PathBuf, String> {
    let uv = walk_for(dir, "uv", 3).ok_or("uv unpacked, but harlo can't find it.")?;
    let home = engines::engines_dir().join("vllm");
    let env = engines::vllm_env();
    let cache = home.join("cache");
    // (harlo's uv first: vLLM's kernel step runs `uv` itself.)
    let path = format!("{}:{}", uv.parent().unwrap_or(Path::new("/")).display(), std::env::var("PATH").unwrap_or_default());
    let run = |program: &Path, args: &[&str], step: &str| -> Result<(), String> {
        report.step(step);
        let mut child = std::process::Command::new(program)
            .args(args)
            .env("PATH", &path)
            .env("UV_PYTHON_INSTALL_DIR", home.join("python"))
            .env("UV_CACHE_DIR", &cache)
            .env("VIRTUAL_ENV", &env)
            .env("NO_COLOR", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        // Its lines (uv's on stderr, vLLM's on stdout) as they come.
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        for pipe in [child.stdout.take().map(|o| Box::new(o) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|e| Box::new(e) as Box<dyn std::io::Read + Send>)].into_iter().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                use std::io::BufRead;
                for line in std::io::BufReader::new(pipe).lines().map_while(Result::ok) {
                    let _ = tx.send(line);
                }
            });
        }
        drop(tx);
        // How far: uv says each big download's size ("Downloading torch
        // (846.1MiB)"); what's come is what this computer received since.
        let started = received_bytes();
        let mut total = 0u64;
        let mut last = String::new();
        loop {
            match rx.recv_timeout(Duration::from_millis(400)) {
                Ok(line) => {
                    let line = harlo_core::signin::plain(&line).trim().to_owned();
                    if line.is_empty() {
                        continue;
                    }
                    if line.starts_with("Downloading ") {
                        total += announced_size(&line);
                    }
                    report.line(&line);
                    last = line;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
            if stop.load(Ordering::Relaxed) {
                let _ = child.kill();
                return Err("Cancelled.".into());
            }
            let got = received_bytes().saturating_sub(started);
            let shown: String = last.chars().take(90).collect();
            report.update(|v| {
                if total > 0 {
                    v["progress"] = json!((got as f64 / total as f64).min(0.99));
                    v["detail"] = json!(format!("{:.1} of {:.1} GB · {shown}", got as f64 / 1e9, total as f64 / 1e9));
                } else {
                    v["detail"] = json!(shown);
                }
            });
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        if status.success() { Ok(()) } else { Err(if last.is_empty() { format!("{} failed.", program.display()) } else { last }) }
    };
    let env_s = env.to_string_lossy().into_owned();
    run(&uv, &["venv", "--python", "3.12", "--managed-python", "--allow-existing", &env_s], "Setting up Python for vLLM")?;
    let python = env.join("bin/python").to_string_lossy().into_owned();
    run(&uv, &["pip", "install", "--python", &python, "--upgrade-package", "vllm", "vllm", "--torch-backend=auto"], "Installing vLLM and PyTorch (several GB)")?;
    // Its prebuilt GPU kernels, where this vLLM has the command (else it
    // fetches them when it starts: not worth failing the setup for).
    if let Err(e) = run(&env.join("bin/vllm"), &["download-kernels"], "Fetching vLLM's GPU kernels") {
        tracing::warn!("vllm download-kernels: {e}");
    }
    // (The downloaded wheels: not needed now.)
    let _ = std::fs::remove_dir_all(&cache);
    engines::installed(Engine::Vllm).filter(|i| i.harlo).map(|i| i.program).ok_or_else(|| "vLLM installed, but harlo can't find its program.".into())
}

/// Builds Basalt from its source (blocking): CMake with this computer's
/// CUDA toolkit, then a Python of its own for its server (`.venv` in the
/// source folder, where `basalt serve` looks). Its program, `bin/basalt`.
fn install_basalt(dir: &Path, report: &Report, stop: &AtomicBool) -> Result<PathBuf, String> {
    let src = std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).find(|p| p.join("CMakeLists.txt").is_file()).ok_or("Basalt's source unpacked, but harlo can't find it.")?;
    // (The CUDA toolkit where distributions put it, if it isn't on PATH.)
    let path = format!("{}:/opt/cuda/bin:/usr/local/cuda/bin", std::env::var("PATH").unwrap_or_default());
    let build = src.join("build");
    let mut configure = vec!["-S".to_owned(), src.to_string_lossy().into_owned(), "-B".to_owned(), build.to_string_lossy().into_owned(), "-DCMAKE_BUILD_TYPE=Release".to_owned()];
    if engines_on_path("ninja") {
        configure.extend(["-G".to_owned(), "Ninja".to_owned()]);
    }
    run_streamed(Path::new("cmake"), &configure, &path, "Preparing Basalt's build", report, stop, |_| None)?;
    run_streamed(Path::new("cmake"), &["--build".to_owned(), build.to_string_lossy().into_owned(), "-j".to_owned(), build_jobs()], &path, "Building Basalt (several minutes)", report, stop, build_progress)?;
    let venv = src.join(".venv");
    let venv_s = venv.to_string_lossy().into_owned();
    run_streamed(Path::new("python3"), &["-m".to_owned(), "venv".to_owned(), venv_s], &path, "Setting up Python for Basalt", report, stop, |_| None)?;
    let requirements = src.join("requirements.txt").to_string_lossy().into_owned();
    run_streamed(&venv.join("bin/python"), &["-m".to_owned(), "pip".to_owned(), "install".to_owned(), "--disable-pip-version-check".to_owned(), "-r".to_owned(), requirements], &path, "Installing Basalt's server", report, stop, |_| None)?;
    engines::installed(Engine::Basalt).filter(|i| i.harlo).map(|i| i.program).ok_or_else(|| "Basalt built, but harlo can't find bin/basalt.".into())
}

/// NInfer from its source (blocking): its RTX 5090 preset, built here with
/// CMake, Ninja and the CUDA toolkit. Its program, `ninfer-serve`.
fn install_ninfer(dir: &Path, report: &Report, stop: &AtomicBool) -> Result<PathBuf, String> {
    let src = std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).find(|p| p.join("CMakePresets.json").is_file()).ok_or("NInfer's source unpacked, but harlo can't find it.")?;
    let path = format!("{}:/opt/cuda/bin:/usr/local/cuda/bin", std::env::var("PATH").unwrap_or_default());
    let configure = ["-S".to_owned(), src.to_string_lossy().into_owned(), "--preset".to_owned(), "rtx5090-release".to_owned()];
    run_streamed(Path::new("cmake"), &configure, &path, "Preparing NInfer's build", report, stop, |_| None)?;
    let build = src.join("build/rtx5090-release");
    run_streamed(Path::new("cmake"), &["--build".to_owned(), build.to_string_lossy().into_owned(), "-j".to_owned(), build_jobs()], &path, "Building NInfer (several minutes)", report, stop, build_progress)?;
    engines::installed(Engine::Ninfer).filter(|i| i.harlo).map(|i| i.program).ok_or_else(|| "NInfer built, but harlo can't find ninfer-serve.".into())
}

fn build_jobs() -> String {
    std::thread::available_parallelism().map_or(4, |n| n.get()).to_string()
}

/// How far a build is, by its own count: "[ 42%] Building …" (make) or
/// "[123/456] …" (Ninja).
fn build_progress(line: &str) -> Option<f64> {
    let inner = line.strip_prefix('[')?.split(']').next()?.trim();
    match inner.split_once('/') {
        Some((a, b)) => Some(a.trim().parse::<f64>().ok()? / b.trim().parse::<f64>().ok()?.max(1.0)),
        None => Some(inner.trim_end_matches('%').trim().parse::<f64>().ok()? / 100.0),
    }
}

/// A repository's files into `folder`, on one bar: big ones over 8 connections,
/// each checked against Hugging Face's SHA-256 (those already there kept).
fn download_files(repo: &str, folder: &Path, files: &[(String, u64)], report: &Report, cancelled: &dyn Fn() -> bool) -> Result<(), String> {
    let total: u64 = files.iter().map(|(_, s)| *s).sum();
    report.step("Downloading the model");
    let mut before = 0;
    for (path, size) in files {
        let dest = folder.join(path);
        if !dest.exists() {
            let mut checking = false;
            let mut progress = |phase: &str, d: u64, _: u64| {
                if phase == "check" && !checking {
                    checking = true;
                    report.step(&format!("Checking {path} against Hugging Face's checksum"));
                } else if phase == "download" && checking {
                    checking = false;
                    report.step("Downloading the model");
                }
                report.progress(if phase == "check" { d } else { before + d }, if phase == "check" { *size } else { total });
            };
            // (Big files: 8 connections at once, checked.)
            if *size > 64 << 20 {
                report.line(&format!("{} → {}", engines::hf_url(repo, path), dest.display()));
                engines::download_parallel(&engines::hf_url(repo, path), &dest, None, 8, &mut progress, cancelled)?;
                report.line("Downloaded; it matches Hugging Face's SHA-256.");
            } else {
                engines::download(&engines::hf_url(repo, path), &dest, None, &mut progress, cancelled)?;
            }
        }
        before += size;
    }
    Ok(())
}

fn engines_on_path(prog: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(prog).is_file()))
}

/// Runs a program (blocking), its output a line at a time under the bar;
/// `progress` reads how far a line says it is (0–1). What it said last when
/// it fails.
fn run_streamed(program: &Path, args: &[String], path: &str, step: &str, report: &Report, stop: &AtomicBool, progress: impl Fn(&str) -> Option<f64>) -> Result<(), String> {
    report.step(step);
    let mut child = std::process::Command::new(program)
        .args(args)
        .env("PATH", path)
        .env("NO_COLOR", "1")
        .env("CLICOLOR", "0")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't run {}: {e}", program.display()))?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    for pipe in [child.stdout.take().map(|o| Box::new(o) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|e| Box::new(e) as Box<dyn std::io::Read + Send>)].into_iter().flatten() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(pipe).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
    }
    drop(tx);
    let mut last = String::new();
    let mut errors: Vec<String> = Vec::new();
    // What it's on: its last line that says how far ("[203/247] Building
    // …"), and since when; its other lines (a compiler's warnings) only
    // before there is one.
    let mut status: Option<(String, std::time::Instant)> = None;
    loop {
        match rx.recv_timeout(Duration::from_millis(400)) {
            Ok(line) => {
                let line = harlo_core::signin::plain(&line).trim().to_owned();
                if line.is_empty() {
                    continue;
                }
                report.line(&line);
                if line.contains("error") || line.contains("Error") {
                    errors.push(line.clone());
                }
                if let Some(p) = progress(&line) {
                    report.update(|v| v["progress"] = json!(p.clamp(0.0, 1.0)));
                    status = Some((line.chars().take(110).collect(), std::time::Instant::now()));
                } else if status.is_none() {
                    let shown: String = line.chars().take(120).collect();
                    report.update(|v| v["detail"] = json!(shown));
                }
                last = line;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        if let Some((line, since)) = &status {
            let secs = since.elapsed().as_secs();
            let text = if secs >= 5 { format!("{line} · {}:{:02}", secs / 60, secs % 60) } else { line.clone() };
            report.update(|v| v["detail"] = json!(text));
        }
        if stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            return Err("Cancelled.".into());
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        return Ok(());
    }
    // (A build's first error says more than its last line.)
    Err(errors.first().cloned().unwrap_or(if last.is_empty() { format!("{step} failed.") } else { last }))
}

/// The bytes this computer has received (all its network but loopback),
/// from /proc/net/dev.
fn received_bytes() -> u64 {
    std::fs::read_to_string("/proc/net/dev")
        .unwrap_or_default()
        .lines()
        .skip(2)
        .filter_map(|l| l.split_once(':'))
        .filter(|(name, _)| name.trim() != "lo")
        .filter_map(|(_, rest)| rest.split_whitespace().next()?.parse::<u64>().ok())
        .sum()
}

/// Graphics memory in use now on the NVIDIA card with the most, in MB
/// (None without one).
fn gpu_memory_used_mb() -> Option<u64> {
    let out = std::process::Command::new("nvidia-smi").args(["--query-gpu=memory.used", "--format=csv,noheader,nounits"]).stdin(std::process::Stdio::null()).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.trim().parse::<u64>().ok()).max()
}

/// The size a log line announces, "(about 5 GB; …)", in bytes.
fn about_size(line: &str) -> Option<u64> {
    let rest = line.split("about ").nth(1)?;
    let mut words = rest.split_whitespace();
    let n: f64 = words.next()?.parse().ok()?;
    let unit = match words.next()?.trim_end_matches(|c: char| !c.is_ascii_alphabetic()) {
        "GB" => 1e9,
        "MB" => 1e6,
        _ => return None,
    };
    Some((n * unit) as u64)
}

/// The amount a log line says it's at: "10.1 of 84.5 GB" (or MB, GiB, MiB;
/// the same unit on both sides), in bytes: (done, total).
fn amount_in(line: &str) -> Option<(u64, u64)> {
    let (head, tail) = line.split_once(" of ")?;
    let done: f64 = head.rsplit(|c: char| !(c.is_ascii_digit() || c == '.')).next()?.parse().ok()?;
    let mut words = tail.split_whitespace();
    let total: f64 = words.next()?.parse().ok()?;
    let unit = match words.next().unwrap_or_default().trim_end_matches(|c: char| !c.is_ascii_alphabetic()) {
        "GB" => 1e9,
        "MB" => 1e6,
        "KB" => 1e3,
        "GiB" => (1u64 << 30) as f64,
        "MiB" => (1u64 << 20) as f64,
        "KiB" => 1024.0,
        _ => return None,
    };
    (total > 0.0).then(|| ((done.min(total) * unit) as u64, (total * unit) as u64))
}

/// The size uv announces for a download ("Downloading torch (846.1MiB)").
fn announced_size(line: &str) -> u64 {
    let Some(inner) = line.rsplit_once('(').and_then(|(_, r)| r.strip_suffix(')')) else { return 0 };
    let split = inner.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(inner.len());
    let n: f64 = inner[..split].trim().parse().unwrap_or(0.0);
    let unit = match inner[split..].trim() {
        "GiB" => 1u64 << 30,
        "MiB" => 1 << 20,
        "KiB" => 1 << 10,
        "GB" => 1_000_000_000,
        "MB" => 1_000_000,
        _ => 1,
    };
    (n * unit as f64) as u64
}

/// A file named `name` under `dir` (a few levels down).
fn walk_for(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let mut subdirs = Vec::new();
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            subdirs.push(p);
        } else if e.file_name() == name {
            return Some(p);
        }
    }
    (depth > 0).then(|| subdirs.into_iter().find_map(|d| walk_for(&d, name, depth - 1))).flatten()
}

/// One setup (blocking): the server's address, name, models and test.
/// A setup that didn't finish (failed, cancelled) leaves nothing behind:
/// the server's record as it was before (none, for a new one).
fn setup(req: &Value, report: &Report, stop: &AtomicBool) -> Result<Value, String> {
    let port = req["port"].as_u64().filter(|p| *p > 0 && *p < 65536).unwrap_or(8080);
    let base = format!("http://127.0.0.1:{port}/v1");
    let before = crate::accounts::server_record(&base);
    let outcome = setup_server(req, report, stop);
    if outcome.is_err() {
        crate::accounts::restore_server_record(&base, &before);
    }
    outcome
}

fn setup_server(req: &Value, report: &Report, stop: &AtomicBool) -> Result<Value, String> {
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
    // (Another server's port, stopped or not: it would take its place.)
    if !adding && let Some((_, other)) = crate::accounts::ports_in_use().into_iter().find(|(p, n)| *p == port && *n != name) {
        return Err(format!("Port {port} belongs to {other}: pick another port."));
    }
    // Basalt reads one format of pack: the model's, known before anything's
    // downloaded (a build too old for it: the latest release, built here).
    let pack = (engine == Engine::Basalt).then(|| basalt_pack_format(req["repo"].as_str().unwrap_or_default().trim(), &model)).flatten();
    let mut use_installed = engines::installed(engine).filter(|_| req["useInstalled"] == true);
    if let (Some(i), Some(pack)) = (&use_installed, pack)
        && let Some(reads) = engines::basalt_build_format(&i.program).filter(|reads| *reads < pack)
    {
        report.line(&format!("This model is in Basalt's format {pack}; Basalt {} reads {reads}: building the latest release.", i.version));
        use_installed = None;
    }
    let program = match use_installed {
        Some(i) => i.program,
        None => install(engine, &hw, report, stop)?,
    };
    if let Some(pack) = pack
        && let Some(reads) = engines::basalt_build_format(&program).filter(|reads| *reads != pack)
    {
        return Err(format!("This model is in Basalt's format {pack} and this Basalt reads {reads}: pick another model."));
    }
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
                    report.line(&format!("{} → {}", engines::hf_url(&repo, part), dest.display()));
                    engines::download_parallel(&engines::hf_url(&repo, part), &dest, None, 8, &mut progress, &cancelled)?;
                    report.line("Downloaded; it matches Hugging Face's SHA-256.");
                }
            }
            let alias = Path::new(&model).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(model.clone());
            let alias = alias.split("-0000").next().unwrap_or(&alias).to_owned();
            let libs = llamacpp_libs(&program);
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
        // (DeepSeek's own context is 8k, too little for an agent: 64k unless
        // picked. Qwen's is Strata's choice for the card.)
        Engine::Strata => {
            let family = engines::strata_family(&model);
            let context = if family == "deepseek41" { format!(" --context {}", req["context"].as_u64().filter(|c| *c >= 2048).unwrap_or(65536)) } else { String::new() };
            (
                format!("{prog} launch --family {family} --model {}{context} --port {port} --access local --no-api-key --yes --no-tui", quote(&model)),
                format!("{prog} launch --stop"),
                String::new(),
            )
        }
        // Basalt: its pack (one file: weights, tokenizer, drafter, vision),
        // and its settings file: the model, where it listens, its name and
        // context (it has no "as much as fits": 64k unless picked).
        Engine::Basalt => {
            let repo = req["repo"].as_str().unwrap_or_default().trim().to_owned();
            let folder = engines::engines_dir().parent().unwrap_or(Path::new(".")).join("models").join(repo.replace('/', "__"));
            let need = req["size"].as_u64().unwrap_or(0) / (1024 * 1024);
            if need > hw.disk_free_mb {
                return Err(format!("It's {} GB and only {} GB are free.", need / 1024, hw.disk_free_mb / 1024));
            }
            let dest = folder.join(&model);
            if !dest.exists() {
                report.step("Downloading the model");
                let mut checking = false;
                let mut progress = |phase: &str, d: u64, t: u64| {
                    if phase == "check" && !checking {
                        checking = true;
                        report.step("Checking the model against Hugging Face's checksum");
                    }
                    report.progress(d, t);
                };
                report.line(&format!("{} → {}", engines::hf_url(&repo, &model), dest.display()));
                engines::download_parallel(&engines::hf_url(&repo, &model), &dest, None, 8, &mut progress, &cancelled)?;
                report.line("Downloaded; it matches Hugging Face's SHA-256.");
            }
            let alias = Path::new(&model).file_stem().map(|s| s.to_string_lossy().trim_end_matches("_v2").to_owned()).unwrap_or(model.clone());
            let context = req["context"].as_u64().filter(|c| *c >= 2048).unwrap_or(65536);
            let config = engines::engines_dir().join("basalt").join("servers").join(format!("basalt-{port}.json"));
            let settings = json!({
                "server": { "model": dest.to_string_lossy(), "host": "127.0.0.1", "port": port, "alias": alias },
                "engine": { "max-context": context, "kv": "int8" },
            });
            std::fs::create_dir_all(config.parent().unwrap_or(Path::new("/"))).map_err(|e| e.to_string())?;
            std::fs::write(&config, serde_json::to_string_pretty(&settings).unwrap_or_default()).map_err(|e| format!("Couldn't write {}: {e}", config.display()))?;
            (format!("exec {prog} serve --config {}", quote(&config.to_string_lossy())), String::new(), alias)
        }
        // NInfer: the model's files in harlo's models folder, and its settings
        // file: the deployment its model cards give for an RTX 5090 (every
        // official model has Vision, MTP and the optimized head).
        Engine::Ninfer => {
            let repo = model.clone();
            let folder = engines::engines_dir().parent().unwrap_or(Path::new(".")).join("models").join(repo.replace('/', "__"));
            report.step("Listing the model's files");
            let files = engines::hf_ninfer_files(&repo)?;
            let total: u64 = files.iter().map(|(_, s)| *s).sum();
            if total / (1024 * 1024) > hw.disk_free_mb {
                return Err(format!("It's {} GB and only {} GB are free.", total / (1 << 30), hw.disk_free_mb / 1024));
            }
            download_files(&repo, &folder, &files, report, &cancelled)?;
            let alias = engines::ninfer_name(&repo);
            let context = req["context"].as_u64().filter(|c| *c >= 2048).unwrap_or(65536);
            let config = engines::engines_dir().join("ninfer").join("servers").join(format!("ninfer-{port}.toml"));
            let word = |s: &str| serde_json::to_string(s).unwrap_or_default();
            let settings = format!(
                "[model]\npath = {}\nvision = true\n\n[engine]\nmax_context = {context}\nmax_concurrency = 2\nkv_dtype = \"fp8\"\ncontext_capacity = \"auto\"\nmemory_headroom = \"256MiB\"\ncuda_graph = true\n\n[speculative]\nbackend = \"mtp\"\ndraft_tokens = 3\nproposal_head = \"optimized\"\n\n[vision]\nmemory_budget = \"1GiB\"\n\n[server]\nhost = \"127.0.0.1\"\nport = {port}\nmodel_id = {}\n",
                word(&folder.join(&files[0].0).to_string_lossy()),
                word(&alias)
            );
            std::fs::create_dir_all(config.parent().unwrap_or(Path::new("/"))).map_err(|e| e.to_string())?;
            std::fs::write(&config, settings).map_err(|e| format!("Couldn't write {}: {e}", config.display()))?;
            (format!("exec {prog} --config {}", quote(&config.to_string_lossy())), String::new(), alias)
        }
        // vLLM: the repository's files in harlo's models folder, served with
        // its family's parsers (agents need its tool calls read).
        Engine::Vllm => {
            let repo = model.clone();
            let folder = engines::engines_dir().parent().unwrap_or(Path::new(".")).join("models").join(repo.replace('/', "__"));
            report.step("Listing the model's files");
            let files = engines::hf_vllm_files(&repo)?;
            let total: u64 = files.iter().map(|(_, s)| *s).sum();
            if total / (1024 * 1024) > hw.disk_free_mb {
                return Err(format!("It's {} GB and only {} GB are free.", total / (1 << 30), hw.disk_free_mb / 1024));
            }
            download_files(&repo, &folder, &files, report, &cancelled)?;
            let config: Value = std::fs::read_to_string(folder.join("config.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            let (tools, thinking) = engines::vllm_parsers(config["model_type"].as_str().unwrap_or_default(), &repo);
            let alias = repo.rsplit('/').next().unwrap_or(&repo).to_owned();
            let context = req["context"].as_u64().filter(|c| *c >= 2048).map_or("auto".to_owned(), |c| c.to_string());
            let start = format!(
                "HF_HUB_OFFLINE=1 VLLM_NO_USAGE_STATS=1 DO_NOT_TRACK=1 exec {prog} serve {} --served-model-name {} --host 127.0.0.1 --port {port} --max-model-len {context} --gpu-memory-utilization 0.90 --enable-auto-tool-choice --tool-call-parser {tools}{}",
                quote(&folder.to_string_lossy()),
                quote(&alias),
                thinking.map(|t| format!(" --reasoning-parser {t}")).unwrap_or_default()
            );
            (start, String::new(), alias)
        }
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
        // (Progress in its log, as vLLM's "Capturing CUDA graphs: 17%|…" or
        // Strata's "Downloading the model files: 10.1 of 84.5 GB": the bar
        // follows it.)
        // An amount it says now and then ("10.1 of 84.5 GB", Strata's every
        // few GB): between its lines, what this computer received since, so
        // it moves as it downloads; its next line puts it right.
        // (A line announcing a size, "Downloading the draft layer (about 5 GB…)":
        // what's received since it, of that. Nothing received for a few
        // seconds: it's working on something else, the line as it is.)
        let mut said: Option<(String, u64, u64, u64)> = None; // (line, done, total, received then)
        let mut moving = (received_bytes(), std::time::Instant::now());
        // (A quiet phase, "Starting the engine and loading the model": how
        // long so far, and the card's memory filling as the model loads.)
        let since = std::time::Instant::now();
        let mut card: (Option<u64>, std::time::Instant) = (gpu_memory_used_mb(), std::time::Instant::now());
        let waited = crate::accounts::wait_up(
            &base,
            limit,
            &mut |line| {
                report.server_log(&crate::accounts::log_path(&base));
                let pct = line.split('%').next().and_then(|head| head.rsplit(|c: char| !c.is_ascii_digit()).next()).and_then(|n| n.parse::<f64>().ok()).filter(|p| line.contains("%|") && *p <= 100.0).map(|p| p / 100.0);
                if let Some(p) = pct {
                    report.update(|v| v["progress"] = json!(p));
                    return report.detail(line);
                }
                let received = received_bytes();
                // (A megabyte: a computer's usual chatter isn't a download.)
                if received > moving.0 + 1_000_000 {
                    moving = (received, std::time::Instant::now());
                }
                let quiet = moving.1.elapsed() > Duration::from_secs(5);
                // Strata checks each file of a big model when it arrives, and
                // says little more until the next line: the bar keeps the
                // download's place instead of running back to one file's check,
                // and bytes arriving under a finished check are the next file's.
                let checking = line.contains("nothing is downloaded");
                if checking && (quiet || said.is_none()) {
                    let place = said.as_ref().map(|(_, d, t, then)| ((d + received.saturating_sub(*then)).min(t * 999 / 1000)) as f64 / (*t).max(1) as f64);
                    let place = place.or_else(|| amount_in(line).map(|(d, t)| d as f64 / t as f64)).unwrap_or(-1.0);
                    report.update(|v| {
                        v["progress"] = json!(place);
                        v["detail"] = json!(line);
                    });
                    return;
                }
                let downloading = if checking { said.as_ref().map(|s| s.0.clone()) } else { None };
                let line = downloading.as_deref().unwrap_or(line);
                let amount = amount_in(line).map(|(d, t)| (d, t, true)).or_else(|| about_size(line).map(|t| (0, t, false)));
                let Some((done, total, exact)) = amount.filter(|_| !quiet) else {
                    said = None;
                    if card.1.elapsed() > Duration::from_secs(3) {
                        card = (gpu_memory_used_mb(), std::time::Instant::now());
                    }
                    let secs = since.elapsed().as_secs();
                    let text = format!(
                        "{line} · {}:{:02}{}",
                        secs / 60,
                        secs % 60,
                        card.0.map(|mb| format!(" · {:.1} GB on the card", mb as f64 / 1024.0)).unwrap_or_default()
                    );
                    report.update(|v| {
                        v["progress"] = json!(-1.0);
                        v["detail"] = json!(text);
                    });
                    return;
                };
                if said.as_ref().is_none_or(|(l, d, t, _)| l != line || *d != done || *t != total) {
                    said = Some((line.to_owned(), done, total, received));
                }
                let (_, d, t, then) = said.clone().unwrap_or_default();
                let now = (d + received.saturating_sub(then)).min(t * 999 / 1000);
                let estimate = now > d || !exact;
                let head = if exact { line.split(" of ").next().unwrap_or(line).trim_end_matches(|c: char| c.is_ascii_digit() || c == '.').trim_end() } else { line.split(" (").next().unwrap_or(line) };
                let text = format!("{head}{} {}{:.1} of {:.1} GB", if exact { "" } else { ":" }, if estimate { "≈ " } else { "" }, now as f64 / 1e9, t as f64 / 1e9);
                report.update(|v| {
                    v["progress"] = json!(now as f64 / t.max(1) as f64);
                    v["detail"] = json!(text);
                });
            },
            &cancelled,
        );
        if let Err(e) = waited {
            report.server_log(&crate::accounts::log_path(&base));
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
                report.line(status);
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
    report.line(&format!("{id} answered “{answer}” at {speed:.0} tokens/s{}", if thinks { ", thinking first" } else { "" }));
    // The context it set (llama.cpp's /props, Ollama's loaded model): not
    // in /models.
    let served = match engine {
        // (vLLM's and NInfer's /models say it: max_model_len; Basalt's is what
        // harlo set.)
        Engine::Strata | Engine::Vllm | Engine::Ninfer => None,
        Engine::Basalt => Some(req["context"].as_u64().filter(|c| *c >= 2048).unwrap_or(65536)),
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
                    // (harlo updates what it installed; Strata's own installer works over any install of it.)
                    let installed = engines::installed(*e).map(|i| json!({ "program": i.program.to_string_lossy(), "version": i.version, "harlo": i.harlo, "updatable": i.harlo || *e == Engine::Strata }));
                    json!({ "id": e.id(), "name": e.name(), "tagline": e.tagline(), "fit": fit, "why": why, "installed": installed })
                })
                .collect();
            let strata: Vec<Value> = engines::STRATA_MODELS.iter().map(|(id, what, gb)| json!({ "id": id, "name": id.to_uppercase(), "what": what, "needGb": gb })).collect();
            let deepseek: Vec<Value> = engines::STRATA_DEEPSEEK.iter().map(|(id, name, what, gb, disk)| json!({ "id": id, "name": name, "what": what, "needGb": gb, "diskGb": disk })).collect();
            let v = json!({ "state": "ready", "hardware": hw.json(), "engines": list, "strataModels": strata, "strataDeepseek": deepseek });
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
        crate::runtime().spawn_blocking(move || {
            let hw = engines::detect();
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
        // (Free now, and not a port of a server harlo knows, stopped or not.)
        let taken: Vec<u16> = crate::accounts::ports_in_use().into_iter().map(|(p, _)| p).collect();
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
        let report = Report { thread: self.qt_thread(), state: Arc::new(std::sync::Mutex::new(first)), output: Default::default() };
        crate::runtime().spawn_blocking(move || {
            let outcome = if req["update"].is_string() { update(&req, &report, &stop) } else { setup(&req, &report, &stop) };
            // (The end, in the output too: and all of it shown now.)
            match &outcome {
                Ok(_) => report.line("✓ Done."),
                Err(e) => report.line(&format!("✗ {e}")),
            }
            report.show_output(true);
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

    pub fn check_updates(mut self: Pin<&mut Self>, force: bool) {
        if !force && self.checked.is_some_and(|t| t.elapsed() < Duration::from_secs(3600)) {
            return;
        }
        self.as_mut().rust_mut().checked = Some(std::time::Instant::now());
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let mut updates = serde_json::Map::new();
            for e in engines::ENGINES {
                let Some(i) = engines::installed(e).filter(|i| i.harlo || e == Engine::Strata) else { continue };
                // (Offline, or GitHub's limit: nothing to say about this one.)
                match engines::latest_version(e) {
                    Ok(latest) => {
                        let available = engines::update_available(e, &i.version, &latest);
                        updates.insert(e.id().to_owned(), json!({ "latest": latest.trim_start_matches('v'), "available": available }));
                    }
                    Err(err) => tracing::info!("{} update check: {err}", e.name()),
                }
            }
            let text = Value::Object(updates).to_string();
            let _ = thread.queue(move |mut this| this.as_mut().set_updates(QString::from(&text)));
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_amounts_in_log_lines() {
        assert_eq!(super::amount_in("Downloading the model files: 10.1 of 84.5 GB"), Some((10_100_000_000, 84_500_000_000)));
        assert_eq!(super::amount_in("Fetched 512 of 1024 MiB"), Some((512 << 20, 1024 << 20)));
        assert_eq!(super::amount_in("Loading 3 of 7 shards"), None);
        assert_eq!(super::amount_in("Ready"), None);
        assert_eq!(super::about_size("Downloading the draft layer (about 5 GB; it makes answers about twice as fast)"), Some(5_000_000_000));
        assert_eq!(super::about_size("Preparing the draft layer"), None);
    }

    #[test]
    fn reads_the_sizes_uv_announces() {
        assert_eq!(super::announced_size("Downloading torch (846.1MiB)"), (846.1 * (1u64 << 20) as f64) as u64);
        assert_eq!(super::announced_size("Downloading flashinfer-jit-cache (1.4GiB)"), (1.4 * (1u64 << 30) as f64) as u64);
        assert_eq!(super::announced_size("Downloading cpython-3.12.12-linux-x86_64-gnu (download)"), 0);
        assert!(super::received_bytes() > 0 || !std::path::Path::new("/proc/net/dev").exists());
    }
}
