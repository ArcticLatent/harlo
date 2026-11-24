//! Starting Harlo when the user logs in, through an XDG Autostart entry: a
//! desktop entry in `$XDG_CONFIG_HOME/autostart` that the desktop runs at
//! login.

// Rust guideline compliant 2026-10-09

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The entry's file name in the autostart folder.
const FILE: &str = "harlo.desktop";

/// Where a program and its arguments must be quoted in an `Exec` line, per
/// the Desktop Entry specification's reserved characters.
const RESERVED: &[char] = &[
    ' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(', ')',
    '`',
];

/// Why the autostart entry couldn't be read or changed.
#[derive(Debug)]
pub struct Error {
    /// The entry's file.
    path: PathBuf,
    /// What went wrong.
    source: io::Error,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.source)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// The autostart folder on this computer, such as `~/.config/autostart`.
///
/// Returns `None` when neither the config folder nor the home folder can be
/// found.
#[must_use]
pub fn folder() -> Option<PathBuf> {
    let config = dirs::config_dir().or_else(|| Some(dirs::home_dir()?.join(".config")))?;
    Some(config.join("autostart"))
}

/// Whether Harlo starts at login: its entry is in `folder`.
#[must_use]
pub fn enabled(folder: &Path) -> bool {
    folder.join(FILE).is_file()
}

/// Makes Harlo start at login by running `program`, or stops it.
///
/// # Errors
///
/// Fails when the entry can't be written or removed.
pub fn set(folder: &Path, program: &Path, on: bool) -> Result<(), Error> {
    let path = folder.join(FILE);
    let failed = |source| Error {
        path: path.clone(),
        source,
    };
    if !on {
        return match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(failed(e)),
            _ => Ok(()),
        };
    }
    fs::create_dir_all(folder).map_err(failed)?;
    fs::write(&path, entry(program)).map_err(failed)
}

/// Sets it as [`set`] does, on a blocking thread.
///
/// # Errors
///
/// Returns the errors of [`set`], or an error when the thread is cancelled.
pub async fn set_later(folder: PathBuf, program: PathBuf, on: bool) -> Result<(), Error> {
    let path = folder.join(FILE);
    tokio::task::spawn_blocking(move || set(&folder, &program, on))
        .await
        .map_err(|join| Error {
            path,
            source: io::Error::other(join.to_string()),
        })?
}

/// The desktop entry that runs `program`.
#[must_use]
pub fn entry(program: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Harlo\n\
         Comment=Coding agents on your computer\n\
         Exec={}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        exec_argument(&program.to_string_lossy())
    )
}

/// `argument` as one argument of an `Exec` line: quoted when it holds a
/// reserved character, then escaped as a desktop entry string.
fn exec_argument(argument: &str) -> String {
    let quoted = if argument.contains(RESERVED) {
        let mut quoted = String::from('"');
        for c in argument.chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                quoted.push('\\');
            }
            quoted.push(c);
        }
        quoted.push('"');
        quoted
    } else {
        argument.to_owned()
    };
    // Field codes start with `%`, and a string value escapes `\`.
    quoted.replace('%', "%%").replace('\\', "\\\\")
}
