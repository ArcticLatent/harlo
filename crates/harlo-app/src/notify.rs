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

pub fn send(tag: &str, title: &str, body: &str, sound: bool, on_open: impl FnOnce() + Send + 'static) {
    let (tag, title, body) = (tag.to_owned(), title.to_owned(), body.to_owned());
    std::thread::spawn(move || {
        let mut cmd = Command::new("notify-send");
        cmd.args(["--app-name=harlo", "--print-id", "--action=default=Open"]).arg(format!("--icon={}", icon()));
        if let Some(id) = ids().lock().unwrap().get(&tag) {
            cmd.arg(format!("--replace-id={id}"));
        }
        if sound {
            cmd.arg("--hint=string:sound-name:message-new-instant");
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
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let line = line.trim();
                if let Ok(id) = line.parse::<u32>() {
                    ids().lock().unwrap().insert(tag.clone(), id);
                } else if line == "default"
                    && let Some(f) = on_open.take()
                {
                    f();
                }
            }
        }
        let _ = child.wait();
    });
}
