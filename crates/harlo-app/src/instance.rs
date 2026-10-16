//! One harlo per user: the first instance listens on a local socket; a later
//! `harlo` (or `harlo --toggle-dropdown`, for a desktop shortcut) hands its
//! command to it and exits.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

type Handler = Box<dyn Fn(String) + Send + Sync>;

fn handler() -> &'static Mutex<Option<Handler>> {
    static H: OnceLock<Mutex<Option<Handler>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(None))
}

/// Commands that arrive before the app is ready wait here.
fn early() -> &'static Mutex<Vec<String>> {
    static E: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    E.get_or_init(|| Mutex::new(Vec::new()))
}

fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    // The data folder is part of the name, so test runs with their own
    // XDG_DATA_HOME don't talk to the running app.
    let data = harlo_core::db::default_path();
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in data.to_string_lossy().bytes() {
        hash = (hash ^ b as u64).wrapping_mul(0x100000001b3);
    }
    dir.join(format!("harlo-{:08x}.sock", hash as u32))
}

/// The command this launch carries: `toggle-dropdown`, `toggle-mini-chat`,
/// `setup` (the setup guide), or `show`.
fn command() -> &'static str {
    if std::env::args().any(|a| a == "--toggle-dropdown") {
        "toggle-dropdown"
    } else if std::env::args().any(|a| a == "--mini-chat") {
        "toggle-mini-chat"
    } else if std::env::args().any(|a| a == "--setup") {
        "setup"
    } else {
        "show"
    }
}

/// Self-tests stay out of it unless they test it (`--ipc-test`).
fn skip() -> bool {
    let args: Vec<String> = std::env::args().collect();
    !args.iter().any(|a| a == "--ipc-test") && args.iter().any(|a| a.starts_with("--selftest") || a == "--new-instance")
}

/// Hands this launch's command to a running harlo. Returns true when one
/// took it (this process should exit).
pub fn forward_to_running() -> bool {
    if skip() {
        return false;
    }
    let Ok(mut stream) = UnixStream::connect(socket_path()) else { return false };
    if stream.write_all(format!("{}\n", command()).as_bytes()).is_err() {
        return false;
    }
    // The running instance answers with its pid: say where the launch went,
    // and warn when that is an older build (after a rebuild the new binary
    // would otherwise just raise the old window).
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));
    let mut reply = String::new();
    let _ = BufReader::new(&stream).read_line(&mut reply);
    match reply.trim().parse::<u32>() {
        Ok(pid) => {
            let theirs = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
            let ours = std::env::current_exe().ok();
            let stale = theirs.as_ref().is_some_and(|t| t.to_string_lossy().ends_with(" (deleted)") || Some(t) != ours.as_ref());
            if stale {
                eprintln!("harlo: an older build is already running (pid {pid}); showed it instead of starting this one. Quit it (tray icon → Quit harlo, or `kill {pid}`) to run this build.");
            } else {
                eprintln!("harlo: already running (pid {pid}); showed its window.");
            }
        }
        Err(_) => eprintln!("harlo: already running; showed its window."),
    }
    true
}

/// Listens for later launches. Call once, before the UI exists.
pub fn listen() {
    if skip() {
        return;
    }
    let path = socket_path();
    // A socket left by a crashed instance: nobody answered, so replace it.
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("single-instance socket {}: {e}", path.display());
            return;
        }
    };
    // This launch's own command counts too (`harlo --toggle-dropdown` as the first instance).
    if command() != "show" {
        early().lock().unwrap().push(command().to_owned());
    }
    std::thread::Builder::new()
        .name("harlo-instance".into())
        .spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_ok() {
                    let _ = stream.write_all(format!("{}\n", std::process::id()).as_bytes());
                    dispatch(line.trim().to_owned());
                }
            }
        })
        .ok();
}

fn dispatch(cmd: String) {
    if cmd.is_empty() {
        return;
    }
    match handler().lock().unwrap().as_ref() {
        Some(h) => h(cmd),
        None => early().lock().unwrap().push(cmd),
    }
}

/// Receives commands from later launches (on the listener thread).
/// Commands that arrived earlier are delivered now.
pub fn set_handler(h: impl Fn(String) + Send + Sync + 'static) {
    let pending: Vec<String> = std::mem::take(&mut *early().lock().unwrap());
    for cmd in pending {
        h(cmd);
    }
    *handler().lock().unwrap() = Some(Box::new(h));
}

/// Removes the socket when the app quits.
pub fn cleanup() {
    let _ = std::fs::remove_file(socket_path());
}
