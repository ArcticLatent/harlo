//! Optional log of harness traffic (`--debug-protocol`, or Settings →
//! Harnesses): one JSONL file per harness process with every frame sent and
//! received and its stderr, for tracking down protocol problems. Applies to
//! harnesses started after it is turned on.
//!
//! Each line: `{"t": "<UTC time>", "dir": "in" | "out" | "stderr" | "start", ...}`
//! with the frame under `frame` (or the raw text under `text` when a line
//! is not JSON).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::HarnessKind;

/// Log files kept; older ones are deleted when a new one starts.
const KEEP_FILES: usize = 40;

static DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Turns the log on (into `dir`) or off (`None`).
pub fn set_dir(dir: Option<PathBuf>) {
    *DIR.lock().unwrap() = dir;
}

pub fn dir() -> Option<PathBuf> {
    DIR.lock().unwrap().clone()
}

fn now() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    crate::branches::chrono_like(d.as_secs(), d.subsec_millis())
}

pub(crate) struct ProtocolLog {
    out: Mutex<BufWriter<File>>,
}

impl ProtocolLog {
    /// A new log file for one harness process, when logging is on.
    pub(crate) fn open(kind: HarnessKind, program: &Path, args: &[String], cwd: &Path) -> Option<Arc<Self>> {
        let dir = dir()?;
        std::fs::create_dir_all(&dir).ok()?;
        prune(&dir);
        let stamp: String = now().chars().filter(|c| c.is_ascii_digit()).take(14).collect();
        let name = format!("{}-{}-{}-{}.jsonl", &stamp[..8], &stamp[8..], kind.as_str(), std::process::id());
        // Two harnesses in the same second: number the second one.
        let mut path = dir.join(&name);
        let mut n = 2;
        while path.exists() {
            path = dir.join(name.replace(".jsonl", &format!("-{n}.jsonl")));
            n += 1;
        }
        let file = File::create(&path).ok()?;
        tracing::info!("logging harness traffic to {}", path.display());
        let log = Arc::new(Self { out: Mutex::new(BufWriter::new(file)) });
        log.write(json!({ "t": now(), "dir": "start", "program": program, "args": args, "cwd": cwd }));
        Some(log)
    }

    /// One line read from or written to the harness (`in` / `out`).
    pub(crate) fn frame(&self, dir: &str, raw: &[u8]) {
        let line = match serde_json::from_slice::<Value>(raw) {
            Ok(frame) => json!({ "t": now(), "dir": dir, "frame": frame }),
            Err(_) => json!({ "t": now(), "dir": dir, "text": String::from_utf8_lossy(raw).trim_end() }),
        };
        self.write(line);
    }

    pub(crate) fn stderr(&self, line: &str) {
        self.write(json!({ "t": now(), "dir": "stderr", "text": line }));
    }

    fn write(&self, line: Value) {
        let mut out = self.out.lock().unwrap();
        // Flushed per line: the point is to have it when something breaks.
        let _ = writeln!(out, "{line}").and_then(|_| out.flush());
    }
}

/// Keeps the newest `KEEP_FILES - 1` logs, making room for one more.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut logs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).collect();
    if logs.len() < KEEP_FILES {
        return;
    }
    // Names start with the time, so name order is age order.
    logs.sort();
    for old in &logs[..logs.len() + 1 - KEEP_FILES] {
        let _ = std::fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_frames_and_prunes() {
        let dir = std::env::temp_dir().join(format!("harlo-protocol-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..KEEP_FILES + 3 {
            std::fs::write(dir.join(format!("20000101-0000{i:02}-pi-1.jsonl")), "").unwrap();
        }
        set_dir(Some(dir.clone()));
        let log = ProtocolLog::open(HarnessKind::Pi, Path::new("/usr/bin/pi"), &["--mode".into(), "rpc".into()], Path::new("/tmp")).unwrap();
        log.frame("out", br#"{"type":"prompt","message":"hi"}"#);
        log.frame("in", b"not json\n");
        log.stderr("warning: something");
        set_dir(None);
        drop(log);

        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
        files.sort();
        assert_eq!(files.len(), KEEP_FILES);
        assert!(!files.iter().any(|f| f.ends_with("20000101-000000-pi-1.jsonl")), "oldest pruned");
        let newest = files.last().unwrap();
        let lines: Vec<Value> = std::fs::read_to_string(newest).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0]["dir"], "start");
        assert_eq!(lines[0]["args"][1], "rpc");
        assert_eq!(lines[1]["frame"]["type"], "prompt");
        assert_eq!(lines[2]["text"], "not json");
        assert_eq!(lines[3]["dir"], "stderr");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
