//! Saved sessions on remote hosts, for the sidebar: the pi / omp / Claude / Harlo
//! session files of a host are listed over SSH and changed ones read in a
//! batch; OpenCode's come from its server there and Hermes's from harlo's
//! index. All are reported like local scan results (paths and folders as
//! `ssh://host/…`): those outside the host's projects go in its Other
//! sessions, as here.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use anyhow::Result;
use harlo_harness::store::{self, SessionInfo};
use harlo_harness::HarnessKind;

use crate::remote::{self, Host};
use crate::scanner::ScanDelta;

/// Lists `<kind>\t<mtime>\t<size>\t<path>` for each session file (GNU find,
/// else BSD stat), then `has\t<agent>` for OpenCode and Hermes when they're
/// there (so the first look needn't wait for the host's check).
/// `HARLO_TEST_PI_SESSIONS` (tests) replaces pi's folder.
const LIST_SCRIPT: &str = r#"list() { [ -d "$2" ] || return 0
if find "$2" -maxdepth 0 -printf '' 2>/dev/null; then find "$2" -mindepth 2 -maxdepth 2 -name '*.jsonl' -type f -printf "$1\t%T@\t%s\t%p\n"
else for f in "$2"/*/*.jsonl; do [ -f "$f" ] && printf '%s\t%s\t%s\t%s\n' "$1" "$(stat -f %m "$f")" "$(stat -f %z "$f")" "$f"; done; fi; }
list omp "$HOME/.omp/agent/sessions"; list pi "${HARLO_TEST_PI_SESSIONS:-${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}/sessions}"; list claude "${CLAUDE_CONFIG_DIR:-$HOME/.claude}/projects"; list harlo "${XDG_DATA_HOME:-$HOME/.local/share}/harlo-agent/sessions"
for p in opencode hermes; do case ":${HARLO_TEST_HIDE_AGENTS:-}:" in *":$p:"*) continue;; esac; command -v "$p" >/dev/null 2>&1 && printf 'has\t%s\n' "$p"; done; true"#;

/// The agent a listed file is of.
fn listed_kind(name: &str) -> HarnessKind {
    match name {
        "omp" => HarnessKind::Omp,
        "claude" => HarnessKind::Claude,
        "harlo" => HarnessKind::Harlo,
        _ => HarnessKind::Pi,
    }
}

/// Record and head/tail separators in a batch read (control characters
/// never appear raw in JSON lines).
const RECORD: char = '\u{1e}';
const TAIL: char = '\u{1d}';
const BATCH: usize = 40;

#[derive(Default)]
pub struct RemoteScanner {
    /// Each file's (mtime, size) when last read.
    known: HashMap<PathBuf, (i64, u64)>,
    /// What was read (None: not a session).
    infos: HashMap<PathBuf, Option<SessionInfo>>,
    /// Reported to the sidebar (inside a project).
    shown: HashSet<PathBuf>,
}

impl RemoteScanner {
    /// One host: what changed since the last poll among its sessions.
    /// `roots`: that host's project folders (`ssh://` paths), each of which
    /// OpenCode is asked about too.
    pub fn poll(&mut self, host: &Host, roots: &[PathBuf]) -> Result<ScanDelta> {
        self.poll_with(host, roots, true)
    }

    /// Like [`poll`](Self::poll), but OpenCode's sessions aren't asked for
    /// (its server there may take a while to start): the ones already shown
    /// stay. A host's first look, so the others show at once.
    pub fn poll_quick(&mut self, host: &Host, roots: &[PathBuf]) -> Result<ScanDelta> {
        self.poll_with(host, roots, false)
    }

    fn poll_with(&mut self, host: &Host, roots: &[PathBuf], ask_opencode: bool) -> Result<ScanDelta> {
        let listing = remote::run_ok(host, LIST_SCRIPT, Duration::from_secs(30))?;
        let mut seen: HashMap<PathBuf, (HarnessKind, i64, u64)> = HashMap::new();
        let mut there: HashSet<&str> = HashSet::new();
        for line in listing.lines() {
            if let Some(agent) = line.strip_prefix("has\t") {
                there.insert(agent.trim());
                continue;
            }
            let mut parts = line.splitn(4, '\t');
            let (Some(kind), Some(mtime), Some(size), Some(path)) = (parts.next(), parts.next(), parts.next(), parts.next()) else { continue };
            let kind = listed_kind(kind);
            let mtime = mtime.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
            seen.insert(remote::join(&host.name, path), (kind, mtime, size.parse().unwrap_or(0)));
        }
        // OpenCode's sessions: from its server there (its database is there).
        let mut from_api: HashMap<PathBuf, SessionInfo> = HashMap::new();
        let found = |agent: &str| there.contains(agent) || remote::found_on(&host.name, agent);
        let mut opencode_listed = false;
        if ask_opencode && found("opencode") {
            let folders: Vec<String> = roots.iter().map(|r| harlo_harness::on_host(r)).collect();
            match harlo_harness::opencode_remote_sessions(&remote::launch(host), &folders) {
                Ok(list) => {
                    opencode_listed = true;
                    for (info, changed) in list {
                        // (Its last change in the size's place: a rename shows too.)
                        let mtime = info.modified.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
                        seen.insert(info.path.clone(), (HarnessKind::OpenCode, mtime, changed));
                        from_api.insert(info.path.clone(), info);
                    }
                }
                Err(e) => tracing::debug!("{}: OpenCode's sessions: {e:#}", host.name),
            }
        }
        // An ACP agent's: the ones harlo started there (its index).
        for kind in [HarnessKind::Hermes] {
            if found(kind.as_str()) {
                for info in harlo_harness::acp::remote_sessions(kind, &host.name) {
                    let mtime = info.modified.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
                    seen.insert(info.path.clone(), (kind, mtime, info.size));
                    from_api.insert(info.path.clone(), info);
                }
            }
        }
        let prefix = format!("ssh://{}/", host.name);
        // OpenCode not asked (or not answering): what it listed before stays.
        if !opencode_listed {
            for (path, info) in &self.infos {
                if let Some(info) = info.as_ref().filter(|i| i.harness == HarnessKind::OpenCode && path.to_string_lossy().starts_with(&prefix))
                    && let Some((m, sz)) = self.known.get(path)
                {
                    seen.entry(path.clone()).or_insert((info.harness, *m, *sz));
                }
            }
        }
        note_files(&prefix, &seen);
        // Gone from the host.
        let gone: Vec<PathBuf> = self.known.keys().filter(|p| p.to_string_lossy().starts_with(&prefix) && !seen.contains_key(*p)).cloned().collect();
        for p in &gone {
            self.known.remove(p);
            self.infos.remove(p);
        }
        // New or changed: read them.
        let changed: Vec<(PathBuf, HarnessKind, i64, u64)> = seen
            .iter()
            .filter(|(p, (_, m, s))| self.known.get(*p) != Some(&(*m, *s)))
            .map(|(p, (k, m, s))| (p.clone(), *k, *m, *s))
            .collect();
        // (OpenCode's and ACP agents' came with the listing.)
        let listed = |k: HarnessKind| matches!(k, HarnessKind::OpenCode | HarnessKind::Hermes);
        for (path, _, mtime, size) in changed.iter().filter(|c| listed(c.1)) {
            self.known.insert(path.clone(), (*mtime, *size));
            self.infos.insert(path.clone(), from_api.remove(path));
        }
        let files: Vec<(PathBuf, HarnessKind, i64, u64)> = changed.iter().filter(|c| !listed(c.1)).cloned().collect();
        for chunk in files.chunks(BATCH) {
            let texts = read_batch(host, chunk.iter().map(|c| c.0.as_path()))?;
            for (path, kind, mtime, size) in chunk {
                let info = texts.get(path).and_then(|(head, tail)| {
                    let modified = UNIX_EPOCH + Duration::from_secs((*mtime).max(0) as u64);
                    let mut info = store::session_from_text(*kind, path, *size, modified, head, tail.as_deref())?;
                    info.cwd = remote::join(&host.name, &info.cwd.to_string_lossy());
                    Some(info)
                });
                self.known.insert(path.clone(), (*mtime, *size));
                self.infos.insert(path.clone(), info);
            }
        }
        // What the sidebar should show now.
        let mut delta = ScanDelta::default();
        let changed_paths: HashSet<&PathBuf> = changed.iter().map(|c| &c.0).collect();
        let host_paths: Vec<PathBuf> = self.infos.keys().filter(|p| p.to_string_lossy().starts_with(&prefix)).cloned().collect();
        for path in host_paths {
            let visible = self.infos.get(&path).is_some_and(|i| i.is_some());
            match (visible, self.shown.contains(&path)) {
                (true, false) => {
                    self.shown.insert(path.clone());
                    delta.upserts.push(self.infos[&path].clone().expect("visible"));
                }
                (true, true) if changed_paths.contains(&path) => delta.upserts.push(self.infos[&path].clone().expect("visible")),
                (false, true) => {
                    self.shown.remove(&path);
                    delta.removed.push(path);
                }
                _ => {}
            }
        }
        for p in gone {
            if self.shown.remove(&p) {
                delta.removed.push(p);
            }
        }
        Ok(delta)
    }

    /// Everything of a host leaves the sidebar (it was removed).
    pub fn forget_host(&mut self, host_name: &str) -> ScanDelta {
        let prefix = format!("ssh://{host_name}/");
        note_files(&prefix, &HashMap::new());
        self.known.retain(|p, _| !p.to_string_lossy().starts_with(&prefix));
        self.infos.retain(|p, _| !p.to_string_lossy().starts_with(&prefix));
        let removed: Vec<PathBuf> = self.shown.iter().filter(|p| p.to_string_lossy().starts_with(&prefix)).cloned().collect();
        for p in &removed {
            self.shown.remove(p);
        }
        ScanDelta { upserts: Vec::new(), removed }
    }
}

/// The pi, Oh My Pi and Harlo session files last listed on each host, for Stats.
fn files() -> &'static std::sync::Mutex<HashMap<PathBuf, (HarnessKind, i64, u64)>> {
    static FILES: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, (HarnessKind, i64, u64)>>> = std::sync::OnceLock::new();
    FILES.get_or_init(Default::default)
}

/// A host's listing replaces what was noted for it (`prefix`: `ssh://host/`).
fn note_files(prefix: &str, seen: &HashMap<PathBuf, (HarnessKind, i64, u64)>) {
    let mut files = files().lock().unwrap();
    files.retain(|p, _| !p.to_string_lossy().starts_with(prefix));
    files.extend(seen.iter().filter(|(_, (k, ..))| matches!(k, HarnessKind::Pi | HarnessKind::Omp | HarnessKind::Harlo)).map(|(p, v)| (p.clone(), *v)));
}

/// pi's, Oh My Pi's and Harlo's session files on other machines, as last listed:
/// `(ssh path, agent, modified (seconds), size)`.
pub fn session_files() -> Vec<(PathBuf, HarnessKind, i64, u64)> {
    files().lock().unwrap().iter().map(|(p, (k, m, s))| (p.clone(), *k, *m, *s)).collect()
}

/// Reads session files on a host in one command: each whole when small,
/// else its first and last part (cut to complete lines).
fn read_batch<'a>(host: &Host, paths: impl Iterator<Item = &'a Path>) -> Result<HashMap<PathBuf, (String, Option<String>)>> {
    let paths: Vec<&Path> = paths.collect();
    let files: Vec<String> = paths.iter().map(|p| remote::quote(&harlo_harness::on_host(p))).collect();
    let script = format!(
        "for f in {}; do printf '\\036%s\\n' \"$f\"; s=$(wc -c < \"$f\" 2>/dev/null || echo 0); \
         if [ \"$s\" -le {limit} ]; then cat \"$f\"; else head -c {part} \"$f\"; printf '\\n\\035\\n'; tail -c {part} \"$f\"; fi; done",
        files.join(" "),
        limit = store::FULL_READ_LIMIT_BYTES,
        part = store::PART_BYTES,
    );
    let out = remote::run(host, &script, Duration::from_secs(60))?;
    if !out.status.success() && out.stdout.is_empty() {
        anyhow::bail!("{}", remote::ssh_error(&host.name, &out));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut result = HashMap::new();
    for record in text.split(RECORD).skip(1) {
        let Some((path, body)) = record.split_once('\n') else { continue };
        let entry = match body.split_once(&format!("\n{TAIL}\n")) {
            Some((head, tail)) => (
                store::lossy_complete_lines(head.as_bytes(), false),
                Some(store::lossy_complete_lines(tail.as_bytes(), true)),
            ),
            None => (body.to_owned(), None),
        };
        result.insert(remote::join(&host.name, path), entry);
    }
    Ok(result)
}

/// Seconds since the epoch of a time (for tests).
#[cfg(test)]
fn secs(t: std::time::SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Against a real SSH server, with fake session files in a scratch home
    /// (not run by default): `HARLO_TEST_SSH=… cargo test -p harlo-core -- --ignored remote_scan`
    #[test]
    fn lists_harlos_sessions() {
        let home = crate::scratch::Scratch::new(std::env::temp_dir().join(format!("harlo-remote-list-{}", std::process::id())));
        let dir = home.join(".local/share/harlo-agent/sessions/--work--");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("s.jsonl"), "{}\n").unwrap();
        let out = std::process::Command::new("sh").args(["-c", LIST_SCRIPT]).env_clear().env("HOME", &*home).env("PATH", "/usr/bin:/bin").output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text.lines().find(|l| l.starts_with("harlo\t")).unwrap_or_else(|| panic!("{text}"));
        assert!(line.ends_with("/.local/share/harlo-agent/sessions/--work--/s.jsonl"), "{line}");
        assert_eq!(listed_kind("harlo"), HarnessKind::Harlo);
        assert_eq!(listed_kind("pi"), HarnessKind::Pi);
    }

    #[test]
    #[ignore]
    fn remote_scan() {
        let args = remote::parse_ssh_command(&std::env::var("HARLO_TEST_SSH").expect("HARLO_TEST_SSH")).unwrap();
        let host = Host { name: "t".into(), ssh: args };
        remote::set_hosts(vec![host.clone()]);
        // A pi sessions folder of our own (set through the SSH environment).
        let dir = std::env::temp_dir().join(format!("harlo-remote-scan-{}", std::process::id()));
        let sessions = dir.join("pi/sessions/--proj--");
        std::fs::create_dir_all(&sessions).unwrap();
        let proj = dir.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let header = |cwd: &Path, text: &str| format!(
            "{{\"type\":\"session\",\"id\":\"s1\",\"cwd\":\"{}\",\"timestamp\":\"2026-01-01T00:00:00Z\"}}\n{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"{text}\"}}]}}}}\n",
            cwd.display()
        );
        std::fs::write(sessions.join("a.jsonl"), header(&proj, "inside the project")).unwrap();
        std::fs::write(sessions.join("b.jsonl"), header(Path::new("/elsewhere"), "outside")).unwrap();
        // A big one: head and tail.
        let big = format!("{}{}", header(&proj, "big one"), "{\"type\":\"custom\",\"data\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"}\n".repeat(10_000));
        std::fs::write(sessions.join("c.jsonl"), big).unwrap();
        let host = Host { ssh: { let mut a = host.ssh.clone(); a.insert(0, "-o".into()); a.insert(1, format!("SetEnv=HARLO_TEST_PI_SESSIONS={}", dir.join("pi/sessions").display())); a }, ..host };
        let mut scanner = RemoteScanner::default();
        let roots = vec![remote::join("t", &proj.to_string_lossy())];
        let delta = scanner.poll(&host, &roots).unwrap();
        let mut titles: Vec<String> = delta.upserts.iter().map(|i| i.first_message.clone()).collect();
        titles.sort();
        assert_eq!(titles, ["big one", "inside the project", "outside"], "{:?}", delta.upserts);
        assert!(delta.upserts.iter().all(|i| i.path.starts_with("ssh://t/") && i.harness == HarnessKind::Pi && secs(i.modified) > 0));
        // (Outside the project: in Other sessions, its folder on the host.)
        assert!(delta.upserts.iter().any(|i| i.cwd == remote::join("t", "/elsewhere")));
        // Nothing changed: nothing to report.
        assert!(scanner.poll(&host, &roots).unwrap().is_empty());
        // Removed on the host.
        std::fs::remove_file(sessions.join("a.jsonl")).unwrap();
        let delta = scanner.poll(&host, &roots).unwrap();
        assert_eq!(delta.removed.len(), 1);
        // No projects: they stay (in Other sessions).
        assert!(scanner.poll(&host, &[]).unwrap().removed.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
