//! Start at login: an XDG autostart entry (`~/.config/autostart/harlo.desktop`)
//! that launches harlo in the background (into the tray).

use std::path::{Path, PathBuf};

/// The autostart entry's path (`$XDG_CONFIG_HOME/autostart/harlo.desktop`).
pub fn entry_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("autostart").join("harlo.desktop"))
}

/// A desktop-entry Exec argument, quoted as the spec asks.
fn quote(arg: &str) -> String {
    let mut out = String::from("\"");
    for c in arg.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// The entry for `exe`.
pub fn entry(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=harlo\nComment=Coding agents (pi, omp), started in the background\nExec={} --background\nIcon=harlo\nTerminal=false\nX-GNOME-Autostart-enabled=true\nX-KDE-autostart-after=panel\n",
        quote(&exe.to_string_lossy())
    )
}

pub fn is_enabled() -> bool {
    entry_path().is_some_and(|p| p.exists())
}

/// Writes (`on`) or removes the entry; `exe` is the program to start.
pub fn set(on: bool, exe: &Path) -> std::io::Result<()> {
    let Some(path) = entry_path() else { return Err(std::io::Error::other("no home folder")) };
    if on {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, entry(exe))
    } else {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_quotes_the_program() {
        let e = entry(Path::new("/home/me/my apps/pi$qt"));
        assert!(e.contains("Exec=\"/home/me/my apps/pi\\$qt\" --background\n"), "{e}");
        assert!(e.starts_with("[Desktop Entry]\nType=Application\n"));
        assert!(e.contains("Icon=harlo\n"));
    }
}
