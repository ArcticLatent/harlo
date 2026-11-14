//! The tools the model can call, their descriptions, and running them.
//!
//! Files the session has read or written are remembered as they were (their
//! size, modification time and a hash of what was in them): an edit or
//! overwrite of a file that changed since then (another helper, the user)
//! is refused until it's read again. A file a tool makes (generated code, a
//! lockfile) is refused the first time it's changed (see `generated`).

pub mod bash;
pub mod edit;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde_json::{Value, json};

use crate::sandbox::Sandbox;

/// Lines `read` shows at most, and bytes.
const READ_LINES: usize = 2000;
const READ_BYTES: usize = 50 * 1024;
/// A file longer than one read, read without a range: its start only (what
/// a read shows stays in every later request).
const PREVIEW_LINES: usize = 250;
const PREVIEW_BYTES: usize = 12 * 1024;
/// Entries `ls` and `find` list at most; matches `grep` shows.
const LIST_MAX: usize = 500;
const GREP_MAX: usize = 300;
/// More matches than this, or more bytes: the first ones, a count per
/// file, and all of them in a file (what a search shows stays in every
/// later request). `HARLO_AGENT_GREP_CAP=off`: up to 300 shown.
const GREP_SHOWN: usize = 100;
const GREP_BYTES: usize = 12 * 1024;

/// The most of one line of a search's output kept (rg cuts its own at 300
/// columns; grep doesn't).
const LINE_CAP: usize = 4096;

/// A search's matches as they come: the first ones (what's shown), how
/// many and in which files, and, once there are more than are shown, all
/// of them in a file (written as they come, not kept here).
struct Matches {
    base: String,
    hidden: Vec<String>,
    in_folder: bool,
    spill_dir: PathBuf,
    /// Lines the search gave (hidden ones too).
    seen: usize,
    total: usize,
    bytes: usize,
    first: Vec<String>,
    counts: HashMap<String, usize>,
    spill: Option<(std::io::BufWriter<std::fs::File>, String)>,
    /// The file couldn't be made, or writing it failed part way.
    unsaved: bool,
    broke: bool,
}

impl Matches {
    fn new(base: String, hidden: Vec<String>, in_folder: bool, spill_dir: PathBuf) -> Self {
        Matches { base, hidden, in_folder, spill_dir, seen: 0, total: 0, bytes: 0, first: Vec::new(), counts: HashMap::new(), spill: None, unsaved: false, broke: false }
    }

    fn spilled(&self) -> bool {
        grep_capped() && (self.total > GREP_SHOWN || self.bytes > GREP_BYTES)
    }

    fn take(&mut self, raw: &[u8]) {
        use std::io::Write;
        self.seen += 1;
        let text = String::from_utf8_lossy(raw);
        let l = text.strip_prefix(self.base.as_str()).unwrap_or(&text);
        if names_hidden(&self.hidden, l) {
            return;
        }
        self.total += 1;
        self.bytes += l.len() + 1;
        if self.in_folder
            && let Some((file, _)) = l.split_once(':')
        {
            match self.counts.get_mut(file) {
                Some(n) => *n += 1,
                None => {
                    self.counts.insert(file.to_owned(), 1);
                }
            }
        }
        if self.spilled() && self.spill.is_none() && !self.unsaved {
            // (Every line so far is still among the first.)
            match bash::open_spill(&self.spill_dir) {
                Some((f, path)) => {
                    let mut f = std::io::BufWriter::with_capacity(1 << 16, f);
                    let so_far: String = self.first.iter().map(|l| format!("{l}\n")).collect();
                    self.broke = f.write_all(so_far.as_bytes()).is_err();
                    self.spill = Some((f, path));
                }
                None => self.unsaved = true,
            }
        }
        if let Some((f, _)) = &mut self.spill
            && !self.broke
        {
            self.broke = f.write_all(l.as_bytes()).and_then(|_| f.write_all(b"\n")).is_err();
        }
        if self.first.len() < GREP_MAX {
            self.first.push(l.to_owned());
        }
    }

    /// Many matches: a line saying how many and in which files (the most
    /// first), where all of them are, then the first ones.
    fn many(mut self) -> String {
        use std::io::Write;
        if let Some((f, _)) = &mut self.spill
            && !self.broke
        {
            self.broke = f.flush().is_err();
        }
        let mut first = Vec::new();
        let mut size = 0;
        for l in &self.first {
            if first.len() >= GREP_SHOWN || size + l.len() + 1 > GREP_BYTES {
                break;
            }
            size += l.len() + 1;
            first.push(l.as_str());
        }
        let mut files: Vec<(&String, &usize)> = self.counts.iter().collect();
        files.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let where_ = match files.len() {
            0 | 1 => String::new(),
            n => {
                let named: Vec<String> = files.iter().take(8).map(|(f, c)| format!("{f} {c}")).collect();
                let more = if n > 8 { format!(", {} more files", n - 8) } else { String::new() };
                format!(" in {n} files ({}{more})", named.join(", "))
            }
        };
        let saved = match &self.spill {
            Some((_, p)) if self.broke => format!("Only the first of them are in {p}: writing the rest failed (is the disk full?)."),
            Some((_, p)) => format!("All of them are in {p}: grep that, or read it with offset."),
            None => "The rest couldn't be saved.".into(),
        };
        format!("[{} matches{where_}; the first {} shown. {saved} Or narrow the pattern, path or glob.]\n{}", self.total, first.len(), first.join("\n"))
    }
}

fn grep_capped() -> bool {
    std::env::var("HARLO_AGENT_GREP_CAP").map_or(true, |v| v.trim() != "off")
}

/// How long a grep or find may run: a minute (`HARLO_AGENT_SEARCH_SECS`
/// sets another).
fn search_secs() -> u64 {
    std::env::var("HARLO_AGENT_SEARCH_SECS").ok().and_then(|v| v.trim().parse().ok()).filter(|s| *s > 0).unwrap_or(60)
}

pub struct Tools {
    pub cwd: PathBuf,
    /// Where long command output is saved.
    pub spill_dir: PathBuf,
    /// Files as this session last saw them.
    seen: Mutex<HashMap<PathBuf, Seen>>,
    /// Generated files the model was told about: a change made again goes
    /// through.
    told_generated: Mutex<HashSet<PathBuf>>,
    /// Outputs set aside (see `stubs`), by call id, for `recall`.
    originals: Mutex<HashMap<String, Original>>,
    /// Where commands run, and what the file tools may touch (None: the
    /// sandbox is off).
    pub sandbox: Option<Arc<Sandbox>>,
    /// Commands running in the background (`bash` with `background`).
    pub jobs: Arc<crate::jobs::Jobs>,
    /// Language servers started for `lsp`.
    pub lsp: Arc<crate::lsp::Servers>,
    /// Folders below `cwd` whose own rules (AGENTS.md) the model has, as
    /// paths relative to `cwd`.
    rules_given: Mutex<HashSet<PathBuf>>,
    /// Of those, the folders whose rules went out with this batch's results:
    /// the model hasn't read them yet, so a change there waits for its next
    /// reply.
    rules_new: Mutex<HashSet<PathBuf>>,
}

/// Of a folder's own rules file, what goes with a tool result.
const NESTED_RULES_KEPT: usize = 8_000;
/// How a folder's rules start in a tool result (and are found again in the
/// history): `[Rules from sub/AGENTS.md, for the files under sub/:]`.
const RULES_FROM: &str = "[Rules from ";
const RULES_FOR: &str = ", for the files under ";

/// A file as the session last saw it.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Seen {
    modified: SystemTime,
    size: u64,
    /// Of its bytes.
    hash: u64,
    /// When this was taken.
    at: SystemTime,
}

/// A modification time this close to when it was taken may not move when
/// the file is written again (some file systems' times are that coarse, and
/// a write in the same tick keeps it): such a file's contents are compared.
const RACY: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether a search's line `l` is of a hidden path (`under`: `hidden_names`).
fn names_hidden(under: &[String], l: &str) -> bool {
    under.iter().any(|h| l.strip_prefix(h.as_str()).is_some_and(|rest| rest.is_empty() || rest.starts_with('/') || rest.starts_with(':')))
}

/// Past this, `read` takes a file as it comes instead of whole.
const BIG_READ: u64 = 8 << 20;

/// A big file read as it comes ([`stream_lines`]).
struct Streamed {
    lines: usize,
    bytes: u64,
    /// The lines from the start asked for: at most the count, about the
    /// bytes asked for, each cut a little past them.
    kept: Vec<Vec<u8>>,
    /// The first kept line's whole length (it may be kept cut).
    first_len: usize,
    binary: bool,
    /// As [`hash_of`] makes it.
    hash: u64,
}

/// A line too long to show whole: its start (up to [`READ_BYTES`]), and a
/// note that says so, with how to read on (no anchor: it isn't the line).
fn long_line(path: &Path, number: usize, line: &str, whole: usize) -> String {
    let shown = &line[..line.floor_char_boundary(READ_BYTES)];
    let from = shown.len() + 1;
    format!(
        "{shown}\n[Line {number} is {} KB; only its first {} bytes are shown. Read on with bash: sed -n '{number}p' '{}' | cut -b {from}-{}, or grep -o what you need.]",
        whole.div_ceil(1024),
        shown.len(),
        path.display(),
        from + READ_BYTES - 1
    )
}

/// `path`'s lines counted (as `str::lines` would), the ones from `start`
/// kept (see [`Streamed`]), and its hash: only a chunk of it in memory.
fn stream_lines(path: &Path, start: usize, max: usize, max_bytes: usize) -> std::io::Result<Streamed> {
    use std::hash::Hasher;
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let size = f.metadata()?.len();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    // (A slice hashes its length first.)
    h.write_usize(size as usize);
    let mut buf = vec![0u8; 1 << 16];
    let (mut lines, mut bytes, mut binary) = (0usize, 0u64, false);
    let (mut kept, mut kept_bytes, mut cur, mut open) = (Vec::new(), 0usize, Vec::new(), false);
    let (mut cur_len, mut first_len) = (0usize, 0usize);
    let cap = max_bytes + 1;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let part = &buf[..n];
        h.write(part);
        if bytes < 8192 && part[..n.min(8192 - bytes as usize)].contains(&0) {
            binary = true;
        }
        bytes += n as u64;
        let mut rest = part;
        loop {
            let end = rest.iter().position(|b| *b == b'\n');
            let seg = &rest[..end.unwrap_or(rest.len())];
            let keep = lines >= start && kept.len() < max && kept_bytes <= max_bytes;
            if keep && cur.len() < cap {
                cur.extend_from_slice(&seg[..seg.len().min(cap - cur.len())]);
            }
            cur_len += seg.len();
            open |= !seg.is_empty();
            let Some(i) = end else { break };
            if keep {
                if cur.last() == Some(&b'\r') {
                    cur.pop();
                }
                if kept.is_empty() {
                    first_len = cur_len;
                }
                kept_bytes += cur.len() + 1;
                kept.push(std::mem::take(&mut cur));
            }
            cur.clear();
            cur_len = 0;
            lines += 1;
            open = false;
            rest = &rest[i + 1..];
        }
    }
    if open {
        if lines >= start && kept.len() < max && kept_bytes <= max_bytes {
            if kept.is_empty() {
                first_len = cur_len;
            }
            kept.push(cur);
        }
        lines += 1;
    }
    // (Grown or shrunk while read: its hash can't match, as it shouldn't.)
    let hash = if bytes == size { h.finish() } else { h.finish() ^ 1 };
    Ok(Streamed { lines, bytes, kept, first_len, binary, hash })
}

fn hash_of(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// An output set aside: the result as it was, and the file it read (to
/// say when it's changed since).
#[derive(Clone)]
pub struct Original {
    pub result: Value,
    pub read: Option<PathBuf>,
}

/// A tool's result for the model, whether it failed, and what the log
/// keeps beside it (`details`; null for none).
pub struct Outcome {
    pub text: String,
    pub failed: bool,
    pub details: Value,
    /// Images it returns (pi's image blocks), shown to the model after the
    /// text.
    pub images: Vec<Value>,
}

impl Outcome {
    pub fn new(text: impl Into<String>, failed: bool) -> Outcome {
        Outcome { text: text.into(), failed, details: Value::Null, images: Vec::new() }
    }
}

fn ok(text: impl Into<String>) -> Outcome {
    Outcome::new(text, false)
}

fn fail(text: impl Into<String>) -> Outcome {
    Outcome::new(text, true)
}

/// Tools that only look: several run at once.
pub fn read_only(name: &str) -> bool {
    // (`jobs` touches only this session's own background commands.)
    // (`todo` only keeps the model's own list.)
    // (`lsp` asks a language server, which runs none of the project's code.)
    matches!(name, "read" | "grep" | "find" | "ls" | "recall" | "current_time" | "jobs" | "todo" | "lsp")
}

/// The tools' definitions for the Responses API: those in `only` (all
/// when None), and `task` with them when `task` (the profiles it may name).
/// `patch`: apply_patch where edit would be.
pub fn specs_of(only: Option<&[&str]>, task: Option<&[crate::profiles::Profile]>, patch: bool) -> Value {
    let mut all: Vec<Value> = specs().as_array().cloned().unwrap_or_default();
    // (Anchored edits, when measured.)
    if crate::hashline::on() {
        for t in all.iter_mut().filter(|t| t["name"] == "edit") {
            *t = crate::hashline::spec();
        }
    }
    // (The model's list of steps, when it's being measured.)
    if crate::todo::on() {
        all.push(crate::todo::spec());
    }
    // (No network in the sandbox: nothing to fetch.)
    if !crate::fetch::possible() {
        all.retain(|t| t["name"] != "fetch");
    }
    if let Some(only) = only {
        all.retain(|t| t["name"].as_str().is_some_and(|n| only.contains(&n)));
    }
    if patch {
        for t in all.iter_mut() {
            if t["name"] == "edit" {
                *t = crate::patch::spec();
            } else if let Some(d) = t["description"].as_str() {
                t["description"] = json!(crate::patch::reworded(d));
            }
        }
    }
    if let Some(profiles) = task {
        all.push(crate::task::spec(profiles));
    }
    Value::Array(all)
}

/// The tools' definitions for the Responses API.
pub fn specs() -> Value {
    let f = |name: &str, description: &str, properties: Value, required: &[&str]| {
        json!({ "type": "function", "name": name, "description": description, "strict": false,
                "parameters": { "type": "object", "properties": properties, "required": required, "additionalProperties": false } })
    };
    json!([
        f("read", "Read a text file: up to 2000 lines (50 KB) from `offset` (1-based line), saying how to read on. A longer file read without offset or limit shows its first 250 lines and its size: grep it, or read a range.",
          json!({ "path": { "type": "string", "description": "Relative to the working directory, or absolute" }, "offset": { "type": "integer" }, "limit": { "type": "integer" } }), &["path"]),
        f("write", "Create a file, or replace a whole file you've read in this session. Makes missing folders. To change part of a file, use edit.",
          json!({ "path": { "type": "string" }, "content": { "type": "string" } }), &["path", "content"]),
        f("edit", "Replace text in a file. Each old_text must match exactly one place in the file as it is now (copy it from read, with enough lines around it to be unique); edits apply together. Refused if the file changed since you read it.",
          json!({ "path": { "type": "string" }, "edits": { "type": "array", "items": { "type": "object", "properties": { "old_text": { "type": "string" }, "new_text": { "type": "string" } }, "required": ["old_text", "new_text"], "additionalProperties": false } } }), &["path", "edits"]),
        f("bash", "Run a bash command in the working directory. No input (stdin closed; pagers and editors off). Times out after 120 s unless `timeout` (seconds) says otherwise. Long output shows its start and end, all of it saved to a file. Don't edit files with it: use edit or write. \
                   background: true runs it in the background (a server, a watcher, a long test run): you get its job id at once and are told when it ends (timeout 30 min by default, up to 2 h); jobs shows its output, waits for it or stops it. Don't add & yourself.",
          json!({ "command": { "type": "string" }, "timeout": { "type": "integer" }, "background": { "type": "boolean" },
                  "outside_sandbox": { "type": "boolean", "description": "Run outside the sandbox: only when it failed because of the sandbox and really needs what it hides or protects (e.g. git push over SSH); say why first. The user is asked every time." } }), &["command"]),
        f("grep", "Search file contents for a regular expression (ripgrep syntax, respects .gitignore). Shows file:line: text; with many matches, the first, a count per file, and all of them in a file.",
          json!({ "pattern": { "type": "string" }, "path": { "type": "string" }, "glob": { "type": "string", "description": "Only files matching this, e.g. *.rs" }, "ignore_case": { "type": "boolean" } }), &["pattern"]),
        f("find", "Find files by name with a glob (e.g. **/*.toml), respecting .gitignore. Up to 500.",
          json!({ "pattern": { "type": "string" }, "path": { "type": "string" } }), &["pattern"]),
        f("ls", "List a folder (folders end with /). Up to 500 entries.",
          json!({ "path": { "type": "string" } }), &[]),
        f("fetch", "Read a URL (http or https): a page as Markdown (its main part), text and JSON as they are, an image shown, another file saved for you to open with a command. Up to 50 KB shown; a longer page is saved whole to a file. \
                    For documentation, API references, issues, changelogs: what the project doesn't have.",
          json!({ "url": { "type": "string" } }), &["url"]),
        f("jobs", "Background commands (bash with background: true): list them, see one's last lines, wait for one to end (timeout seconds, 60 by default), or stop one and all it started.",
          json!({ "action": { "type": "string", "enum": ["list", "output", "wait", "stop"] }, "id": { "type": "string", "description": "j1, j2…, for output, wait and stop" },
                  "lines": { "type": "integer", "description": "How many last lines (50 by default)" }, "timeout": { "type": "integer" } }), &["action"]),
        f("ask", "Ask the user questions and wait for their answers, shown as a card with your options and a field for their own answer. \
               For a decision that's theirs, or one the task leaves open that changes what you'd do (which approach, what to keep, what they meant); not for what you can find out, or to ask leave for what they already asked for. \
               One to four questions, two to six options each (the one you recommend first). The answer comes back as text.",
          json!({ "questions": { "type": "array", "minItems": 1, "maxItems": 4, "items": {
              "type": "object",
              "properties": {
                  "question": { "type": "string", "description": "The whole question, ending with ?" },
                  "header": { "type": "string", "description": "A word or two" },
                  "options": { "type": "array", "minItems": 2, "maxItems": 6, "items": { "type": "object", "properties": {
                      "label": { "type": "string", "description": "A few words" },
                      "description": { "type": "string", "description": "What it means or leads to" },
                  }, "required": ["label"], "additionalProperties": false } },
                  "multiple": { "type": "boolean", "description": "May pick more than one (default false)" },
              },
              "required": ["question", "options"], "additionalProperties": false,
          } } }), &["questions"]),
        f("current_time", "The date and time now: local (with the zone and its offset from UTC), UTC, and Unix seconds.", json!({}), &[]),
        f("recall", "Get back a tool's output that was set aside to save context (its place in the conversation says so, with the id to give). It comes back as it was then; to see a file as it is now, read it again.",
          json!({ "id": { "type": "string" } }), &["id"]),
    ])
}

impl Tools {
    pub fn new(cwd: PathBuf, spill_dir: PathBuf) -> Self {
        let jobs = Arc::new(crate::jobs::Jobs::new(&spill_dir));
        Tools { cwd, spill_dir, seen: Mutex::new(HashMap::new()), told_generated: Mutex::new(HashSet::new()), originals: Mutex::new(HashMap::new()), sandbox: None, jobs, lsp: Arc::default(), rules_given: Mutex::default(), rules_new: Mutex::default() }
    }

    /// Outputs set aside, kept for `recall` (`replace`: the session's
    /// started over or changed, its old ones go).
    pub fn set_aside(&self, originals: impl IntoIterator<Item = (String, Original)>, replace: bool) {
        let mut kept = self.originals.lock().unwrap();
        if replace {
            kept.clear();
        }
        kept.extend(originals);
    }

    /// The rules of folders below the working directory, on the way down to
    /// `files`, that the model doesn't have yet: each folder's AGENTS.md (or
    /// CLAUDE.md), as text to add to the tool result that touched them.
    /// The working directory's own rules, and those above it, are in the
    /// instructions already; dependencies' and hidden folders' aren't sent.
    fn nested_rules(&self, files: &[PathBuf]) -> Option<String> {
        let mut out = String::new();
        let mut given = self.rules_given.lock().unwrap();
        let mut new = self.rules_new.lock().unwrap();
        for file in files {
            for rel in self.rule_folders(file) {
                if given.contains(&rel) {
                    continue;
                }
                let dir = self.cwd.join(&rel);
                let Some(rules) = ["AGENTS.md", "CLAUDE.md"].map(|n| dir.join(n)).into_iter().find(|p| p.is_file()) else { continue };
                given.insert(rel.clone());
                // (Hidden: nothing to add. Read itself: its text is the
                // result, which the model reads with the rest.)
                if self.hidden(&rules).is_some() {
                    continue;
                }
                if rules == *file {
                    new.insert(rel);
                    continue;
                }
                // (Read up to what's sent, however big the file.)
                let mut raw = Vec::new();
                let Ok(f) = std::fs::File::open(&rules) else { continue };
                if std::io::Read::read_to_end(&mut std::io::Read::take(f, NESTED_RULES_KEPT as u64 + 4), &mut raw).is_err() {
                    continue;
                }
                let whole = std::fs::metadata(&rules).map_or(0, |m| m.len()) as usize;
                let text = String::from_utf8_lossy(&raw);
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                let shown = rules.strip_prefix(&self.cwd).unwrap_or(&rules).display();
                out.push_str(&format!("\n\n{RULES_FROM}{shown}{RULES_FOR}{}/:]\n", rel.display()));
                new.insert(rel.clone());
                if whole > NESTED_RULES_KEPT {
                    let cut = text.floor_char_boundary(NESTED_RULES_KEPT);
                    out.push_str(&format!("{}\n[… cut at {} KB: read {shown} for the rest.]", &text[..cut], NESTED_RULES_KEPT / 1000));
                } else {
                    out.push_str(text);
                }
            }
        }
        (!out.is_empty()).then_some(out)
    }

    /// The folders below the working directory on the way down to `file`
    /// whose rules count (relative; not hidden ones, dependencies' or
    /// vendored ones, or what's below them).
    fn rule_folders(&self, file: &Path) -> Vec<PathBuf> {
        use std::path::Component;
        let mut out = Vec::new();
        let Some(folders) = file.strip_prefix(&self.cwd).ok().and_then(Path::parent) else { return out };
        let mut rel = PathBuf::new();
        for c in folders.components() {
            let Component::Normal(name) = c else { break };
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "node_modules" || name == "vendor" {
                break;
            }
            rel.push(&*name);
            out.push(rel.clone());
        }
        out
    }

    /// A folder on the way to `files` whose rules went out earlier in this
    /// batch: the model asked for the change before it read them.
    fn rules_unread(&self, files: &[PathBuf]) -> Option<PathBuf> {
        let new = self.rules_new.lock().unwrap();
        files.iter().flat_map(|f| self.rule_folders(f)).find(|rel| new.contains(rel))
    }

    /// A new batch of calls: the model has read the rules sent with the
    /// last one's results.
    pub fn batch_starts(&self) {
        self.rules_new.lock().unwrap().clear();
    }

    /// Which folders' rules the model has: the ones whose text is in
    /// `history` (its tool results not set aside), after it was loaded,
    /// compacted or had outputs set aside.
    pub fn rules_seen_in(&self, history: &[Value]) {
        let mut given = self.rules_given.lock().unwrap();
        given.clear();
        for m in history.iter().filter(|m| m["role"] == "toolResult" && m["harloStub"] != true) {
            for text in m["content"].as_array().into_iter().flatten().filter_map(|c| c["text"].as_str()) {
                for line in text.lines().filter_map(|l| l.strip_prefix(RULES_FROM)) {
                    if let Some(dir) = line.split_once(RULES_FOR).and_then(|(_, d)| d.strip_suffix("/:]")) {
                        given.insert(PathBuf::from(dir));
                    }
                }
            }
        }
    }

    /// Whether any output is set aside (so `recall` is offered).
    pub fn has_originals(&self) -> bool {
        !self.originals.lock().unwrap().is_empty()
    }

    /// An output set aside, as it was (with a note when the file it read
    /// has changed since).
    fn recall(&self, id: &str) -> Outcome {
        let id = id.trim();
        let Some(o) = self.originals.lock().unwrap().get(id).cloned() else {
            return fail(format!("Nothing was set aside with the id {id:?}: copy the id from the note where the output was."));
        };
        let text = o.result["content"].as_array().into_iter().flatten().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n");
        let when = o.result["timestamp"].as_u64().map(|ms| std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms));
        let changed = o.read.as_ref().filter(|p| {
            let modified = std::fs::metadata(self.path(&p.to_string_lossy())).and_then(|m| m.modified()).ok();
            matches!((when, modified), (Some(w), Some(m)) if m > w)
        });
        let mut out = Outcome::new(text, o.result["isError"] == true);
        out.images = o.result["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "image").cloned().collect();
        if let Some(p) = changed {
            out.text.push_str(&format!("\n\n[{} has changed since this was read: read it again for what it is now.]", p.display()));
        }
        out
    }

    /// The project's folder: the sandbox's, or the repository's top
    /// folder, or the working directory.
    pub fn project(&self) -> PathBuf {
        if let Some(sb) = &self.sandbox {
            return sb.project.clone();
        }
        std::process::Command::new("git")
            .arg("-C")
            .arg(&self.cwd)
            .args(["rev-parse", "--show-toplevel"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| self.cwd.clone())
    }

    /// Why the sandbox keeps the file tools from looking at `path`.
    fn hidden(&self, path: &Path) -> Option<Outcome> {
        self.sandbox.as_ref().and_then(|sb| sb.may_read(path).err()).map(fail)
    }

    /// Why the sandbox keeps the file tools from changing `path`.
    fn fenced(&self, path: &Path) -> Option<Outcome> {
        self.sandbox.as_ref().and_then(|sb| sb.may_write(path).err()).map(fail)
    }

    /// The hidden paths under `root`, as a search names them (whole, and
    /// from the working folder).
    fn hidden_names(&self, root: &Path) -> Vec<String> {
        let Some(sb) = &self.sandbox else { return Vec::new() };
        let base = format!("{}/", self.cwd.display());
        let under: Vec<String> = sb.hidden_under(root).iter().map(|h| h.to_string_lossy().into_owned()).collect();
        let shown = |h: &str| h.strip_prefix(&base).map(str::to_owned);
        under.iter().flat_map(|h| [Some(h.clone()), shown(h)]).flatten().collect()
    }

    fn path(&self, p: &str) -> PathBuf {
        let p = p.trim();
        let p = match p.strip_prefix("~/") {
            Some(rest) => std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(rest),
            None => PathBuf::from(p),
        };
        if p.is_absolute() { p } else { self.cwd.join(p) }
    }

    /// Remembers `path` as it is: `bytes`, what was just read from it or
    /// written to it.
    fn remember(&self, path: &Path, bytes: &[u8]) {
        let Ok(m) = std::fs::metadata(path) else { return };
        let Ok(modified) = m.modified() else { return };
        let seen = Seen { modified, size: m.len(), hash: hash_of(bytes), at: SystemTime::now() };
        self.seen.lock().unwrap().insert(path.to_path_buf(), seen);
    }

    /// Why `path` shouldn't be changed by hand (it's generated; `head`:
    /// its start), the first time.
    fn generated(&self, path: &Path, head: &[u8]) -> Option<Outcome> {
        let why = crate::generated::refusal(path, head)?;
        self.told_generated.lock().unwrap().insert(path.to_path_buf()).then(|| fail(why))
    }

    /// Whether `path` is still as the session last saw it: by its size and
    /// time, or by what's in it when those can't tell (`now`: its contents,
    /// when already read).
    fn unchanged(&self, path: &Path, seen: &Seen, now: Option<&[u8]>) -> bool {
        let Ok(m) = std::fs::metadata(path) else { return false };
        if m.len() != seen.size {
            return false;
        }
        let racy = seen.at.duration_since(seen.modified).map_or(true, |d| d < RACY);
        if m.modified().ok() == Some(seen.modified) && !racy {
            return true;
        }
        // (Touched, or rewritten as it was: still what was read.)
        match now {
            Some(bytes) => hash_of(bytes) == seen.hash,
            None => std::fs::read(path).is_ok_and(|b| hash_of(&b) == seen.hash),
        }
    }

    /// Why an edit or overwrite of `path` must wait, if it must (`now`:
    /// what's in it, when already read).
    fn stale(&self, path: &Path, must_have_read: bool, now: Option<&[u8]>) -> Option<String> {
        if !path.exists() {
            return None;
        }
        let seen = self.seen.lock().unwrap().get(path).copied();
        match seen {
            Some(seen) if self.unchanged(path, &seen, now) => None,
            Some(_) => Some(format!("{} changed since you last read it (someone else edited it). Read it again, then make your change.", path.display())),
            None if must_have_read => Some(format!("{} exists and you haven't read it in this session: read it first, then replace it (or change part of it with edit).", path.display())),
            None => None,
        }
    }

    /// A command: `stop` (the turn's) ends it, with what it printed so far.
    pub async fn bash(&self, args: &Value, stop: Option<&tokio_util::sync::CancellationToken>) -> Outcome {
        let s = |k: &str| args[k].as_str().unwrap_or_default();
        let sandbox = self.sandbox.as_deref();
        if let Some(why) = sandbox.and_then(|sb| sb.broken.as_ref()) {
            return fail(format!("Commands can't run: the sandbox can't start here ({why}). Install bubblewrap, or turn Harlo's sandbox off in Settings → Providers → Harlo."));
        }
        let inside = sandbox.filter(|_| args["outside_sandbox"] != true);
        if args["background"] == true {
            let (text, failed) = self.jobs.start(s("command"), &self.cwd, args["timeout"].as_u64(), inside);
            return Outcome::new(text, failed);
        }
        let (text, failed) = bash::run(s("command"), &self.cwd, args["timeout"].as_u64(), &self.spill_dir, inside, stop).await;
        Outcome::new(text, failed)
    }

    pub async fn call(&self, name: &str, args: &Value) -> Outcome {
        let s = |k: &str| args[k].as_str().unwrap_or_default();
        match name {
            "read" => {
                let mut out = self.read(s("path"), args["offset"].as_u64(), args["limit"].as_u64()).await;
                if !out.failed
                    && let Some(rules) = self.nested_rules(&[self.path(s("path"))])
                {
                    out.text.push_str(&rules);
                }
                out
            }
            "write" | "edit" | "apply_patch" => {
                // A folder's own rules come before a change in it, not after:
                // the first change there is held back with them, to be made
                // again if it still fits.
                let touched: Vec<PathBuf> = match name {
                    "apply_patch" => crate::patch::parse(s("input"))
                        .unwrap_or_default()
                        .iter()
                        .flat_map(|op| match op {
                            crate::patch::Op::Add { path, .. } | crate::patch::Op::Delete { path } => vec![path],
                            crate::patch::Op::Update { path, move_to, .. } => std::iter::once(path).chain(move_to.as_ref()).collect(),
                        })
                        .map(|p| self.path(p))
                        .collect(),
                    _ => vec![self.path(s("path"))],
                };
                if let Some(rules) = self.nested_rules(&touched) {
                    return fail(format!("Nothing was changed yet: this touches a folder with rules of its own, which you hadn't seen (below). Check the change follows them, then make it again.{rules}"));
                }
                // (Its rules came with another call's result in this reply:
                // asked for before they were read.)
                if let Some(folder) = self.rules_unread(&touched) {
                    return fail(format!("Nothing was changed yet: {}/ has rules of its own, which came with another result of this reply, after you asked for this. Check the change follows them, then make it again.", folder.display()));
                }
                let mut out = match name {
                    "write" => self.write(s("path"), s("content")),
                    "edit" => self.edit(s("path"), &args["edits"]),
                    _ => self.patch(s("input")),
                };
                if out.failed {
                    return out;
                }
                let changed: Vec<PathBuf> = match name {
                    "apply_patch" => crate::patch::parse(s("input"))
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|op| match op {
                            crate::patch::Op::Add { path, .. } => Some(path),
                            crate::patch::Op::Update { move_to: Some(to), .. } => Some(to),
                            crate::patch::Op::Update { path, .. } => Some(path),
                            crate::patch::Op::Delete { .. } => None,
                        })
                        .map(|p| self.path(p))
                        .collect(),
                    _ => vec![self.path(s("path"))],
                };
                if crate::lsp::diagnostics_on()
                    && let Some(errors) = self.lsp.errors_after(&changed, &self.cwd, self.sandbox.as_deref()).await
                {
                    out.text = format!("{}\n\n{errors}", out.text);
                }
                out
            }
            "bash" => self.bash(args, None).await,
            "grep" => self.grep(s("pattern"), s("path"), s("glob"), args["ignore_case"].as_bool() == Some(true)).await,
            "find" => self.find(s("pattern"), s("path")).await,
            "ls" => self.ls(s("path")),
            "recall" => self.recall(s("id")),
            "fetch" => crate::fetch::run(s("url"), &self.spill_dir).await,
            "todo" => crate::todo::run(args),
            "lsp" => {
                let path = self.path(s("path"));
                if let Some(no) = self.hidden(&path) {
                    return no;
                }
                self.lsp.call(args, path, &self.cwd, self.sandbox.as_deref()).await
            }
            "jobs" => {
                let (text, failed) = self.jobs.call(args).await;
                Outcome::new(text, failed)
            }
            "current_time" => ok(crate::clock::describe(&crate::clock::now())),
            _ => fail(format!("There's no tool called {name}.")),
        }
    }

    async fn read(&self, p: &str, offset: Option<u64>, limit: Option<u64>) -> Outcome {
        if p.trim().is_empty() {
            return fail("Give the file's path.");
        }
        let path = self.path(p);
        if let Some(no) = self.hidden(&path) {
            return no;
        }
        // A big one (not a picture): read as it comes, not whole.
        if std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() > BIG_READ) && !self.picture(&path) {
            return self.read_big(path, offset, limit).await;
        }
        // (Off the async threads: a large file, or a slow disk, doesn't
        // hold up a stop.)
        let reading = path.clone();
        let read = tokio::task::spawn_blocking(move || std::fs::read(reading)).await.unwrap_or_else(|e| Err(std::io::Error::other(e.to_string())));
        let bytes = match read {
            Ok(b) => b,
            Err(e) if path.is_dir() => return fail(format!("{} is a folder: use ls ({e}).", path.display())),
            Err(e) => return fail(format!("Can't read {}: {e}", path.display())),
        };
        self.remember(&path, &bytes);
        if crate::images::mime_of(&bytes).is_some() {
            return match crate::images::fit_apart(bytes).await {
                Ok(f) => {
                    let mut out = ok(format!("[Image: {}, {}.]", path.display(), f.describe()));
                    out.images.push(f.block());
                    out
                }
                Err(why) => fail(format!("{} is an image, but it can't be shown: {why}.", path.display())),
            };
        }
        if bytes[..bytes.len().min(8192)].contains(&0) {
            return ok(format!("{} is a binary file ({} bytes).", path.display(), bytes.len()));
        }
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            return ok("(empty file)");
        }
        let start = offset.unwrap_or(1).max(1) as usize - 1;
        if start >= lines.len() {
            return fail(format!("The file has {} lines; offset {} is past its end.", lines.len(), start + 1));
        }
        let preview = offset.is_none() && limit.is_none() && (lines.len() > READ_LINES || text.len() > READ_BYTES);
        if preview
            && crate::outline::on()
            && let Some(outline) = crate::outline::of(&path, &text)
        {
            return ok(format!(
                "[{} is {} lines ({} KB), more than one read shows. Its outline follows (line: definition): read the parts you need with offset and limit (up to {READ_LINES} lines), or grep it.]\n{outline}",
                path.display(),
                lines.len(),
                text.len().div_ceil(1024),
            ));
        }
        let (max, max_bytes) = if preview { (PREVIEW_LINES, PREVIEW_BYTES) } else { (limit.map_or(READ_LINES, |l| (l as usize).clamp(1, READ_LINES)), READ_BYTES) };
        let mut out = String::new();
        let mut n = 0;
        let anchored = crate::hashline::on();
        for l in &lines[start..] {
            if n >= max || out.len() + l.len() + 1 > max_bytes {
                break;
            }
            if anchored {
                out.push_str(&crate::hashline::annotate(start + n + 1, l));
            } else {
                out.push_str(l);
            }
            out.push('\n');
            n += 1;
        }
        if n == 0 {
            // One line longer than the limit: its start, said so.
            out = long_line(&path, start + 1, lines[start], lines[start].len());
            n = 1;
        }
        let end = start + n;
        if preview {
            return ok(format!(
                "[{} is {} lines ({} KB), more than one read shows. These are lines 1-{end}: grep it for what you need, or read a range with offset and limit (up to {READ_LINES} lines).]\n{}",
                path.display(),
                lines.len(),
                text.len().div_ceil(1024),
                out.trim_end_matches('\n')
            ));
        }
        if end < lines.len() {
            out.push_str(&format!("\n[Lines {}-{} of {}. Read on with offset={}.]", start + 1, end, lines.len(), end + 1));
        }
        ok(out.trim_end_matches('\n').to_owned())
    }

    /// Whether `path` starts as a picture does.
    fn picture(&self, path: &Path) -> bool {
        use std::io::Read;
        let mut head = [0u8; 64];
        let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut head)).unwrap_or(0);
        crate::images::mime_of(&head[..n]).is_some()
    }

    /// `read` of a file past [`BIG_READ`]: its lines counted and the ones
    /// asked for kept as it's read, the rest not held (no outline: it would
    /// need the whole text).
    async fn read_big(&self, path: PathBuf, offset: Option<u64>, limit: Option<u64>) -> Outcome {
        let start = offset.unwrap_or(1).max(1) as usize - 1;
        let preview = offset.is_none() && limit.is_none();
        let (max, max_bytes) = if preview { (PREVIEW_LINES, PREVIEW_BYTES) } else { (limit.map_or(READ_LINES, |l| (l as usize).clamp(1, READ_LINES)), READ_BYTES) };
        let reading = path.clone();
        let read = tokio::task::spawn_blocking(move || stream_lines(&reading, start, max, max_bytes)).await.unwrap_or_else(|e| Err(std::io::Error::other(e.to_string())));
        let got = match read {
            Ok(g) => g,
            Err(e) => return fail(format!("Can't read {}: {e}", path.display())),
        };
        if let (Ok(m), Ok(modified)) = (std::fs::metadata(&path), std::fs::metadata(&path).and_then(|m| m.modified())) {
            self.seen.lock().unwrap().insert(path.clone(), Seen { modified, size: m.len(), hash: got.hash, at: SystemTime::now() });
        }
        if got.binary {
            return ok(format!("{} is a binary file ({} bytes).", path.display(), got.bytes));
        }
        if start >= got.lines {
            return fail(format!("The file has {} lines; offset {} is past its end.", got.lines, start + 1));
        }
        let kept: Vec<String> = got.kept.iter().map(|l| String::from_utf8_lossy(l).into_owned()).collect();
        let mut out = String::new();
        let mut n = 0;
        let anchored = crate::hashline::on();
        for l in &kept {
            if n >= max || out.len() + l.len() + 1 > max_bytes {
                break;
            }
            if anchored {
                out.push_str(&crate::hashline::annotate(start + n + 1, l));
            } else {
                out.push_str(l);
            }
            out.push('\n');
            n += 1;
        }
        if n == 0 {
            // One line longer than the limit: its start, said so.
            out = kept.first().map(|l| long_line(&path, start + 1, l, got.first_len)).unwrap_or_default();
            n = 1;
        }
        let end = start + n;
        if preview {
            return ok(format!(
                "[{} is {} lines ({} KB), more than one read shows. These are lines 1-{end}: grep it for what you need, or read a range with offset and limit (up to {READ_LINES} lines).]\n{}",
                path.display(),
                got.lines,
                got.bytes.div_ceil(1024),
                out.trim_end_matches('\n')
            ));
        }
        if end < got.lines {
            out.push_str(&format!("\n[Lines {}-{} of {}. Read on with offset={}.]", start + 1, end, got.lines, end + 1));
        }
        ok(out.trim_end_matches('\n').to_owned())
    }

    fn write(&self, p: &str, content: &str) -> Outcome {
        if p.trim().is_empty() {
            return fail("Give the file's path.");
        }
        let path = self.path(p);
        if let Some(no) = self.fenced(&path) {
            return no;
        }
        if let Some(why) = self.stale(&path, true, None) {
            return fail(why);
        }
        if let Some(no) = std::fs::read(&path).ok().and_then(|head| self.generated(&path, &head)) {
            return no;
        }
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return fail(format!("Can't make {}: {e}", parent.display()));
        }
        let existed = path.exists();
        if let Err(e) = std::fs::write(&path, content) {
            return fail(format!("Can't write {}: {e}", path.display()));
        }
        self.remember(&path, content.as_bytes());
        ok(format!("{} {} ({} lines).", if existed { "Replaced" } else { "Created" }, path.display(), content.lines().count()))
    }

    fn edit(&self, p: &str, edits: &Value) -> Outcome {
        if crate::hashline::on() {
            return self.edit_anchored(p, edits);
        }
        let path = self.path(p);
        let list: Vec<edit::Edit> = edits
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| edit::Edit { old: e["old_text"].as_str().unwrap_or_default().to_owned(), new: e["new_text"].as_str().unwrap_or_default().to_owned() })
            .collect();
        if list.is_empty() {
            return fail("Give at least one edit (old_text and new_text).");
        }
        if let Some(no) = self.fenced(&path) {
            return no;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => return fail(format!("Can't read {}: {e}", path.display())),
        };
        if let Some(why) = self.stale(&path, false, Some(content.as_bytes())) {
            return fail(why);
        }
        if let Some(no) = self.generated(&path, content.as_bytes()) {
            return no;
        }
        match edit::apply(&content, &list) {
            Ok((new, first)) => {
                if let Err(e) = std::fs::write(&path, &new) {
                    return fail(format!("Can't write {}: {e}", path.display()));
                }
                self.remember(&path, new.as_bytes());
                ok(format!("Edited {} ({} change{}, from line {first}).", path.display(), list.len(), if list.len() == 1 { "" } else { "s" }))
            }
            Err(why) => fail(why),
        }
    }

    /// `edit` by anchors (`HARLO_AGENT_EDIT=hashline`): a stale line is
    /// caught by its tag, so a file changed elsewhere since isn't refused
    /// as a whole.
    fn edit_anchored(&self, p: &str, edits: &Value) -> Outcome {
        let path = self.path(p);
        if let Some(no) = self.fenced(&path) {
            return no;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => return fail(format!("Can't read {}: {e}", path.display())),
        };
        if let Some(no) = self.generated(&path, content.as_bytes()) {
            return no;
        }
        match crate::hashline::apply(&content, edits) {
            Ok((new, view)) => {
                if let Err(e) = std::fs::write(&path, &new) {
                    return fail(format!("Can't write {}: {e}", path.display()));
                }
                self.remember(&path, new.as_bytes());
                ok(format!("Edited {}. Now:\n{view}", path.display()))
            }
            Err(why) => fail(why),
        }
    }

    /// `apply_patch` (`HARLO_AGENT_EDIT=patch`): every file's change is
    /// worked out and checked first, then all are written.
    fn patch(&self, input: &str) -> Outcome {
        use crate::patch::Op;
        let ops = match crate::patch::parse(input) {
            Ok(ops) => ops,
            Err(why) => return fail(why),
        };
        // What each file becomes (None: removed), and what's said of it.
        let mut plan: Vec<(PathBuf, Option<String>)> = Vec::new();
        let mut said: Vec<String> = Vec::new();
        for op in &ops {
            let path = self.path(op.path());
            if plan.iter().any(|(p, _)| *p == path) {
                return fail(format!("{} is in the patch twice: give all of a file's hunks under one `*** Update File:`.", op.path()));
            }
            if let Some(no) = self.fenced(&path) {
                return no;
            }
            match op {
                Op::Add { content, .. } => {
                    if let Some(why) = self.stale(&path, true, None) {
                        return fail(why);
                    }
                    if let Some(no) = std::fs::read(&path).ok().and_then(|head| self.generated(&path, &head)) {
                        return no;
                    }
                    let n = content.lines().count();
                    said.push(format!("{} {} ({n} line{})", if path.exists() { "replaced" } else { "created" }, op.path(), if n == 1 { "" } else { "s" }));
                    plan.push((path, Some(content.clone())));
                }
                Op::Delete { .. } => {
                    if !path.is_file() {
                        return fail(format!("Can't delete {}: there's no such file.", op.path()));
                    }
                    if let Some(why) = self.stale(&path, false, None) {
                        return fail(why);
                    }
                    said.push(format!("deleted {}", op.path()));
                    plan.push((path, None));
                }
                Op::Update { move_to, hunks, .. } => {
                    let content = match std::fs::read_to_string(&path) {
                        Ok(c) => c,
                        Err(e) => return fail(format!("Can't read {}: {e}", op.path())),
                    };
                    if let Some(why) = self.stale(&path, false, Some(content.as_bytes())) {
                        return fail(why);
                    }
                    if let Some(no) = self.generated(&path, content.as_bytes()) {
                        return no;
                    }
                    let (new, first) = if hunks.is_empty() { (content, 1) } else {
                        match crate::patch::apply(&content, hunks, op.path()) {
                            Ok(r) => r,
                            Err(why) => return fail(why),
                        }
                    };
                    let n = hunks.len();
                    let changes = if n == 0 { String::new() } else { format!(" ({n} hunk{}, from line {first})", if n == 1 { "" } else { "s" }) };
                    match move_to {
                        Some(to) => {
                            let dest = self.path(to);
                            if let Some(no) = self.fenced(&dest) {
                                return no;
                            }
                            if let Some(why) = self.stale(&dest, true, None) {
                                return fail(why);
                            }
                            said.push(format!("edited {} and moved it to {to}{changes}", op.path()));
                            plan.push((path, None));
                            plan.push((dest, Some(new)));
                        }
                        None => {
                            said.push(format!("edited {}{changes}", op.path()));
                            plan.push((path, Some(new)));
                        }
                    }
                }
            }
        }
        for (path, content) in &plan {
            let done = match content {
                Some(c) => path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(path, c)),
                None => std::fs::remove_file(path),
            };
            if let Err(e) = done {
                return fail(format!("Can't change {}: {e} (the files before it in the patch were changed).", path.display()));
            }
            match content {
                Some(c) => self.remember(path, c.as_bytes()),
                None => {
                    self.seen.lock().unwrap().remove(path);
                }
            }
        }
        let mut text = said.join("; ");
        if let Some(c) = text.get(..1) {
            text = format!("{}{}.", c.to_uppercase(), &text[1..]);
        }
        ok(text)
    }

    fn ls(&self, p: &str) -> Outcome {
        let path = if p.trim().is_empty() { self.cwd.clone() } else { self.path(p) };
        if let Some(no) = self.hidden(&path) {
            return no;
        }
        let entries = match std::fs::read_dir(&path) {
            Ok(e) => e,
            Err(e) => return fail(format!("Can't list {}: {e}", path.display())),
        };
        let mut names: Vec<String> = entries.flatten().map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if e.file_type().is_ok_and(|t| t.is_dir()) { format!("{name}/") } else { name }
        }).collect();
        names.sort();
        let total = names.len();
        names.truncate(LIST_MAX);
        let mut out = names.join("\n");
        if total > LIST_MAX {
            out.push_str(&format!("\n[{} more not shown]", total - LIST_MAX));
        }
        ok(if out.is_empty() { "(empty folder)".into() } else { out })
    }

    /// A search whose output is taken a line at a time as it comes (`each`;
    /// each line cut at [`LINE_CAP`]), not all of it kept: stopped with the
    /// call, or after [`search_secs`]. Its exit and what it said on stderr
    /// (the start).
    async fn search_lines(&self, program: &str, args: &[String], each: &mut (dyn FnMut(&[u8]) + Send)) -> std::io::Result<(std::process::ExitStatus, String)> {
        use tokio::io::AsyncReadExt;
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args).current_dir(&self.cwd).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
        let mut child = cmd.spawn()?;
        let (mut out, mut err) = (child.stdout.take().expect("piped"), child.stderr.take().expect("piped"));
        let work = async {
            let (mut buf, mut ebuf) = (vec![0u8; 1 << 16], [0u8; 4096]);
            let (mut line, mut said) = (Vec::new(), Vec::new());
            let (mut o_open, mut e_open) = (true, true);
            while o_open || e_open {
                tokio::select! {
                    n = out.read(&mut buf), if o_open => match n {
                        Ok(0) | Err(_) => o_open = false,
                        Ok(n) => {
                            let mut rest = &buf[..n];
                            while let Some(i) = rest.iter().position(|b| *b == b'\n') {
                                line.extend_from_slice(&rest[..i.min(LINE_CAP.saturating_sub(line.len()))]);
                                each(&line);
                                line.clear();
                                rest = &rest[i + 1..];
                            }
                            line.extend_from_slice(&rest[..rest.len().min(LINE_CAP.saturating_sub(line.len()))]);
                        }
                    },
                    n = err.read(&mut ebuf), if e_open => match n {
                        Ok(0) | Err(_) => e_open = false,
                        Ok(n) => said.extend_from_slice(&ebuf[..n.min((64usize << 10).saturating_sub(said.len()))]),
                    },
                }
            }
            if !line.is_empty() {
                each(&line);
            }
            let status = child.wait().await?;
            Ok::<_, std::io::Error>((status, String::from_utf8_lossy(&said).into_owned()))
        };
        let secs = search_secs();
        match tokio::time::timeout(std::time::Duration::from_secs(secs), work).await {
            Ok(r) => r,
            Err(_) => Err(std::io::Error::new(std::io::ErrorKind::TimedOut, format!("The search ran past {secs} s and was stopped: narrow the path, pattern or glob."))),
        }
    }

    async fn grep(&self, pattern: &str, p: &str, glob: &str, ignore_case: bool) -> Outcome {
        if pattern.is_empty() {
            return fail("Give a pattern.");
        }
        let path = if p.trim().is_empty() { self.cwd.clone() } else { self.path(p) };
        if let Some(no) = self.hidden(&path) {
            return no;
        }
        let mut args: Vec<String> = vec!["-n".into(), "--no-heading".into(), "--color".into(), "never".into(), "--max-columns".into(), "300".into(), "-g".into(), "!.git/".into()];
        if ignore_case {
            args.push("-i".into());
        }
        if !glob.is_empty() {
            args.extend(["-g".into(), glob.into()]);
        }
        args.extend(["-e".into(), pattern.into(), "--".into(), path.to_string_lossy().into_owned()]);
        let mut got = Matches::new(format!("{}/", self.cwd.display()), self.hidden_names(&path), path.is_dir(), self.spill_dir.clone());
        let ran = match self.search_lines("rg", &args, &mut |l| got.take(l)).await {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // No ripgrep: grep.
                let mut g = vec!["-rnI".to_owned()];
                if ignore_case {
                    g.push("-i".into());
                }
                if !glob.is_empty() {
                    g.push(format!("--include={glob}"));
                }
                g.extend(["-E".into(), "-e".into(), pattern.into(), "--".into(), path.to_string_lossy().into_owned()]);
                self.search_lines("grep", &g, &mut |l| got.take(l)).await
            }
            r => r,
        };
        let (status, said) = match ran {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => return fail(e.to_string()),
            Err(e) => return fail(format!("Can't search: {e}")),
        };
        if !status.success() && got.seen == 0 {
            return if said.trim().is_empty() { ok("No matches.") } else { fail(said.trim().to_owned()) };
        }
        if got.total == 0 {
            return ok("No matches.");
        }
        if got.spilled() {
            return ok(got.many());
        }
        let total = got.total;
        let mut shown = got.first.into_iter().take(GREP_MAX).collect::<Vec<_>>().join("\n");
        if total > GREP_MAX {
            shown.push_str(&format!("\n[{} matches; first {GREP_MAX} shown. Narrow the pattern, path or glob.]", total));
        }
        ok(shown)
    }

    async fn find(&self, pattern: &str, p: &str) -> Outcome {
        if pattern.is_empty() {
            return fail("Give a glob, e.g. **/*.rs");
        }
        let path = if p.trim().is_empty() { self.cwd.clone() } else { self.path(p) };
        if let Some(no) = self.hidden(&path) {
            return no;
        }
        let at = path.to_string_lossy().into_owned();
        let rg: Vec<String> = ["--files", "--color", "never", "-g", pattern, "-g", "!.git/", "--", &at].map(str::to_owned).into();
        // (Taken as they come: the first LIST_MAX by name kept, a max-heap of
        // them, and how many there are; not every path held and sorted.)
        let base = format!("{}/", self.cwd.display());
        let under = self.hidden_names(&path);
        let mut first = std::collections::BinaryHeap::<String>::new();
        let mut total = 0usize;
        let mut take = |raw: &[u8]| {
            let l = String::from_utf8_lossy(raw);
            let l = l.strip_prefix(base.as_str()).unwrap_or(&l);
            if l.is_empty() || names_hidden(&under, l) {
                return;
            }
            total += 1;
            if first.len() < LIST_MAX {
                first.push(l.to_owned());
            } else if first.peek().is_some_and(|last| l < last.as_str()) {
                first.pop();
                first.push(l.to_owned());
            }
        };
        let ran = match self.search_lines("rg", &rg, &mut take).await {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let name = pattern.rsplit('/').next().unwrap_or(pattern);
                let find: Vec<String> = [&at, "-name", name, "-not", "-path", "*/.git/*"].map(str::to_owned).into();
                self.search_lines("find", &find, &mut take).await
            }
            r => r,
        };
        match ran {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => return fail(e.to_string()),
            Err(e) => return fail(format!("Can't search: {e}")),
        }
        let mut shown = first.into_sorted_vec().join("\n");
        if total > LIST_MAX {
            shown.push_str(&format!("\n[{total} files; first {LIST_MAX} shown]"));
        }
        ok(if shown.is_empty() { "No files match.".into() } else { shown })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder's own AGENTS.md comes once, with the first file read or
    /// changed under it; the top folder's (in the instructions) doesn't.
    #[tokio::test]
    async fn a_folders_rules_come_once_with_its_first_file() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-nested-rules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["billing/ledger", "web", "node_modules/dep", ".hidden"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::write(dir.join("AGENTS.md"), "Top rule.").unwrap();
        std::fs::write(dir.join("billing/AGENTS.md"), "Money is in cents.").unwrap();
        std::fs::write(dir.join("billing/ledger/CLAUDE.md"), "Entries never change.").unwrap();
        std::fs::write(dir.join("node_modules/dep/AGENTS.md"), "A dependency's rule.").unwrap();
        std::fs::write(dir.join(".hidden/AGENTS.md"), "Hidden rule.").unwrap();
        std::fs::write(dir.join("web/AGENTS.md"), "x".repeat(NESTED_RULES_KEPT + 500)).unwrap();
        for f in ["main.rs", "billing/ledger/post.rs", "billing/tax.rs", "node_modules/dep/i.js", ".hidden/a"] {
            std::fs::write(dir.join(f), "fn x() {}\n").unwrap();
        }
        let t = Tools::new(dir.clone(), dir.join(".out"));
        async fn read(t: &Tools, p: &str) -> String {
            t.call("read", &json!({ "path": p })).await.text
        }
        // None here: the top folder's are in the instructions already.
        assert!(!read(&t, "main.rs").await.contains(RULES_FROM));
        let first = read(&t, "billing/ledger/post.rs").await;
        assert!(first.contains("[Rules from billing/AGENTS.md, for the files under billing/:]\nMoney is in cents."), "{first}");
        assert!(first.contains("[Rules from billing/ledger/CLAUDE.md, for the files under billing/ledger/:]\nEntries never change."), "{first}");
        assert!(!first.contains("Top rule."), "{first}");
        // Once: not with the next file there, nor with an edit (in the
        // model's next reply, once it read them).
        t.batch_starts();
        assert!(!read(&t, "billing/tax.rs").await.contains(RULES_FROM));
        let edited = t.call("edit", &json!({ "path": "billing/tax.rs", "edits": [{ "old_text": "x", "new_text": "y" }] })).await;
        assert!(!edited.failed && !edited.text.contains(RULES_FROM), "{}", edited.text);
        // A dependency's and a hidden folder's aren't sent.
        assert!(!read(&t, "node_modules/dep/i.js").await.contains(RULES_FROM));
        assert!(!read(&t, ".hidden/a").await.contains(RULES_FROM));
        // A new file, written first thing in a folder with rules: held back,
        // with them (cut to size), then made when asked again.
        let held = t.call("write", &json!({ "path": "web/app.js", "content": "1" })).await;
        assert!(held.failed && held.text.starts_with("Nothing was changed yet") && !dir.join("web/app.js").exists(), "{}", held.text);
        assert!(held.text.contains("[Rules from web/AGENTS.md, for the files under web/:]"), "{}", held.text);
        assert!(held.text.contains("cut at 8 KB: read web/AGENTS.md for the rest") && held.text.len() < NESTED_RULES_KEPT + 500, "{}", held.text.len());
        t.batch_starts();
        let written = t.call("write", &json!({ "path": "web/app.js", "content": "1" })).await;
        assert!(!written.failed && !written.text.contains(RULES_FROM) && dir.join("web/app.js").exists(), "{}", written.text);
        // A patch: a deletion counts its folder, a move both ends.
        std::fs::create_dir_all(dir.join("ops")).unwrap();
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(dir.join("ops/AGENTS.md"), "Keep the scripts.").unwrap();
        std::fs::write(dir.join("lib/AGENTS.md"), "Library rules.").unwrap();
        std::fs::write(dir.join("ops/run.sh"), "echo\n").unwrap();
        let gone = t.call("apply_patch", &json!({ "input": "*** Begin Patch\n*** Delete File: ops/run.sh\n*** End Patch" })).await;
        assert!(gone.failed && gone.text.contains("[Rules from ops/AGENTS.md") && dir.join("ops/run.sh").exists(), "{}", gone.text);
        let moved = t.call("apply_patch", &json!({ "input": "*** Begin Patch\n*** Update File: main.rs\n*** Move to: lib/main.rs\n@@\n-fn x() {}\n+fn y() {}\n*** End Patch" })).await;
        assert!(moved.failed && moved.text.contains("[Rules from lib/AGENTS.md") && !moved.text.contains("ops/AGENTS.md") && dir.join("main.rs").exists(), "{}", moved.text);
        t.batch_starts();
        let moved = t.call("apply_patch", &json!({ "input": "*** Begin Patch\n*** Update File: main.rs\n*** Move to: lib/main.rs\n@@\n-fn x() {}\n+fn y() {}\n*** End Patch" })).await;
        assert!(!moved.failed && dir.join("lib/main.rs").exists(), "{}", moved.text);
        // Gone from the context (set aside, or compacted): sent again.
        let history = vec![json!({ "role": "toolResult", "toolName": "read", "content": [{ "type": "text", "text": first }] })];
        t.rules_seen_in(&history);
        assert!(!read(&t, "billing/tax.rs").await.contains(RULES_FROM), "billing's are still in the history");
        assert!(read(&t, "web/app.js").await.contains("[Rules from web/AGENTS.md"), "web's aren't any more");
        t.rules_seen_in(&[json!({ "role": "toolResult", "harloStub": true, "content": [{ "type": "text", "text": first }] })]);
        assert!(read(&t, "billing/tax.rs").await.contains("[Rules from billing/AGENTS.md"), "a stub doesn't keep them");
        // Reading the rules file itself counts as having them.
        let t = Tools::new(dir.clone(), dir.join(".out"));
        assert!(!read(&t, "billing/AGENTS.md").await.contains(RULES_FROM));
        assert!(!read(&t, "billing/tax.rs").await.contains("billing/AGENTS.md"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Rules that came with one call's result hold back every change in
    /// their folder later in the same batch: the model asked for those
    /// before it read them.
    #[tokio::test]
    async fn rules_from_a_batch_hold_its_later_changes() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-batch-rules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["billing/ledger", "web", "lib"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::write(dir.join("billing/AGENTS.md"), "New files start with a header.").unwrap();
        std::fs::write(dir.join("web/AGENTS.md"), "Tabs.").unwrap();
        std::fs::write(dir.join("lib/AGENTS.md"), "Library rules.").unwrap();
        std::fs::write(dir.join("web/a.js"), "1\n").unwrap();
        std::fs::write(dir.join("main.rs"), "fn x() {}\n").unwrap();
        let t = Tools::new(dir.clone(), dir.join(".out"));
        async fn write_to(t: &Tools, p: &str) -> Outcome {
            t.call("write", &json!({ "path": p, "content": "no header" })).await
        }
        let write = |p: &'static str| write_to(&t, p);
        // Two writes in one reply: the first brings the rules, the second
        // (deeper, the same folder's rules) waits too, and says why.
        t.batch_starts();
        let first = write("billing/first.txt").await;
        let second = write("billing/ledger/second.txt").await;
        assert!(first.failed && first.text.contains("[Rules from billing/AGENTS.md"), "{}", first.text);
        assert!(second.failed && second.text.starts_with("Nothing was changed yet: billing/ has rules") && !second.text.contains(RULES_FROM), "{}", second.text);
        assert!(!dir.join("billing/first.txt").exists() && !dir.join("billing/ledger/second.txt").exists());
        // The same call twice: both wait.
        let again = write("billing/first.txt").await;
        assert!(again.failed && !dir.join("billing/first.txt").exists(), "{}", again.text);
        // A file elsewhere isn't held.
        let elsewhere = write("notes.txt").await;
        assert!(!elsewhere.failed && dir.join("notes.txt").exists(), "{}", elsewhere.text);
        // The next reply: made.
        t.batch_starts();
        assert!(!write("billing/first.txt").await.failed && dir.join("billing/first.txt").exists());
        assert!(!write("billing/ledger/second.txt").await.failed);
        // A read, then a change, in one reply: the read brings the rules, the
        // change waits; so does one after reading the rules file itself.
        t.batch_starts();
        assert!(t.call("read", &json!({ "path": "web/a.js" })).await.text.contains("[Rules from web/AGENTS.md"));
        let edited = t.call("edit", &json!({ "path": "web/a.js", "edits": [{ "old_text": "1", "new_text": "2" }] })).await;
        assert!(edited.failed && edited.text.starts_with("Nothing was changed yet: web/ has rules"), "{}", edited.text);
        assert_eq!(std::fs::read_to_string(dir.join("web/a.js")).unwrap(), "1\n");
        let t = Tools::new(dir.clone(), dir.join(".out"));
        t.batch_starts();
        assert!(t.call("read", &json!({ "path": "lib/AGENTS.md" })).await.text.contains("Library rules."));
        let moved = t.call("apply_patch", &json!({ "input": "*** Begin Patch\n*** Update File: main.rs\n*** Move to: lib/main.rs\n@@\n-fn x() {}\n+fn y() {}\n*** End Patch" })).await;
        assert!(moved.failed && moved.text.starts_with("Nothing was changed yet: lib/ has rules") && dir.join("main.rs").exists(), "{}", moved.text);
        t.batch_starts();
        assert!(!t.call("apply_patch", &json!({ "input": "*** Begin Patch\n*** Update File: main.rs\n*** Move to: lib/main.rs\n@@\n-fn x() {}\n+fn y() {}\n*** End Patch" })).await.failed);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn many_matches_show_the_first_and_where_the_rest_are() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-grep-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("logs")).unwrap();
        std::fs::write(dir.join("logs/a.log"), (1..=150).map(|i| format!("event {i} ok\n")).collect::<String>()).unwrap();
        std::fs::write(dir.join("logs/b.log"), (1..=30).map(|i| format!("event {i} late\n")).collect::<String>()).unwrap();
        let t = Tools::new(dir.clone(), dir.join(".out"));
        let out = t.call("grep", &json!({ "pattern": "event", "path": "logs" })).await.text;
        assert!(out.starts_with("[180 matches in 2 files (logs/a.log 150, logs/b.log 30); the first 100 shown. All of them are in "), "{}", &out[..200]);
        assert_eq!(out.lines().count(), 101, "the note, then 100 matches");
        let saved = out.split("All of them are in ").nth(1).unwrap().split(": grep that").next().unwrap();
        assert_eq!(std::fs::read_to_string(saved).unwrap().lines().count(), 180);
        // In one file: no count per file. A few: all of them, as they are.
        let one = t.call("grep", &json!({ "pattern": "ok", "path": "logs/a.log" })).await.text;
        assert!(one.starts_with("[150 matches; the first 100 shown."), "{}", &one[..120]);
        let few = t.call("grep", &json!({ "pattern": "late", "path": "logs" })).await.text;
        assert_eq!(few.lines().count(), 30);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_big_file_is_read_as_it_comes_the_same() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-bigread-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 9 MB: CRLF lines, then one with no end.
        let mut text: String = (1..=300_000).map(|i| format!("line {i:07} of the big one\r\n")).collect();
        text.push_str("the last, unended");
        std::fs::write(dir.join("big.txt"), &text).unwrap();
        assert!(text.len() as u64 > BIG_READ);
        let got = stream_lines(&dir.join("big.txt"), 0, 1, 100).unwrap();
        assert_eq!((got.lines, got.bytes, got.hash), (text.lines().count(), text.len() as u64, hash_of(text.as_bytes())), "counted and hashed as when read whole");
        let t = Tools::new(dir.clone(), dir.join(".out"));
        let ranged = t.call("read", &json!({ "path": "big.txt", "offset": 299_999, "limit": 5 })).await.text;
        assert_eq!(ranged, "line 0299999 of the big one\nline 0300000 of the big one\nthe last, unended");
        let mid = t.call("read", &json!({ "path": "big.txt", "offset": 10, "limit": 2 })).await.text;
        assert_eq!(mid, "line 0000010 of the big one\nline 0000011 of the big one\n\n[Lines 10-11 of 300001. Read on with offset=12.]");
        let whole = t.call("read", &json!({ "path": "big.txt" })).await.text;
        assert!(whole.starts_with(&format!("[{} is 300001 lines (", dir.join("big.txt").display())) && whole.contains("\nline 0000001 of the big one\n"), "{}", &whole[..200]);
        let past = t.call("read", &json!({ "path": "big.txt", "offset": 400_000 })).await;
        assert!(past.failed && past.text.contains("has 300001 lines"), "{}", past.text);
        // Read so: an edit after it isn't taken as made on a changed file.
        let edit = t.call("edit", &json!({ "path": "big.txt", "edits": [{ "old_text": "line 0000002 of", "new_text": "line two of" }] })).await;
        assert!(!edit.failed, "{}", edit.text);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A line longer than one read shows says it's cut, how long it is
    /// and how to read on, whether the file is read whole or streamed.
    #[tokio::test]
    async fn a_cut_long_line_says_so() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-longline-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let long = "x".repeat(200 * 1024);
        std::fs::write(dir.join("small.txt"), format!("first\n{long}\nlast\n")).unwrap();
        let mut big: String = (1..=400_000).map(|i| format!("line {i:07} of the big one\n")).collect();
        big.insert_str(big.find('\n').unwrap() + 1, &format!("{long}\n"));
        std::fs::write(dir.join("big.txt"), &big).unwrap();
        assert!(big.len() as u64 > BIG_READ);
        let t = Tools::new(dir.clone(), dir.join(".out"));
        for name in ["small.txt", "big.txt"] {
            let out = t.call("read", &json!({ "path": name, "offset": 2, "limit": 3 })).await.text;
            let (shown, note) = out.split_once('\n').unwrap();
            assert_eq!(shown.len(), READ_BYTES, "{name}: the line's start");
            let want = format!("[Line 2 is 200 KB; only its first {READ_BYTES} bytes are shown. Read on with bash: sed -n '2p' '{}' | cut -b {}-{}, or grep -o what you need.]", dir.join(name).display(), READ_BYTES + 1, 2 * READ_BYTES);
            assert!(note.starts_with(&want), "{name}: {note}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// find lists the first LIST_MAX names in order, and how many there are,
    /// however many match.
    #[tokio::test]
    async fn find_shows_the_first_names_and_the_count() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-find-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        // Made out of order, so the first ones found aren't the first by name.
        for i in (0..1200).rev() {
            std::fs::write(dir.join(format!("src/f{i:04}.rs")), "").unwrap();
        }
        std::fs::write(dir.join("src/notes.md"), "").unwrap();
        let t = Tools::new(dir.clone(), dir.join(".out"));
        let out = t.call("find", &json!({ "pattern": "*.rs" })).await.text;
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), LIST_MAX + 1, "the names, then the note");
        let want: Vec<String> = (0..LIST_MAX).map(|i| format!("src/f{i:04}.rs")).collect();
        assert_eq!(lines[..LIST_MAX], want.iter().map(String::as_str).collect::<Vec<_>>()[..]);
        assert_eq!(lines[LIST_MAX], format!("[1200 files; first {LIST_MAX} shown]"));
        let few = t.call("find", &json!({ "pattern": "*.md" })).await.text;
        assert_eq!(few, "src/notes.md");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_huge_file_read_whole_shows_its_start_and_size() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text: String = (1..=5000).map(|i| format!("line {i}\n")).collect();
        std::fs::write(dir.join("big.log"), &text).unwrap();
        std::fs::write(dir.join("small.rs"), (1..=1500).map(|i| format!("// {i}\n")).collect::<String>()).unwrap();
        let t = Tools::new(dir.clone(), dir.join(".out"));
        let out = t.call("read", &json!({ "path": "big.log" })).await.text;
        assert!(out.starts_with(&format!("[{} is 5000 lines (", dir.join("big.log").display())), "{}", &out[..120]);
        assert!(out.ends_with("\nline 250") && !out.contains("line 251"));
        let ranged = t.call("read", &json!({ "path": "big.log", "offset": 4000, "limit": 3 })).await.text;
        assert_eq!(ranged, "line 4000\nline 4001\nline 4002\n\n[Lines 4000-4002 of 5000. Read on with offset=4003.]");
        let small = t.call("read", &json!({ "path": "small.rs" })).await.text;
        assert!(small.ends_with("// 1500"), "a file that fits is read whole");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn an_image_read_is_shown_and_recalled() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-image-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dot = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==").unwrap();
        std::fs::write(dir.join("dot.png"), &dot).unwrap();
        std::fs::write(dir.join("fake.png"), b"\x89PNG not really").unwrap();
        let t = Tools::new(dir.clone(), dir.join(".out"));
        let o = t.call("read", &json!({ "path": "dot.png" })).await;
        assert_eq!(o.text, format!("[Image: {}, 1×1 PNG, 1 KB.]", dir.join("dot.png").display()));
        assert_eq!((o.images.len(), o.images[0]["mimeType"].as_str()), (1, Some("image/png")));
        let bad = t.call("read", &json!({ "path": "fake.png" })).await;
        assert!(bad.failed && bad.text.contains("is an image, but it can't be shown"), "{}", bad.text);
        // Set aside, then recalled: the image comes back with it.
        let result = json!({ "role": "toolResult", "toolCallId": "c1", "toolName": "read", "content": [{ "type": "text", "text": o.text }, o.images[0]], "timestamp": 0 });
        t.set_aside([("c1".to_owned(), Original { result, read: None })], true);
        let back = t.call("recall", &json!({ "id": "c1" })).await;
        assert_eq!((back.images.len(), back.failed), (1, false));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_same_size_rewrite_in_the_same_tick_is_caught() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-racy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let t = Tools::new(dir.clone(), dir.join(".out"));
        let file = dir.join("a.txt");
        std::fs::write(&file, "aaaa\n").unwrap();
        assert!(!t.call("read", &json!({ "path": "a.txt" })).await.failed);
        // Rewritten at the same size, its time as it was: only what's in it
        // tells.
        let modified = std::fs::metadata(&file).unwrap().modified().unwrap();
        std::fs::write(&file, "bbbb\n").unwrap();
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(modified).unwrap();
        let r = t.call("edit", &json!({ "path": "a.txt", "edits": [{ "old_text": "aaaa", "new_text": "x" }] })).await;
        assert!(r.failed && r.text.contains("changed since you last read it"), "{}", r.text);
        let w = t.call("write", &json!({ "path": "a.txt", "content": "y" })).await;
        assert!(w.failed && w.text.contains("changed since you last read it"), "{}", w.text);
        // Touched, the same inside: not a change.
        t.call("read", &json!({ "path": "a.txt" })).await;
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(modified + std::time::Duration::from_secs(60)).unwrap();
        let r = t.call("edit", &json!({ "path": "a.txt", "edits": [{ "old_text": "bbbb", "new_text": "c" }] })).await;
        assert!(!r.failed, "{}", r.text);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "c\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_patch_applies_whole_or_not_at_all() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-patch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let t = Tools::new(dir.clone(), dir.join(".out"));
        std::fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").unwrap();
        std::fs::write(dir.join("old.txt"), "gone\n").unwrap();
        let patch = |body: &str| json!({ "input": format!("*** Begin Patch\n{body}\n*** End Patch") });
        // A hunk that doesn't match: nothing written, the new file included.
        let r = t.call("apply_patch", &patch("*** Add File: sub/new.txt\n+hi\n*** Update File: a.txt\n-four\n+4")).await;
        assert!(r.failed && r.text.contains("a.txt"), "{}", r.text);
        assert!(!dir.join("sub/new.txt").exists());
        let r = t
            .call("apply_patch", &patch("*** Add File: sub/new.txt\n+hi\n*** Update File: a.txt\n*** Move to: b.txt\n@@\n one\n-two\n+2\n*** Delete File: old.txt"))
            .await;
        assert!(!r.failed, "{}", r.text);
        assert_eq!(r.text, "Created sub/new.txt (1 line); edited a.txt and moved it to b.txt (1 hunk, from line 1); deleted old.txt.");
        assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "one\n2\nthree\n");
        assert!(!dir.join("a.txt").exists() && !dir.join("old.txt").exists());
        // Changed elsewhere since: refused until read again.
        std::fs::write(dir.join("b.txt"), "one\n2\nthree\nfour\n").unwrap();
        let r = t.call("apply_patch", &patch("*** Update File: b.txt\n-2\n+two")).await;
        assert!(r.failed && r.text.contains("changed since you last read it"), "{}", r.text);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_generated_file_is_refused_once() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-generated-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let t = Tools::new(dir.clone(), dir.join(".out"));
        std::fs::write(dir.join("api.go"), "// Code generated by stringer. DO NOT EDIT.\n\npackage api\n").unwrap();
        std::fs::write(dir.join("Cargo.lock"), "version = 4\n").unwrap();
        let edit = json!({ "path": "api.go", "edits": [{ "old_text": "package api", "new_text": "package apis" }] });
        let first = t.call("edit", &edit).await;
        assert!(first.failed && first.text.contains("is generated") && first.text.contains("make the same change again"), "{}", first.text);
        assert!(!t.call("edit", &edit).await.failed, "the same change again goes through");
        // Overwriting a lockfile, read first: told once.
        t.call("read", &json!({ "path": "Cargo.lock" })).await;
        let w = json!({ "path": "Cargo.lock", "content": "version = 3\n" });
        assert!(t.call("write", &w).await.text.contains("Change Cargo.toml"));
        assert!(!t.call("write", &w).await.failed);
        // A new file isn't anyone's yet.
        assert!(!t.call("write", &json!({ "path": "new.pb.go", "content": "package x\n" })).await.failed);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn refuses_to_overwrite_what_changed_since_read() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-tools-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let t = Tools::new(dir.clone(), dir.join(".out"));
        let file = dir.join("a.txt");
        std::fs::write(&file, "one\n").unwrap();
        // Not read yet: replacing it is refused, editing it isn't.
        assert!(t.call("write", &json!({ "path": "a.txt", "content": "x" })).await.text.contains("haven't read it"));
        assert!(!t.call("read", &json!({ "path": "a.txt" })).await.failed);
        // Someone else changes it.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&file, "one\ntwo\n").unwrap();
        let r = t.call("edit", &json!({ "path": "a.txt", "edits": [{ "old_text": "one", "new_text": "1" }] })).await;
        assert!(r.failed && r.text.contains("changed since you last read it"), "{}", r.text);
        t.call("read", &json!({ "path": "a.txt" })).await;
        let r = t.call("edit", &json!({ "path": "a.txt", "edits": [{ "old_text": "one", "new_text": "1" }] })).await;
        assert!(!r.failed, "{}", r.text);
        // Its own edit doesn't count as someone else's.
        assert!(!t.call("edit", &json!({ "path": "a.txt", "edits": [{ "old_text": "two", "new_text": "2" }] })).await.failed);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "1\n2\n");
        let _ = std::fs::remove_dir_all(dir);
    }
}
