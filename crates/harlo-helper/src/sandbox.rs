//! The sandbox Harlo's commands run in: bubblewrap (`bwrap`), with the
//! system read-only, the project, /tmp and tool caches writable, and the
//! places that hold secrets (keys, sign-ins, browser profiles, the agents'
//! own sessions) hidden. The file tools keep to the same view.
//!
//! On by default (`HARLO_SANDBOX`: `on|off|no-network`); `sandbox.json` in
//! harlo's settings folder (`~/.config/harlo`) adds folders and
//! variables. A project can't loosen it. A machine that can't sandbox
//! refuses commands and says how to fix that.

// Rust guideline compliant 2026-10-09

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde_json::Value;

/// How commands run, for this process.
#[derive(Clone, Debug, PartialEq)]
pub struct Policy {
    pub on: bool,
    pub network: bool,
    /// From sandbox.json: more folders commands may change, more to hide,
    /// and variables passed through though their names look secret.
    pub writable: Vec<PathBuf>,
    pub hidden: Vec<PathBuf>,
    pub env: Vec<String>,
}

impl Policy {
    pub fn off() -> Policy {
        Policy { on: false, network: true, writable: Vec::new(), hidden: Vec::new(), env: Vec::new() }
    }
}

static POLICY: OnceLock<Policy> = OnceLock::new();

/// Sets this process's policy: `flag` (`on`, `off`, `no-network`), else
/// `HARLO_SANDBOX`, else on; sandbox.json's extras with it.
pub fn configure(flag: Option<&str>) -> anyhow::Result<()> {
    let env = std::env::var("HARLO_SANDBOX").or_else(|_| std::env::var("HARLO_AGENT_SANDBOX")).ok().filter(|v| !v.trim().is_empty());
    let mode = flag.map(str::to_owned).or(env).unwrap_or_else(|| "on".into());
    let (on, network) = match mode.trim() {
        "on" | "1" | "true" => (true, true),
        "no-network" | "offline" => (true, false),
        "off" | "0" | "false" => (false, true),
        other => anyhow::bail!("--sandbox takes on, off or no-network, not {other:?}"),
    };
    let file = std::fs::read_to_string(crate::paths::sandbox_file()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or(Value::Null);
    let home = home();
    let paths = |k: &str| -> Vec<PathBuf> { file[k].as_array().into_iter().flatten().filter_map(Value::as_str).map(|p| expand(p, &home)).collect() };
    let names = file["env"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
    let _ = POLICY.set(Policy { on, network, writable: paths("writable"), hidden: paths("hidden"), env: names });
    Ok(())
}

/// This process's policy (off until configured: tests, and tools made
/// outside a session).
pub fn policy() -> Policy {
    POLICY.get().cloned().unwrap_or_else(Policy::off)
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

fn expand(p: &str, home: &Path) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(p),
    }
}

/// Whether bubblewrap can make a sandbox here (asked once): None when it
/// can, else why not.
pub fn unavailable() -> Option<String> {
    static WHY: OnceLock<Option<String>> = OnceLock::new();
    WHY.get_or_init(|| {
        let out = std::process::Command::new("bwrap")
            .args(["--unshare-all", "--die-with-parent", "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "true"])
            .stdin(std::process::Stdio::null())
            .output();
        match out {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some("bubblewrap (bwrap) isn't installed".into()),
            Err(e) => Some(format!("bwrap couldn't start: {e}")),
            Ok(o) if o.status.success() => None,
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr);
                let line = err.lines().next().unwrap_or("").trim();
                Some(format!("this system doesn't let bubblewrap make a sandbox for your user ({})", if line.is_empty() { "it failed" } else { line }))
            }
        }
    })
    .clone()
}

/// Under the home folder: hidden from commands and the file tools.
const HIDDEN_IN_HOME: &[&str] = &[
    ".ssh", ".gnupg", ".aws", ".azure", ".kube", ".docker", ".netrc", ".git-credentials", ".pki", ".password-store",
    ".Xauthority", ".npmrc", ".pypirc", ".cargo/credentials", ".cargo/credentials.toml",
    ".mozilla", ".thunderbird", ".config/google-chrome", ".config/chromium", ".config/BraveSoftware",
    ".config/gcloud", ".config/gh", ".config/hub", ".local/share/keyrings",
    // The agents' sign-ins and sessions.
    ".claude", ".claude.json", ".pi", ".omp", ".hermes", ".codex", ".config/opencode", ".local/share/opencode", ".config/harlo", ".local/share/harlo",
];
/// Under the home folder, inside a hidden place: shown read-only. harlo's
/// worktrees (a session reads its helpers' work there; its own is
/// writable), and skills (Claude Code's among them, its plugins' too).
const SHOWN_IN_HOME: &[&str] = &[".local/share/harlo/worktrees", ".claude/skills", ".agents/skills", ".claude/plugins/cache", ".claude/plugins/marketplaces"];
/// Elsewhere: sockets that would let a command act as you (the Docker
/// daemon's is as good as root).
const HIDDEN_ELSEWHERE: &[&str] = &["/var/run/docker.sock", "/run/docker.sock", "/tmp/.X11-unix"];
/// Under the home folder: tool caches commands may write.
const CACHES: &[&str] = &[".cache", ".cargo", ".rustup", ".npm", ".yarn", ".local/share/pnpm", ".local/share/uv", "go", ".m2", ".gradle", ".nuget", ".bun", ".deno"];
/// Caches made before the sandbox opens them, with the command that writes
/// each ("" any): only a folder that exists can be made writable, so a tool's
/// first use (`uvx` on a machine where uv never ran) failed with "Read-only
/// file system" and cost the agent a few turns working around it. (Not
/// `~/go`: a folder in plain sight, often moved with `GOPATH`.)
const MADE: &[(&str, &str)] = &[(".cache", ""), (".npm", "npm"), (".yarn", "yarn"), (".local/share/pnpm", "pnpm"), (".local/share/uv", "uv"), (".m2", "mvn"), (".gradle", "gradle"), (".nuget", "dotnet"), (".bun", "bun")];
/// Variables naming caches elsewhere, writable too.
const CACHE_VARS: &[&str] = &["XDG_CACHE_HOME", "CARGO_HOME", "RUSTUP_HOME", "GOPATH", "GOMODCACHE", "GOCACHE", "npm_config_cache", "PNPM_HOME", "YARN_CACHE_FOLDER", "UV_CACHE_DIR", "UV_TOOL_DIR", "UV_PYTHON_INSTALL_DIR", "PIP_CACHE_DIR", "GRADLE_USER_HOME", "BUN_INSTALL", "DENO_DIR"];
/// Variables a command doesn't get (with those whose names look secret).
const DROPPED_VARS: &[&str] = &["SSH_AUTH_SOCK", "SSH_AGENT_PID", "GPG_AGENT_INFO", "DBUS_SESSION_BUS_ADDRESS", "DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY"];

/// The sandbox for one working directory, its paths resolved.
#[derive(Debug, Clone)]
pub struct Sandbox {
    pub network: bool,
    /// The project: the repository's top folder, or the working directory
    /// (the working directory when the repository holds the home folder).
    pub project: PathBuf,
    /// Whether commands may change the project: not when it's the home
    /// folder or holds it (a session there, or at /, keeps home read-only).
    pub project_writable: bool,
    /// Where commands may write (existing, resolved), the project first.
    pub writable: Vec<PathBuf>,
    /// What's hidden (existing, resolved). The deepest of these and
    /// `writable` wins: a project inside a hidden folder (a worktree in
    /// harlo's data) is shown and writable, the rest of that folder isn't.
    pub hidden: Vec<PathBuf>,
    /// Shown read-only inside a hidden place (existing, resolved): harlo's
    /// worktrees. The deepest place wins here too.
    pub shown: Vec<PathBuf>,
    env: Vec<String>,
    /// Why it can't start here (commands are refused).
    pub broken: Option<String>,
}

/// The sandbox for `cwd` under this process's policy (None: it's off).
pub fn for_dir(cwd: &Path) -> Option<Arc<Sandbox>> {
    let p = policy();
    if !p.on {
        return None;
    }
    let mut s = Sandbox::new(&p, cwd, &home(), &crate::paths::old_agent_config(), crate::paths::old_agent_sessions().as_deref());
    s.broken = unavailable();
    Some(Arc::new(s))
}

/// A path as it really is: resolved where it exists, its missing end kept.
pub fn resolve(p: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut at = p.to_path_buf();
    loop {
        if let Ok(real) = at.canonicalize() {
            return missing.iter().rev().fold(real, |acc: PathBuf, part: &OsString| acc.join(part));
        }
        match (at.file_name().map(std::ffi::OsStr::to_os_string), at.parent()) {
            (Some(name), Some(parent)) => {
                missing.push(name);
                at = parent.to_path_buf();
            }
            _ => return p.to_path_buf(),
        }
    }
}

/// Clean `.` and `..` out of an absolute path without touching the disk.
fn clean(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// The tool caches to open writable: [`CACHES`] under `home`, and those
/// under `.local/share` in `data` (`XDG_DATA_HOME`) too when it's elsewhere.
///
/// Any of [`MADE`] that isn't there yet is created first when its command is
/// on `PATH` (`on_path`), so the tool can write it on its first use.
fn caches(home: &Path, data: Option<&Path>, on_path: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for c in CACHES {
        let mut places = vec![home.join(c)];
        if let (Some(rest), Some(d)) = (c.strip_prefix(".local/share/"), data) {
            places.push(d.join(rest));
        }
        let tool = MADE.iter().find(|(f, _)| f == c).map(|(_, t)| *t);
        for p in places {
            if let Some(t) = tool
                && !p.exists()
                && (t.is_empty() || on_path(t))
            {
                let _ = std::fs::create_dir_all(&p);
            }
            out.push(p);
        }
    }
    out
}

/// Whether `tool` is a file in one of `PATH`'s folders.
fn on_path(tool: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(tool).is_file()))
}

impl Sandbox {
    /// `config`: harlo-agent's settings folder; `sessions`: its sessions
    /// (both hidden, wherever they are).
    pub fn new(p: &Policy, cwd: &Path, home: &Path, config: &Path, sessions: Option<&Path>) -> Sandbox {
        let cwd = resolve(cwd);
        let holds_home = |p: &Path| resolve(home).starts_with(p);
        // (A repository of the home folder, say dotfiles, isn't the project.)
        let (project, common) = match git_folders(&cwd) {
            (Some(top), common) if !holds_home(&top) => (top, common),
            _ => (cwd.clone(), None),
        };
        let project_writable = !holds_home(&project);
        let existing = |paths: Vec<PathBuf>| -> Vec<PathBuf> {
            let mut out: Vec<PathBuf> = Vec::new();
            for p in paths {
                if let Ok(real) = p.canonicalize()
                    && !out.contains(&real)
                {
                    out.push(real);
                }
            }
            out
        };
        let mut writable = Vec::new();
        if project_writable {
            writable.push(project.clone());
            writable.extend(common.filter(|c| !c.starts_with(&project)));
        }
        writable.push(PathBuf::from("/tmp"));
        let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|d| d.is_absolute() && !home.starts_with(d));
        writable.extend(caches(home, data.as_deref(), on_path));
        // (Caches moved elsewhere; none that holds home. The cache folder
        // made if it isn't there, as ~/.cache is.)
        let moved: Vec<PathBuf> = CACHE_VARS.iter().filter_map(std::env::var_os).map(PathBuf::from).filter(|c| c.is_absolute() && !home.starts_with(c)).collect();
        if let Some(c) = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).filter(|c| moved.contains(c) && !c.exists()) {
            let _ = std::fs::create_dir_all(c);
        }
        writable.extend(moved);
        writable.extend(p.writable.iter().cloned());
        let writable = existing(writable);
        let mut hidden: Vec<PathBuf> = HIDDEN_IN_HOME.iter().map(|h| home.join(h)).collect();
        hidden.extend(HIDDEN_ELSEWHERE.iter().map(PathBuf::from));
        // (Its sockets: keyring, agents, the desktop's.)
        hidden.push(PathBuf::from(format!("/run/user/{}", crate::paths::uid())));
        if let Some(cargo) = std::env::var_os("CARGO_HOME").map(PathBuf::from) {
            hidden.extend(["credentials", "credentials.toml"].map(|c| cargo.join(c)));
        }
        hidden.push(config.to_path_buf());
        if let Some(s) = sessions {
            hidden.push(s.parent().unwrap_or(s).to_path_buf());
        }
        hidden.extend(p.hidden.iter().cloned());
        let hidden = existing(hidden);
        // (Skills, read-only: harlo-agent's own too.)
        let mut shown: Vec<PathBuf> = SHOWN_IN_HOME.iter().map(|s| home.join(s)).collect();
        shown.push(config.join("skills"));
        let shown = existing(shown);
        Sandbox { network: p.network, project, project_writable, writable, hidden, shown, env: p.env.clone(), broken: None }
    }

    /// bwrap's arguments to run `bash -c command` in `cwd`.
    pub fn args(&self, command: &str, cwd: &Path) -> Vec<OsString> {
        let mut a: Vec<OsString> = vec!["--unshare-all".into()];
        if self.network {
            a.push("--share-net".into());
        }
        a.extend(["--die-with-parent", "--new-session", "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc"].map(OsString::from));
        // Shallow to deep, so the deeper one wins: a cache's credentials
        // hidden inside it, harlo's worktrees shown inside its hidden data
        // folder, a project (one of them) writable there.
        enum Mount {
            Writable,
            Shown,
            Hidden,
        }
        let mut mounts: Vec<(&PathBuf, Mount)> = self
            .writable
            .iter()
            .map(|w| (w, Mount::Writable))
            .chain(self.shown.iter().map(|s| (s, Mount::Shown)))
            .chain(self.hidden.iter().map(|h| (h, Mount::Hidden)))
            .collect();
        mounts.sort_by_key(|(p, _)| p.components().count());
        for (p, kind) in mounts {
            if matches!(kind, Mount::Writable) {
                a.extend(["--bind".into(), p.clone().into_os_string(), p.clone().into_os_string()]);
            } else if matches!(kind, Mount::Shown) {
                a.extend(["--ro-bind".into(), p.clone().into_os_string(), p.clone().into_os_string()]);
            } else if p.is_dir() {
                a.extend(["--tmpfs".into(), p.clone().into_os_string()]);
            } else {
                a.extend(["--ro-bind".into(), "/dev/null".into(), p.clone().into_os_string()]);
            }
        }
        a.extend(["--chdir".into(), cwd.as_os_str().to_os_string()]);
        a.extend(["--", "bash", "-c", command].map(OsString::from));
        a
    }

    /// The variables a command gets: this process's, less those that
    /// would let it act as you.
    pub fn env(&self) -> Vec<(OsString, OsString)> {
        std::env::vars_os().filter(|(k, _)| k.to_str().is_some_and(|k| self.env.iter().any(|p| p == k) || !secret(k))).collect()
    }

    fn hidden_at(&self, real: &Path) -> Option<&Path> {
        if let Ok(rest) = real.strip_prefix("/proc")
            && rest.components().any(|c| matches!(c.as_os_str().to_str(), Some("environ" | "mem")))
        {
            // (Harlo's own variables, its keys among them.)
            return Some(Path::new("/proc/…/environ"));
        }
        // (The deepest of the hidden, shown and writable places that hold
        // it wins.)
        fn deepest<'a>(list: &'a [PathBuf], real: &Path) -> Option<&'a PathBuf> {
            list.iter().filter(|p| real.starts_with(p)).max_by_key(|p| p.components().count())
        }
        let open = [deepest(&self.writable, real), deepest(&self.shown, real)].into_iter().flatten().max_by_key(|p| p.components().count());
        match (deepest(&self.hidden, real), open) {
            (Some(h), Some(o)) if o.starts_with(h) && o != h => None,
            (h, _) => h.map(PathBuf::as_path),
        }
    }

    /// Hidden places inside `root` (searched by grep or find), as paths
    /// under `root` as given, to leave out of what they list. (A writable
    /// or shown place inside one of them is left out too: a search from
    /// above it doesn't go into the hidden folder at all.)
    pub fn hidden_under(&self, root: &Path) -> Vec<PathBuf> {
        let real = resolve(&clean(root));
        self.hidden.iter().filter_map(|h| h.strip_prefix(&real).ok()).filter(|rest| !rest.as_os_str().is_empty()).map(|rest| root.join(rest)).collect()
    }

    /// Whether the file tools may look at `path` (Err: why not).
    pub fn may_read(&self, path: &Path) -> anyhow::Result<()> {
        let real = resolve(&clean(path));
        match self.hidden_at(&real) {
            Some(h) => Err(anyhow::anyhow!("{} is hidden by Harlo's sandbox ({} holds secrets: keys, sign-ins or other sessions). If the task really needs it, ask the user.", path.display(), h.display())),
            None => Ok(()),
        }
    }

    /// Whether the file tools may change `path` (Err: why not).
    pub fn may_write(&self, path: &Path) -> anyhow::Result<()> {
        self.may_read(path)?;
        let real = resolve(&clean(path));
        if self.writable.iter().any(|w| real.starts_with(w)) {
            return Ok(());
        }
        if !self.project_writable {
            return Err(anyhow::anyhow!(
                "{} can't be changed: this session works in {}, the home folder or above it, which Harlo's sandbox keeps read-only (only /tmp and tool caches can be changed). If the task really needs this file, ask the user.",
                path.display(),
                self.project.display()
            ));
        }
        Err(anyhow::anyhow!("{} is outside the folders Harlo's sandbox lets you change ({}, /tmp and tool caches). Change files in the project; if the task really needs this file, ask the user.", path.display(), self.project.display()))
    }

    /// The same; `escape`: whether its bash tool has `outside_sandbox` (an
    /// agent without it is told to leave such a command to the user).
    pub fn describe_for(&self, escape: bool) -> String {
        if let Some(why) = &self.broken {
            return format!("Commands can't run on this machine: Harlo's sandbox can't start ({why}). Tell the user if you need to run one.");
        }
        let changeable = if self.project_writable {
            format!("{}, /tmp and tool caches", self.project.display())
        } else if escape {
            format!("/tmp and tool caches (this session works in {}, the home folder or above it, which stays read-only: to change a file there, use outside_sandbox for a command and say why)", self.project.display())
        } else {
            format!("/tmp and tool caches (this session works in {}, the home folder or above it, which stays read-only)", self.project.display())
        };
        let failed = if escape {
            "run it again with outside_sandbox: true and say why: the user is asked first"
        } else {
            "don't work around it: say which command the user should run themselves"
        };
        format!(
            "Commands run in a sandbox: they can change only {changeable} (the rest is read-only), and places that hold secrets (SSH keys, credentials, sign-ins, browser profiles) are hidden. The network is {}. If a command fails because of the sandbox (say, git push needs SSH keys), {failed}.",
            if self.network { "available" } else { "off" }
        )
    }
}

/// Whether a variable's name looks like it holds a secret (or is one of
/// those that let a command act as you).
fn secret(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    DROPPED_VARS.contains(&n.as_str())
        || n.ends_with("_KEY")
        || n.ends_with("_TOKEN")
        || n.ends_with("_SECRET")
        || n.contains("PASSWORD")
        || n.contains("PASSWD")
        || n.contains("CREDENTIAL")
        || n.starts_with("AWS_")
        || n == "TOKEN"
}

/// The project a command in `cwd` works in: the repository's top folder,
/// or `cwd` (when the repository holds the home folder too).
pub fn project_of(cwd: &Path, home: &Path) -> PathBuf {
    let cwd = resolve(cwd);
    match git_folders(&cwd).0 {
        Some(top) if !resolve(home).starts_with(&top) => top,
        _ => cwd,
    }
}

/// The repository's top folder and its shared git folder (a worktree's
/// is elsewhere), for a folder in one.
fn git_folders(cwd: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let out = std::process::Command::new("git").arg("-C").arg(cwd).args(["rev-parse", "--show-toplevel", "--git-common-dir"]).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output();
    let Some(o) = out.ok().filter(|o| o.status.success()) else { return (None, None) };
    let text = String::from_utf8_lossy(&o.stdout);
    let mut lines = text.lines();
    let top = lines.next().map(|l| resolve(Path::new(l.trim())));
    let common = lines.next().map(|l| {
        let p = Path::new(l.trim());
        resolve(&if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) })
    });
    (top, common)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub struct Scratch(pub PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Scratch {
        let d = std::env::temp_dir().join(format!("harlo-sandbox-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d.canonicalize().unwrap())
    }

    /// A scratch home outside /tmp (which the sandbox leaves writable),
    /// with `proj` a repository of its own.
    pub fn home_outside_tmp(name: &str) -> Scratch {
        let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/sandbox-tests").join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("home/proj")).unwrap();
        let d = d.canonicalize().unwrap();
        assert!(std::process::Command::new("git").arg("-C").arg(d.join("home/proj")).args(["init", "-q"]).status().unwrap().success());
        Scratch(d)
    }

    fn on() -> Policy {
        Policy { on: true, network: true, writable: Vec::new(), hidden: Vec::new(), env: Vec::new() }
    }

    #[test]
    fn its_view_of_the_folders() {
        let s = scratch("view");
        let home = s.0.join("home");
        for d in ["proj/src", ".ssh", ".cargo", ".config/harlo-agent", "data/harlo-agent/sessions", "other"] {
            std::fs::create_dir_all(home.join(d)).unwrap();
        }
        std::fs::write(home.join(".cargo/credentials.toml"), "token").unwrap();
        std::fs::write(home.join(".netrc"), "x").unwrap();
        let sb = Sandbox::new(&on(), &home.join("proj/src"), &home, &home.join(".config/harlo-agent"), Some(&home.join("data/harlo-agent/sessions")));
        assert_eq!(sb.project, home.join("proj/src"), "not a repository: the working directory");
        assert_eq!(sb.writable[0], home.join("proj/src"));
        assert!(sb.writable.contains(&home.join(".cargo")), "an existing cache");
        assert!(sb.writable.contains(&home.join(".cache")), "~/.cache, made");
        assert_eq!(sb.writable.iter().any(|w| w.ends_with(".npm")), on_path("npm"), "npm's cache only with npm");
        for h in [".ssh", ".netrc", ".cargo/credentials.toml", ".config/harlo-agent", "data/harlo-agent"] {
            assert!(sb.hidden.contains(&home.join(h)), "{h} hidden: {:?}", sb.hidden);
        }
        let args: Vec<String> = sb.args("echo hi", &home.join("proj/src")).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let at = |s: &str| args.iter().position(|a| a == s).unwrap();
        assert_eq!(&args[..2], ["--unshare-all", "--share-net"]);
        // The read-only root first, then shallow to deep (a cache's
        // credentials stay hidden inside it).
        assert!(at("--ro-bind") < at("--bind") && at("--ro-bind") < at("--tmpfs"));
        let creds = home.join(".cargo/credentials.toml").to_string_lossy().into_owned();
        let cargo = home.join(".cargo").to_string_lossy().into_owned();
        let creds_at = args.iter().position(|a| *a == creds).unwrap();
        assert_eq!(args[creds_at - 1], "/dev/null");
        assert!(at(&cargo) < creds_at, "the cache's bind before its credentials are hidden");
        assert_eq!(&args[args.len() - 3..], ["bash", "-c", "echo hi"]);
        let offline = Sandbox::new(&Policy { network: false, ..on() }, &home.join("proj"), &home, &home.join("none"), None);
        assert!(!offline.args("x", &home).iter().any(|a| a == "--share-net"));
    }

    /// A tool's cache that isn't there yet is made for a tool on `PATH`
    /// (uv's under `XDG_DATA_HOME` too, when that's elsewhere), not for one
    /// that isn't; and a command can write it.
    #[test]
    fn caches_a_tool_hasnt_made_yet_are_writable() {
        let s = home_outside_tmp("caches");
        let (home, data) = (s.0.join("home"), s.0.join("data"));
        std::fs::create_dir_all(home.join("proj")).unwrap();
        let listed = caches(&home, Some(&data), |t| t == "uv");
        for made in [".cache", ".local/share/uv"] {
            assert!(home.join(made).is_dir(), "{made} made");
        }
        assert!(data.join("uv").is_dir() && listed.contains(&data.join("uv")), "uv's folder under XDG_DATA_HOME");
        assert!(!home.join(".npm").exists() && !home.join("go").exists(), "nothing for tools that aren't there");
        // (Writable in the sandbox, as the cache uv makes in it on first use.)
        let sb = Sandbox::new(&on(), &home.join("proj"), &home, &home.join("none"), None);
        let cache = home.join(".cache/uv");
        let out = std::process::Command::new("bwrap").args(sb.args(&format!("mkdir -p {} && echo made", cache.display()), &home.join("proj"))).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "made", "{}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn a_project_inside_a_hidden_folder_is_shown_alone() {
        // A worktree in harlo's data: the worktree is there and writable,
        // the other worktrees (its helpers') readable, the rest (harlo's
        // database) hidden.
        let s = home_outside_tmp("inside");
        let home = s.0.join("home");
        let data = home.join(".local/share/harlo");
        for d in ["worktrees/p/mine", "worktrees/p/other"] {
            std::fs::create_dir_all(data.join(d)).unwrap();
        }
        std::fs::write(data.join("harlo.db"), "x").unwrap();
        let sb = Sandbox::new(&on(), &data.join("worktrees/p/mine"), &home, &home.join("none"), None);
        assert!(sb.hidden.contains(&data) && sb.project_writable, "{:?}", sb.hidden);
        assert!(sb.may_read(&data.join("worktrees/p/mine/a.rs")).is_ok() && sb.may_write(&data.join("worktrees/p/mine/a.rs")).is_ok());
        assert!(sb.may_read(&data.join("harlo.db")).unwrap_err().to_string().contains("hidden"));
        assert!(sb.may_read(&data.join("worktrees/p/other/x")).is_ok() && sb.may_write(&data.join("worktrees/p/other/x")).is_err());
        assert!(sb.may_write(&data.join("worktrees/p/mine/../other/x")).is_err(), "through ..");
        assert!(sb.may_read(&data.join("worktrees/../harlo.db")).is_err(), "through .. out of the worktrees");
        let args: Vec<String> = sb.args("x", &data.join("worktrees/p/mine")).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let tmpfs = args.iter().position(|a| *a == data.to_string_lossy()).unwrap();
        let shown = args.iter().position(|a| *a == data.join("worktrees").to_string_lossy()).unwrap();
        let bind = args.iter().rposition(|a| *a == data.join("worktrees/p/mine").to_string_lossy()).unwrap();
        assert!(args[tmpfs - 1] == "--tmpfs" && args[shown - 1] == "--ro-bind" && tmpfs < shown && shown < bind, "hidden, then the worktrees read-only, then the project writable: {args:?}");
        // A session elsewhere: the worktrees readable, not writable.
        let other = Sandbox::new(&on(), &home, &home, &home.join("none"), None);
        assert!(other.may_read(&data.join("worktrees/p/mine/a.rs")).is_ok() && other.may_write(&data.join("worktrees/p/mine/a.rs")).is_err());
        assert!(other.may_read(&data.join("harlo.db")).is_err());
    }

    #[test]
    fn a_worktrees_git_folder_is_writable() {
        let s = scratch("worktree");
        let main = s.0.join("main");
        std::fs::create_dir_all(&main).unwrap();
        let git = |dir: &Path, args: &[&str]| {
            let ok = std::process::Command::new("git").arg("-C").arg(dir).args(["-c", "user.email=t@t", "-c", "user.name=t"]).args(args).output().unwrap().status.success();
            assert!(ok, "git {args:?}");
        };
        git(&main, &["init", "-q"]);
        git(&main, &["commit", "-q", "--allow-empty", "-m", "x"]);
        git(&main, &["worktree", "add", "-q", s.0.join("wt").to_str().unwrap()]);
        std::fs::create_dir_all(s.0.join("wt/sub")).unwrap();
        let sb = Sandbox::new(&on(), &s.0.join("wt/sub"), &s.0.join("home"), &s.0.join("none"), None);
        assert_eq!(sb.project, s.0.join("wt"), "the repository's top folder");
        assert!(sb.writable.contains(&main.join(".git")), "{:?}", sb.writable);
    }

    #[test]
    fn the_file_tools_keep_to_it() {
        let s = home_outside_tmp("files");
        let home = s.0.join("home");
        for d in ["proj", ".ssh", "other"] {
            std::fs::create_dir_all(home.join(d)).unwrap();
        }
        std::fs::write(home.join(".ssh/id"), "k").unwrap();
        std::os::unix::fs::symlink(home.join(".ssh"), home.join("proj/keys")).unwrap();
        let sb = Sandbox::new(&on(), &home.join("proj"), &home, &home.join("none"), None);
        sb.may_read(&home.join("proj/a.rs")).unwrap();
        assert!(sb.may_read(&home.join("other/x")).is_ok(), "the rest of home is readable");
        assert!(sb.may_read(&home.join(".ssh/id")).unwrap_err().to_string().contains("hidden by Harlo's sandbox"));
        assert!(sb.may_read(&home.join("proj/keys/id")).is_err(), "through a link");
        assert!(sb.may_read(&home.join("proj/../.ssh/id")).is_err(), "through ..");
        assert!(sb.may_write(&home.join("proj/new/dir/f.rs")).is_ok(), "a new file in the project");
        sb.may_write(Path::new("/tmp/x")).unwrap();
        assert!(sb.may_write(&home.join("other/x")).unwrap_err().to_string().contains("outside the folders"));
        assert!(sb.may_write(&home.join("proj/../other/x")).is_err());
    }

    #[test]
    fn the_home_folder_is_never_the_writable_project() {
        let s = home_outside_tmp("home");
        let home = s.0.join("home");
        std::fs::create_dir_all(home.join("notes")).unwrap();
        // A session in the home folder, or above it: home stays read-only.
        for cwd in [home.clone(), s.0.clone(), PathBuf::from("/")] {
            let sb = Sandbox::new(&on(), &cwd, &home, &home.join("none"), None);
            assert!(!sb.project_writable && !sb.writable.iter().any(|w| home.starts_with(w)), "{cwd:?}: {:?}", sb.writable);
            assert!(sb.may_write(&home.join("x")).unwrap_err().to_string().contains("keeps read-only"), "{cwd:?}");
            sb.may_write(Path::new("/tmp/x")).unwrap();
            assert!(sb.describe_for(false).contains("stays read-only"));
        }
        // A repository of the whole home folder (dotfiles): a folder in it
        // is the project, not home.
        assert!(std::process::Command::new("git").arg("-C").arg(&home).args(["init", "-q"]).status().unwrap().success());
        let sb = Sandbox::new(&on(), &home.join("notes"), &home, &home.join("none"), None);
        assert_eq!((sb.project.as_path(), sb.project_writable), (home.join("notes").as_path(), true));
        assert!(sb.may_write(&home.join("notes/a.md")).is_ok() && sb.may_write(&home.join("x")).is_err());
        assert!(!sb.writable.contains(&home.join(".git")), "nor its git folder");
        // A project of its own inside home is as before.
        let sb = Sandbox::new(&on(), &home.join("proj"), &home, &home.join("none"), None);
        assert!(sb.project_writable && sb.project == home.join("proj"));
    }

    #[test]
    fn secret_variables_stay_out() {
        for n in ["OPENAI_API_KEY", "GITHUB_TOKEN", "AWS_PROFILE", "DB_PASSWORD", "SSH_AUTH_SOCK", "DISPLAY", "my_secret"] {
            assert!(secret(n), "{n}");
        }
        for n in ["PATH", "HOME", "CARGO_HOME", "LANG", "KEYBOARD_LAYOUT"] {
            assert!(!secret(n), "{n}");
        }
    }
}
