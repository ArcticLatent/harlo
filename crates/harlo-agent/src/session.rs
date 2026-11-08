//! The session log, in pi's session format: JSONL, a `session` header,
//! then one entry a line (`id`, `parentId`, `timestamp`), each message a
//! `message` entry. The file is the history: what's sent to the model is
//! derived from its messages, so a resumed session sends what a running
//! one would.
//!
//! Files go in `<root>/--<cwd with / as ->--/<time>_<id>.jsonl`, written
//! from the first entry on. Each entry is one write; a line cut off by a
//! crash is skipped when the file is read again.

use std::fs::{File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};

/// `$XDG_DATA_HOME/harlo-agent/sessions` (or `~/.local/share/…`).
pub fn root() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(data.join("harlo-agent/sessions"))
}

/// The folder of a working directory's sessions.
pub fn dir_for(root: &Path, cwd: &Path) -> PathBuf {
    let s = cwd.to_string_lossy();
    let safe: String = s.strip_prefix('/').unwrap_or(&s).chars().map(|c| if matches!(c, '/' | '\\' | ':') { '-' } else { c }).collect();
    root.join(format!("--{safe}--"))
}

/// The working directory's newest session.
pub fn latest(root: &Path, cwd: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir_for(root, cwd))
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max()
        .map(|(_, p)| p)
}

pub struct Log {
    pub id: String,
    pub path: PathBuf,
    cwd: PathBuf,
    file: Option<File>,
    /// The newest entry's id (the next one's parent).
    last: Option<String>,
    /// Writes nothing (`--no-session`).
    off: bool,
    /// The session it was forked from (the header's `parentSession`).
    parent: Option<PathBuf>,
}

/// A session read back: its log (appending continues it), the messages
/// on its current branch, and the model and thinking level it last used.
pub struct Loaded {
    pub log: Log,
    /// What the model sees: after a compaction, its summary first, then
    /// the messages it kept and those since.
    pub messages: Vec<Value>,
    /// Each message's entry id.
    pub ids: Vec<String>,
    /// The files the last compaction listed (read, changed).
    pub files: (Vec<String>, Vec<String>),
    pub model: Option<String>,
    pub effort: Option<String>,
    /// Its name (`session_info`), if it was given one.
    pub name: Option<String>,
    /// Outputs set aside, for `recall`.
    pub originals: Vec<(String, crate::tools::Original)>,
}

/// What a session's log (and its subagents', beside it) says it used:
/// tokens sent and received, not counting those the cache served.
pub fn spent(path: &Path) -> u64 {
    if path.as_os_str().is_empty() {
        return 0;
    }
    let of = |p: &Path| -> u64 {
        std::fs::read_to_string(p)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|e| e["type"] == "message" && e["message"]["role"] == "assistant")
            .map(|e| e["message"]["usage"]["input"].as_u64().unwrap_or(0) + e["message"]["usage"]["output"].as_u64().unwrap_or(0))
            .sum()
    };
    let subs: u64 = std::fs::read_dir(path.with_extension("")).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "jsonl")).map(|p| of(&p)).sum();
    of(path) + subs
}

/// What a session holds for the agent going on with it.
#[derive(Default)]
pub struct Past {
    pub messages: Vec<Value>,
    pub ids: Vec<String>,
    pub files: (Vec<String>, Vec<String>),
    /// Outputs set aside (their stubs are in `messages`), for `recall`.
    pub originals: Vec<(String, crate::tools::Original)>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

impl Loaded {
    /// Its log, what the agent goes on from, and its name.
    pub fn into_parts(self) -> (Log, Past, Option<String>) {
        (self.log, Past { messages: self.messages, ids: self.ids, files: self.files, originals: self.originals, model: self.model, effort: self.effort }, self.name)
    }
}

/// A session file's header and entries.
fn read_entries(path: &Path) -> Result<(Value, Vec<Value>)> {
    let text = std::fs::read_to_string(path).with_context(|| format!("Couldn't read the session {}", path.display()))?;
    let mut header: Option<Value> = None;
    let mut entries: Vec<Value> = Vec::new();
    for line in text.lines() {
        // A line cut off by a crash isn't JSON: skipped.
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v["type"] == "session" {
            header.get_or_insert(v);
        } else if v["id"].is_string() {
            entries.push(v);
        }
    }
    let Some(header) = header else { bail!("{} isn't a session file", path.display()) };
    Ok((header, entries))
}

impl Log {
    /// A new session; its file is made with the first entry.
    pub fn new(root: &Path, cwd: &Path) -> Log {
        let id = uuid();
        let path = dir_for(root, cwd).join(format!("{}_{id}.jsonl", iso(SystemTime::now()).replace([':', '.'], "-")));
        Log { id, path, cwd: cwd.to_owned(), file: None, last: None, off: false, parent: None }
    }

    /// A session kept at `path`, started by the session at `parent` (a
    /// subagent's).
    pub fn at(path: PathBuf, cwd: &Path, parent: &Path) -> Log {
        Log { id: uuid(), path, cwd: cwd.to_owned(), file: None, last: None, off: false, parent: Some(parent.to_owned()) }
    }

    /// A session kept nowhere.
    pub fn ephemeral(cwd: &Path) -> Log {
        Log { id: uuid(), path: PathBuf::new(), cwd: cwd.to_owned(), file: None, last: None, off: true, parent: None }
    }

    pub fn open(path: &Path) -> Result<Loaded> {
        let (header, entries) = read_entries(path)?;
        // The current branch: from the newest entry back to the root.
        let leaf = entries.last().and_then(|e| e["id"].as_str()).map(str::to_owned);
        Self::load_branch(path, &header, &entries, leaf)
    }

    /// The session at `path` going on from just before your message
    /// `entry`, in the same file: what you send next starts a new branch
    /// there (the old one stays, to browse). Nothing is written until then.
    /// That message's text, and the session read back.
    pub fn branch_before(path: &Path, entry: &str) -> Result<(Loaded, String)> {
        let (header, entries) = read_entries(path)?;
        let picked = entries
            .iter()
            .find(|e| e["id"] == entry && e["type"] == "message" && e["message"]["role"] == "user")
            .with_context(|| format!("There's no message of yours with the id {entry} in this session"))?;
        let said = user_text(&picked["message"]["content"]);
        let leaf = picked["parentId"].as_str().map(str::to_owned);
        Ok((Self::load_branch(path, &header, &entries, leaf)?, said))
    }

    /// The branch ending at `leaf` (None: none, a new root).
    fn load_branch(path: &Path, header: &Value, entries: &[Value], leaf: Option<String>) -> Result<Loaded> {
        let by_id: std::collections::HashMap<&str, &Value> = entries.iter().filter_map(|e| Some((e["id"].as_str()?, e))).collect();
        let mut branch = Vec::new();
        let mut at = leaf.as_deref();
        while let Some(id) = at {
            let Some(e) = by_id.get(id) else { break };
            if branch.len() > entries.len() {
                break; // a loop
            }
            branch.push(*e);
            at = e["parentId"].as_str();
        }
        branch.reverse();
        let (mut model, mut effort, mut name) = (None, None, None);
        let mut messages = Vec::new();
        let mut ids: Vec<String> = Vec::new();
        let mut files = (Vec::new(), Vec::new());
        let mut originals = Vec::new();
        for e in &branch {
            let id = e["id"].as_str().unwrap_or_default().to_owned();
            match e["type"].as_str() {
                Some("message") => {
                    messages.push(e["message"].clone());
                    ids.push(id);
                }
                Some("custom_message") => {
                    messages.push(json!({ "role": "custom", "customType": e["customType"], "content": e["content"], "display": e["display"] }));
                    ids.push(id);
                }
                // The summary, then the messages from the first one it kept.
                Some("compaction") => {
                    let from = e["firstKeptEntryId"].as_str().and_then(|k| ids.iter().position(|i| i == k)).unwrap_or(ids.len());
                    let kept_messages = messages.split_off(from);
                    let kept_ids = ids.split_off(from);
                    messages = vec![summary_message(e)];
                    ids = vec![id];
                    messages.extend(kept_messages);
                    ids.extend(kept_ids);
                    let list = |k: &str| e["details"][k].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(str::to_owned)).collect();
                    files = (list("readFiles"), list("modifiedFiles"));
                }
                // Old outputs set aside: their stubs in place.
                Some("harlo_stubs") => originals.extend(crate::stubs::replay(e, &mut messages)),
                // (`provider/model`; a bare one is the plan's.)
                Some("model_change") => {
                    model = e["modelId"].as_str().map(|m| match e["provider"].as_str().filter(|p| !p.is_empty()) {
                        Some(p) => format!("{p}/{m}"),
                        None => m.to_owned(),
                    })
                }
                Some("thinking_level_change") => effort = e["thinkingLevel"].as_str().filter(|l| *l != "off").map(str::to_owned),
                Some("session_info") => name = e["name"].as_str().filter(|n| !n.is_empty()).map(str::to_owned),
                _ => {}
            }
        }
        let log = Log {
            id: header["id"].as_str().map(str::to_owned).unwrap_or_else(uuid),
            path: path.to_owned(),
            cwd: header["cwd"].as_str().map(PathBuf::from).unwrap_or_default(),
            file: None,
            last: leaf,
            off: false,
            parent: None,
        };
        Ok(Loaded { log, messages, ids, model, effort, name, files, originals })
    }

    /// Appends `entry` (its `type` and fields), giving it an id, its parent
    /// and the time: the id.
    pub fn append(&mut self, mut entry: Value) -> Result<String> {
        let id = short_id();
        if self.off {
            return Ok(id);
        }
        entry["id"] = json!(id);
        entry["parentId"] = json!(self.last);
        entry["timestamp"] = json!(iso(SystemTime::now()));
        let mut line = String::new();
        if self.file.is_none() {
            let (file, fresh, torn) = self.open_file()?;
            self.file = Some(file);
            if fresh {
                line.push_str(&self.header().to_string());
                line.push('\n');
            } else if torn {
                line.push('\n');
            }
        }
        line.push_str(&entry.to_string());
        line.push('\n');
        let file = self.file.as_mut().expect("opened");
        file.write_all(line.as_bytes()).with_context(|| format!("Couldn't write the session {}", self.path.display()))?;
        self.last = Some(id.clone());
        Ok(id)
    }

    fn header(&self) -> Value {
        let mut h = json!({ "type": "session", "version": 3, "id": self.id, "timestamp": iso(SystemTime::now()), "cwd": self.cwd });
        if let Some(p) = &self.parent {
            h["parentSession"] = json!(p);
        }
        h
    }

    /// A message's entry: its id.
    pub fn message(&mut self, message: &Value) -> Result<String> {
        if message["role"] == "custom" {
            return self.append(json!({ "type": "custom_message", "customType": message["customType"], "content": message["content"], "display": message["display"] }));
        }
        self.append(json!({ "type": "message", "message": message }))
    }

    /// (the file, whether it's new, whether its last line was cut off).
    fn open_file(&self) -> Result<(File, bool, bool)> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("Couldn't make {}", dir.display()))?;
        }
        let mut file = OpenOptions::new().create(true).append(true).read(true).open(&self.path).with_context(|| format!("Couldn't open {}", self.path.display()))?;
        let len = file.metadata()?.len();
        let mut torn = false;
        if len > 0 {
            let mut last = [0u8; 1];
            file.seek(SeekFrom::End(-1))?;
            file.read_exact(&mut last)?;
            torn = last[0] != b'\n';
        }
        Ok((file, len == 0, torn))
    }
}

/// A compaction entry as the message the model gets in its place.
pub fn summary_message(e: &Value) -> Value {
    json!({ "role": "compactionSummary", "summary": e["summary"], "tokensBefore": e["tokensBefore"], "timestamp": now_ms() })
}

/// A user message's text (its text blocks, one after another).
fn user_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

fn random_hex(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn short_id() -> String {
    random_hex(4)
}

/// A random (version 4) UUID.
fn uuid() -> String {
    let mut b = [0u8; 16];
    let _ = getrandom::fill(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// `2026-11-06T21:04:05.123Z`.
pub fn iso(t: SystemTime) -> String {
    let d = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z", rem / 3600, rem % 3600 / 60, rem % 60, d.subsec_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("harlo-agent-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn times_and_folders() {
        assert_eq!(iso(SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1_762_463_045_123)), "2025-11-06T21:04:05.123Z");
        assert_eq!(iso(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(951_782_400)), "2000-02-29T00:00:00.000Z");
        assert_eq!(dir_for(Path::new("/r"), Path::new("/home/u/my proj")), PathBuf::from("/r/--home-u-my proj--"));
    }

    #[test]
    fn a_session_reads_back_and_continues() {
        let root = tmp("log");
        let cwd = Path::new("/home/u/proj");
        let mut log = Log::new(&root, cwd);
        assert!(!log.path.exists(), "nothing is written before the first entry");
        log.append(json!({ "type": "model_change", "provider": "openai", "modelId": "m1" })).unwrap();
        let user = json!({ "role": "user", "content": [{ "type": "text", "text": "hi" }], "timestamp": 1 });
        let stop = json!({ "role": "custom", "customType": "turn_stopped", "content": "stopped", "display": false });
        log.message(&user).unwrap();
        log.message(&stop).unwrap();
        // A crash in the middle of a line.
        let mut f = OpenOptions::new().append(true).open(&log.path).unwrap();
        f.write_all(br#"{"type":"message","id":"x"#).unwrap();
        let path = log.path.clone();
        assert_eq!(latest(&root, cwd), Some(path.clone()));

        let mut loaded = Log::open(&path).unwrap();
        assert_eq!(loaded.log.id, log.id);
        assert_eq!(loaded.model.as_deref(), Some("openai/m1"));
        assert_eq!(loaded.messages, vec![user.clone(), stop.clone()]);
        let reply = json!({ "role": "assistant", "content": [{ "type": "text", "text": "hello" }] });
        loaded.log.message(&reply).unwrap();
        let again = Log::open(&path).unwrap();
        assert_eq!(again.messages, vec![user, stop, reply], "the cut-off line is skipped and the next entry starts its own line");
        let text = std::fs::read_to_string(&path).unwrap();
        let header: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!((header["type"].as_str(), header["version"].as_u64(), header["cwd"].as_str()), (Some("session"), Some(3), Some("/home/u/proj")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_compaction_reads_back_as_its_summary_and_what_it_kept() {
        let root = tmp("compaction");
        let cwd = Path::new("/home/u/proj");
        let mut log = Log::new(&root, cwd);
        let msg = |role: &str, text: &str| json!({ "role": role, "content": [{ "type": "text", "text": text }] });
        let first = log.message(&msg("user", "old task")).unwrap();
        log.message(&msg("assistant", "old work")).unwrap();
        let kept = log.message(&msg("user", "newer")).unwrap();
        log.message(&msg("assistant", "newer work")).unwrap();
        let c = log.append(json!({ "type": "compaction", "summary": "## Goal\nold task", "firstKeptEntryId": kept, "tokensBefore": 900, "details": { "readFiles": ["a.rs"], "modifiedFiles": ["b.rs"] } })).unwrap();
        let after = log.message(&msg("user", "after")).unwrap();
        let loaded = Log::open(&log.path).unwrap();
        let roles: Vec<&str> = loaded.messages.iter().map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["compactionSummary", "user", "assistant", "user"]);
        assert_eq!((loaded.messages[0]["summary"].as_str(), loaded.messages[0]["tokensBefore"].as_u64()), (Some("## Goal\nold task"), Some(900)));
        assert_eq!(loaded.messages[1]["content"][0]["text"], "newer");
        assert_eq!(loaded.ids, [c.clone(), kept.clone(), loaded.ids[2].clone(), after.clone()]);
        assert_eq!(loaded.files, (vec!["a.rs".to_owned()], vec!["b.rs".to_owned()]));
        assert_ne!(loaded.ids[0], first);
        // A branch after it keeps the summary; one before it doesn't.
        let (f, _) = Log::branch_before(&log.path, &after).unwrap();
        assert_eq!(f.messages.len(), 3);
        assert_eq!(f.messages[0]["role"], "compactionSummary");
        let (f, said) = Log::branch_before(&log.path, &kept).unwrap();
        assert_eq!((f.messages.len(), said.as_str()), (2, "newer"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_branch_starts_before_a_message_in_the_same_file() {
        let root = tmp("branch");
        let cwd = Path::new("/home/u/proj");
        let mut log = Log::new(&root, cwd);
        let msg = |role: &str, text: &str| json!({ "role": role, "content": [{ "type": "text", "text": text }] });
        log.append(json!({ "type": "model_change", "provider": "openai", "modelId": "m1" })).unwrap();
        log.message(&msg("user", "first")).unwrap();
        log.message(&msg("assistant", "one")).unwrap();
        log.append(json!({ "type": "session_info", "name": "Named" })).unwrap();
        log.message(&msg("user", "second")).unwrap();
        log.message(&msg("assistant", "two")).unwrap();
        let old = std::fs::read_to_string(&log.path).unwrap();
        let entries = |text: &str| text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).collect::<Vec<_>>();
        let users: Vec<String> = entries(&old).iter().filter(|e| e["message"]["role"] == "user").map(|e| e["id"].as_str().unwrap().to_owned()).collect();

        // From the second message: the first exchange, its name and model;
        // nothing written until you send.
        let (mut b, said) = Log::branch_before(&log.path, &users[1]).unwrap();
        assert_eq!(said, "second");
        assert_eq!(b.log.path, log.path);
        assert_eq!(b.messages, vec![msg("user", "first"), msg("assistant", "one")]);
        assert_eq!((b.model.as_deref(), b.name.as_deref()), (Some("openai/m1"), Some("Named")));
        assert_eq!(std::fs::read_to_string(&log.path).unwrap(), old, "nothing written yet");
        assert_eq!(std::fs::read_dir(log.path.parent().unwrap()).unwrap().count(), 1, "no other session");
        let edited = b.log.message(&msg("user", "second, edited")).unwrap();
        // The file: the old branch kept, the new one beside it, from the
        // entry before the edited message; it's the one that's reopened.
        let now = entries(&std::fs::read_to_string(&log.path).unwrap());
        assert_eq!(now.len(), entries(&old).len() + 1);
        let new = now.iter().find(|e| e["id"] == edited).unwrap();
        let second = now.iter().find(|e| e["id"] == users[1]).unwrap();
        assert_eq!(new["parentId"], second["parentId"]);
        let again = Log::open(&log.path).unwrap();
        assert_eq!(again.messages.len(), 3, "it goes on from the new branch");
        assert_eq!(again.messages[2], msg("user", "second, edited"));

        // From the first: no messages, the model kept.
        let (b, said) = Log::branch_before(&log.path, &users[0]).unwrap();
        assert_eq!((said.as_str(), b.messages.len(), b.model.as_deref()), ("first", 0, Some("openai/m1")));
        // Not a message of yours: refused.
        let reply = entries(&old).into_iter().find(|e| e["message"]["role"] == "assistant").unwrap();
        assert!(Log::branch_before(&log.path, reply["id"].as_str().unwrap()).is_err());
        assert!(Log::branch_before(&log.path, "nope").is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
