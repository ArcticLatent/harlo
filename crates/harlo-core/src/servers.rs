//! Model servers harlo set up (Settings → Model servers), on the machine
//! this runs on: their start and stop commands and models (harlo's
//! settings), starting them with their output to a log, waiting until they
//! answer, and stopping them, however they're running. The app uses it for
//! this computer's; `harlo-engines` for another machine's, there.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use harlo_harness::HarnessKind;
use serde_json::{Value, json};

use crate::model_servers;

/// After a server harlo set up starts (`ensure_running`): what the app does
/// then (the context it serves, into the agents that have it), saying what
/// couldn't be done.
static AFTER_START: std::sync::OnceLock<fn(&str) -> Vec<String>> = std::sync::OnceLock::new();

pub fn set_after_start(f: fn(&str) -> Vec<String>) {
    let _ = AFTER_START.set(f);
}

/// Servers keep running when what started them ends (`harlo-engines` on
/// another machine: it exits once the server answers).
static DETACHED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_detached(on: bool) {
    DETACHED.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Where harlo keeps the models it downloads.
pub fn models_dir() -> PathBuf {
    crate::engines::engines_dir().parent().map(|d| d.join("models")).unwrap_or_default()
}

/// The first word of a shell command line, unquoted ('…', \x).
pub fn shell_word(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.trim_start().chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => out.extend(chars.by_ref().take_while(|c| *c != '\'')),
            '\\' => out.extend(chars.next()),
            c if c.is_whitespace() => break,
            c => out.push(c),
        }
    }
    out
}

/// The model files of a server harlo set up with llama.cpp: the GGUF its
/// start command loads (`-m '…'`) and its other parts, in harlo's folder.
pub fn model_files(url: &str) -> Vec<PathBuf> {
    let start = commands_of(url)["start"].as_str().unwrap_or_default().to_owned();
    // (Strata's: the set's folders in its own data folder.)
    if start.contains(" launch --family ") {
        return strata_model(&start).map(|m| strata_files(url, &m)).unwrap_or_default();
    }
    // (Basalt's: the pack its settings file names.)
    if let Some(rest) = start.split(" serve --config ").nth(1).filter(|_| start.contains("basalt")) {
        let settings: Value = std::fs::read_to_string(shell_word(rest)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        return settings["server"]["model"].as_str().map(PathBuf::from).filter(|p| p.starts_with(models_dir()) && p.is_file()).into_iter().collect();
    }
    // (NInfer's: the entry file its settings name, and the parts beside it.)
    if let Some(rest) = start.split(" --config ").nth(1).filter(|_| start.contains("ninfer-serve")) {
        let settings = std::fs::read_to_string(shell_word(rest)).unwrap_or_default();
        let entry = settings.lines().find_map(|l| serde_json::from_str::<String>(l.strip_prefix("path = ")?).ok()).map(PathBuf::from);
        let Some((dir, name)) = entry.filter(|p| p.starts_with(models_dir())).and_then(|p| Some((p.parent()?.to_path_buf(), p.file_name()?.to_string_lossy().into_owned()))) else { return Vec::new() };
        return std::fs::read_dir(&dir).into_iter().flatten().flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&name)).map(|e| e.path()).collect();
    }
    // (vLLM's: the model's folder, all of it.)
    if let Some(rest) = start.split(" serve ").nth(1).filter(|_| start.contains("vllm")) {
        let dir = PathBuf::from(shell_word(rest));
        if !dir.starts_with(models_dir()) || dir == models_dir() {
            return Vec::new();
        }
        return files_under(&dir);
    }
    // (A router's: each model of its preset.)
    let paths: Vec<PathBuf> = match start.split(" --models-preset ").nth(1) {
        Some(rest) => crate::engines::read_preset(Path::new(&shell_word(rest))).into_iter().map(|m| m.path).collect(),
        None => start.split(" -m ").nth(1).map(|rest| PathBuf::from(shell_word(rest))).into_iter().collect(),
    };
    paths
        .iter()
        .filter_map(|path| Some((path.parent()?.to_path_buf(), path.file_name()?.to_string_lossy().into_owned())))
        .filter(|(dir, _)| dir.starts_with(models_dir()))
        .flat_map(|(dir, file)| crate::engines::gguf_parts(&file).into_iter().map(move |p| dir.join(p)))
        .filter(|p| p.is_file())
        .collect()
}

/// Every file in a folder, however deep.
pub fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() { dirs.push(p) } else { files.push(p) }
        }
    }
    files
}

/// A set of Strata's on this computer, as its setup left it.
struct StrataModel {
    /// `--model`, in lower case.
    name: String,
    deepseek: bool,
    /// Strata's data folder (models, packs, mtp).
    data: PathBuf,
    pack: PathBuf,
    /// Its settings file and start script (in Strata's install folder).
    own: Vec<PathBuf>,
}

/// The set a Strata start command names, from the settings Strata wrote.
fn strata_model(start: &str) -> Option<StrataModel> {
    strata_model_in(&PathBuf::from(std::env::var_os("HOME")?).join(".local/share/strata/install"), start)
}

fn strata_model_in(install: &Path, start: &str) -> Option<StrataModel> {
    if !start.contains(" launch --family ") {
        return None;
    }
    let name = shell_word(start.split(" --model ").nth(1)?).to_lowercase();
    let deepseek = start.split(" --family ").nth(1).is_some_and(|rest| shell_word(rest) == "deepseek41");
    let (file, stem) = std::fs::read_dir(install).ok()?.flatten().find_map(|e| {
        let file = e.file_name().to_string_lossy().into_owned();
        let stem = file.strip_prefix("strata-")?.strip_suffix(".json")?.to_owned();
        (stem == name || stem.ends_with(&format!("-{name}"))).then_some((e.path(), stem))
    })?;
    let config: Value = serde_json::from_str(&std::fs::read_to_string(&file).ok()?).ok()?;
    let args = config["args"].as_array()?;
    let pack = args.iter().position(|a| a == "--pack").and_then(|i| args.get(i + 1)?.as_str()).map(PathBuf::from)?;
    let data = pack.parent().filter(|p| p.ends_with("packs"))?.parent()?.to_path_buf();
    Some(StrataModel { name, deepseek, data, pack, own: vec![file, install.join(format!("run-{stem}.sh"))] })
}

/// A Strata set's files: its model and pack folders, unless another of
/// harlo's servers uses the set; what the sets of its family share, with the
/// last of them.
fn strata_files(url: &str, model: &StrataModel) -> Vec<PathBuf> {
    let others: Vec<StrataModel> = set_up_servers().into_iter().filter(|(u, _)| u != url).filter_map(|(_, v)| strata_model(v["start"].as_str()?)).collect();
    strata_set_files(model, &others)
}

fn strata_set_files(model: &StrataModel, other_servers: &[StrataModel]) -> Vec<PathBuf> {
    if other_servers.iter().any(|o| o.name == model.name) {
        return Vec::new();
    }
    let in_folder = |dir: &Path| -> Vec<PathBuf> { std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect() };
    let name_of = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let models = in_folder(&model.data.join("models"));
    let mut files = files_under(&model.pack);
    for m in models.iter().filter(|m| m.is_dir() && (name_of(m) == model.name || name_of(m).ends_with(&format!("-{}", model.name)))) {
        files.extend(files_under(m));
    }
    // (Others of its family with a pack here need what they share.)
    let packs = in_folder(&model.data.join("packs"));
    let others = packs.iter().filter(|p| **p != model.pack && name_of(p).starts_with("deepseek") == model.deepseek).count();
    if others == 0 {
        if model.deepseek {
            files.extend(models.iter().filter(|m| name_of(m) == "deepseek-v4.1-flash-shared").flat_map(|m| files_under(m)));
        } else {
            files.extend(models.iter().filter(|m| m.is_file() && name_of(m).starts_with("mmproj-qwen")).cloned());
            files.extend(files_under(&model.data.join("mtp")));
        }
    }
    files.extend(model.own.iter().filter(|f| f.is_file()).cloned());
    files
}

/// Servers harlo started, by address: their process (a group of its own).
pub static STARTED: Mutex<Option<HashMap<String, std::process::Child>>> = Mutex::new(None);

fn server_key(url: &str) -> String {
    format!("model_server.{}", url.trim_end_matches('/'))
}

pub fn commands_of(url: &str) -> Value {
    crate::db::Db::open(&crate::db::default_path())
        .ok()
        .and_then(|d| d.get(&server_key(url)).ok().flatten())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({ "start": "", "stop": "" }))
}

/// Where a server's output goes: harlo's logs folder.
pub fn log_path(url: &str) -> PathBuf {
    let name: String = url.split("://").last().unwrap_or(url).chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    crate::db::default_path().parent().map(|d| d.join("logs/servers")).unwrap_or_default().join(format!("{}.log", name.trim_matches('-')))
}

pub fn log_tail(url: &str) -> String {
    let text = std::fs::read_to_string(log_path(url)).unwrap_or_default();
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let tail = &lines[lines.len().saturating_sub(3)..];
    // (Its error, without what it printed before it.)
    let from = tail.iter().position(|l| l.to_lowercase().contains("error:")).unwrap_or(0);
    start_failure(&tail[from..].join("\n"))
}

/// Why a server didn't start, said plainly where the engine's words don't.
pub fn start_failure(log: &str) -> String {
    if log.contains("Device budget cannot hold") {
        return "The model and this context don't fit on your graphics card together. Set it up with a smaller context, or stop what else is using the card.".to_owned();
    }
    log.to_owned()
}

/// Saves a server's start and stop commands (empty ones: harlo forgets it).
pub fn save_commands(url: &str, start: &str, stop: &str) {
    let v = json!({ "start": start.trim(), "stop": stop.trim() });
    if let Ok(db) = crate::db::Db::open(&crate::db::default_path()) {
        let _ = db.set(&server_key(url), &v.to_string());
    }
}

/// Saves the name and models of a server harlo set up (after its test), so
/// it's listed, and can go to agents, before any agent has it.
pub fn save_server(url: &str, name: &str, models: &[Value]) {
    let mut v = commands_of(url);
    v["name"] = json!(name);
    v["models"] = json!(models);
    if let Ok(db) = crate::db::Db::open(&crate::db::default_path()) {
        let _ = db.set(&server_key(url), &v.to_string());
    }
}

/// The servers harlo set up (it can start them): `(address, saved)`.
pub fn set_up_servers() -> Vec<(String, Value)> {
    let Ok(db) = crate::db::Db::open(&crate::db::default_path()) else { return Vec::new() };
    db.with_prefix("model_server.")
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, v)| Some((k.strip_prefix("model_server.")?.to_owned(), serde_json::from_str::<Value>(&v).ok()?)))
        .filter(|(_, v)| v["start"].as_str().is_some_and(|c| !c.is_empty()))
        .collect()
}

/// The engine of a server harlo set up (from its start command); "" for
/// one it didn't.
pub fn engine_of(url: &str) -> &'static str {
    let start = commands_of(url)["start"].as_str().unwrap_or_default().to_owned();
    if start.contains("llama-server") {
        "llama.cpp"
    } else if start.contains("ollama") {
        "Ollama"
    } else if start.contains("strata") {
        "Strata"
    } else if start.contains("basalt") {
        "Basalt"
    } else if start.contains("ninfer-serve") {
        "NInfer"
    } else if start.contains("vllm") {
        "vLLM"
    } else if start.is_empty() {
        ""
    } else {
        "Your command"
    }
}

/// What harlo keeps of a server it set up (its commands, name, models), to
/// put back if a setup over it doesn't finish.
pub fn server_record(url: &str) -> Value {
    commands_of(url)
}

pub fn restore_server_record(url: &str, v: &Value) {
    if let Ok(db) = crate::db::Db::open(&crate::db::default_path()) {
        let _ = db.set(&server_key(url), &v.to_string());
    }
}

/// The ports of this computer's model servers harlo knows (set up by it, or
/// in an agent's config), running or not: a new one doesn't take them.
pub fn ports_in_use() -> Vec<(u16, String)> {
    let port = |url: &str| url.split("://").nth(1)?.split('/').next()?.rsplit_once(':')?.1.parse::<u16>().ok();
    let mut out: Vec<(u16, String)> = Vec::new();
    for (url, v) in set_up_servers() {
        if let Some(p) = port(&url) {
            out.push((p, v["name"].as_str().filter(|n| !n.is_empty()).map_or_else(|| engine_of(&url).to_lowercase(), str::to_owned)));
        }
    }
    for k in model_servers::AGENTS {
        for s in model_servers::servers(k) {
            let host = s.base_url.split("://").nth(1).unwrap_or_default().split(['/', ':']).next().unwrap_or_default().to_owned();
            if ["127.0.0.1", "localhost"].contains(&host.as_str())
                && let Some(p) = port(&s.base_url)
                && !out.iter().any(|(q, _)| *q == p)
            {
                out.push((p, s.name));
            }
        }
    }
    out
}

/// A server's start command ("" for one harlo didn't set up).
pub fn start_command(url: &str) -> String {
    commands_of(url)["start"].as_str().unwrap_or_default().to_owned()
}

/// Stops a server harlo set up, however it's running (started by this
/// harlo or left by an earlier one), until it stops answering.
pub fn stop_any(url: &str) {
    match STARTED.lock().unwrap().as_mut().and_then(|m| m.remove(url)) {
        Some(child) => stop_group(child),
        None => stop_pids(&leftovers(url)),
    }
    // (Its own stop command too, for a server not harlo's process: Strata's.)
    if model_servers::answers(url) && commands_of(url)["stop"].as_str().is_some_and(|c| !c.is_empty()) {
        run_stop(url);
    }
    for _ in 0..40 {
        if !model_servers::answers(url) {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// The models harlo saved for a server it set up (after its tests).
pub fn saved_models_of(url: &str) -> Vec<model_servers::Found> {
    saved_models(&commands_of(url))
}

pub fn saved_models(v: &Value) -> Vec<model_servers::Found> {
    v["models"].as_array().into_iter().flatten().filter_map(|m| Some(model_servers::Found { id: m["id"].as_str()?.to_owned(), context: m["context"].as_u64(), reasoning: m["reasoning"] == true, ..Default::default() })).collect()
}

/// Starts a process from one thread that lives as long as harlo: the
/// parent-death signal goes with the thread that started it, and a worker
/// thread ending would stop the server.
fn spawn_from_launcher(cmd: std::process::Command) -> std::io::Result<std::process::Child> {
    type Job = (std::process::Command, std::sync::mpsc::Sender<std::io::Result<std::process::Child>>);
    static LAUNCHER: std::sync::OnceLock<Mutex<std::sync::mpsc::Sender<Job>>> = std::sync::OnceLock::new();
    let tx = LAUNCHER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("server-launcher".into())
            .spawn(move || {
                for (mut cmd, reply) in rx {
                    let _ = reply.send(cmd.spawn());
                }
            })
            .expect("a thread");
        Mutex::new(tx)
    });
    let (reply, answer) = std::sync::mpsc::channel();
    tx.lock().unwrap().send((cmd, reply)).map_err(|_| std::io::Error::other("the launcher stopped"))?;
    answer.recv().map_err(|_| std::io::Error::other("the launcher stopped"))?
}

/// Runs a server's start command in a shell from the home folder, its
/// output to its log, as one of harlo's (stopped when harlo quits).
pub fn launch(url: &str, command: &str) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let log = log_path(url);
    let _ = std::fs::create_dir_all(log.parent().unwrap_or(std::path::Path::new("/")));
    let out = std::fs::File::create(&log).map_err(|e| format!("Couldn't run it: {e}"))?;
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c")
        .arg(command)
        .current_dir(std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/")))
        .stdin(std::process::Stdio::null())
        .stdout(out.try_clone().map_err(|e| format!("Couldn't run it: {e}"))?)
        .stderr(out);
    let child = if DETACHED.load(std::sync::atomic::Ordering::Relaxed) {
        // A session of its own: it outlives what started it and the SSH
        // command that ran that (and is stopped as a group all the same).
        // SAFETY: only async-signal-safe calls between fork and exec.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        cmd.spawn().map_err(|e| format!("Couldn't run it: {e}"))?
    } else {
        cmd.process_group(0);
        // It stops when harlo does, however harlo ends (Ctrl+C, a crash):
        // the kernel tells it when its parent goes.
        // SAFETY: only async-signal-safe calls between fork and exec.
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        spawn_from_launcher(cmd).map_err(|e| format!("Couldn't run it: {e}"))?
    };
    STARTED.lock().unwrap().get_or_insert_with(HashMap::new).insert(url.to_owned(), child);
    Ok(())
}

/// Waits (blocking) until a launched server answers: whether harlo still
/// holds its process (a launcher like `strata launch` starts the server and
/// exits, which is fine), or why it didn't come up. `tick` gets its log's
/// last line now and then; `stop()` gives up.
pub fn wait_up(url: &str, limit: Duration, tick: &mut dyn FnMut(&str), stop: &dyn Fn() -> bool) -> Result<bool, String> {
    let deadline = Instant::now() + limit;
    let mut held = true;
    loop {
        if held {
            let exited = STARTED.lock().unwrap().as_mut().and_then(|m| m.get_mut(url)).map(|c| c.try_wait().ok().flatten());
            match exited {
                None => return Err("Stopped.".into()),
                Some(Some(status)) => {
                    STARTED.lock().unwrap().as_mut().map(|m| m.remove(url));
                    if !status.success() {
                        let tail = log_tail(url);
                        return Err(if tail.is_empty() { format!("It stopped ({status}).") } else { tail });
                    }
                    // A launcher: the server goes on without it.
                    held = false;
                }
                Some(None) => {}
            }
        }
        if model_servers::answers(url) {
            return Ok(held);
        }
        if stop() {
            return Err("Cancelled.".into());
        }
        if Instant::now() > deadline {
            return Err(format!("It hasn't answered in {} minutes.", limit.as_secs() / 60));
        }
        // (Its last line; of a progress bar redrawn in place with carriage
        // returns, as vLLM's, only its latest drawing.)
        let last = std::fs::read_to_string(log_path(url)).ok().and_then(|t| {
            let line = t.lines().rev().map(str::trim).find(|l| !l.is_empty())?.to_owned();
            line.split('\r').map(str::trim).rfind(|s| !s.is_empty()).map(str::to_owned)
        });
        if let Some(line) = last {
            tick(&line);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Starts a server harlo set up unless it answers already (blocking, until
/// it serves).
pub fn ensure_running(url: &str) -> Result<(), String> {
    if !needs_start(url) {
        return Ok(());
    }
    // (A faulted engine answers: stopped first.)
    if model_servers::answers(url) {
        stop_any(url);
    }
    let command = commands_of(url)["start"].as_str().unwrap_or_default().to_owned();
    launch(url, &command)?;
    wait_up(url, Duration::from_secs(15 * 60), &mut |_| {}, &|| false)?;
    for problem in AFTER_START.get().map(|f| f(url)).unwrap_or_default() {
        tracing::warn!("{problem}");
    }
    Ok(())
}

/// The servers harlo set up on an engine (its name, as `engine_of` says it).
pub fn servers_of(engine: &str) -> Vec<String> {
    set_up_servers().into_iter().map(|(url, _)| url).filter(|url| engine_of(url) == engine).collect()
}

/// A server's start and stop commands.
pub fn commands(url: &str) -> (String, String) {
    let v = commands_of(url);
    (v["start"].as_str().unwrap_or_default().to_owned(), v["stop"].as_str().unwrap_or_default().to_owned())
}

/// Whether a server has to be started before a message: it's down, or its
/// engine faulted.
pub fn needs_start(url: &str) -> bool {
    !model_servers::answers(url) || engine_faulted(url)
}

/// A Strata engine that hit a graphics-card fault, from the engine log its
/// launch named.
fn engine_faulted(url: &str) -> bool {
    if engine_of(url) != "Strata" {
        return false;
    }
    let said = std::fs::read_to_string(log_path(url)).unwrap_or_default();
    let Some(path) = said.lines().rev().find_map(|l| l.trim().strip_prefix("Engine log")?.trim().split("   ").next().map(str::to_owned)) else { return false };
    std::fs::read_to_string(path).is_ok_and(|log| crate::engines::strata_engine_faulted(&log))
}

pub fn stop_launched(url: &str) {
    if let Some(child) = STARTED.lock().unwrap().as_mut().and_then(|m| m.remove(url)) {
        stop_group(child);
    }
}

/// Stops the servers harlo started (it's quitting).
pub fn stop_started_servers() {
    for (_, child) in STARTED.lock().unwrap().take().unwrap_or_default() {
        stop_group(child);
    }
    // A server with its own stop command (Strata's launcher starts its
    // server and exits: nothing of harlo's to stop), still answering.
    for (url, v) in set_up_servers() {
        if v["stop"].as_str().is_some_and(|c| !c.is_empty()) && model_servers::answers(&url) {
            run_stop(&url);
        }
    }
    // And those an earlier harlo left running (it was stopped with Ctrl+C,
    // say, before servers went with it).
    let left: Vec<i32> = set_up_servers().iter().flat_map(|(u, _)| leftovers(u)).collect();
    if !left.is_empty() {
        stop_pids(&left);
    }
}

/// Runs a server's own stop command (blocking, a minute at most).
fn run_stop(url: &str) {
    let stop = commands_of(url)["stop"].as_str().unwrap_or_default().to_owned();
    let Ok(mut child) = std::process::Command::new("sh").arg("-c").arg(&stop).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() else { return };
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while child.try_wait().ok().flatten().is_none() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Servers harlo set up that an earlier harlo left running (it ended
/// before stopping them): one of harlo's engines serving the address's
/// port (`--port N`, or Ollama's `OLLAMA_HOST`).
pub fn leftovers(url: &str) -> Vec<i32> {
    let port = url.split("://").nth(1).and_then(|r| r.split('/').next()).and_then(|h| h.rsplit_once(':')).map(|(_, p)| p.to_owned()).unwrap_or_default();
    if port.parse::<u16>().is_err() {
        return Vec::new();
    }
    let engines = crate::engines::engines_dir();
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    dir.flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| *pid != std::process::id() as i32)
        .filter(|pid| {
            if !std::fs::read_link(format!("/proc/{pid}/exe")).is_ok_and(|exe| exe.starts_with(&engines)) {
                return false;
            }
            // A running harlo's (this one's, another window's): not left over.
            let parent = std::fs::read_to_string(format!("/proc/{pid}/status")).ok().and_then(|t| t.lines().find_map(|l| l.strip_prefix("PPid:")).and_then(|p| p.trim().parse::<i32>().ok()));
            if parent.is_some_and(|pp| std::fs::read_link(format!("/proc/{pp}/exe")).is_ok_and(|exe| exe.file_name().is_some_and(|n| n.to_string_lossy().starts_with("harlo")))) {
                return false;
            }
            let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
            let args: Vec<&[u8]> = cmdline.split(|b| *b == 0).collect();
            let host = format!(":{port}");
            args.windows(2).any(|w| w[0] == b"--port" && w[1] == port.as_bytes())
                || std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default().split(|b| *b == 0).any(|v| v.starts_with(b"OLLAMA_HOST=") && v.ends_with(host.as_bytes()))
        })
        .collect()
}

/// Asks processes to stop, then makes them.
pub fn stop_pids(pids: &[i32]) {
    for pid in pids {
        // SAFETY: a signal to a server process harlo set up.
        unsafe { libc::kill(*pid, libc::SIGTERM) };
    }
    for _ in 0..50 {
        if pids.iter().all(|p| !Path::new(&format!("/proc/{p}")).exists()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    for pid in pids {
        // SAFETY: as above.
        unsafe { libc::kill(*pid, libc::SIGKILL) };
    }
}

/// Asks the server's process group to stop, then makes it.
pub fn stop_group(mut child: std::process::Child) {
    let pid = child.id() as i32;
    // SAFETY: a signal to the group harlo started.
    unsafe { libc::kill(-pid, libc::SIGTERM) };
    for _ in 0..50 {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // SAFETY: as above.
    unsafe { libc::kill(-pid, libc::SIGKILL) };
    let _ = child.wait();
}

/// Removes a server (blocking): out of each agent that has it; one harlo set
/// up stopped and forgotten, its models deleted with `delete_files`.
/// `step(what, done, of)` says how far. What went wrong, whether OpenCode
/// was busy (its open sessions keep the server until they close), and what
/// it did ("Taken out of OpenCode and pi. Deleted 2 models from Ollama
/// (15.2 GB).").
/// Takes a server out of every agent here that has it (by its name and
/// address; `reread`: the agent picks the change up, false while it
/// can't); one harlo set up, no agent having it now, is stopped and
/// forgotten, and its model files deleted with `delete_files`. `step` says
/// how far. `(errors, an agent couldn't reread, what it did)`.
pub fn remove(name: &str, url: &str, delete_files: bool, step: &dyn Fn(&str, usize, usize), reread: &dyn Fn(HarnessKind) -> bool) -> (Vec<String>, bool, String) {
    let mut errors = Vec::new();
    let mut busy = false;
    let mut agents: Vec<&str> = Vec::new();
    let mut summary: Vec<String> = Vec::new();
    let gb = |b: u64| format!("{:.1} GB", b as f64 / 1e9);
    // (Ollama's models to delete: named before the agents' entries go.)
    let ollama = if delete_files && engine_of(url) == "Ollama" { ollama_models_of(url) } else { Vec::new() };
    for k in model_servers::AGENTS {
        if !model_servers::servers(k).iter().any(|s| s.name == name && s.base_url == url) {
            continue;
        }
        step(&format!("Taking it out of {}'s config", k.label()), 0, 0);
        match model_servers::remove(k, name) {
            Ok(()) => {
                busy |= !reread(k);
                agents.push(k.label());
            }
            Err(e) => errors.push(format!("{}: {e}", k.label())),
        }
    }
    if !agents.is_empty() {
        summary.push(format!("Taken out of {}.", agents.join(", ")));
    }
    // (Another machine's, used here: its forward goes with it.)
    model_servers::forget_unused_uses();
    // Set up by harlo, and no agent has it now: stopped and forgotten.
    let still = model_servers::AGENTS.iter().any(|k| model_servers::servers(*k).iter().any(|s| s.base_url == url));
    if still || !commands_of(url)["start"].as_str().is_some_and(|c| !c.is_empty()) {
        return (errors, busy, summary.join(" "));
    }
    // Ollama's models out of its store first (it must run for that).
    if !ollama.is_empty() {
        if !model_servers::answers(url) {
            step("Starting Ollama to delete its models", 0, 0);
        }
        match ensure_running(url) {
            Ok(()) => {
                let sizes = crate::engines::ollama_sizes(url);
                let (mut count, mut freed) = (0, 0);
                for (i, n) in ollama.iter().enumerate() {
                    step(&format!("Deleting {} from Ollama", n.strip_prefix("hf.co/").unwrap_or(n)), i, ollama.len());
                    match crate::engines::ollama_delete(url, n) {
                        Ok(()) => {
                            count += 1;
                            freed += sizes.iter().find(|(s, _)| s == n).map_or(0, |(_, b)| *b);
                        }
                        Err(e) => errors.push(e),
                    }
                }
                if count > 0 {
                    summary.push(format!("Deleted {count} {} from Ollama ({}).", if count == 1 { "model" } else { "models" }, gb(freed)));
                }
            }
            Err(e) => errors.push(format!("Ollama didn't start to delete its models: {e}")),
        }
    }
    let files = if delete_files { model_files(url) } else { Vec::new() };
    // (Empty folders go up to here: harlo's models folder, or Strata's.)
    let root = files_root(url);
    step("Stopping the server", 0, 0);
    stop_any(url);
    // (A llama.cpp router's preset, Basalt's settings file, too.)
    for flag in [" --models-preset ", " serve --config "] {
        if let Some(file) = start_command(url).split(flag).nth(1).map(shell_word).filter(|f| f.contains("/harlo/")) {
            let _ = std::fs::remove_file(file);
        }
    }
    save_commands(url, "", "");
    let _ = std::fs::remove_file(log_path(url));
    let (mut count, mut freed) = (0, 0);
    for (i, f) in files.iter().enumerate() {
        step(&format!("Deleting {}", f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()), i, files.len());
        let size = std::fs::metadata(f).map(|m| m.len()).unwrap_or(0);
        match std::fs::remove_file(f) {
            Ok(()) => {
                count += 1;
                freed += size;
            }
            Err(e) => errors.push(format!("{}: {e}", f.display())),
        }
    }
    summary.push("Stopped it; harlo forgot how to start it.".into());
    if count > 0 {
        summary.push(format!("Deleted {count} model {} ({}).", if count == 1 { "file" } else { "files" }, gb(freed)));
    }
    // Its folder too, once empty (the models folder stays).
    for f in &files {
        let mut dir = f.parent();
        while let Some(d) = dir.filter(|d| d.starts_with(&root) && *d != root) {
            if std::fs::remove_dir(d).is_err() {
                break;
            }
            dir = d.parent();
        }
    }
    (errors, busy, summary.join(" "))
}

/// The models an Ollama server serves in harlo: what its setup saved, and
/// what agents list for it (taken before they're taken out).
pub fn ollama_models_of(url: &str) -> Vec<String> {
    let mut names: Vec<String> = saved_models(&commands_of(url)).into_iter().map(|m| m.id).collect();
    for k in model_servers::AGENTS {
        for s in model_servers::servers(k).into_iter().filter(|s| s.base_url == url) {
            names.extend(s.models);
        }
    }
    names.sort();
    names.dedup();
    names
}

/// The servers harlo set up here, for the Model servers page: `[{
/// baseUrl, name, engine, models: [{ id, context, reasoning }], running,
/// log (its path, when there is one) }]` (blocking: asks each).
pub fn list() -> Vec<Value> {
    set_up_servers()
        .into_iter()
        .map(|(url, v)| {
            let name = v["name"].as_str().filter(|n| !n.is_empty()).map_or_else(|| engine_of(&url).to_lowercase(), str::to_owned);
            let log = Some(log_path(&url)).filter(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            json!({ "baseUrl": url, "name": name, "engine": engine_of(&url), "models": v["models"].as_array().cloned().unwrap_or_default(), "running": model_servers::answers(&url), "log": log })
        })
        .collect()
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_strata_sets_files_are_its_own() {
        use std::path::Path;
        let root = std::env::temp_dir().join(format!("harlo-strata-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (install, data) = (root.join("install"), root.join("data"));
        let touch = |p: &Path| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        };
        for f in [
            "models/deepseek-v4.1-flash-IST-DAS-2.00bpw/a.gguf",
            "models/deepseek-v4.1-flash-IST-DAS-2.25bpw/b.gguf",
            "models/deepseek-v4.1-flash-shared/engram.gguf",
            "models/IQ2_XS/q.gguf",
            "models/IQ3_S/partial.gguf",
            "models/mmproj-Qwen3.8-Flash-Next-BF16.gguf",
            "mtp/mtp-q2_0.gguf",
            "packs/deepseek-v4.1-flash-ist-das-2.00bpw/tokenizer/t.json",
            "packs/iq2_xs/ds.txt",
        ] {
            touch(&data.join(f));
        }
        let config = |file: &str, pack: &str| {
            let pack = data.join("packs").join(pack);
            std::fs::create_dir_all(&install).unwrap();
            std::fs::write(install.join(file), serde_json::json!({ "args": ["--pack", pack.to_string_lossy(), "--eos", "1"] }).to_string()).unwrap();
        };
        config("strata-deepseek-v4.1-flash-ist-das-2.00bpw.json", "deepseek-v4.1-flash-ist-das-2.00bpw");
        config("strata-iq2_xs.json", "iq2_xs");
        touch(&install.join("run-iq2_xs.sh"));
        let start = |family: &str, model: &str| format!("/x/strata launch --family {family} --model {model} --port 8080 --access local");
        let deepseek = super::strata_model_in(&install, &start("deepseek41", "ist-das-2.00bpw")).unwrap();
        let qwen = super::strata_model_in(&install, &start("qwen", "iq2_xs")).unwrap();
        assert_eq!(deepseek.data, data);
        assert!(super::strata_model_in(&install, "exec /x/llama-server --models-preset y").is_none());
        let names = |files: Vec<std::path::PathBuf>| -> Vec<String> {
            let mut n: Vec<String> = files.iter().map(|f| f.strip_prefix(&root).unwrap().to_string_lossy().into_owned()).collect();
            n.sort();
            n
        };
        // The last DeepSeek set: what the sets share too; never Qwen's or another set's.
        assert_eq!(
            names(super::strata_set_files(&deepseek, &[])),
            [
                "data/models/deepseek-v4.1-flash-IST-DAS-2.00bpw/a.gguf",
                "data/models/deepseek-v4.1-flash-shared/engram.gguf",
                "data/packs/deepseek-v4.1-flash-ist-das-2.00bpw/tokenizer/t.json",
                "install/strata-deepseek-v4.1-flash-ist-das-2.00bpw.json",
            ]
        );
        // The last Qwen set: its shared parts; not another size left half-downloaded.
        assert_eq!(
            names(super::strata_set_files(&qwen, &[])),
            ["data/models/IQ2_XS/q.gguf", "data/models/mmproj-Qwen3.8-Flash-Next-BF16.gguf", "data/mtp/mtp-q2_0.gguf", "data/packs/iq2_xs/ds.txt", "install/run-iq2_xs.sh", "install/strata-iq2_xs.json"]
        );
        // Another DeepSeek pack: the shared parts stay.
        touch(&data.join("packs/deepseek-v4.1-flash-ist-das-2.25bpw/ds.txt"));
        assert!(!names(super::strata_set_files(&deepseek, &[])).iter().any(|n| n.contains("shared")));
        // Another server on the same set: nothing goes.
        let again = super::strata_model_in(&install, &start("deepseek41", "ist-das-2.00bpw")).unwrap();
        assert!(super::strata_set_files(&deepseek, &[again]).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// The folder a server's model files are in, empty ones up to it removed
/// with them: harlo's models folder, or Strata's data folder.
pub fn files_root(url: &str) -> PathBuf {
    strata_model(&start_command(url)).map_or_else(models_dir, |m| m.data)
}
