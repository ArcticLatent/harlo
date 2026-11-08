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
}

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
        f("read", "Read a text file. Shows up to 2000 lines (50 KB) from `offset` (1-based line); says how to read on when there's more. A file longer than that, read without offset or limit, shows its first 250 lines and its size: grep it for what you need, or read a range.",
          json!({ "path": { "type": "string", "description": "Relative to the working directory, or absolute" }, "offset": { "type": "integer" }, "limit": { "type": "integer" } }), &["path"]),
        f("write", "Create a file, or replace a whole file you've read in this session. Makes missing folders. To change part of a file, use edit.",
          json!({ "path": { "type": "string" }, "content": { "type": "string" } }), &["path", "content"]),
        f("edit", "Replace text in a file. Each edit's old_text must match exactly one place in the file as it is now (copy it from read, with enough lines around it to be unique); all edits apply together. Refused if the file changed since you last read it.",
          json!({ "path": { "type": "string" }, "edits": { "type": "array", "items": { "type": "object", "properties": { "old_text": { "type": "string" }, "new_text": { "type": "string" } }, "required": ["old_text", "new_text"], "additionalProperties": false } } }), &["path", "edits"]),
        f("bash", "Run a bash command in the working directory. It can't read input (stdin is closed; pagers and editors are off). Times out after 120 s unless `timeout` (seconds) says otherwise. Long output shows its start and end, with the whole of it saved to a file. Don't edit files with it: use edit or write. \
                   With background: true it runs in the background (a server, a watcher, a long test run): you get its job id at once and are told when it ends (it times out after 30 min unless timeout says otherwise, up to 2 h); jobs shows its output, waits for it or stops it. Don't add & yourself.",
          json!({ "command": { "type": "string" }, "timeout": { "type": "integer" }, "background": { "type": "boolean" },
                  "outside_sandbox": { "type": "boolean", "description": "Run it outside the sandbox. Only when it failed because of the sandbox and really needs what the sandbox hides or protects (e.g. git push over SSH); say why first. The user is asked every time." } }), &["command"]),
        f("grep", "Search file contents for a regular expression (ripgrep syntax; respects .gitignore). Shows file:line: text, up to 300 matches.",
          json!({ "pattern": { "type": "string" }, "path": { "type": "string" }, "glob": { "type": "string", "description": "Only files matching this, e.g. *.rs" }, "ignore_case": { "type": "boolean" } }), &["pattern"]),
        f("find", "Find files by name with a glob (e.g. **/*.toml), respecting .gitignore. Up to 500.",
          json!({ "pattern": { "type": "string" }, "path": { "type": "string" } }), &["pattern"]),
        f("ls", "List a folder (folders end with /). Up to 500 entries.",
          json!({ "path": { "type": "string" } }), &[]),
        f("fetch", "Read a web page or file at a URL (http or https): a page comes back as Markdown (its main part, without menus), text and JSON as they are, an image is shown, another kind of file is saved for you to open with a command. Up to 50 KB is shown; all of a longer page is saved to a file to read on. \
                    For documentation, an API's reference, an issue or a changelog: what the project doesn't have.",
          json!({ "url": { "type": "string" } }), &["url"]),
        f("jobs", "Background commands (started by bash with background: true): list them, see one's output (its last lines), wait for one to end (up to timeout seconds, 60 by default), or stop one with everything it started.",
          json!({ "action": { "type": "string", "enum": ["list", "output", "wait", "stop"] }, "id": { "type": "string", "description": "The job's id (j1, j2…), for output, wait and stop" },
                  "lines": { "type": "integer", "description": "How many of the last lines to show (50 by default)" }, "timeout": { "type": "integer" } }), &["action"]),
        f("ask", "Ask the user questions and wait for their answers: harlo shows them as a card with your options to pick from, and a field for their own answer. \
               For a decision that's theirs, or one the task leaves open and that changes what you'd do (which approach, what to keep, what they meant); not for what you can find out yourself, or to ask leave for what they already asked for. \
               One to four questions, each with two to six options (put the one you recommend first). The answer comes back as text.",
          json!({ "questions": { "type": "array", "minItems": 1, "maxItems": 4, "items": {
              "type": "object",
              "properties": {
                  "question": { "type": "string", "description": "The whole question, ending with a question mark" },
                  "header": { "type": "string", "description": "A label of a word or two" },
                  "options": { "type": "array", "minItems": 2, "maxItems": 6, "items": { "type": "object", "properties": {
                      "label": { "type": "string", "description": "A few words" },
                      "description": { "type": "string", "description": "What it means, or what follows from it" },
                  }, "required": ["label"], "additionalProperties": false } },
                  "multiple": { "type": "boolean", "description": "Whether they may pick more than one (default false)" },
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
        Tools { cwd, spill_dir, seen: Mutex::new(HashMap::new()), told_generated: Mutex::new(HashSet::new()), originals: Mutex::new(HashMap::new()), sandbox: None, jobs, lsp: Arc::default() }
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

    /// grep's or find's lines, less those in hidden places under `root`.
    fn unhidden(&self, root: &Path, lines: Vec<String>) -> Vec<String> {
        let Some(sb) = &self.sandbox else { return lines };
        let base = format!("{}/", self.cwd.display());
        let under: Vec<String> = sb.hidden_under(root).iter().map(|h| h.to_string_lossy().into_owned()).collect();
        if under.is_empty() {
            return lines;
        }
        let shown = |h: &str| h.strip_prefix(&base).map(str::to_owned);
        let under: Vec<String> = under.iter().flat_map(|h| [Some(h.clone()), shown(h)]).flatten().collect();
        lines
            .into_iter()
            .filter(|l| !under.iter().any(|h| l.strip_prefix(h.as_str()).is_some_and(|rest| rest.is_empty() || rest.starts_with('/') || rest.starts_with(':'))))
            .collect()
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

    pub async fn call(&self, name: &str, args: &Value) -> Outcome {
        let s = |k: &str| args[k].as_str().unwrap_or_default();
        match name {
            "read" => self.read(s("path"), args["offset"].as_u64(), args["limit"].as_u64()),
            "write" => self.write(s("path"), s("content")),
            "edit" => self.edit(s("path"), &args["edits"]),
            "apply_patch" => self.patch(s("input")),
            "bash" => {
                let sandbox = self.sandbox.as_deref();
                if let Some(why) = sandbox.and_then(|sb| sb.broken.as_ref()) {
                    return fail(format!("Commands can't run: the sandbox can't start here ({why}). Install bubblewrap, or turn Harlo's sandbox off in Settings → Providers → Harlo."));
                }
                let inside = sandbox.filter(|_| args["outside_sandbox"] != true);
                if args["background"] == true {
                    let (text, failed) = self.jobs.start(s("command"), &self.cwd, args["timeout"].as_u64(), inside);
                    return Outcome::new(text, failed);
                }
                let (text, failed) = bash::run(s("command"), &self.cwd, args["timeout"].as_u64(), &self.spill_dir, inside).await;
                Outcome::new(text, failed)
            }
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

    fn read(&self, p: &str, offset: Option<u64>, limit: Option<u64>) -> Outcome {
        if p.trim().is_empty() {
            return fail("Give the file's path.");
        }
        let path = self.path(p);
        if let Some(no) = self.hidden(&path) {
            return no;
        }
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if path.is_dir() => return fail(format!("{} is a folder: use ls ({e}).", path.display())),
            Err(e) => return fail(format!("Can't read {}: {e}", path.display())),
        };
        self.remember(&path, &bytes);
        if crate::images::mime_of(&bytes).is_some() {
            return match crate::images::fit(&bytes) {
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
            // One line longer than the limit.
            out = lines[start].chars().take(READ_BYTES).collect();
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
        let out = match tokio::process::Command::new("rg").args(&args).current_dir(&self.cwd).stdin(std::process::Stdio::null()).output().await {
            Ok(o) => o,
            // No ripgrep: grep.
            Err(_) => {
                let mut g = vec!["-rnI".to_owned()];
                if ignore_case {
                    g.push("-i".into());
                }
                if !glob.is_empty() {
                    g.push(format!("--include={glob}"));
                }
                g.extend(["-E".into(), "-e".into(), pattern.into(), "--".into(), path.to_string_lossy().into_owned()]);
                match tokio::process::Command::new("grep").args(&g).current_dir(&self.cwd).stdin(std::process::Stdio::null()).output().await {
                    Ok(o) => o,
                    Err(e) => return fail(format!("Can't search: {e}")),
                }
            }
        };
        if !out.status.success() && out.stdout.is_empty() {
            let err = String::from_utf8_lossy(&out.stderr);
            return if err.trim().is_empty() { ok("No matches.") } else { fail(err.trim().to_owned()) };
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let base = format!("{}/", self.cwd.display());
        let lines: Vec<String> = text.lines().map(|l| l.strip_prefix(&base).unwrap_or(l).to_owned()).collect();
        let lines = self.unhidden(&path, lines);
        if lines.is_empty() {
            return ok("No matches.");
        }
        let total = lines.len();
        let mut shown = lines.into_iter().take(GREP_MAX).collect::<Vec<_>>().join("\n");
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
        let out = tokio::process::Command::new("rg").args(["--files", "--color", "never", "-g", pattern, "-g", "!.git/", "--"]).arg(&path).current_dir(&self.cwd).stdin(std::process::Stdio::null()).output().await;
        let text = match out {
            Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
            Err(_) => {
                let name = pattern.rsplit('/').next().unwrap_or(pattern);
                match tokio::process::Command::new("find").arg(&path).args(["-name", name, "-not", "-path", "*/.git/*"]).output().await {
                    Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
                    Err(e) => return fail(format!("Can't search: {e}")),
                }
            }
        };
        let base = format!("{}/", self.cwd.display());
        let mut files: Vec<String> = self.unhidden(&path, text.lines().map(|l| l.strip_prefix(&base).unwrap_or(l).to_owned()).collect());
        files.sort();
        let total = files.len();
        files.truncate(LIST_MAX);
        let mut shown = files.join("\n");
        if total > LIST_MAX {
            shown.push_str(&format!("\n[{total} files; first {LIST_MAX} shown]"));
        }
        ok(if shown.is_empty() { "No files match.".into() } else { shown })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
