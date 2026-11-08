//! `lsp`: a language server's answers about the code, where a text search
//! can't tell same-named things apart: where a symbol is defined, every
//! use of it, its implementations, who calls it, its type and docs.
//!
//! - One server per language and project root, started at the first call
//!   and kept for the session (ended at exit, with its process group). It
//!   runs in the sandbox as commands do; rust-analyzer is started without
//!   build scripts and proc macros, so a query never runs the project's
//!   code.
//! - The model names a line and the symbol on it (no columns to count).
//!   Files it asks about are sent to the server as they are on disk each
//!   time, so its own edits are seen.
//! - Offered when a server for one of the project's languages is
//!   installed and starts (looked for on the PATH and in the folders
//!   servers are usually installed in; `HARLO_AGENT_LSP=off`: never).
//!   harlo's settings show each language's (`status`). Measured: finding a method's
//!   callers among same-named ones took 87% fewer tokens with it; Rust
//!   tasks that didn't use it took about 2% more (its definition).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::ChildStdin;
use tokio::sync::{Mutex, OnceCell, oneshot, watch};

use crate::sandbox::Sandbox;
use crate::tools::Outcome;

/// A call's wait, starting the server included.
const TIMEOUT: Duration = Duration::from_secs(90);
const MAX_LOCATIONS: usize = 100;
const MAX_CHARS: usize = 16_000;
/// Of a source line shown beside a location.
const LINE_CHARS: usize = 160;

struct Lang {
    name: &'static str,
    exts: &'static [&'static str],
    /// The servers it can use, the first installed one wins.
    servers: &'static [&'static [&'static str]],
    /// Files that mark a project of it (and its root).
    markers: &'static [&'static str],
}

const LANGS: &[Lang] = &[
    Lang { name: "Rust", exts: &["rs"], servers: &[&["rust-analyzer"]], markers: &["Cargo.toml"] },
    Lang {
        name: "Python",
        exts: &["py", "pyi"],
        servers: &[&["basedpyright-langserver", "--stdio"], &["pyright-langserver", "--stdio"], &["pylsp"]],
        markers: &["pyproject.toml", "setup.py", "setup.cfg", "requirements.txt"],
    },
    Lang { name: "Go", exts: &["go"], servers: &[&["gopls"]], markers: &["go.mod"] },
    Lang {
        name: "TypeScript/JavaScript",
        exts: &["ts", "tsx", "js", "jsx", "mjs", "cjs"],
        servers: &[&["typescript-language-server", "--stdio"]],
        markers: &["tsconfig.json", "jsconfig.json", "package.json"],
    },
    Lang {
        name: "C/C++",
        exts: &["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx"],
        servers: &[&["clangd"]],
        markers: &["compile_commands.json", "compile_flags.txt", "CMakeLists.txt", ".clangd"],
    },
];

fn language_id(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "py" | "pyi" => "python",
        "go" => "go",
        "ts" => "typescript",
        "tsx" => "typescriptreact",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "javascriptreact",
        "c" | "h" => "c",
        _ => "cpp",
    }
}

fn lang_of(path: &Path) -> Option<(usize, &'static str)> {
    let ext = path.extension()?.to_str()?;
    LANGS.iter().position(|l| l.exts.contains(&ext)).map(|i| (i, language_id(ext)))
}

/// Folders servers are installed in, besides the PATH (an app started
/// from the desktop menu may not have the shell's).
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    if let Some(c) = std::env::var_os("CARGO_HOME") {
        dirs.push(PathBuf::from(c).join("bin"));
    }
    if let Some(g) = std::env::var_os("GOPATH") {
        dirs.extend(std::env::split_paths(&g).map(|g| g.join("bin")));
    }
    if let Some(n) = std::env::var_os("NPM_CONFIG_PREFIX").or_else(|| std::env::var_os("npm_config_prefix")) {
        dirs.push(PathBuf::from(n).join("bin"));
    }
    if !home.as_os_str().is_empty() {
        for d in [".cargo/bin", ".local/bin", "go/bin", ".npm-global/bin", ".local/share/npm/bin", ".bun/bin", ".volta/bin", ".local/share/pnpm"] {
            dirs.push(home.join(d));
        }
    }
    dirs.extend(["/usr/local/bin", "/usr/bin"].map(PathBuf::from));
    let mut seen = HashSet::new();
    dirs.retain(|d| d.is_absolute() && seen.insert(d.clone()));
    dirs
}

/// What a language's server is here.
#[derive(Clone, Debug, PartialEq)]
enum Found {
    /// Its command, the program by its whole path.
    Ready(Vec<String>),
    /// None of its servers is installed.
    Missing,
    /// One is there but doesn't start (its path, and what it said).
    Broken(PathBuf, String),
}

/// Whether `program` starts, asked its version (by the program beside it
/// that says it: Pyright's server only talks over its pipe). One still
/// running after a few seconds started. rustup's stand-in for a component
/// that isn't installed fails at once.
fn starts(program: &Path) -> Result<(), String> {
    let name = program.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let (check, args): (&str, &[&str]) = match name {
        "basedpyright-langserver" => ("basedpyright", &["--version"]),
        "pyright-langserver" => ("pyright", &["--version"]),
        "gopls" => ("gopls", &["version"]),
        n => (n, &["--version"]),
    };
    let check = program.with_file_name(check);
    if !check.is_file() {
        return Ok(());
    }
    let mut child = std::process::Command::new(&check)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("it can't be run ({e})"))?;
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                let mut said = String::new();
                if let Some(mut err) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut err, &mut said);
                }
                let said = said.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().trim_start_matches("error: ").to_owned();
                return Err(if said.is_empty() { format!("it fails to start ({status})") } else { said });
            }
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(());
            }
        }
    }
}

/// Looks for a language's servers: the first copy that's there and starts.
fn look_for(li: usize, dirs: &[PathBuf]) -> Found {
    let mut broken = None;
    for server in LANGS[li].servers {
        for program in dirs.iter().map(|d| d.join(server[0])).filter(|p| p.is_file()) {
            match starts(&program) {
                Ok(()) => {
                    let mut command = vec![program.to_string_lossy().into_owned()];
                    command.extend(server[1..].iter().map(|a| (*a).to_owned()));
                    return Found::Ready(command);
                }
                Err(why) => {
                    broken.get_or_insert((program, why));
                }
            }
        }
    }
    broken.map_or(Found::Missing, |(p, why)| Found::Broken(p, why))
}

/// A language's server command, looked for once (when a project of it
/// first asks).
fn installed(li: usize) -> Option<&'static Vec<String>> {
    static FOUND: [OnceLock<Found>; 5] = [const { OnceLock::new() }; 5];
    const _: () = assert!(LANGS.len() == 5);
    match FOUND[li].get_or_init(|| look_for(li, &search_dirs())) {
        Found::Ready(command) => Some(command),
        _ => None,
    }
}

/// How to install a language's server (`manager`: the system's package
/// manager, for the one that comes with it).
fn install_hint(li: usize, manager: Option<&str>) -> String {
    match LANGS[li].name {
        "Rust" => "rustup component add rust-analyzer".into(),
        "Python" => "pipx install basedpyright".into(),
        "Go" => "go install golang.org/x/tools/gopls@latest".into(),
        "TypeScript/JavaScript" => "npm install -g typescript-language-server typescript".into(),
        _ => match manager {
            Some("pacman") => "sudo pacman -S clang".into(),
            Some("apt") => "sudo apt install clangd".into(),
            Some("dnf") => "sudo dnf install clang-tools-extra".into(),
            Some("zypper") => "sudo zypper install clang".into(),
            Some("brew") => "brew install llvm".into(),
            _ => "Install clangd with your package manager".into(),
        },
    }
}

/// Each language's server as it is here, for harlo's settings
/// (`harlo-agent lsp-status`): `[{ language, servers, state: "ready" |
/// "missing" | "broken", path, why, install }]`.
pub fn status() -> Value {
    let dirs = search_dirs();
    let manager = ["pacman", "apt", "dnf", "zypper", "brew"].into_iter().find(|m| dirs.iter().any(|d| d.join(m).is_file()));
    let list: Vec<Value> = (0..LANGS.len())
        .map(|li| {
            let (state, path, why) = match look_for(li, &dirs) {
                Found::Ready(c) => ("ready", c[0].clone(), String::new()),
                Found::Missing => ("missing", String::new(), String::new()),
                Found::Broken(p, why) => ("broken", p.to_string_lossy().into_owned(), why),
            };
            let servers: Vec<&str> = LANGS[li].servers.iter().map(|s| s[0]).collect();
            json!({ "language": LANGS[li].name, "servers": servers, "state": state, "path": path, "why": why, "install": install_hint(li, manager) })
        })
        .collect();
    Value::Array(list)
}

/// Whether the tool is offered in `cwd`: a server installed for a language
/// the project has (a marker file at its top), unless turned off.
pub fn offered(cwd: &Path) -> bool {
    std::env::var("HARLO_AGENT_LSP").map_or(true, |v| v.trim() != "off") && !languages_here(cwd).is_empty()
}

/// The languages with a server installed that `cwd`'s project has.
fn languages_here(cwd: &Path) -> Vec<&'static str> {
    (0..LANGS.len()).filter(|&li| LANGS[li].markers.iter().any(|m| cwd.join(m).exists()) && installed(li).is_some()).map(|li| LANGS[li].name).collect()
}

pub fn spec() -> Value {
    json!({
        "type": "function", "name": "lsp", "strict": false,
        "description": "Ask the project's language server about a symbol, when a text search can't tell same-named things apart (a method several types have, a name that's shadowed or aliased): where it's defined, every reference to it, its implementations, the functions that call it, or its type and docs. Give the file, the line (from 1) and the symbol's name as it's written on that line. Answers are file:line:column with the line's text. For plain lookups, grep and read are quicker.",
        "parameters": { "type": "object", "properties": {
            "operation": { "type": "string", "enum": ["definition", "references", "implementations", "callers", "hover"] },
            "path": { "type": "string" },
            "line": { "type": "integer", "description": "From 1" },
            "symbol": { "type": "string", "description": "The name on that line (its first whole-word use there, unless you give occurrence)" },
            "occurrence": { "type": "integer", "description": "Which use of it on the line, from 1 (1 by default)" },
        }, "required": ["operation", "path", "line", "symbol"], "additionalProperties": false },
    })
}

/// The servers this session started, by language and root.
#[derive(Default)]
pub struct Servers {
    running: StdMutex<HashMap<(usize, PathBuf), Arc<OnceCell<Result<Arc<Server>, String>>>>>,
}

impl Servers {
    /// `path`: the file asked about, made whole (and allowed to be read).
    pub async fn call(&self, args: &Value, path: PathBuf, cwd: &Path, sandbox: Option<&Sandbox>) -> Outcome {
        let fail = |t: String| Outcome::new(t, true);
        let s = |k: &str| args[k].as_str().unwrap_or_default().trim();
        let op = s("operation");
        if !["definition", "references", "implementations", "callers", "hover"].contains(&op) {
            return fail("Give operation: definition, references, implementations, callers or hover.".into());
        }
        if let Some(why) = sandbox.and_then(|sb| sb.broken.as_ref()) {
            return fail(format!("The language server can't start: the sandbox can't start here ({why})."));
        }
        let Some((li, language)) = lang_of(&path) else {
            return fail(format!("No language server for {}: there are servers for {}.", path.display(), LANGS.iter().map(|l| l.name).collect::<Vec<_>>().join(", ")));
        };
        let Some(command) = installed(li) else {
            let names: Vec<&str> = LANGS[li].servers.iter().map(|s| s[0]).collect();
            return fail(format!("No {} language server is installed ({}): use grep and read.", LANGS[li].name, names.join(", ")));
        };
        let text = match std::fs::read(&path) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => return fail(format!("Can't read {}: {e}", path.display())),
        };
        let line = args["line"].as_u64().unwrap_or(0) as usize;
        let Some(line_text) = text.lines().nth(line.wrapping_sub(1)) else {
            return fail(format!("{} has {} lines: give a line from 1.", path.display(), text.lines().count()));
        };
        let Some(column) = column_of(line_text, s("symbol"), args["occurrence"].as_u64().unwrap_or(1).max(1) as usize) else {
            return fail(format!("{:?} isn't on line {line} of {}: {}", s("symbol"), path.display(), line_text.trim()));
        };
        let deadline = Instant::now() + TIMEOUT;
        let root = root_of(&path, cwd, LANGS[li].markers);
        let server = match self.server(li, &root, command, sandbox).await {
            Ok(s) => s,
            Err(e) => return fail(e),
        };
        match server.ask(op, &path, language, &text, line - 1, line_text, column, deadline).await {
            Ok(answer) => Outcome::new(answer.render(cwd, op), false),
            Err(e) => {
                if server.exited() {
                    self.running.lock().unwrap().remove(&(li, root));
                }
                fail(e)
            }
        }
    }

    /// The server for a language and root: running, or started.
    async fn server(&self, li: usize, root: &Path, command: &[String], sandbox: Option<&Sandbox>) -> Result<Arc<Server>, String> {
        let key = (li, root.to_path_buf());
        let cell = {
            let mut running = self.running.lock().unwrap();
            if running.get(&key).and_then(|c| c.get()).is_some_and(|r| r.as_ref().is_ok_and(|s| s.exited())) {
                running.remove(&key);
            }
            running.entry(key).or_default().clone()
        };
        cell.get_or_init(|| Server::start(li, root, command, sandbox)).await.clone()
    }
}

/// The root a file's server works in: the topmost folder with a marker
/// between the file and the project's top (`cwd`), else the nearest one
/// above it, else the project's top.
fn root_of(file: &Path, cwd: &Path, markers: &[&str]) -> PathBuf {
    let has = |d: &Path| markers.iter().any(|m| d.join(m).exists());
    let mut best = None;
    for d in file.ancestors().skip(1) {
        if has(d) {
            best = Some(d.to_path_buf());
            if !d.starts_with(cwd) {
                break;
            }
        }
        if d == cwd {
            break;
        }
    }
    best.unwrap_or_else(|| if file.starts_with(cwd) { cwd.to_path_buf() } else { file.parent().unwrap_or(cwd).to_path_buf() })
}

/// The character column (from 0) of the `nth` whole-word use of `symbol`.
fn column_of(line: &str, symbol: &str, nth: usize) -> Option<usize> {
    if symbol.is_empty() {
        return None;
    }
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let chars: Vec<char> = line.chars().collect();
    let want: Vec<char> = symbol.chars().collect();
    let found: Vec<usize> = (0..chars.len().saturating_sub(want.len() - 1))
        .filter(|&i| chars[i..i + want.len()] == want[..])
        .filter(|&i| (i == 0 || !word(chars[i - 1]) || !word(want[0])) && chars.get(i + want.len()).is_none_or(|c| !word(*c) || !word(want[want.len() - 1])))
        .collect();
    found.get(nth - 1).copied()
}

/// Positions as the server counts them.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Encoding {
    Utf8,
    Utf16,
}

impl Encoding {
    fn to_server(self, line: &str, chars: usize) -> usize {
        let before = line.chars().take(chars);
        match self {
            Encoding::Utf8 => before.map(char::len_utf8).sum(),
            Encoding::Utf16 => before.map(char::len_utf16).sum(),
        }
    }

    fn from_server(self, line: &str, units: usize) -> usize {
        let mut n = 0;
        let mut chars = 0;
        for c in line.chars() {
            if n >= units {
                break;
            }
            n += if self == Encoding::Utf8 { c.len_utf8() } else { c.len_utf16() };
            chars += 1;
        }
        chars
    }
}

#[derive(Clone, Default)]
struct Busy {
    /// rust-analyzer's own word on whether it's done (None: it hasn't said).
    quiescent: Option<bool>,
    /// Work it said it started and hasn't ended.
    progress: usize,
    changed: Option<Instant>,
    exited: bool,
}

type Pending = Arc<StdMutex<HashMap<i64, oneshot::Sender<Result<Value, String>>>>>;

pub struct Server {
    name: String,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    next: AtomicI64,
    busy: watch::Receiver<Busy>,
    started: Instant,
    encoding: Encoding,
    caps: Value,
    /// Documents sent: their version and text.
    docs: Mutex<HashMap<PathBuf, (i64, String)>>,
    pgid: i32,
    stderr: Arc<StdMutex<String>>,
    /// rust-analyzer says when it's done; others are watched for progress.
    says_when_done: bool,
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.pgid > 0 {
            // SAFETY: killpg(2) on the process group this server leads.
            unsafe { libc::killpg(self.pgid, libc::SIGKILL) };
            crate::jobs::forget(self.pgid);
        }
    }
}

async fn send(stdin: &Mutex<ChildStdin>, message: &Value) -> Result<(), String> {
    let body = message.to_string();
    let mut w = stdin.lock().await;
    w.write_all(format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.map_err(|e| format!("the server stopped ({e})"))?;
    w.flush().await.map_err(|e| format!("the server stopped ({e})"))
}

/// The settings a server asks for (rust-analyzer: nothing that runs the
/// project's code, files watched by the server itself).
fn settings(name: &str) -> Value {
    match name {
        "rust-analyzer" => json!({
            "checkOnSave": false,
            "cargo": { "buildScripts": { "enable": false }, "extraEnv": { "CARGO_NET_OFFLINE": "true" } },
            "procMacro": { "enable": false },
            "files": { "watcher": "server" },
        }),
        _ => Value::Null,
    }
}

impl Server {
    async fn start(li: usize, root: &Path, command: &[String], sandbox: Option<&Sandbox>) -> Result<Arc<Server>, String> {
        let name = command[0].clone();
        let line = format!("exec {}", command.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" "));
        let mut cmd = crate::tools::bash::command(&line, root, sandbox);
        cmd.stdin(Stdio::piped()).kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| format!("Couldn't start {name}: {e}"))?;
        let pgid = child.id().map_or(0, |p| p as i32);
        if pgid > 0 {
            crate::jobs::end_at_exit(pgid);
        }
        let (Some(stdin), Some(stdout), Some(mut err)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
            return Err(format!("Couldn't talk to {name}."));
        };
        let stdin = Arc::new(Mutex::new(stdin));
        let pending: Pending = Arc::default();
        let (tx, busy) = watch::channel(Busy::default());
        let stderr = Arc::new(StdMutex::new(String::new()));
        {
            let stderr = stderr.clone();
            tokio::spawn(async move {
                let mut buf = vec![0; 8192];
                while let Ok(n) = err.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    let mut s = stderr.lock().unwrap();
                    s.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if s.len() > 8192 {
                        let cut = s.len() - 4096;
                        let cut = (cut..s.len()).find(|i| s.is_char_boundary(*i)).unwrap_or(cut);
                        s.drain(..cut);
                    }
                }
            });
        }
        tokio::spawn(read_loop(stdout, stdin.clone(), pending.clone(), tx, name.clone(), root.to_path_buf(), child));
        let mut server = Server {
            says_when_done: name == "rust-analyzer",
            name,
            stdin,
            pending,
            next: AtomicI64::new(1),
            busy,
            started: Instant::now(),
            encoding: Encoding::Utf16,
            caps: Value::Null,
            docs: Mutex::default(),
            pgid,
            stderr,
        };
        let root_uri = uri(root);
        let init = json!({
            "processId": std::process::id(),
            "clientInfo": { "name": "harlo-agent" },
            "rootUri": root_uri,
            "rootPath": root,
            "workspaceFolders": [{ "uri": root_uri, "name": root.file_name().and_then(|n| n.to_str()).unwrap_or("project") }],
            "initializationOptions": settings(&server.name),
            "capabilities": {
                "general": { "positionEncodings": ["utf-8", "utf-16"] },
                "window": { "workDoneProgress": true },
                "workspace": { "configuration": true, "workspaceFolders": true },
                "textDocument": {
                    "synchronization": { "dynamicRegistration": false },
                    "definition": { "linkSupport": true },
                    "implementation": { "linkSupport": true },
                    "references": {},
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "callHierarchy": {},
                    "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
                },
                "experimental": { "serverStatusNotification": true },
            },
        });
        let deadline = Instant::now() + TIMEOUT;
        let reply = server.request("initialize", init, deadline).await.map_err(|e| format!("{} didn't start: {e}{}", server.name, server.last_words()))?;
        server.caps = reply["capabilities"].clone();
        if server.caps["positionEncoding"] == "utf-8" {
            server.encoding = Encoding::Utf8;
        }
        send(&server.stdin, &json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} })).await?;
        let _ = li;
        Ok(Arc::new(server))
    }

    fn exited(&self) -> bool {
        self.busy.borrow().exited
    }

    /// The end of what the server wrote to stderr, for a failure.
    fn last_words(&self) -> String {
        let s = self.stderr.lock().unwrap();
        let tail: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
        let tail = tail[tail.len().saturating_sub(3)..].join(" / ");
        if tail.is_empty() { String::new() } else { format!(" ({})", tail.chars().take(400).collect::<String>()) }
    }

    async fn request(&self, method: &str, params: Value, deadline: Instant) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        send(&self.stdin, &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })).await?;
        match tokio::time::timeout(deadline.saturating_duration_since(Instant::now()), rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err(format!("{} stopped{}", self.name, self.last_words())),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                let _ = send(&self.stdin, &json!({ "jsonrpc": "2.0", "method": "$/cancelRequest", "params": { "id": id } })).await;
                Err(format!("{} didn't answer in {} s.", self.name, TIMEOUT.as_secs()))
            }
        }
    }

    /// Waits until the server has finished what it's doing (indexing).
    async fn settled(&self, deadline: Instant) -> Result<(), String> {
        let mut busy = self.busy.clone();
        loop {
            let b = busy.borrow_and_update().clone();
            if b.exited {
                return Err(format!("{} stopped{}", self.name, self.last_words()));
            }
            let quiet_for = b.changed.map_or(self.started.elapsed(), |c| c.elapsed());
            let done = match b.quiescent {
                Some(q) => q,
                None if self.says_when_done => false,
                None => b.progress == 0 && quiet_for >= Duration::from_millis(400) && self.started.elapsed() >= Duration::from_millis(800),
            };
            if done {
                return Ok(());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!("{} is still reading the project ({} s so far): ask again in a while, or use grep and read meanwhile.", self.name, self.started.elapsed().as_secs()));
            }
            let _ = tokio::time::timeout(left.min(Duration::from_millis(200)), busy.changed()).await;
        }
    }

    /// Sends `path` as it is now (and any other document sent before that
    /// changed on disk since).
    async fn sync(&self, path: &Path, language: &str, text: &str) -> Result<(), String> {
        let mut docs = self.docs.lock().await;
        let others: Vec<PathBuf> = docs.keys().filter(|p| p.as_path() != path).cloned().collect();
        for p in others {
            match std::fs::read(&p) {
                Ok(b) => {
                    let now = String::from_utf8_lossy(&b).into_owned();
                    let (version, old) = docs.get_mut(&p).unwrap();
                    if *old != now {
                        *version += 1;
                        send(&self.stdin, &json!({ "jsonrpc": "2.0", "method": "textDocument/didChange", "params": { "textDocument": { "uri": uri(&p), "version": *version }, "contentChanges": [{ "text": now }] } })).await?;
                        *old = now;
                    }
                }
                Err(_) => {
                    docs.remove(&p);
                    send(&self.stdin, &json!({ "jsonrpc": "2.0", "method": "textDocument/didClose", "params": { "textDocument": { "uri": uri(&p) } } })).await?;
                }
            }
        }
        match docs.get_mut(path) {
            Some((version, old)) if old != text => {
                *version += 1;
                send(&self.stdin, &json!({ "jsonrpc": "2.0", "method": "textDocument/didChange", "params": { "textDocument": { "uri": uri(path), "version": *version }, "contentChanges": [{ "text": text }] } })).await?;
                *old = text.to_owned();
            }
            Some(_) => {}
            None => {
                send(&self.stdin, &json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": { "textDocument": { "uri": uri(path), "languageId": language, "version": 1, "text": text } } })).await?;
                docs.insert(path.to_path_buf(), (1, text.to_owned()));
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn ask(&self, op: &str, path: &Path, language: &str, text: &str, line: usize, line_text: &str, column: usize, deadline: Instant) -> Result<Answer, String> {
        let provider = match op {
            "definition" => "definitionProvider",
            "references" => "referencesProvider",
            "implementations" => "implementationProvider",
            "callers" => "callHierarchyProvider",
            _ => "hoverProvider",
        };
        if self.caps[provider].is_null() || self.caps[provider] == false {
            return Err(format!("{} can't answer {op}.", self.name));
        }
        self.sync(path, language, text).await?;
        let at = json!({ "textDocument": { "uri": uri(path) }, "position": { "line": line, "character": self.encoding.to_server(line_text, column) } });
        // (Asked again once when the server says the code changed under it.)
        for tries in 0.. {
            self.settled(deadline).await?;
            let result = match op {
                "definition" => self.request("textDocument/definition", at.clone(), deadline).await,
                "implementations" => self.request("textDocument/implementation", at.clone(), deadline).await,
                "references" => {
                    let mut p = at.clone();
                    p["context"] = json!({ "includeDeclaration": true });
                    self.request("textDocument/references", p, deadline).await
                }
                "hover" => self.request("textDocument/hover", at.clone(), deadline).await,
                _ => match self.request("textDocument/prepareCallHierarchy", at.clone(), deadline).await {
                    Ok(items) => match items.as_array().and_then(|a| a.first()) {
                        Some(item) => self.request("callHierarchy/incomingCalls", json!({ "item": item }), deadline).await.map(|calls| json!({ "item": item, "calls": calls })),
                        None => Ok(Value::Null),
                    },
                    Err(e) => Err(e),
                },
            };
            match result {
                Err(e) if tries == 0 && e.contains("content modified") => continue,
                Err(e) => return Err(format!("{}: {e}", self.name)),
                Ok(mut v) => {
                    if op == "callers" {
                        self.name_containers(&mut v["calls"], deadline).await;
                    }
                    return Ok(Answer { value: v, encoding: self.encoding, op: op.to_owned() });
                }
            }
        }
        unreachable!()
    }
}

impl Server {
    /// Sets each caller's `container` (the type or class its function is
    /// in, from its file's outline), so a method reads as `Type::method`.
    async fn name_containers(&self, calls: &mut Value, deadline: Instant) {
        if self.caps["documentSymbolProvider"].is_null() || self.caps["documentSymbolProvider"] == false {
            return;
        }
        let Some(calls) = calls.as_array_mut() else { return };
        let mut outlines: HashMap<String, Value> = HashMap::new();
        for c in calls.iter_mut().take(MAX_LOCATIONS) {
            let Some(u) = c["from"]["uri"].as_str().map(str::to_owned) else { continue };
            if !outlines.contains_key(&u) {
                let outline = self.request("textDocument/documentSymbol", json!({ "textDocument": { "uri": u } }), deadline).await.unwrap_or(Value::Null);
                outlines.insert(u.clone(), outline);
            }
            let at = &c["from"]["selectionRange"]["start"];
            let (line, col) = (at["line"].as_u64().unwrap_or(0), at["character"].as_u64().unwrap_or(0));
            if let Some(name) = container(&outlines[&u], line, col) {
                c["from"]["container"] = json!(name);
            }
        }
    }
}

/// The name of what holds the symbol at `line`/`col` in an outline (nested
/// document symbols): the nearest type, class or impl around it.
fn container(outline: &Value, line: u64, col: u64) -> Option<String> {
    let inside = |r: &Value| {
        let (s, e) = (&r["start"], &r["end"]);
        let pos = (line, col);
        let start = (s["line"].as_u64().unwrap_or(0), s["character"].as_u64().unwrap_or(0));
        let end = (e["line"].as_u64().unwrap_or(0), e["character"].as_u64().unwrap_or(0));
        start <= pos && pos <= end
    };
    let clean = |n: &str| -> Option<String> {
        let n = n.strip_prefix("impl ").unwrap_or(n);
        // (`impl Trait for Type`: the type's.)
        let n = n.rsplit_once(" for ").map_or(n, |(_, ty)| ty);
        let n = n.split('<').next().unwrap_or(n).trim();
        (!n.is_empty()).then(|| n.to_owned())
    };
    // (A flat outline: each symbol names what holds it.)
    if let Some(flat) = outline.as_array().filter(|a| a.first().is_some_and(|s| s.get("location").is_some())) {
        let sym = flat.iter().filter(|s| inside(&s["location"]["range"])).last()?;
        return sym["containerName"].as_str().and_then(clean);
    }
    let mut held = None;
    let mut level = outline.as_array()?;
    loop {
        let Some(sym) = level.iter().find(|s| inside(&s["range"])) else { break };
        let kids = sym["children"].as_array();
        // (The symbol itself: what holds it is the name before.)
        if inside(&sym["selectionRange"]) && kids.is_none_or(|k| !k.iter().any(|c| inside(&c["range"]))) {
            break;
        }
        // Struct, class, interface, enum, namespace, module, object.
        if matches!(sym["kind"].as_u64(), Some(5 | 11 | 10 | 23 | 3 | 2 | 19)) || sym["name"].as_str().is_some_and(|n| n.starts_with("impl ")) {
            if let Some(n) = clean(sym["name"].as_str().unwrap_or_default()) {
                held = Some(n);
            }
        }
        match kids {
            Some(k) => level = k,
            None => break,
        }
    }
    held
}

/// Reads the server's messages: answers to our requests, its own requests
/// (answered plainly), and what it says about its work.
async fn read_loop(out: tokio::process::ChildStdout, stdin: Arc<Mutex<ChildStdin>>, pending: Pending, busy: watch::Sender<Busy>, name: String, root: PathBuf, mut child: tokio::process::Child) {
    let mut r = BufReader::new(out);
    let mut active: HashSet<String> = HashSet::new();
    loop {
        let mut length = None;
        loop {
            let mut header = String::new();
            match r.read_line(&mut header).await {
                Ok(0) | Err(_) => {
                    length = None;
                    break;
                }
                Ok(_) => {}
            }
            let h = header.trim();
            if h.is_empty() {
                if length.is_some() {
                    break;
                }
                continue;
            }
            if let Some(v) = h.strip_prefix("Content-Length:").or_else(|| h.strip_prefix("content-length:")) {
                length = v.trim().parse::<usize>().ok();
            }
        }
        let Some(n) = length else { break };
        let mut body = vec![0; n];
        if r.read_exact(&mut body).await.is_err() {
            break;
        }
        let Ok(m) = serde_json::from_slice::<Value>(&body) else { continue };
        let method = m["method"].as_str();
        match (method, m.get("id")) {
            (None, Some(id)) => {
                let Some(id) = id.as_i64() else { continue };
                if let Some(tx) = pending.lock().unwrap().remove(&id) {
                    let _ = tx.send(match m.get("error") {
                        Some(e) => Err(e["message"].as_str().unwrap_or("failed").to_owned()),
                        None => Ok(m["result"].clone()),
                    });
                }
            }
            (Some(method), Some(id)) => {
                let result = match method {
                    "workspace/configuration" => {
                        let items = m["params"]["items"].as_array().map_or(0, Vec::len);
                        let s = settings(&name);
                        Value::Array(m["params"]["items"].as_array().into_iter().flatten().take(items).map(|i| if i["section"].as_str().is_none_or(|x| x.starts_with(&name)) { s.clone() } else { Value::Null }).collect())
                    }
                    "workspace/workspaceFolders" => json!([{ "uri": uri(&root), "name": "project" }]),
                    _ => Value::Null,
                };
                let _ = send(&stdin, &json!({ "jsonrpc": "2.0", "id": id, "result": result })).await;
            }
            (Some("$/progress"), None) => {
                let token = m["params"]["token"].to_string();
                match m["params"]["value"]["kind"].as_str() {
                    Some("begin") => {
                        active.insert(token);
                    }
                    Some("end") => {
                        active.remove(&token);
                    }
                    _ => {}
                }
                let n = active.len();
                busy.send_modify(|b| {
                    b.progress = n;
                    b.changed = Some(Instant::now());
                });
            }
            (Some("experimental/serverStatus"), None) => {
                let q = m["params"]["quiescent"].as_bool();
                busy.send_modify(|b| {
                    b.quiescent = q;
                    b.changed = Some(Instant::now());
                });
            }
            _ => {}
        }
    }
    for (_, tx) in pending.lock().unwrap().drain() {
        let _ = tx.send(Err("it stopped".into()));
    }
    busy.send_modify(|b| b.exited = true);
    let _ = child.wait().await;
}

fn quote(a: &str) -> String {
    if a.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=".contains(c)) { a.to_owned() } else { format!("'{}'", a.replace('\'', "'\\''")) }
}

fn uri(path: &Path) -> String {
    let mut s = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(PathBuf::from(String::from_utf8_lossy(&out).into_owned()))
}

struct Answer {
    value: Value,
    encoding: Encoding,
    op: String,
}

/// A place in a file: its path, line (from 0) and server column.
struct Place {
    path: PathBuf,
    line: usize,
    column: usize,
}

fn place(v: &Value) -> Option<Place> {
    let (u, range) = match v.get("targetUri") {
        Some(u) => (u, v.get("targetSelectionRange").or(v.get("targetRange"))?),
        None => (v.get("uri")?, v.get("range")?),
    };
    Some(Place { path: path_of(u.as_str()?)?, line: range["start"]["line"].as_u64()? as usize, column: range["start"]["character"].as_u64()? as usize })
}

impl Answer {
    fn render(&self, cwd: &Path, op: &str) -> String {
        let mut files: HashMap<PathBuf, Vec<String>> = HashMap::new();
        let mut line_of = |p: &Path, n: usize| -> String {
            let lines = files.entry(p.to_path_buf()).or_insert_with(|| std::fs::read(p).map(|b| String::from_utf8_lossy(&b).lines().map(str::to_owned).collect()).unwrap_or_default());
            lines.get(n).cloned().unwrap_or_default()
        };
        let show = |p: &Path| p.strip_prefix(cwd).map_or_else(|_| p.display().to_string(), |r| r.display().to_string());
        let mut loc = |pl: &Place| -> String {
            let text = line_of(&pl.path, pl.line);
            let col = self.encoding.from_server(&text, pl.column) + 1;
            let t = text.trim();
            let t = if t.chars().count() > LINE_CHARS { format!("{}…", t.chars().take(LINE_CHARS).collect::<String>()) } else { t.to_owned() };
            format!("{}:{}:{}: {t}", show(&pl.path), pl.line + 1, col)
        };
        let v = &self.value;
        let out = match self.op.as_str() {
            "hover" => {
                let c = &v["contents"];
                let text = match c {
                    Value::String(s) => s.clone(),
                    Value::Array(a) => a.iter().map(|x| x.as_str().map(str::to_owned).unwrap_or_else(|| x["value"].as_str().unwrap_or_default().to_owned())).collect::<Vec<_>>().join("\n\n"),
                    _ => c["value"].as_str().unwrap_or_default().to_owned(),
                };
                if text.trim().is_empty() { "Nothing to show for that symbol.".to_owned() } else { text.trim().to_owned() }
            }
            "callers" => {
                let calls: Vec<&Value> = v["calls"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
                if v.is_null() || calls.is_empty() {
                    "No callers found.".to_owned()
                } else {
                    let mut rows = Vec::new();
                    for c in calls.iter().take(MAX_LOCATIONS) {
                        let from = &c["from"];
                        let name = match from["container"].as_str() {
                            Some(c) => format!("{c}::{}", from["name"].as_str().unwrap_or("?")),
                            None => from["name"].as_str().unwrap_or("?").to_owned(),
                        };
                        let detail = from["detail"].as_str().map(|d| format!(" ({})", d.trim())).unwrap_or_default();
                        let Some(path) = from["uri"].as_str().and_then(path_of) else { continue };
                        let def_line = from["selectionRange"]["start"]["line"].as_u64().unwrap_or(0) as usize;
                        rows.push(format!("{name}{detail}, {}:{}", show(&path), def_line + 1));
                        for r in c["fromRanges"].as_array().into_iter().flatten() {
                            let pl = Place { path: path.clone(), line: r["start"]["line"].as_u64().unwrap_or(0) as usize, column: r["start"]["character"].as_u64().unwrap_or(0) as usize };
                            rows.push(format!("  calls it at {}", loc(&pl)));
                        }
                    }
                    let more = calls.len().saturating_sub(MAX_LOCATIONS);
                    let mut s = format!("{} caller{} of {}:\n{}", calls.len(), if calls.len() == 1 { "" } else { "s" }, v["item"]["name"].as_str().unwrap_or("it"), rows.join("\n"));
                    if more > 0 {
                        s.push_str(&format!("\n[{more} more not shown]"));
                    }
                    s
                }
            }
            _ => {
                let all: Vec<Place> = match v {
                    Value::Array(a) => a.iter().filter_map(place).collect(),
                    Value::Null => Vec::new(),
                    one => place(one).into_iter().collect(),
                };
                if all.is_empty() {
                    format!("No {} found.", if op == "definition" { "definition" } else { op })
                } else {
                    let mut rows: Vec<String> = all.iter().take(MAX_LOCATIONS).map(&mut loc).collect();
                    if all.len() > MAX_LOCATIONS {
                        rows.push(format!("[{} more not shown]", all.len() - MAX_LOCATIONS));
                    }
                    let head = if op == "references" { format!("{} reference{}:\n", all.len(), if all.len() == 1 { "" } else { "s" }) } else { String::new() };
                    head + &rows.join("\n")
                }
            }
        };
        if out.chars().count() > MAX_CHARS { format!("{}\n[Cut at {MAX_CHARS} characters]", out.chars().take(MAX_CHARS).collect::<String>()) } else { out }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_symbol_on_its_line() {
        let line = "    let ledger = &mut self.ledger; ledger.post(e); repost(1);";
        assert_eq!(column_of(line, "ledger", 1), Some(8));
        assert_eq!(column_of(line, "ledger", 3), Some(35));
        assert_eq!(column_of(line, "post", 1), Some(42), "not inside repost");
        assert_eq!(column_of(line, "post", 2), None);
        assert_eq!(column_of(line, "nope", 1), None);
        assert_eq!(column_of("a::b", "::", 1), Some(1));
    }

    #[test]
    fn counts_columns_as_the_server_does() {
        let line = "é😀x = 1";
        assert_eq!(Encoding::Utf16.to_server(line, 2), 3);
        assert_eq!(Encoding::Utf8.to_server(line, 2), 6);
        assert_eq!(Encoding::Utf16.from_server(line, 3), 2);
        assert_eq!(Encoding::Utf8.from_server(line, 6), 2);
    }

    #[test]
    fn uris_round_trip() {
        let p = Path::new("/tmp/a b/ü#x.rs");
        assert_eq!(uri(p), "file:///tmp/a%20b/%C3%BC%23x.rs");
        assert_eq!(path_of(&uri(p)).unwrap(), p);
    }

    #[test]
    fn roots_at_the_topmost_marker_in_the_project() {
        let dir = std::env::temp_dir().join(format!("harlo-lsp-root-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("crates/a/src")).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "").unwrap();
        std::fs::write(dir.join("crates/a/Cargo.toml"), "").unwrap();
        assert_eq!(root_of(&dir.join("crates/a/src/lib.rs"), &dir, &["Cargo.toml"]), dir);
        assert_eq!(root_of(&dir.join("crates/a/src/lib.rs"), &dir.join("crates/a"), &["Cargo.toml"]), dir.join("crates/a"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    impl Servers {
        async fn call_at(&self, dir: &Path, args: &Value, _: &Path, _: Option<&Sandbox>) -> Outcome {
            self.call(args, dir.join(args["path"].as_str().unwrap()), dir, None).await
        }
    }

    /// A server is one that starts: rustup's stand-in for a component that
    /// isn't installed doesn't count, and says why.
    #[test]
    fn finds_servers_that_start() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("harlo-lsp-find-{}", std::process::id()));
        let (good, bad) = (dir.join("good"), dir.join("bad"));
        std::fs::create_dir_all(&good).unwrap();
        std::fs::create_dir_all(&bad).unwrap();
        let script = |path: PathBuf, body: &str| {
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        script(bad.join("rust-analyzer"), "echo \"error: Unknown binary 'rust-analyzer' in official toolchain 'stable'.\" >&2; exit 1");
        script(good.join("rust-analyzer"), "echo rust-analyzer 1.0");
        assert_eq!(look_for(0, std::slice::from_ref(&bad)), Found::Broken(bad.join("rust-analyzer"), "Unknown binary 'rust-analyzer' in official toolchain 'stable'.".into()));
        // A working one further on wins.
        assert_eq!(look_for(0, &[bad.clone(), good.clone()]), Found::Ready(vec![good.join("rust-analyzer").to_string_lossy().into_owned()]));
        // Pyright's server is asked through the program beside it, if there is one.
        script(good.join("pyright-langserver"), "exit 1");
        assert_eq!(look_for(1, std::slice::from_ref(&good)), Found::Ready(vec![good.join("pyright-langserver").to_string_lossy().into_owned(), "--stdio".into()]));
        script(good.join("pyright"), "exit 3");
        assert!(matches!(look_for(1, std::slice::from_ref(&good)), Found::Broken(..)));
        assert_eq!(look_for(2, &[good, bad]), Found::Missing);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// rust-analyzer on a small crate: two types' `post`, told apart.
    #[tokio::test]
    async fn rust_analyzer_tells_same_named_methods_apart() {
        if installed(0).is_none() {
            eprintln!("(no rust-analyzer: skipped)");
            return;
        }
        let dir = std::env::temp_dir().join(format!("harlo-lsp-ra-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"t\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::write(
            dir.join("src/lib.rs"),
            "pub struct Ledger;\npub struct Outbox;\nimpl Ledger { pub fn post(&self) {} }\nimpl Outbox { pub fn post(&self) {} }\n\npub fn books(l: &Ledger) { l.post(); }\npub fn mail(o: &Outbox) { o.post(); }\npub fn both(l: &Ledger) {\n    let ledger = Outbox;\n    ledger.post();\n    l.post();\n}\npub struct Desk { l: Ledger }\nimpl Desk {\n    pub fn file(&self) { self.l.post(); }\n}\n",
        )
        .unwrap();
        let servers = Servers::default();
        let ask = |op: &str, line: u64, symbol: &str| json!({ "operation": op, "path": "src/lib.rs", "line": line, "symbol": symbol });
        let refs = servers.call_at(&dir, &ask("references", 3, "post"), &dir, None).await;
        assert!(!refs.failed, "{}", refs.text);
        assert!(refs.text.starts_with("4 references:"), "{}", refs.text);
        assert!(refs.text.contains("src/lib.rs:6:") && refs.text.contains("src/lib.rs:11:") && !refs.text.contains("src/lib.rs:10:"), "{}", refs.text);
        let callers = servers.call_at(&dir, &ask("callers", 3, "post"), &dir, None).await;
        assert!(callers.text.contains("books") && callers.text.contains("both") && !callers.text.contains("mail"), "{}", callers.text);
        assert!(callers.text.contains("Desk::file (pub fn file(&self)), src/lib.rs:15"), "{}", callers.text);
        let def = servers.call_at(&dir, &ask("definition", 10, "post"), &dir, None).await;
        assert!(def.text.starts_with("src/lib.rs:4:"), "Outbox's: {}", def.text);
        // Edited since: seen as it is now.
        std::fs::write(dir.join("src/lib.rs"), std::fs::read_to_string(dir.join("src/lib.rs")).unwrap().replace("pub fn mail(o: &Outbox) { o.post(); }", "pub fn mail(o: &Ledger) { o.post(); }")).unwrap();
        let refs = servers.call_at(&dir, &ask("references", 3, "post"), &dir, None).await;
        assert!(refs.text.starts_with("5 references:"), "{}", refs.text);
        let hover = servers.call_at(&dir, &ask("hover", 6, "l"), &dir, None).await;
        assert!(hover.text.contains("Ledger"), "{}", hover.text);
        let missing = servers.call_at(&dir, &ask("references", 6, "nope"), &dir, None).await;
        assert!(missing.failed && missing.text.contains("isn't on line 6"), "{}", missing.text);
        drop(servers);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
