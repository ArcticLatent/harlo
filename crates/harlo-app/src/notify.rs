//! Desktop notifications through `notify-send` (libnotify). Clicking
//! "Open" runs `on_open`. A notification with the same `tag` as an earlier
//! one replaces it instead of stacking.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

/// harlo's icon as a file (notification servers take a path).
fn icon() -> String {
    static ICON: OnceLock<String> = OnceLock::new();
    ICON.get_or_init(|| {
        let dir = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .unwrap_or_else(std::env::temp_dir)
            .join("harlo");
        let path = dir.join("harlo.svg");
        let svg: &[u8] = include_bytes!("../icons/app/harlo.svg");
        let ok = std::fs::read(&path).is_ok_and(|b| b == svg) || (std::fs::create_dir_all(&dir).is_ok() && std::fs::write(&path, svg).is_ok());
        if ok { path.to_string_lossy().into_owned() } else { "utilities-terminal".to_owned() }
    })
    .clone()
}

/// The notification shown last for each tag (its server id).
fn ids() -> &'static Mutex<HashMap<String, u32>> {
    static IDS: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    IDS.get_or_init(Default::default)
}

/// `sound`: one of harlo's sounds to play with it ([`SOUNDS`]), or none.
/// The desktop's own sound stays off either way, so it sounds the same
/// everywhere.
pub fn send(tag: &str, title: &str, body: &str, sound: Option<&str>, on_open: impl FnOnce() + Send + 'static) {
    let (tag, title, body, sound) = (tag.to_owned(), title.to_owned(), body.to_owned(), sound.map(str::to_owned));
    std::thread::spawn(move || {
        let mut cmd = Command::new("notify-send");
        cmd.args(["--app-name=harlo", "--print-id", "--action=default=Open"]).arg(format!("--icon={}", icon()));
        if let Some(id) = ids().lock().unwrap().get(&tag) {
            cmd.arg(format!("--replace-id={id}"));
        }
        cmd.arg("--hint=boolean:suppress-sound:true");
        if let Some(sound) = sound.as_deref().filter(|s| !s.is_empty()) {
            play_sound(sound);
        }
        cmd.arg(&title).arg(&body).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!("notify-send unavailable: {e}");
                return;
            }
        };
        // The id comes first; the chosen action when one is clicked.
        let mut on_open = Some(on_open);
        let mut shown = None;
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let line = line.trim();
                if let Ok(id) = line.parse::<u32>() {
                    shown = Some(id);
                    ids().lock().unwrap().insert(tag.clone(), id);
                } else if line == "default"
                    && let Some(f) = on_open.take()
                {
                    f();
                }
            }
        }
        let _ = child.wait();
        // Gone from the screen: the next one with its tag is a new
        // notification (one that replaced it would only change it in the
        // history, unseen).
        let mut ids = ids().lock().unwrap();
        if ids.get(&tag).is_some_and(|id| shown.is_some_and(|s| s == *id)) {
            ids.remove(&tag);
        }
    });
}

/// harlo's sounds, the same on every desktop: KDE's Ocean theme's
/// (CC-BY-SA-4.0, see `sounds/NOTICE`), by their names there.
const SOUNDS: [(&str, &[u8]); 8] = [
    ("message-new-instant", include_bytes!("../sounds/message-new-instant.oga")),
    ("dialog-warning", include_bytes!("../sounds/dialog-warning.oga")),
    ("desktop-login", include_bytes!("../sounds/desktop-login.oga")),
    ("service-login", include_bytes!("../sounds/service-login.oga")),
    ("completion-success", include_bytes!("../sounds/completion-success.oga")),
    ("outcome-success", include_bytes!("../sounds/outcome-success.oga")),
    ("bell-window-system", include_bytes!("../sounds/bell-window-system.oga")),
    ("alarm-clock-elapsed", include_bytes!("../sounds/alarm-clock-elapsed.oga")),
];

/// Plays one of harlo's sounds (see [`SOUNDS`]) through PipeWire or
/// PulseAudio, whichever is there.
pub fn play_sound(name: &str) {
    let Some(file) = sound_file(name) else {
        tracing::debug!("no sound {name}");
        return;
    };
    std::thread::spawn(move || {
        let quiet = |c: &mut Command| c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
        for player in ["pw-play", "paplay"] {
            if quiet(Command::new(player).arg(&file)) {
                return;
            }
        }
        if !quiet(Command::new("canberra-gtk-play").arg("-f").arg(&file)) {
            tracing::debug!("no player for {}", file.display());
        }
    });
}

/// The sound's file in harlo's cache (written from the one built in, so a
/// player can open it).
fn sound_file(name: &str) -> Option<PathBuf> {
    let (_, bytes) = SOUNDS.iter().find(|(n, _)| *n == name)?;
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("harlo/sounds");
    let path = dir.join(format!("{name}.oga"));
    let ok = std::fs::read(&path).is_ok_and(|b| b == *bytes) || (std::fs::create_dir_all(&dir).is_ok() && std::fs::write(&path, bytes).is_ok());
    ok.then_some(path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_sound_offered_is_built_in() {
        for (id, _) in harlo_core::notifications::RESET_SOUNDS.iter().filter(|(id, _)| !id.is_empty()) {
            assert!(super::SOUNDS.iter().any(|(n, _)| n == id), "{id}");
        }
        assert!(super::SOUNDS.iter().all(|(_, b)| b.starts_with(b"OggS")));
    }
}
