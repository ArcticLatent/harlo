//! What draws harlo's window (Settings → General → Graphics): Qt
//! Quick's OpenGL or Vulkan renderer, or its software one. Chosen before
//! the application starts, so a change takes effect on the next start.
//!
//! A choice other than Automatic is on trial until the window has drawn
//! once (`drawn`): if harlo didn't get that far (no working Vulkan, say),
//! the next start goes back to Automatic and says so. Only a start that
//! died before drawing counts: one whose window failed to load (a QML
//! error), one that quit normally, or one started hidden in the tray says
//! nothing about the graphics. `--graphics=<choice>`
//! picks one for a single run, and an explicit `QSG_RHI_BACKEND` /
//! `QT_QUICK_BACKEND` in the environment wins over both.

use harlo_core::db::Db;

const CHOICE: &str = "settings.graphics";
/// The choice being tried (cleared once the window has drawn).
const TRIAL: &str = "graphics.trial";
/// A choice that failed and was dropped, to tell the user.
const DROPPED: &str = "graphics.dropped";

/// The choice this run started with ("auto" when none; "" when the
/// environment decided).
static STARTED: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn started_with() -> String {
    STARTED.get().cloned().unwrap_or_else(|| "auto".into())
}

fn db() -> Option<Db> {
    Db::open(&harlo_core::db::default_path()).ok()
}

/// Sets up the chosen renderer (before the application is created).
pub fn apply() {
    if std::env::var_os("QSG_RHI_BACKEND").is_some() || std::env::var_os("QT_QUICK_BACKEND").is_some() {
        let _ = STARTED.set(String::new());
        return;
    }
    let once = std::env::args().find_map(|a| a.strip_prefix("--graphics=").map(str::to_owned));
    // Started hidden in the tray: nothing is drawn until it's opened, so
    // this start doesn't put the choice on trial.
    let hidden = std::env::args().any(|a| a == "--background");
    let db = db();
    let get = |key: &str| db.as_ref().and_then(|d| d.get(key).ok().flatten()).unwrap_or_default();
    let choice = match &once {
        Some(c) => c.clone(),
        None => {
            let d = decide(&get(CHOICE), &get(TRIAL));
            if let Some(db) = &db {
                if let Some(dropped) = &d.dropped {
                    tracing::warn!("graphics: {dropped} didn't start last time; back to automatic");
                    let _ = db.set(CHOICE, "auto");
                    let _ = db.set(DROPPED, dropped);
                }
                if !hidden || d.trial.is_empty() {
                    let _ = db.set(TRIAL, &d.trial);
                }
            }
            d.choice
        }
    };
    let _ = STARTED.set(if choice.is_empty() { "auto".into() } else { choice.clone() });
    // SAFETY: still single-threaded; nothing reads the environment yet.
    match choice.as_str() {
        "opengl" => unsafe { std::env::set_var("QSG_RHI_BACKEND", "opengl") },
        "vulkan" => unsafe { std::env::set_var("QSG_RHI_BACKEND", "vulkan") },
        "software" => unsafe { std::env::set_var("QT_QUICK_BACKEND", "software") },
        _ => {}
    }
}

/// What a start does with the saved choice and the one on trial.
#[derive(Debug, PartialEq, Eq)]
struct Decision {
    /// The renderer to use.
    choice: String,
    /// On trial from now ("" for none).
    trial: String,
    /// Dropped: it was on trial and the window never drew.
    dropped: Option<String>,
}

fn decide(saved: &str, trial: &str) -> Decision {
    let auto = |dropped| Decision { choice: "auto".into(), trial: String::new(), dropped };
    match saved {
        "" | "auto" => auto(None),
        c if c == trial => auto(Some(c.to_owned())),
        c => Decision { choice: c.to_owned(), trial: c.to_owned(), dropped: None },
    }
}

/// The window has drawn: the choice works.
pub fn drawn() {
    if let Some(db) = db() {
        let _ = db.set(TRIAL, "");
    }
}

/// This start ended or failed for some other reason than the graphics:
/// the trial is off (the next start tries the choice afresh).
pub fn inconclusive() {
    drawn();
}

/// A choice that failed to start and was dropped ("" if none); told once.
pub fn take_dropped() -> String {
    let Some(db) = db() else { return String::new() };
    let dropped = db.get(DROPPED).ok().flatten().unwrap_or_default();
    if !dropped.is_empty() {
        let _ = db.set(DROPPED, "");
    }
    dropped
}

/// Starts harlo again once this one has quit (the caller then quits).
pub fn restart() {
    let Ok(exe) = std::env::current_exe() else { return };
    let pid = std::process::id().to_string();
    // A waiter outside harlo: a new harlo started while this one runs
    // would only hand over to it.
    use std::os::unix::process::CommandExt;
    let _ = std::process::Command::new("sh")
        .process_group(0)
        .args(["-c", r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done; shift; exec "$@""#, "sh", &pid])
        .arg(exe)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_choice_is_tried_then_dropped_if_it_never_drew() {
        assert_eq!(decide("", ""), Decision { choice: "auto".into(), trial: "".into(), dropped: None });
        // First start with Vulkan: on trial.
        assert_eq!(decide("vulkan", ""), Decision { choice: "vulkan".into(), trial: "vulkan".into(), dropped: None });
        // It drew (the trial was cleared): still Vulkan, on trial again until it draws.
        assert_eq!(decide("vulkan", ""), decide("vulkan", ""));
        // It never drew: back to automatic, and say so.
        assert_eq!(decide("vulkan", "vulkan"), Decision { choice: "auto".into(), trial: "".into(), dropped: Some("vulkan".into()) });
        // Switched while another was on trial: the new one is tried.
        assert_eq!(decide("software", "vulkan").choice, "software");
    }
}
