//! A project's wiki, kept by OpenWiki: linked Markdown pages in the
//! repository's `openwiki/` folder, each fact tied to the code it came from,
//! so an update redoes only the pages whose code changed. Agents search it
//! and read just the sections they need.
//!
//! harlo ships OpenWiki (its own Node and a pinned package, in
//! `lib/harlo/openwiki`) and runs its tool server (`openwiki mcp`), one per
//! machine, started on first use: four tools that only read (search, read,
//! and the linked wikis and workspaces) and six that write a wiki, one page
//! at a time, with the session's own model. OpenWiki's telemetry stays off
//! unless the user allows it. On another machine harlo puts the runtime in
//! its cache there once per version (x86_64 or ARM, with the C library
//! Node's builds need).
//!
//! What harlo shows without Node: the pages (from their frontmatter), the
//! links between them, how fresh the wiki is (`.last-update.json`, commits
//! since), a run's progress (`.run.json`), and a plain text search.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::remote::{self, Host};

/// The wiki's folder, at the repository's top.
pub const DIR: &str = "openwiki";
/// The tools that only read a wiki.
pub const READ_TOOLS: [&str; 4] = ["openwiki_search", "openwiki_read", "openwiki_list_workspaces", "openwiki_list_wikis"];
/// The tools that write one (a wiki session's only).
pub const WRITE_TOOLS: [&str; 6] = ["openwiki_begin", "openwiki_submit_plan", "openwiki_next_page", "openwiki_inspect_page_claims", "openwiki_submit_page", "openwiki_finish"];
/// Files in the folder that aren't pages of their own.
const NOT_PAGES: [&str; 3] = ["index.md", "log.md", "INSTRUCTIONS.md"];
const CLI: &str = "node_modules/openwiki/dist/cli/cli.js";
const SKILL: &str = "node_modules/openwiki/integrations/openwiki/SKILL.md";
const PROTOCOL: &str = "2025-06-18";

// ---- The runtime ----

/// harlo's OpenWiki: a folder with `bin/node`, the package, and `VERSION`
/// ("openwiki 0.9.0, node 24.22.3 x64, lock …").
#[derive(Debug, Clone, PartialEq)]
pub struct Runtime {
    pub dir: PathBuf,
}

impl Runtime {
    pub fn node(&self) -> PathBuf {
        self.dir.join("bin/node")
    }

    pub fn cli(&self) -> PathBuf {
        self.dir.join(CLI)
    }

    /// What `VERSION` says, "" when it's missing.
    pub fn version(&self) -> String {
        std::fs::read_to_string(self.dir.join("VERSION")).map(|s| s.trim().to_owned()).unwrap_or_default()
    }

    /// OpenWiki's version ("0.9.0").
    pub fn openwiki_version(&self) -> String {
        self.version().strip_prefix("openwiki ").and_then(|s| s.split(',').next()).unwrap_or_default().to_owned()
    }

    /// The machines it runs on (`x86_64`, `aarch64`), from its Node's build.
    pub fn arch(&self) -> &'static str {
        if self.version().contains(" arm64") { "aarch64" } else { "x86_64" }
    }

    /// OpenWiki's own steps for writing a wiki through its tools (the
    /// version's skill), for a wiki session's prompt.
    pub fn skill(&self) -> Option<String> {
        std::fs::read_to_string(self.dir.join(SKILL)).ok()
    }

    fn usable(&self) -> bool {
        self.node().is_file() && self.cli().is_file()
    }
}

/// Where the runtime might be, beside harlo's program in `exe_dir`: an
/// install's `lib/harlo/openwiki`, else a build's `target/openwiki` (a
/// test's program is in the build's deps folder).
pub fn candidates(exe_dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![exe_dir.join("../lib/harlo/openwiki"), exe_dir.join("../openwiki")];
    if exe_dir.ends_with("deps") {
        out.push(exe_dir.join("../../openwiki"));
    }
    out
}

/// This harlo's runtime (`HARLO_OPENWIKI` names another, for tests).
pub fn runtime() -> Option<Runtime> {
    if let Some(dir) = std::env::var_os("HARLO_OPENWIKI").filter(|d| !d.is_empty()) {
        return Some(Runtime { dir: dir.into() }).filter(Runtime::usable);
    }
    let exe = std::env::current_exe().ok()?;
    candidates(exe.parent()?).into_iter().map(|dir| Runtime { dir }).find(Runtime::usable)
}

// ---- Telemetry ----

static TELEMETRY: AtomicBool = AtomicBool::new(false);

/// Whether OpenWiki may send its usage telemetry (the user's setting; off
/// unless they allow it). A change restarts the servers.
pub fn set_telemetry(allowed: bool) {
    if TELEMETRY.swap(allowed, Ordering::SeqCst) != allowed {
        stop_all();
    }
}

/// What the server's environment says about telemetry.
fn telemetry_env() -> &'static str {
    if TELEMETRY.load(Ordering::SeqCst) { "" } else { "OPENWIKI_TELEMETRY_DISABLED=1 DO_NOT_TRACK=1" }
}

// ---- The tool server ----

type Waiting = Arc<Mutex<HashMap<u64, mpsc::Sender<Value>>>>;

/// A running `openwiki mcp`: requests go in on its stdin, answers come back
/// by id on its stdout (a thread reads them), so sessions can call it at
/// the same time.
pub struct Server {
    child: Mutex<Child>,
    stdin: Arc<Mutex<ChildStdin>>,
    waiting: Waiting,
    next: AtomicU64,
    alive: Arc<AtomicBool>,
    said: Arc<Mutex<String>>,
    /// "openwiki 0.9.0".
    pub info: String,
    /// Its tools as it lists them (name, description, inputSchema, …).
    pub tools: Vec<Value>,
}

impl Server {
    /// Starts `cmd` (a tool server on stdio), initializes it, lists its
    /// tools and checks OpenWiki's are all there.
    pub fn start(mut cmd: Command) -> Result<Server> {
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().context("starting OpenWiki")?;
        let stdin = Arc::new(Mutex::new(child.stdin.take().expect("piped")));
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let waiting: Waiting = Arc::default();
        let alive = Arc::new(AtomicBool::new(true));
        let said = Arc::new(Mutex::new(String::new()));
        {
            let said = said.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let mut s = said.lock().unwrap();
                    s.push_str(&line);
                    s.push('\n');
                    if s.len() > 8192 {
                        let cut = s.len() - 4096;
                        let cut = (cut..s.len()).find(|i| s.is_char_boundary(*i)).unwrap_or(s.len());
                        s.drain(..cut);
                    }
                }
            });
        }
        {
            let (waiting, alive, stdin) = (waiting.clone(), alive.clone(), stdin.clone());
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
                    let id = msg.get("id").cloned().unwrap_or(Value::Null);
                    if msg.get("method").is_some() {
                        // The server asking something: a ping is answered,
                        // anything else declined.
                        if !id.is_null() {
                            let reply = if msg["method"] == "ping" {
                                json!({"jsonrpc": "2.0", "id": id, "result": {}})
                            } else {
                                json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "harlo doesn't offer that"}})
                            };
                            let _ = writeln!(stdin.lock().unwrap(), "{reply}");
                        }
                        continue;
                    }
                    if let Some(tx) = id.as_u64().and_then(|i| waiting.lock().unwrap().remove(&i)) {
                        let _ = tx.send(msg);
                    }
                }
                alive.store(false, Ordering::SeqCst);
                waiting.lock().unwrap().clear();
            });
        }
        let mut server = Server { child: Mutex::new(child), stdin, waiting, next: AtomicU64::new(1), alive, said, info: String::new(), tools: Vec::new() };
        let init = server.request("initialize", json!({"protocolVersion": PROTOCOL, "capabilities": {}, "clientInfo": {"name": "harlo", "version": env!("CARGO_PKG_VERSION")}}), Duration::from_secs(60))?;
        let info = &init["serverInfo"];
        server.info = format!("{} {}", info["name"].as_str().unwrap_or("openwiki"), info["version"].as_str().unwrap_or_default()).trim().to_owned();
        server.notify("notifications/initialized")?;
        let listed = server.request("tools/list", json!({}), Duration::from_secs(30))?;
        server.tools = listed["tools"].as_array().cloned().unwrap_or_default();
        let missing: Vec<&str> = READ_TOOLS.iter().chain(&WRITE_TOOLS).copied().filter(|n| !server.tools.iter().any(|t| t["name"] == *n)).collect();
        if !missing.is_empty() {
            bail!("{} doesn't have the tools harlo expects (no {}): this harlo needs its own OpenWiki", server.info, missing.join(", "));
        }
        Ok(server)
    }

    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// The tool `name` as listed.
    pub fn tool(&self, name: &str) -> Option<&Value> {
        self.tools.iter().find(|t| t["name"] == name)
    }

    fn notify(&self, method: &str) -> Result<()> {
        writeln!(self.stdin.lock().unwrap(), "{}", json!({"jsonrpc": "2.0", "method": method})).context("OpenWiki stopped")
    }

    /// A request's result (its `error` as an error).
    fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.waiting.lock().unwrap().insert(id, tx);
        let sent = writeln!(self.stdin.lock().unwrap(), "{}", json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        if sent.is_err() || !self.alive() {
            self.waiting.lock().unwrap().remove(&id);
            bail!("OpenWiki stopped{}", self.last_words());
        }
        let msg = match rx.recv_timeout(timeout) {
            Ok(m) => m,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.waiting.lock().unwrap().remove(&id);
                bail!("OpenWiki didn't answer {method} in {} s", timeout.as_secs());
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("OpenWiki stopped{}", self.last_words()),
        };
        if let Some(e) = msg.get("error") {
            bail!("OpenWiki: {}", e["message"].as_str().unwrap_or("it failed"));
        }
        Ok(msg["result"].clone())
    }

    /// Calls a tool: its result (`content`, `isError`, …) as OpenWiki gives it.
    pub fn call(&self, name: &str, args: Value, timeout: Duration) -> Result<Value> {
        self.request("tools/call", json!({"name": name, "arguments": args}), timeout)
    }

    /// What it last said on stderr, for an error (": …", or "").
    fn last_words(&self) -> String {
        let said = self.said.lock().unwrap();
        said.lines().rev().find(|l| !l.trim().is_empty()).map(|l| format!(": {}", l.trim())).unwrap_or_default()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let mut child = self.child.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// The servers running, by machine ("" for this one).
static SERVERS: Mutex<Vec<(String, Arc<Server>)>> = Mutex::new(Vec::new());

/// The tool server for files on `host` (None: this machine), started (and
/// on another machine, put there) when it isn't running.
pub fn server(host: Option<&str>) -> Result<Arc<Server>> {
    let key = host.unwrap_or_default().to_owned();
    {
        let mut servers = SERVERS.lock().unwrap();
        servers.retain(|(_, s)| s.alive());
        if let Some((_, s)) = servers.iter().find(|(k, _)| *k == key) {
            return Ok(s.clone());
        }
    }
    // (Started without the lock: putting it on a machine can take minutes.)
    let rt = runtime().ok_or_else(|| anyhow::anyhow!("This harlo has no OpenWiki: reinstall harlo"))?;
    let cmd = match host {
        None => {
            let mut cmd = Command::new(rt.node());
            cmd.arg(rt.cli()).args(["mcp", "--host", "harlo"]);
            if !TELEMETRY.load(Ordering::SeqCst) {
                cmd.env("OPENWIKI_TELEMETRY_DISABLED", "1").env("DO_NOT_TRACK", "1");
            }
            cmd
        }
        Some(name) => {
            let h = remote::host(name).ok_or_else(|| anyhow::anyhow!("No remote instance named {name}"))?;
            let dir = on_host(&h, &rt)?;
            let q = remote::quote_path;
            remote::command(&h, &format!("{} exec {} {} mcp --host harlo", telemetry_env(), q(&format!("{dir}/bin/node")), q(&format!("{dir}/{CLI}"))))
        }
    };
    let started = Arc::new(Server::start(cmd)?);
    let mut servers = SERVERS.lock().unwrap();
    // (Another call may have started one meanwhile: that one stays.)
    if let Some((_, s)) = servers.iter().find(|(k, s)| *k == key && s.alive()) {
        return Ok(s.clone());
    }
    servers.push((key, started.clone()));
    Ok(started)
}

/// Stops every tool server (harlo quitting, or the telemetry setting
/// changed).
pub fn stop_all() {
    SERVERS.lock().unwrap().clear();
}

/// Calls an OpenWiki tool for the repository at `root` (here or on its
/// host), `root` filled in as OpenWiki wants it.
pub fn call(root: &Path, tool: &str, mut args: Value) -> Result<Value> {
    let (host, dir) = match remote::split(root) {
        Some((h, d)) => (Some(h), d),
        None => (None, root.to_string_lossy().into_owned()),
    };
    if !args.is_object() {
        args = json!({});
    }
    args["root"] = Value::String(dir);
    // (Writing a page checks its claims against the code: it can take a while.)
    let timeout = Duration::from_secs(if READ_TOOLS.contains(&tool) { 90 } else { 600 });
    server(host.as_deref())?.call(tool, args, timeout)
}

/// Whether the runtime starts and has its tools: "openwiki 0.9.0", else
/// what went wrong (a package's start check).
pub fn check() -> Result<String> {
    let rt = runtime().ok_or_else(|| anyhow::anyhow!("no OpenWiki beside harlo"))?;
    let mut cmd = Command::new(rt.node());
    cmd.arg(rt.cli()).args(["mcp", "--host", "harlo"]).env("OPENWIKI_TELEMETRY_DISABLED", "1").env("DO_NOT_TRACK", "1");
    // (Its own home, so a check writes nothing of the user's.)
    let home = std::env::temp_dir().join(format!("harlo-openwiki-check-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&home);
    cmd.env("HOME", &home);
    let out = Server::start(cmd).map(|s| format!("{} ({} tools)", s.info, s.tools.len()));
    let _ = std::fs::remove_dir_all(&home);
    out
}

// ---- On another machine ----

/// The runtime as it goes over (tar, gzipped), and a name for this build.
static PACKED: OnceLock<std::result::Result<(Vec<u8>, String), String>> = OnceLock::new();

fn packed(rt: &Runtime) -> Result<&'static (Vec<u8>, String)> {
    PACKED
        .get_or_init(|| {
            let version = rt.version();
            let id = version.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3));
            let out = Command::new("tar").arg("-C").arg(&rt.dir).args(["-czf", "-", "."]).stdin(Stdio::null()).stderr(Stdio::null()).output().map_err(|e| format!("tar: {e}"))?;
            if !out.status.success() {
                return Err("packing OpenWiki failed".into());
            }
            Ok((out.stdout, format!("{id:016x}")))
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// The runtime's folder on `host`, put there first when this version isn't
/// (older ones go).
fn on_host(host: &Host, rt: &Runtime) -> Result<String> {
    let arch = crate::remote_helper::arch_of(host)?;
    if arch != rt.arch() {
        bail!("harlo's OpenWiki runs on {} machines; {} is {arch}", rt.arch(), host.name);
    }
    let (gz, id) = packed(rt)?;
    let home = remote::home(host)?;
    let dir = format!("{home}/.cache/harlo/openwiki-{id}");
    let q = remote::quote_path;
    let out = remote::run(host, &format!("[ -x {}/bin/node ] && echo ready || echo missing", q(&dir)), Duration::from_secs(30))?;
    if String::from_utf8_lossy(&out.stdout).trim() != "ready" {
        let out = remote::run_with_input(host, &put_script(&dir), Some(gz), Duration::from_secs(900))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            bail!("harlo's OpenWiki doesn't run on {}: {}", host.name, err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("it failed").trim());
        }
    }
    Ok(dir)
}

/// Unpacks the runtime from stdin into `dir` (beside it first, so a broken
/// upload leaves nothing), checks Node and its native module load there,
/// then drops older versions.
fn put_script(dir: &str) -> String {
    let q = remote::quote_path;
    format!(
        r#"d={dir}; t="$d.part.$$"; rm -rf "$t" && mkdir -p "$t" && tar -xzf - -C "$t" && "$t/bin/node" -e 'new (require(process.argv[1]))(":memory:")' "$t/node_modules/better-sqlite3" || {{ rm -rf "$t"; exit 1; }}; rm -rf "$d" && mv "$t" "$d" && for o in "${{d%/*}}"/openwiki-*; do [ "$o" = "$d" ] || rm -rf "$o"; done"#,
        dir = q(dir)
    )
}

// ---- The wiki, read from its files ----

/// A page of the wiki, as its frontmatter says.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Page {
    /// In the wiki's folder ("quickstart.md", "architecture/jobs.md").
    pub path: String,
    /// Its `title`, else its first heading, else its file name.
    pub title: String,
    pub description: String,
    /// Its `type` ("overview", "architecture", "workflow", …).
    pub kind: String,
    /// When it was last checked against the code (`verified.at`), "" when
    /// it hasn't been.
    pub verified: String,
    /// The pages it links to (their paths), in order, once each.
    pub links: Vec<String>,
}

/// The wiki's pages under `root` (here or on its host): quickstart first,
/// then the top folder's, then each folder's, by path.
pub fn pages(root: &Path) -> Vec<Page> {
    let mut pages: Vec<Page> = load(root).iter().filter(|(p, _)| !NOT_PAGES.contains(&p.as_str())).map(|(p, t)| describe(p, t)).collect();
    pages.sort_by_key(|p| (p.path != "quickstart.md", p.path.contains('/'), p.path.clone()));
    let known: Vec<String> = pages.iter().map(|p| p.path.clone()).collect();
    for p in &mut pages {
        p.links.retain(|l| known.contains(l));
    }
    pages
}

/// The Markdown files under `root/openwiki`: `(path in it, text)`. On
/// another machine, all in one command. Hidden folders (claims, run state)
/// are left out.
fn load(root: &Path) -> Vec<(String, String)> {
    match remote::split(root) {
        Some((host, dir)) => load_on(&host, &dir),
        None => {
            let base = root.join(DIR);
            let mut found = Vec::new();
            walk(&base, &base, 0, &mut found);
            found.into_iter().map(|p| {
                let text = std::fs::read_to_string(base.join(&p)).unwrap_or_default();
                (p, text)
            }).collect()
        }
    }
}

fn walk(base: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
    if depth > 8 || out.len() > 2000 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = e.path();
        if path.is_dir() {
            walk(base, &path, depth + 1, out);
        } else if name.ends_with(".md")
            && let Ok(rel) = path.strip_prefix(base)
        {
            out.push(rel.to_string_lossy().into_owned());
        }
    }
}

/// [`load`] on `host`: each page's path, then its text (at most 256 KB).
fn load_on(host: &str, dir: &str) -> Vec<(String, String)> {
    let Some(host) = remote::host(host) else { return Vec::new() };
    let script = format!(
        r#"cd {} 2>/dev/null || exit 0; find . -maxdepth 9 -name '.?*' -prune -o -name '*.md' -type f -print | sed 's|^\./||' | head -n 2000 | while IFS= read -r f; do printf '%s\037' "$f"; head -c 262144 -- "$f"; printf '\036'; done; true"#,
        remote::quote_path(&format!("{dir}/{DIR}"))
    );
    let Ok(out) = remote::run(&host, &script, Duration::from_secs(60)) else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .split('\u{1e}')
        .filter_map(|r| r.split_once('\u{1f}'))
        .map(|(p, t)| (p.trim_start_matches('\n').to_owned(), t.to_owned()))
        .collect()
}

/// A page as its file says.
fn describe(rel: &str, text: &str) -> Page {
    let (front, body) = split_front(text);
    let fields = front_fields(front);
    let field = |k: &str| fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone()).unwrap_or_default();
    let mut title = field("title");
    if title.is_empty() {
        title = body.lines().find_map(|l| l.strip_prefix("# ")).map(|t| t.trim().to_owned()).unwrap_or_default();
    }
    if title.is_empty() {
        title = rel.rsplit('/').next().unwrap_or(rel).trim_end_matches(".md").replace(['-', '_'], " ");
    }
    Page { path: rel.to_owned(), title, description: field("description"), kind: field("type"), verified: field("verified.at"), links: links_of(rel, body) }
}

/// The frontmatter (between `---` lines at the top) and the rest.
fn split_front(text: &str) -> (&str, &str) {
    let t = text.trim_start_matches('\u{feff}');
    let Some(rest) = t.strip_prefix("---\n").or_else(|| t.strip_prefix("---\r\n")) else { return ("", t) };
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return (&rest[..at], &rest[at + line.len()..]);
        }
        at += line.len();
    }
    ("", t)
}

/// The frontmatter's plain values: top-level `key: value`, and one level
/// down as `parent.key` (`verified.at`). Quotes are taken off.
fn front_fields(front: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut parent = String::new();
    for line in front.lines() {
        let indented = line.starts_with(' ') || line.starts_with('\t');
        let Some((k, v)) = line.trim().split_once(':') else { continue };
        let (k, v) = (k.trim().trim_start_matches("- "), v.trim());
        if !indented {
            parent = k.to_owned();
        }
        if v.is_empty() {
            continue;
        }
        let v = v.trim_matches(|c| c == '"' || c == '\'').to_owned();
        out.push((if indented { format!("{parent}.{k}") } else { k.to_owned() }, v));
    }
    out
}

/// The pages a page's Markdown links to (relative links to `.md` files,
/// resolved from its folder), in order, once each.
fn links_of(rel: &str, body: &str) -> Vec<String> {
    let base: Vec<&str> = rel.split('/').collect();
    let base = &base[..base.len() - 1];
    let mut out: Vec<String> = Vec::new();
    for (i, _) in body.match_indices("](") {
        let rest = &body[i + 2..];
        let Some(end) = rest.find(')') else { continue };
        let target = rest[..end].split_whitespace().next().unwrap_or_default();
        let target = target.split('#').next().unwrap_or_default();
        if !target.ends_with(".md") || target.contains("://") || target.starts_with('/') {
            continue;
        }
        let mut parts: Vec<&str> = base.to_vec();
        let mut ok = true;
        for c in target.split('/') {
            match c {
                "" | "." => {}
                ".." => ok &= parts.pop().is_some(),
                c => parts.push(c),
            }
        }
        let path = parts.join("/");
        if ok && path != rel && !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// A page's text (`page` in the wiki's folder), here or on its host.
pub fn read(root: &Path, page: &str) -> Result<String, String> {
    let page = page.trim().trim_start_matches("./").trim_start_matches("openwiki/");
    if !page.ends_with(".md") || page.split('/').any(|c| c.is_empty() || c == ".." || c.starts_with('.')) {
        return Err(format!("{page} isn't a page of the wiki"));
    }
    read_file(root, &format!("{DIR}/{page}")).ok_or_else(|| format!("The wiki has no page {page}"))
}

/// A small file under `root` (here or on its host).
fn read_file(root: &Path, rel: &str) -> Option<String> {
    match remote::split(root) {
        Some((host, dir)) => {
            let host = remote::host(&host)?;
            let out = remote::run(&host, &format!("cat -- {}", remote::quote_path(&format!("{dir}/{rel}"))), Duration::from_secs(30)).ok()?;
            out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
        }
        None => std::fs::read_to_string(root.join(rel)).ok(),
    }
}

/// A wiki run in progress (`.run.json`), or one that stopped part way.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Run {
    /// "init" or "update".
    pub mode: String,
    /// "planning" or "generating".
    pub phase: String,
    /// Pages written (or skipped) of the plan's.
    pub done: usize,
    pub total: usize,
    pub started: String,
}

/// The last finished update (`.last-update.json`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LastUpdate {
    pub at: String,
    /// The commit it was written at.
    pub head: String,
    /// "complete" or "interrupted".
    pub status: String,
    pub model: String,
}

/// How the wiki stands.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct State {
    /// The repository has an `openwiki/` folder.
    pub exists: bool,
    pub run: Option<Run>,
    pub last: Option<LastUpdate>,
    /// Commits since the last update that change more than the wiki (None
    /// when that can't be told).
    pub behind: Option<u32>,
}

/// The wiki's state under `root` (here or on its host).
pub fn state(root: &Path) -> State {
    let run = read_file(root, &format!("{DIR}/.run.json")).and_then(|t| serde_json::from_str::<Value>(&t).ok()).map(|v| run_of(&v));
    let last = read_file(root, &format!("{DIR}/.last-update.json")).and_then(|t| serde_json::from_str::<Value>(&t).ok()).map(|v| last_of(&v));
    let exists = run.is_some() || last.is_some() || match remote::split(root) {
        Some(_) => !load(root).is_empty(),
        None => root.join(DIR).is_dir(),
    };
    let behind = last.as_ref().filter(|l| !l.head.is_empty()).and_then(|l| {
        let out = crate::git::output_in(root, &["rev-list", "--count", &format!("{}..HEAD", l.head), "--", ".", ":!openwiki", ":!AGENTS.md", ":!CLAUDE.md"]);
        out.trim().parse().ok()
    });
    State { exists, run, last, behind }
}

fn run_of(v: &Value) -> Run {
    let pages = v["plan"]["pages"].as_array().cloned().unwrap_or_default();
    Run {
        mode: v["mode"].as_str().unwrap_or_default().to_owned(),
        phase: v["phase"].as_str().unwrap_or_default().to_owned(),
        done: pages.iter().filter(|p| p["status"] != "pending").count(),
        total: pages.len(),
        started: v["startedAt"].as_str().unwrap_or_default().to_owned(),
    }
}

fn last_of(v: &Value) -> LastUpdate {
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
    LastUpdate { at: s("updatedAt"), head: s("gitHead"), status: { let st = s("status"); if st.is_empty() { "complete".into() } else { st } }, model: s("model") }
}

/// A plain text search of the pages, for when OpenWiki can't run: each
/// section (a page's part under a heading) that has the query's words,
/// most first: `{page, section, heading, snippet}`.
pub fn search_text(root: &Path, query: &str, limit: usize) -> Vec<Value> {
    search_in(&load(root), query, limit)
}

fn search_in(texts: &[(String, String)], query: &str, limit: usize) -> Vec<Value> {
    let words: Vec<String> = query.split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 1).map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(usize, Value)> = Vec::new();
    for (path, text) in texts.iter().filter(|(p, _)| !NOT_PAGES.contains(&p.as_str())) {
        let (_, body) = split_front(text);
        let mut heading = String::new();
        let mut section = String::new();
        let mut flush = |heading: &str, section: &str| {
            let lower = section.to_lowercase();
            let score: usize = words.iter().map(|w| lower.matches(w.as_str()).count()).sum::<usize>() + words.iter().filter(|w| heading.to_lowercase().contains(w.as_str())).count() * 3;
            let all = words.iter().all(|w| lower.contains(w.as_str()) || heading.to_lowercase().contains(w.as_str()));
            if score > 0 {
                let line = section.lines().find(|l| words.iter().any(|w| l.to_lowercase().contains(w.as_str()))).unwrap_or_default().trim();
                let snippet: String = line.chars().take(200).collect();
                hits.push((score + if all { 100 } else { 0 }, json!({"page": format!("{DIR}/{path}"), "section": anchor(heading), "heading": heading, "snippet": snippet})));
            }
        };
        for line in body.lines() {
            let level = line.chars().take_while(|c| *c == '#').count();
            if (1..=4).contains(&level) && line[level..].starts_with(' ') {
                flush(&heading, &section);
                heading = line[level..].trim().to_owned();
                section.clear();
            } else {
                section.push_str(line);
                section.push('\n');
            }
        }
        flush(&heading, &section);
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    hits.into_iter().take(limit).map(|(_, v)| v).collect()
}

/// A heading's anchor, as GitHub makes them ("Retry control" → "retry-control").
pub fn anchor(heading: &str) -> String {
    heading.trim().to_lowercase().chars().filter_map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { Some(c) } else if c == ' ' { Some('-') } else { None }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PathBuf, impl Drop) {
        struct Gone(PathBuf);
        impl Drop for Gone {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        static N: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!("harlo-openwiki-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let w = root.join(DIR);
        std::fs::create_dir_all(w.join("architecture")).unwrap();
        std::fs::create_dir_all(w.join(".claims")).unwrap();
        std::fs::write(w.join("index.md"), "---\nokf_version: \"0.2\"\n---\n# Index\n").unwrap();
        std::fs::write(w.join("quickstart.md"), "---\ntype: overview\ntitle: Quickstart\ndescription: Where to start.\n---\n# Quickstart\nSee [jobs](architecture/jobs.md#retry-control) and [the queue](./architecture/queue.md), [code](repo://src/a.rs#L1-L4), [web](https://x.y/a.md).\n").unwrap();
        std::fs::write(w.join("architecture/jobs.md"), "---\ntype: architecture\ndescription: \"How jobs run.\"\nverified:\n  by: openwiki/0.9.0\n  at: 2026-11-12T10:00:00Z\n---\n# Jobs\nBack to [start](../quickstart.md).\n## Retry control\nA failed job is retried three times with backoff.\n").unwrap();
        std::fs::write(w.join("architecture/queue.md"), "# The queue\nJobs wait here; see [missing](gone.md).\n").unwrap();
        std::fs::write(w.join(".claims/x.md"), "# not a page\n").unwrap();
        std::fs::write(w.join(".last-update.json"), r#"{"updatedAt":"2026-11-12T10:00:00Z","command":"init","gitHead":"abc123","model":"harlo","status":"complete"}"#).unwrap();
        std::fs::write(w.join(".run.json"), r#"{"schemaVersion":1,"mode":"update","phase":"generating","startedAt":"2026-11-12T11:00:00Z","plan":{"pages":[{"id":"a","status":"complete"},{"id":"b","status":"skipped"},{"id":"c","status":"pending"}],"deletePages":[]}}"#).unwrap();
        (root.clone(), Gone(root))
    }

    #[test]
    fn pages_links_and_order() {
        let (root, _gone) = fixture();
        let pages = pages(&root);
        let paths: Vec<&str> = pages.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, ["quickstart.md", "architecture/jobs.md", "architecture/queue.md"]);
        assert_eq!(pages[0].links, ["architecture/jobs.md", "architecture/queue.md"]);
        assert_eq!(pages[0].kind, "overview");
        assert_eq!(pages[1].title, "Jobs");
        assert_eq!(pages[1].description, "How jobs run.");
        assert_eq!(pages[1].verified, "2026-11-12T10:00:00Z");
        assert_eq!(pages[1].links, ["quickstart.md"]);
        // (A link to a page that isn't there is left out.)
        assert!(pages[2].links.is_empty());
        assert_eq!(pages[2].title, "The queue");
    }

    #[test]
    fn reading_pages() {
        let (root, _gone) = fixture();
        assert!(read(&root, "openwiki/architecture/jobs.md").unwrap().contains("Retry control"));
        assert!(read(&root, "architecture/jobs.md").is_ok());
        assert!(read(&root, "../Cargo.toml").is_err());
        assert!(read(&root, ".claims/x.md").is_err());
        assert!(read(&root, "nope.md").is_err());
    }

    #[test]
    fn state_and_progress() {
        let (root, _gone) = fixture();
        let s = state(&root);
        assert!(s.exists);
        assert_eq!(s.run, Some(Run { mode: "update".into(), phase: "generating".into(), done: 2, total: 3, started: "2026-11-12T11:00:00Z".into() }));
        let last = s.last.unwrap();
        assert_eq!((last.head.as_str(), last.status.as_str()), ("abc123", "complete"));
        // (Not a git repository: how far behind can't be told.)
        assert_eq!(s.behind, None);
        assert!(!state(&root.join("nowhere")).exists);
    }

    #[test]
    fn text_search_finds_sections() {
        let (root, _gone) = fixture();
        let hits = search_text(&root, "retry backoff", 5);
        assert_eq!(hits[0]["page"], "openwiki/architecture/jobs.md");
        assert_eq!(hits[0]["section"], "retry-control");
        assert!(hits[0]["snippet"].as_str().unwrap().contains("backoff"));
        assert!(search_text(&root, "zebra", 5).is_empty());
    }

    #[test]
    fn runtime_places() {
        let c = candidates(Path::new("/opt/harlo/bin"));
        assert_eq!(c[0], Path::new("/opt/harlo/bin/../lib/harlo/openwiki"));
        let c = candidates(Path::new("/w/target/debug/deps"));
        assert_eq!(c[2], Path::new("/w/target/debug/deps/../../openwiki"));
        let rt = Runtime { dir: "/nowhere".into() };
        assert_eq!(rt.arch(), "x86_64");
    }

    /// A stand-in tool server (a shell script): it answers initialize and
    /// tools/list with OpenWiki's tool names, and echoes a call's arguments.
    fn stand_in(tools: &[&str]) -> Command {
        let list = tools.iter().map(|t| format!(r#"{{"name":"{t}","inputSchema":{{"type":"object"}}}}"#)).collect::<Vec<_>>().join(",");
        let script = format!(
            r#"while IFS= read -r line; do
id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p'); [ -n "$id" ] || continue
case "$line" in
*'"initialize"'*) printf '{{"jsonrpc":"2.0","id":%s,"result":{{"serverInfo":{{"name":"openwiki","version":"9.9.9"}}}}}}\n' "$id" ;;
*'"tools/list"'*) printf '{{"jsonrpc":"2.0","id":%s,"result":{{"tools":[{list}]}}}}\n' "$id" ;;
*'"tools/call"'*) args=$(printf '%s' "$line" | sed -n 's/.*"arguments":\({{[^}}]*}}\).*/\1/p'); printf '{{"jsonrpc":"2.0","id":%s,"result":{{"content":[{{"type":"text","text":"ok"}}],"structuredContent":%s}}}}\n' "$id" "$args" ;;
*) printf '{{"jsonrpc":"2.0","id":%s,"error":{{"code":-32601,"message":"no"}}}}\n' "$id" ;;
esac; done"#
        );
        let mut cmd = Command::new("sh");
        cmd.args(["-c", &script]);
        cmd
    }

    #[test]
    fn client_talks_to_a_server() {
        let all: Vec<&str> = READ_TOOLS.iter().chain(&WRITE_TOOLS).copied().collect();
        let s = Server::start(stand_in(&all)).unwrap();
        assert_eq!(s.info, "openwiki 9.9.9");
        assert_eq!(s.tools.len(), 10);
        let r = s.call("openwiki_search", json!({"root": "/r", "query": "x"}), Duration::from_secs(10)).unwrap();
        assert_eq!(r["structuredContent"]["root"], "/r");
        // (Calls at the same time each get their own answer.)
        let s = Arc::new(s);
        let threads: Vec<_> = (0..8).map(|i| {
            let s = s.clone();
            std::thread::spawn(move || s.call("openwiki_read", json!({"page": format!("p{i}")}), Duration::from_secs(10)).unwrap()["structuredContent"]["page"].as_str().unwrap().to_owned())
        }).collect();
        for (i, t) in threads.into_iter().enumerate() {
            assert_eq!(t.join().unwrap(), format!("p{i}"));
        }
        assert!(s.request("nope", json!({}), Duration::from_secs(10)).is_err());
    }

    /// The real OpenWiki (target/openwiki, when fetched) starts, and
    /// searches and reads a fixture wiki in a git repository.
    #[test]
    fn bundled_openwiki_searches() {
        let Some(rt) = runtime() else { return };
        assert!(check().unwrap().starts_with("openwiki "));
        let (root, _gone) = fixture();
        let git = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(&root).args(args).output().unwrap().status.success());
        git(&["init", "-q"]);
        git(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "x"]);
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let mut cmd = Command::new(rt.node());
        cmd.arg(rt.cli()).args(["mcp", "--host", "harlo"]).env("HOME", &home).env("OPENWIKI_TELEMETRY_DISABLED", "1");
        let s = Server::start(cmd).unwrap();
        let r = s.call("openwiki_search", json!({"root": root, "query": "how are failed jobs retried"}), Duration::from_secs(60)).unwrap();
        let text = r.to_string();
        assert!(text.contains("architecture/jobs.md"), "{text}");
        let r = s.call("openwiki_read", json!({"root": root, "page": "openwiki/architecture/jobs.md", "sections": ["retry-control"]}), Duration::from_secs(60)).unwrap();
        assert!(r.to_string().contains("three times"), "{r}");
        assert!(rt.skill().unwrap().contains("openwiki_begin"));
    }

    #[test]
    fn client_notices_changed_tools() {
        let err = Server::start(stand_in(&["openwiki_search", "openwiki_read"])).err().unwrap().to_string();
        assert!(err.contains("openwiki_begin"), "{err}");
    }

    #[test]
    fn client_notices_a_stopped_server() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo 'broken: no node' >&2; exit 1"]);
        let err = Server::start(cmd).err().unwrap().to_string();
        assert!(err.contains("stopped"), "{err}");
    }
}
