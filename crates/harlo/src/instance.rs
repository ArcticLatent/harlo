//! One Harlo per user and state folder: the first one listens on a local
//! socket, and a later launch hands its command to it and exits.
//!
//! The socket's name comes from Harlo's state folder, so runs with their own
//! folders, such as tests and sandboxes, never reach the user's Harlo.

// Rust guideline compliant 2026-10-09

use std::fmt;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a launch waits for the running Harlo to answer.
const ANSWER: Duration = Duration::from_millis(500);

/// FNV-1a's offset basis and prime, for a name that stays the same across
/// builds (the standard library's hasher may change between releases).
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Only the user may connect.
const PRIVATE: u32 = 0o600;

/// What a later launch asks of the running Harlo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Show and focus the window.
    Show,
    /// Show the window with the setup guide open.
    Setup,
}

impl Command {
    /// The command as it goes over the socket.
    fn word(self) -> &'static str {
        match self {
            Self::Show => "show",
            Self::Setup => "setup",
        }
    }

    /// The command `word` names, if any.
    fn parse(word: &str) -> Option<Self> {
        match word.trim() {
            "show" => Some(Self::Show),
            "setup" => Some(Self::Setup),
            _ => None,
        }
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// The socket for Harlo's state folder on this computer, in the user's
/// runtime folder, or `None` when there's no state folder.
#[must_use]
pub fn socket_path() -> Option<PathBuf> {
    let state = dirs::state_dir().or_else(dirs::data_local_dir)?;
    let folder = dirs::runtime_dir().unwrap_or_else(std::env::temp_dir);
    Some(socket_in(&folder, &state))
}

/// The socket in `folder` for Harlo's state folder `state`.
#[must_use]
pub fn socket_in(folder: &Path, state: &Path) -> PathBuf {
    let hash = state.to_string_lossy().bytes().fold(FNV_OFFSET, |hash, b| {
        (hash ^ u64::from(b)).wrapping_mul(FNV_PRIME)
    });
    folder.join(format!("harlo-{:08x}.sock", hash & u64::from(u32::MAX)))
}

/// Hands `command` to the Harlo listening at `path`.
///
/// Returns the running Harlo's process ID when it took the command, or
/// `None` when no Harlo is listening.
///
/// # Errors
///
/// Fails when a Harlo is listening but the command couldn't be sent.
pub fn forward(path: &Path, command: Command) -> io::Result<Option<u32>> {
    let mut stream = match UnixStream::connect(path) {
        Ok(stream) => stream,
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            return Ok(None);
        }
        Err(e) => return Err(e),
    };
    writeln!(stream, "{command}")?;
    stream.set_read_timeout(Some(ANSWER))?;
    let mut reply = String::new();
    BufReader::new(&stream).read_line(&mut reply)?;
    Ok(reply.trim().parse().ok())
}

/// The socket a running Harlo listens on; dropping it removes the socket.
#[derive(Debug)]
pub struct Listener {
    /// The socket's path.
    path: PathBuf,
}

impl Drop for Listener {
    fn drop(&mut self) {
        // Already gone is fine: there's nothing left to clean up.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Listens at `path` for later launches, calling `on` with each command on
/// a thread of its own. A socket left behind by a Harlo that ended is
/// replaced.
///
/// # Errors
///
/// Fails with [`io::ErrorKind::AddrInUse`] when another Harlo listens there,
/// or when the socket can't be made.
pub fn listen(path: &Path, on: impl Fn(Command) + Send + 'static) -> io::Result<Listener> {
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            return Err(io::Error::from(io::ErrorKind::AddrInUse));
        }
        std::fs::remove_file(path)?;
    }
    let socket = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(PRIVATE))?;
    std::thread::Builder::new()
        .name("harlo-instance".to_owned())
        .spawn(move || {
            for stream in socket.incoming().flatten() {
                if let Some(command) = answer(&stream) {
                    on(command);
                }
            }
        })?;
    Ok(Listener {
        path: path.to_owned(),
    })
}

/// Reads one command from `stream` and answers with this process's ID.
fn answer(mut stream: &UnixStream) -> Option<Command> {
    stream.set_read_timeout(Some(ANSWER)).ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    let command = Command::parse(&line)?;
    // The launch may have given up waiting; the command still counts.
    let _ = writeln!(stream, "{}", std::process::id());
    Some(command)
}
