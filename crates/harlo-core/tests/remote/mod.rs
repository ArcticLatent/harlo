//! A private SSH server for tests, standing in for another computer: a
//! loopback port, scratch keys and configs, and sessions with a scratch
//! `HOME` and state folder, no desktop bus and a scratch `bin` first on
//! `PATH`. The client's config never reads the user's `~/.ssh` or agent.

// Rust guideline compliant 2026-10-09

#![allow(dead_code, reason = "each test file uses a different part")]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The system's SSH server.
const SSHD: &str = "/usr/bin/sshd";

/// A private SSH server, stopped when dropped.
pub struct Remote {
    /// The server.
    sshd: Child,
    /// What follows `ssh` to reach it.
    pub target: String,
    /// The scratch folder: keys, configs, and the remote side's folders.
    pub root: PathBuf,
}

impl Drop for Remote {
    fn drop(&mut self) {
        let _ = self.sshd.kill();
        let _ = self.sshd.wait();
    }
}

/// A free port on the loopback address.
pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("a port")
}

/// Makes an ed25519 key at `path`.
fn key(path: &Path) {
    let made = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", "harlo-test", "-f"])
        .arg(path)
        .status()
        .expect("ssh-keygen runs");
    assert!(made.success());
}

impl Remote {
    /// Starts the server in `root`; `None` when this system has no `sshd`.
    pub fn start(root: &Path) -> Option<Self> {
        if !Path::new(SSHD).exists() {
            eprintln!("no {SSHD}: skipped");
            return None;
        }
        for sub in ["sshd", "client", "home", "state", "bin"] {
            std::fs::create_dir_all(root.join(sub)).expect("a folder");
        }
        key(&root.join("sshd/host_key"));
        key(&root.join("client/id"));
        std::fs::copy(
            root.join("client/id.pub"),
            root.join("sshd/authorized_keys"),
        )
        .expect("authorized keys");
        let port = free_port();
        let path = |p: &str| root.join(p).display().to_string();
        let server = format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\n\
             AuthorizedKeysFile {}\nUsePAM no\nStrictModes no\nPasswordAuthentication no\n\
             KbdInteractiveAuthentication no\nPubkeyAuthentication yes\nAllowTcpForwarding yes\n\
             LogLevel ERROR\nSetEnv HOME={} XDG_STATE_HOME={} \
             DBUS_SESSION_BUS_ADDRESS=unix:path={} PATH={}:/usr/bin:/bin\n",
            path("sshd/host_key"),
            path("sshd/pid"),
            path("sshd/authorized_keys"),
            path("home"),
            path("state"),
            path("no-bus"),
            path("bin"),
        );
        std::fs::write(root.join("sshd/config"), server).expect("sshd config");
        let user = std::env::var("USER").expect("USER");
        let client = format!(
            "Host harlo-remote\n  HostName 127.0.0.1\n  Port {port}\n  User {user}\n  \
             IdentityFile {}\n  IdentitiesOnly yes\n  IdentityAgent none\n  \
             UserKnownHostsFile {}\n  GlobalKnownHostsFile /dev/null\n  \
             StrictHostKeyChecking accept-new\n  ControlMaster no\n  ControlPath none\n",
            path("client/id"),
            path("client/known_hosts"),
        );
        std::fs::write(root.join("client/config"), client).expect("client config");
        let sshd = Command::new(SSHD)
            .args(["-D", "-e", "-f"])
            .arg(root.join("sshd/config"))
            .stdin(Stdio::null())
            .stderr(std::fs::File::create(root.join("sshd/log")).expect("the sshd log"))
            .spawn()
            .expect("sshd starts");
        let started = Instant::now();
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "sshd never listened"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        let target = format!(
            "-F {} harlo-remote",
            shell_words::quote(&path("client/config"))
        );
        Some(Self {
            sshd,
            target,
            root: root.to_owned(),
        })
    }

    /// Registers the server at `url` as the remote computer's shared
    /// service.
    pub fn register(&self, url: &str, password: &str) {
        std::fs::create_dir_all(self.root.join("state/opencode")).expect("a folder");
        std::fs::write(
            self.root.join("state/opencode/service.json"),
            registration(url, password),
        )
        .expect("service.json");
    }
}

/// A registration for the server at `url`, as OpenCode writes it.
pub fn registration(url: &str, password: &str) -> String {
    serde_json::json!({
        "url": url,
        "password": password,
        "version": harlo_core::OPENCODE_VERSION,
    })
    .to_string()
}
