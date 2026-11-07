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
}

/// A session read back: its log (appending continues it), the messages
/// on its current branch, and the model and thinking level it last used.
pub struct Loaded {
    pub log: Log,
    pub messages: Vec<Value>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

impl Log {
    /// A new session; its file is made with the first entry.
    pub fn new(root: &Path, cwd: &Path) -> Log {
        let id = uuid();
        let path = dir_for(root, cwd).join(format!("{}_{id}.jsonl", iso(SystemTime::now()).replace([':', '.'], "-")));
        Log { id, path, cwd: cwd.to_owned(), file: None, last: None }
    }

    pub fn open(path: &Path) -> Result<Loaded> {
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
        // The current branch: from the newest entry back to the root.
        let by_id: std::collections::HashMap<&str, &Value> = entries.iter().filter_map(|e| Some((e["id"].as_str()?, e))).collect();
        let mut branch = Vec::new();
        let mut at = entries.last().and_then(|e| e["id"].as_str());
        while let Some(id) = at {
            let Some(e) = by_id.get(id) else { break };
            if branch.len() > entries.len() {
                break; // a loop
            }
            branch.push(*e);
            at = e["parentId"].as_str();
        }
        branch.reverse();
        let (mut model, mut effort) = (None, None);
        let mut messages = Vec::new();
        for e in &branch {
            match e["type"].as_str() {
                Some("message") => messages.push(e["message"].clone()),
                Some("custom_message") => messages.push(json!({ "role": "custom", "customType": e["customType"], "content": e["content"], "display": e["display"] })),
                Some("model_change") => model = e["modelId"].as_str().map(str::to_owned),
                Some("thinking_level_change") => effort = e["thinkingLevel"].as_str().filter(|l| *l != "off").map(str::to_owned),
                _ => {}
            }
        }
        let log = Log {
            id: header["id"].as_str().map(str::to_owned).unwrap_or_else(uuid),
            path: path.to_owned(),
            cwd: header["cwd"].as_str().map(PathBuf::from).unwrap_or_default(),
            file: None,
            last: entries.last().and_then(|e| e["id"].as_str()).map(str::to_owned),
        };
        Ok(Loaded { log, messages, model, effort })
    }

    /// Appends `entry` (its `type` and fields), giving it an id, its parent
    /// and the time.
    pub fn append(&mut self, mut entry: Value) -> Result<()> {
        let id = short_id();
        entry["id"] = json!(id);
        entry["parentId"] = json!(self.last);
        entry["timestamp"] = json!(iso(SystemTime::now()));
        let mut line = String::new();
        if self.file.is_none() {
            let (file, fresh, torn) = self.open_file()?;
            self.file = Some(file);
            if fresh {
                line.push_str(&json!({ "type": "session", "version": 3, "id": self.id, "timestamp": iso(SystemTime::now()), "cwd": self.cwd }).to_string());
                line.push('\n');
            } else if torn {
                line.push('\n');
            }
        }
        line.push_str(&entry.to_string());
        line.push('\n');
        let file = self.file.as_mut().expect("opened");
        file.write_all(line.as_bytes()).with_context(|| format!("Couldn't write the session {}", self.path.display()))?;
        self.last = Some(id);
        Ok(())
    }

    pub fn message(&mut self, message: &Value) -> Result<()> {
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
        assert_eq!(loaded.model.as_deref(), Some("m1"));
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
}
