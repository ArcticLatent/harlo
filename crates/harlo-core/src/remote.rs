//! Remote machines over SSH (Settings → Remote Instances).
//!
//! A location on a remote machine is written `ssh://<host>/<absolute path>`
//! wherever harlo keeps a path (projects, session folders, session files),
//! so the rest of the app keys and joins them like local ones; the places
//! that touch the disk or run programs route them through [`Host`].
//!
//! Every host shares one SSH connection (OpenSSH's ControlMaster, kept for
//! ten minutes after its last use), so the many small commands harlo runs
//! (listing folders, git status, reading session files) don't each pay for
//! a new login. Authentication is non-interactive: keys or an agent.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

const SCHEME: &str = "ssh://";

/// Folders where pi and omp usually live, ahead of the remote PATH (a
/// non-interactive SSH command gets a minimal one).
pub const PATH_PREFIX: &str = "$HOME/.local/bin:$HOME/.bun/bin:$HOME/.pi/agent/bin:$HOME/.opencode/bin:$HOME/.npm-global/bin:$HOME/.cargo/bin:/usr/local/bin";

/// A remote machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// Unique, as shown in harlo and in `ssh://<name>/…`.
    pub name: String,
    /// What follows `ssh`: options and the destination (`user@nas`, an
    /// alias from ~/.ssh/config, `-p 2222 me@10.0.0.5`).
    pub ssh: Vec<String>,
}

static HOSTS: RwLock<Vec<Host>> = RwLock::new(Vec::new());
/// Home folders, learnt on first use.
static HOMES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// The configured hosts (the app sets them from its settings).
pub fn set_hosts(hosts: Vec<Host>) {
    *HOSTS.write().unwrap_or_else(|e| e.into_inner()) = hosts;
}

pub fn hosts() -> Vec<Host> {
    HOSTS.read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn host(name: &str) -> Option<Host> {
    HOSTS.read().unwrap_or_else(|e| e.into_inner()).iter().find(|h| h.name == name).cloned()
}

/// A valid host name: letters, digits, `-`, `_`, `.`.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 40 && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

// ---- remote paths ------------------------------------------------------

/// `ssh://nas/home/me/x` → ("nas", "/home/me/x"); None for local paths.
pub fn split(path: &Path) -> Option<(String, String)> {
    let s = path.to_str()?;
    let rest = s.strip_prefix(SCHEME)?;
    let (host, remote) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    (!host.is_empty()).then(|| (host.to_owned(), remote.to_owned()))
}

pub fn is_remote(path: &Path) -> bool {
    split(path).is_some()
}

/// The host part of a remote path.
pub fn host_of(path: &Path) -> Option<String> {
    split(path).map(|(h, _)| h)
}

/// `nas` + `/home/me/x` → `ssh://nas/home/me/x`.
pub fn join(host: &str, remote_path: &str) -> PathBuf {
    let remote = if remote_path.starts_with('/') { remote_path.to_owned() } else { format!("/{remote_path}") };
    PathBuf::from(format!("{SCHEME}{host}{remote}"))
}

/// For display: `nas:~/x` (with the host's home shortened when known),
/// or the path itself when local.
pub fn display(path: &Path) -> String {
    match split(path) {
        Some((host, remote)) => {
            let home = HOMES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&host).cloned());
            let shown = match home {
                Some(h) if remote == h => "~".to_owned(),
                Some(h) if remote.starts_with(&format!("{h}/")) => format!("~{}", &remote[h.len()..]),
                _ => remote,
            };
            format!("{host}:{shown}")
        }
        None => path.display().to_string(),
    }
}

// ---- hosts in ~/.ssh/config ----------------------------------------------

/// A host you can connect to by name, from your SSH config.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigHost {
    /// The `Host` name (what `ssh <name>` takes).
    pub name: String,
    pub hostname: String,
    pub user: String,
    pub port: String,
}

/// The hosts in `~/.ssh/config` (and its `Include`s), then
/// `/etc/ssh/ssh_config`, by name. Patterns (`*`, `?`, `!x`) and names that
/// can't be hosts are left out.
pub fn ssh_config_hosts() -> Vec<ConfigHost> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let mut hosts = Vec::new();
    for file in [home.join(".ssh/config"), PathBuf::from("/etc/ssh/ssh_config")] {
        parse_ssh_config(&file, &home, &mut seen, &mut hosts, 0);
    }
    let mut unique = std::collections::HashSet::new();
    hosts.retain(|h: &ConfigHost| unique.insert(h.name.clone()));
    hosts.sort_by_key(|h| h.name.to_lowercase());
    hosts
}

fn usable_host_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(['.', '-', '!'])
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '@' | ':' | '[' | ']'))
}

fn parse_ssh_config(file: &Path, home: &Path, seen: &mut std::collections::HashSet<PathBuf>, hosts: &mut Vec<ConfigHost>, depth: usize) {
    if depth > 8 || !seen.insert(file.to_path_buf()) {
        return;
    }
    let Ok(text) = std::fs::read_to_string(file) else { return };
    // The hosts of the current `Host` line (options apply to them).
    let mut current: Vec<usize> = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        let Some((key, value)) = line.split_once(|c: char| c.is_whitespace() || c == '=') else { continue };
        let value = value.trim().trim_start_matches('=').trim().trim_matches('"');
        match key.to_ascii_lowercase().as_str() {
            "host" => {
                current.clear();
                for name in value.split_whitespace() {
                    if usable_host_name(name) {
                        current.push(hosts.len());
                        hosts.push(ConfigHost { name: name.to_owned(), ..Default::default() });
                    }
                }
            }
            // Conditional blocks: their options aren't a host's.
            "match" => current.clear(),
            "hostname" | "user" | "port" => {
                for &i in &current {
                    let h = &mut hosts[i];
                    let slot = match key.to_ascii_lowercase().as_str() {
                        "hostname" => &mut h.hostname,
                        "user" => &mut h.user,
                        _ => &mut h.port,
                    };
                    // The first value wins, as in ssh.
                    if slot.is_empty() {
                        *slot = value.to_owned();
                    }
                }
            }
            "include" => {
                let base = if file.starts_with(home.join(".ssh")) { home.join(".ssh") } else { PathBuf::from("/etc/ssh") };
                for pattern in value.split_whitespace() {
                    let path = if let Some(rest) = pattern.strip_prefix("~/") { home.join(rest) } else if pattern.starts_with('/') { PathBuf::from(pattern) } else { base.join(pattern) };
                    for included in glob_files(&path) {
                        parse_ssh_config(&included, home, seen, hosts, depth + 1);
                    }
                }
            }
            _ => {}
        }
    }
}

/// A path whose file name may have `*` / `?` (as ssh's Include allows).
fn glob_files(path: &Path) -> Vec<PathBuf> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !name.contains(['*', '?']) {
        return if path.is_file() { vec![path.to_path_buf()] } else { Vec::new() };
    }
    let Some(dir) = path.parent() else { return Vec::new() };
    let matches = |s: &str| {
        fn m(p: &[char], s: &[char]) -> bool {
            match (p.first(), s.first()) {
                (None, None) => true,
                (Some('*'), _) => m(&p[1..], s) || (!s.is_empty() && m(p, &s[1..])),
                (Some('?'), Some(_)) => m(&p[1..], &s[1..]),
                (Some(a), Some(b)) if a == b => m(&p[1..], &s[1..]),
                _ => false,
            }
        }
        m(&name.chars().collect::<Vec<_>>(), &s.chars().collect::<Vec<_>>())
    };
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_file() && p.file_name().is_some_and(|n| matches(&n.to_string_lossy()))).collect();
    out.sort();
    out
}

// ---- the SSH command line ------------------------------------------------

/// Splits what the user typed (`ssh -p 2222 me@nas`, or just `me@nas`)
/// into ssh's arguments, without the leading `ssh`. Options that would
/// break harlo's shared connection or run commands are refused.
pub fn parse_ssh_command(text: &str) -> Result<Vec<String>> {
    let mut words = shell_words(text.trim())?;
    if words.first().map(String::as_str) == Some("ssh") {
        words.remove(0);
    }
    if words.is_empty() {
        bail!("Enter a destination, like user@host or a name from ~/.ssh/config.");
    }
    let mut i = 0;
    while i < words.len() {
        let w = &words[i];
        let lower = w.to_ascii_lowercase();
        let option_value = if w == "-o" { words.get(i + 1).map(|v| v.to_ascii_lowercase()) } else { lower.strip_prefix("-o").map(str::to_owned) };
        if let Some(v) = option_value {
            for banned in ["controlmaster", "controlpath", "controlpersist", "localcommand", "permitlocalcommand", "proxycommand=none"] {
                if v.starts_with(banned) {
                    bail!("The SSH option {v} isn't allowed here (harlo manages the connection).");
                }
            }
        }
        if matches!(w.as_str(), "-M" | "-N" | "-f" | "-t" | "-tt" | "-T" | "-O" | "-W" | "-S") {
            bail!("The SSH flag {w} isn't allowed here (harlo manages the connection).");
        }
        i += 1;
    }
    if words.last().is_some_and(|d| d.starts_with('-')) {
        bail!("The last word should be the destination, like user@host.");
    }
    Ok(words)
}

/// POSIX-ish word splitting: spaces separate, quotes group.
fn shell_words(text: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => cur.push(ch),
                        None => bail!("Unclosed quote."),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => cur.extend(chars.next()),
                        Some(ch) => cur.push(ch),
                        None => bail!("Unclosed quote."),
                    }
                }
            }
            '\\' => {
                in_word = true;
                cur.extend(chars.next());
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    Ok(words)
}

/// A word for the remote shell, single-quoted.
pub fn quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+=:@%,".contains(c)) {
        return s.to_owned();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A remote path for the shell: `~` and `~/x` expand, the rest is quoted.
pub fn quote_path(path: &str) -> String {
    if path == "~" {
        "\"$HOME\"".to_owned()
    } else if let Some(rest) = path.strip_prefix("~/") {
        format!("\"$HOME\"/{}", quote(rest))
    } else {
        quote(path)
    }
}

/// Where the shared connection's socket lives (short: socket paths are
/// limited to ~100 bytes).
fn control_path(host: &Host) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    host.ssh.hash(&mut h);
    // ssh adds ~17 characters while it sets the socket up; past ~100 the
    // socket can't be created, so a long runtime folder falls back to /tmp.
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(|d| PathBuf::from(d).join("harlo-ssh"));
    let dir = match runtime {
        Some(d) if d.as_os_str().len() <= 60 => d,
        _ => PathBuf::from(format!("/tmp/harlo-ssh-{}", unsafe_uid())),
    };
    let _ = std::fs::create_dir_all(&dir);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    dir.join(format!("{:016x}", h.finish()))
}

/// The user id, for a private /tmp folder name.
fn unsafe_uid() -> String {
    std::env::var("UID").ok().or_else(|| {
        std::fs::read_to_string("/proc/self/status").ok()?.lines().find_map(|l| l.strip_prefix("Uid:").map(|v| v.split_whitespace().next().unwrap_or("0").to_owned()))
    }).unwrap_or_else(|| "0".into())
}

/// ssh's options: the host's own first (for each option ssh keeps the
/// first value), then the shared connection, no prompts (a new host's key
/// is recorded; a changed one is refused), keep-alives.
fn ssh_options(host: &Host) -> Vec<String> {
    let control = control_path(host);
    let mut args: Vec<String> = host.ssh[..host.ssh.len().saturating_sub(1)].to_vec();
    args.extend([
        "-o", "ControlMaster=auto",
        "-o", "ControlPersist=600",
        "-o", "BatchMode=yes",
        "-o", "ConnectTimeout=10",
        "-o", "ServerAliveInterval=15",
        "-o", "ServerAliveCountMax=3",
        "-o", "StrictHostKeyChecking=accept-new",
    ]
    .iter()
    .map(|s| (*s).to_owned()));
    args.push("-o".into());
    args.push(format!("ControlPath={}", control.display()));
    args
}

/// ssh's arguments up to and including the destination.
pub fn ssh_args(host: &Host) -> Vec<String> {
    let mut args = ssh_options(host);
    args.extend(host.ssh.last().cloned());
    args
}

/// `ssh … <destination> <script>`: the script runs in the remote user's
/// shell with [`PATH_PREFIX`] added.
pub fn command(host: &Host, script: &str) -> Command {
    let mut cmd = Command::new("ssh");
    // (HARLO_TEST_REMOTE_BIN: the click tests' stand-ins, first.)
    cmd.args(ssh_args(host)).arg(format!("PATH=\"${{HARLO_TEST_REMOTE_BIN:+$HARLO_TEST_REMOTE_BIN:}}{PATH_PREFIX}:$PATH\"; export PATH; {script}"));
    cmd.stdin(Stdio::null());
    cmd
}

/// Runs a script on the host and waits (at most `timeout`).
pub fn run(host: &Host, script: &str, timeout: Duration) -> Result<Output> {
    run_with_input(host, script, None, timeout)
}

/// Like [`run`], feeding `input` on stdin.
pub fn run_with_input(host: &Host, script: &str, input: Option<&[u8]>, timeout: Duration) -> Result<Output> {
    let mut cmd = command(host, script);
    cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().context("could not run ssh")?;
    if let (Some(data), Some(mut stdin)) = (input, child.stdin.take()) {
        let data = data.to_vec();
        std::thread::spawn(move || {
            let _ = stdin.write_all(&data);
        });
    }
    // Read output on threads so a chatty command can't block on a full pipe.
    let mut out = child.stdout.take().context("ssh stdout")?;
    let mut err = child.stderr.take().context("ssh stderr")?;
    let out_reader = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = std::io::Read::read_to_end(&mut out, &mut v);
        v
    });
    let err_reader = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = std::io::Read::read_to_end(&mut err, &mut v);
        v
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{} did not answer in {} s", host.name, timeout.as_secs());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(Output { status, stdout: out_reader.join().unwrap_or_default(), stderr: err_reader.join().unwrap_or_default() })
}

/// The script's stdout, or an error with its last stderr line.
pub fn run_ok(host: &Host, script: &str, timeout: Duration) -> Result<String> {
    let out = run(host, script, timeout)?;
    if !out.status.success() {
        bail!("{}", ssh_error(&host.name, &out));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A readable reason for a failed SSH command.
pub fn ssh_error(host: &str, out: &Output) -> String {
    let err = String::from_utf8_lossy(&out.stderr);
    let last = err.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if out.status.code() == Some(255) {
        if last.contains("Permission denied") {
            return format!("{host}: SSH login refused. harlo signs in with your SSH keys or agent (no passwords); try `ssh-copy-id` for this host. ({last})");
        }
        if last.is_empty() {
            return format!("{host}: SSH connection failed.");
        }
        return format!("{host}: {last}");
    }
    if last.is_empty() { format!("{host}: the command failed ({})", out.status) } else { last.to_owned() }
}

/// What a check found on the host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostInfo {
    pub os: String,
    pub home: String,
    /// The agents' paths on the host ("" when missing).
    pub pi: String,
    pub omp: String,
    pub claude: String,
    pub opencode: String,
    pub hermes: String,
    /// Their versions (`program` → "1.0.4").
    pub versions: std::collections::BTreeMap<String, String>,
    /// Claude's sign-in there: "yes" (its credentials file), "keychain"
    /// (macOS keeps it where SSH can't look), "no".
    pub claude_auth: String,
}

/// How an agent starts on `host`: over its SSH connection, with its PATH
/// (and, for the click tests, `HARLO_TEST_REMOTE_BIN` first).
pub fn launch(host: &Host) -> harlo_harness::RemoteLaunch {
    harlo_harness::RemoteLaunch {
        host: host.name.clone(),
        ssh_args: ssh_args(host),
        prelude: format!("PATH=\"${{HARLO_TEST_REMOTE_BIN:+$HARLO_TEST_REMOTE_BIN:}}{PATH_PREFIX}:$PATH\"; export PATH;"),
        cleanup: String::new(),
    }
}

/// The agents harlo runs, as the host check names them.
pub const AGENTS: [&str; 5] = ["omp", "pi", "claude", "opencode", "hermes"];

/// How to install each agent (its official one-line installer).
pub fn install_command(agent: &str) -> &'static str {
    match agent {
        "pi" => "curl -fsSL https://pi.dev/install.sh | sh",
        "omp" => "curl -fsSL https://omp.sh/install | sh",
        "claude" => "curl -fsSL https://claude.ai/install.sh | bash",
        "opencode" => "curl -fsSL https://opencode.ai/install | bash",
        "hermes" => "curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash",
        _ => "",
    }
}

/// The host check: the system, home, each agent (path and `--version`, 10 s
/// at most each) and Claude's sign-in. (Run by the user's login shell, zsh
/// included: no word splitting of variables.)
// (HARLO_TEST_HOME: the click tests' scratch home on the test host.)
const CHECK_SCRIPT: &str = r#"printf 'os=%s\nhome=%s\n' "$(uname -s)" "${HARLO_TEST_HOME:-$HOME}"
ver() { if command -v timeout >/dev/null 2>&1; then timeout 10 "$@"; else "$@"; fi; }
for p in pi omp claude opencode hermes; do
  w=$(command -v "$p" 2>/dev/null || true)
  case ":${HARLO_TEST_HIDE_AGENTS:-}:" in *":$p:"*) w="";; esac
  printf '%s=%s\n' "$p" "$w"
  if [ -n "$w" ]; then printf '%s_version=%s\n' "$p" "$(ver "$w" --version 2>/dev/null </dev/null | head -n 1)"; fi
done
c="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
if [ -s "$c/.credentials.json" ]; then echo claude_auth=yes; elif [ "$(uname -s)" = Darwin ]; then echo claude_auth=keychain; else echo claude_auth=no; fi"#;

/// Connects (or reuses the connection) and looks around.
pub fn check(host: &Host) -> Result<HostInfo> {
    let text = run_ok(host, CHECK_SCRIPT, Duration::from_secs(60))?;
    let mut info = HostInfo::default();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k {
                "os" => info.os = v.to_owned(),
                "home" => info.home = v.to_owned(),
                "pi" => info.pi = v.to_owned(),
                "omp" => info.omp = v.to_owned(),
                "claude" => info.claude = v.to_owned(),
                "opencode" => info.opencode = v.to_owned(),
                "hermes" => info.hermes = v.to_owned(),
                "claude_auth" => info.claude_auth = v.to_owned(),
                k if k.ends_with("_version") => {
                    let number = crate::providers::version_number(v);
                    if !number.is_empty() {
                        info.versions.insert(k.trim_end_matches("_version").to_owned(), number);
                    }
                }
                _ => {}
            }
        }
    }
    if !info.home.is_empty() {
        HOMES.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(host.name.clone(), info.home.clone());
    }
    INFOS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(host.name.clone(), info.clone());
    Ok(info)
}

static INFOS: Mutex<Option<HashMap<String, HostInfo>>> = Mutex::new(None);

/// Whether the last check of the host found `program` (an agent).
pub fn found_on(host_name: &str, program: &str) -> bool {
    info_of(host_name).is_some_and(|i| i.has(program))
}

/// What the last check of the host found (None: not checked yet).
pub fn info_of(host_name: &str) -> Option<HostInfo> {
    INFOS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(host_name)).cloned()
}

impl HostInfo {
    /// The agent is there.
    pub fn has(&self, agent: &str) -> bool {
        match agent {
            "pi" => !self.pi.is_empty(),
            "omp" => !self.omp.is_empty(),
            "claude" => !self.claude.is_empty(),
            "opencode" => !self.opencode.is_empty(),
            "hermes" => !self.hermes.is_empty(),
            _ => false,
        }
    }
}

/// The host's home folder (asked once).
pub fn home(host: &Host) -> Result<String> {
    if let Some(h) = HOMES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&host.name).cloned()) {
        return Ok(h);
    }
    Ok(check(host)?.home)
}

/// Whether the shared connection is up.
pub fn connected(host: &Host) -> bool {
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_options(host)).args(["-O", "check"]).args(host.ssh.last());
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.status().is_ok_and(|s| s.success())
}

/// Closes the shared connection (sessions using it end).
pub fn disconnect(host: &Host) {
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_options(host)).args(["-O", "exit"]).args(host.ssh.last());
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let _ = cmd.status();
}

// ---- files on a host -----------------------------------------------------

type HomeFiles = HashMap<(String, String), (Instant, String)>;
static HOME_FILES: Mutex<Option<HomeFiles>> = Mutex::new(None);

/// A file in the host's home (`rel`, like `.omp/agent/config.yml`), read
/// again only when older than `max_age`; "" when missing or unreachable.
/// Blocks while reading: call it off the UI thread.
pub fn home_file_cached(host_name: &str, rel: &str, max_age: Duration) -> String {
    let key = (host_name.to_owned(), rel.to_owned());
    if let Some((at, text)) = HOME_FILES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&key).cloned())
        && at.elapsed() < max_age {
            return text;
        }
    let text = host(host_name)
        .and_then(|h| run(&h, &format!("cat \"$HOME\"/{} 2>/dev/null", quote(rel)), FILE_TIMEOUT).ok())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    HOME_FILES.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key, (Instant::now(), text.clone()));
    text
}

/// The copy [`home_file_cached`] last read, without blocking.
pub fn home_file_peek(host_name: &str, rel: &str) -> Option<String> {
    let key = (host_name.to_owned(), rel.to_owned());
    HOME_FILES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&key).map(|(_, t)| t.clone()))
}

fn host_and_path(path: &Path) -> Result<(Host, String)> {
    let (name, remote) = split(path).with_context(|| format!("{} is not a remote path", path.display()))?;
    let host = host(&name).with_context(|| format!("unknown remote host {name} (Settings → Remote Instances)"))?;
    Ok((host, remote))
}

const FILE_TIMEOUT: Duration = Duration::from_secs(30);

/// A remote file's bytes.
pub fn read(path: &Path) -> Result<Vec<u8>> {
    let (host, remote) = host_and_path(path)?;
    let out = run(&host, &format!("cat -- {}", quote_path(&remote)), FILE_TIMEOUT)?;
    if !out.status.success() {
        bail!("{}", ssh_error(&host.name, &out));
    }
    Ok(out.stdout)
}

/// Writes a remote file (its folder must exist).
pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let (host, remote) = host_and_path(path)?;
    let out = run_with_input(&host, &format!("cat > {}", quote_path(&remote)), Some(bytes), FILE_TIMEOUT)?;
    if !out.status.success() {
        bail!("{}", ssh_error(&host.name, &out));
    }
    Ok(())
}

/// An entry of a remote folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Seconds since the epoch.
    pub mtime: i64,
}

/// A remote folder's entries (hidden ones too), unsorted.
pub fn list_dir(path: &Path) -> Result<Vec<Entry>> {
    let (host, remote) = host_and_path(path)?;
    // One line per entry: type, size, mtime, name (GNU find, else ls).
    let dir = quote_path(&remote);
    let script = format!(
        "cd {dir} || exit 2; if find . -maxdepth 0 -printf '' 2>/dev/null; then find -L . -mindepth 1 -maxdepth 1 -printf '%y\\t%s\\t%T@\\t%f\\n'; \
         else for f in .* *; do [ \"$f\" = . ] || [ \"$f\" = .. ] || {{ [ -e \"$f\" ] || [ -L \"$f\" ]; }} || continue; \
         if [ -d \"$f\" ]; then t=d; else t=f; fi; printf '%s\\t0\\t0\\t%s\\n' \"$t\" \"$f\"; done; fi"
    );
    let out = run(&host, &script, FILE_TIMEOUT)?;
    if !out.status.success() {
        bail!("{}", ssh_error(&host.name, &out));
    }
    Ok(parse_entries(&String::from_utf8_lossy(&out.stdout)))
}

fn parse_entries(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, '\t');
            let kind = parts.next()?;
            let size = parts.next()?.parse().unwrap_or(0);
            let mtime = parts.next()?.split('.').next()?.parse().unwrap_or(0);
            let name = parts.next()?.to_owned();
            // (Symlinks are followed: `find -L` reports their target's type.)
            Some(Entry { name, is_dir: kind == "d", size, mtime })
        })
        .collect()
}

/// Moves a file to the trash: here with `gio trash`; on a remote host with
/// its `gio`, else into ~/.local/share/Trash/files.
pub fn trash(path: &Path) -> Result<()> {
    let Some((name, f)) = split(path) else {
        match Command::new("gio").arg("trash").arg("--").arg(path).output() {
            Ok(out) if out.status.success() => return Ok(()),
            Ok(out) => bail!("Could not move {} to the trash: {}", path.display(), String::from_utf8_lossy(&out.stderr).trim()),
            Err(_) => bail!("Could not move {} to the trash (needs `gio`).", path.display()),
        }
    };
    let host = host(&name).with_context(|| format!("unknown remote host {name}"))?;
    let q = quote(&f);
    let script = format!("gio trash -- {q} 2>/dev/null || {{ mkdir -p \"$HOME/.local/share/Trash/files\" && mv -- {q} \"$HOME/.local/share/Trash/files/\"; }}");
    let out = run(&host, &script, FILE_TIMEOUT)?;
    if !out.status.success() {
        bail!("{}", ssh_error(&host.name, &out));
    }
    Ok(())
}

/// Whether a remote path exists (and is a folder, when `dir`).
pub fn exists(path: &Path, dir: bool) -> bool {
    let Ok((host, remote)) = host_and_path(path) else { return false };
    let test = if dir { "-d" } else { "-e" };
    run(&host, &format!("test {test} {}", quote_path(&remote)), FILE_TIMEOUT).is_ok_and(|o| o.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_paths() {
        let p = join("nas", "/home/me/proj");
        assert_eq!(p, PathBuf::from("ssh://nas/home/me/proj"));
        assert_eq!(split(&p), Some(("nas".into(), "/home/me/proj".into())));
        assert_eq!(split(Path::new("ssh://nas")), Some(("nas".into(), "/".into())));
        assert_eq!(split(Path::new("/home/me")), None);
        assert_eq!(split(Path::new("ssh:///x")), None);
        // Joining and parents stay remote.
        assert_eq!(split(&p.join("src/main.rs")), Some(("nas".into(), "/home/me/proj/src/main.rs".into())));
        assert_eq!(split(p.parent().unwrap()), Some(("nas".into(), "/home/me".into())));
        assert_eq!(p.file_name().unwrap(), "proj");
        assert_eq!(display(Path::new("/tmp/x")), "/tmp/x");
        assert_eq!(display(&p), "nas:/home/me/proj");
    }

    #[test]
    fn parses_ssh_commands() {
        assert_eq!(parse_ssh_command("ssh me@nas").unwrap(), ["me@nas"]);
        assert_eq!(parse_ssh_command("  homenas ").unwrap(), ["homenas"]);
        assert_eq!(parse_ssh_command("ssh -p 2222 -i '~/my key' me@10.0.0.5").unwrap(), ["-p", "2222", "-i", "~/my key", "me@10.0.0.5"]);
        assert!(parse_ssh_command("").is_err());
        assert!(parse_ssh_command("ssh -o ControlPath=/x me@nas").is_err());
        assert!(parse_ssh_command("ssh -oControlMaster=no me@nas").is_err());
        assert!(parse_ssh_command("ssh -N me@nas").is_err());
        assert!(parse_ssh_command("ssh me@nas -p").is_err());
        assert!(parse_ssh_command("ssh 'me@nas").is_err());
    }

    #[test]
    fn quotes_for_the_shell() {
        assert_eq!(quote("/home/me/proj"), "/home/me/proj");
        assert_eq!(quote("my dir"), "'my dir'");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote(""), "''");
        assert_eq!(quote_path("~"), "\"$HOME\"");
        assert_eq!(quote_path("~/a b"), "\"$HOME\"/'a b'");
    }

    #[test]
    fn parses_listings() {
        let e = parse_entries("d\t4096\t1700000000.5\tsrc\nf\t12\t1700000001.0\tREADME.md\nf\t3\t1\tname\twith tab\n");
        assert_eq!(e.len(), 3);
        assert!(e[0].is_dir && e[0].name == "src" && e[0].mtime == 1_700_000_000);
        assert!(!e[1].is_dir && e[1].size == 12);
        assert_eq!(e[2].name, "name\twith tab");
    }

    #[test]
    fn valid_names() {
        assert!(valid_name("homenas") && valid_name("build-box.2"));
        assert!(!valid_name("") && !valid_name("a b") && !valid_name("a/b"));
    }

    /// Against a real SSH server (not run by default):
    /// `HARLO_TEST_SSH="$(scripts/test-sshd.sh start /tmp/x)" cargo test -p harlo-core -- --ignored over_ssh`
    #[test]
    #[ignore]
    fn over_ssh() {
        let args = parse_ssh_command(&std::env::var("HARLO_TEST_SSH").expect("HARLO_TEST_SSH")).unwrap();
        set_hosts(vec![Host { name: "t".into(), ssh: args }]);
        let h = host("t").unwrap();
        disconnect(&h);
        let started = Instant::now();
        let info = check(&h).unwrap();
        println!("first connection: {:?} {info:?}", started.elapsed());
        assert!(info.os == "Linux" && info.home.starts_with('/'));
        assert!(connected(&h), "the shared connection stays up");
        let started = Instant::now();
        for _ in 0..10 {
            run_ok(&h, "true", Duration::from_secs(5)).unwrap();
        }
        println!("10 commands over the shared connection: {:?}", started.elapsed());
        let dir = std::env::temp_dir().join(format!("harlo-remote-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub dir")).unwrap();
        let remote_dir = join("t", dir.to_str().unwrap());
        write(&remote_dir.join("it's a file.txt"), b"hello\nthere").unwrap();
        assert_eq!(read(&remote_dir.join("it's a file.txt")).unwrap(), b"hello\nthere");
        let mut entries = list_dir(&remote_dir).unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(entries.iter().map(|e| (e.name.as_str(), e.is_dir)).collect::<Vec<_>>(), [("it's a file.txt", false), ("sub dir", true)]);
        assert_eq!(entries[0].size, 11);
        assert!(exists(&remote_dir.join("sub dir"), true) && !exists(&remote_dir.join("nope"), false));
        assert!(read(&remote_dir.join("nope")).is_err());
        // The editor's save and load, and the Files panel's listing.
        let note = remote_dir.join("sub dir/note.md");
        crate::files::write_text(&note, "remote edit\n").unwrap();
        assert_eq!(crate::files::read_text(&note).unwrap(), crate::files::FileContent::Text("remote edit\n".into()));
        let listed = crate::files::list_dir(&remote_dir, "sub dir").unwrap();
        assert_eq!(listed.iter().map(|e| e.rel.as_str()).collect::<Vec<_>>(), ["sub dir/note.md"]);
        let index = crate::files::FileIndex::build(&remote_dir);
        assert!(index.files.contains(&"sub dir/note.md".to_owned()), "{:?}", index.files);
        std::fs::remove_file(dir.join("sub dir/note.md")).unwrap();
        assert_eq!(display(&remote_dir), format!("t:{}", dir.display()).replace(&info.home, "~"));
        disconnect(&h);
        assert!(!connected(&h));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_ssh_config_hosts() {
        let home = std::env::temp_dir().join(format!("harlo-sshcfg-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".ssh/config.d")).unwrap();
        std::fs::write(home.join(".ssh/config"), "Include config.d/*.conf\n\nHost nas homenas\n  HostName 10.0.0.5\n  User me\n  Port 2222\n\nHost *.lab machine/* !bad .host\n  User x\n\nMatch host foo\n  User nope\n\nHost=lab4 # a comment\n  HostName=lab4.local\n").unwrap();
        std::fs::write(home.join(".ssh/config.d/work.conf"), "Host office-box\n  HostName work.example\n").unwrap();
        let mut seen = std::collections::HashSet::new();
        let mut hosts = Vec::new();
        parse_ssh_config(&home.join(".ssh/config"), &home, &mut seen, &mut hosts, 0);
        let names: Vec<&str> = hosts.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["office-box", "nas", "homenas", "lab4"]);
        assert_eq!(hosts[1], ConfigHost { name: "nas".into(), hostname: "10.0.0.5".into(), user: "me".into(), port: "2222".into() });
        assert_eq!(hosts[3].hostname, "lab4.local");
        assert_eq!(hosts[0].hostname, "work.example");
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[cfg(test)]
mod real_ssh_config {
    /// This machine's SSH config hosts, read-only (not run by default).
    #[test]
    #[ignore]
    fn list() {
        for h in super::ssh_config_hosts() {
            println!("host {} (hostname set: {}, user set: {}, port set: {})", h.name, !h.hostname.is_empty(), !h.user.is_empty(), !h.port.is_empty());
        }
    }
}
