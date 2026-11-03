//! Keeping a window above the others on KDE's Wayland, where Qt's
//! `WindowStaysOnTopHint` has no effect: a one-off KWin script finds the
//! window (this process's, by title) and sets its "keep above". Elsewhere
//! the hint does it and this does nothing.

use std::process::{Command, Stdio};

/// Whether the desktop needs asking (KDE on Wayland).
fn needed() -> bool {
    let wayland = std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland");
    let kde = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|p| p.eq_ignore_ascii_case("kde")));
    wayland && kde
}

/// A JavaScript string for `text`.
fn js_string(text: &str) -> String {
    serde_json::Value::from(text).to_string()
}

fn script(pid: u32, title: &str, on: bool) -> String {
    format!(
        "const list = workspace.windowList ? workspace.windowList() : workspace.clientList();\n\
         for (const w of list) if (w.pid === {pid} && w.caption === {}) w.keepAbove = {on};\n",
        js_string(title)
    )
}

/// Asks KWin over D-Bus (blocking; run it off the UI thread).
pub fn set(title: &str, on: bool) -> Result<(), String> {
    if !needed() {
        return Ok(());
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let name = format!("harlo-keep-above-{}", std::process::id());
    let file = dir.join(format!("{name}.js"));
    std::fs::write(&file, script(std::process::id(), title, on)).map_err(|e| e.to_string())?;
    let call = |path: &str, method: &str, args: &[&str]| -> Result<String, String> {
        let out = Command::new("gdbus")
            .args(["call", "--session", "--dest", "org.kde.KWin", "--object-path", path, "--method", method])
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() { Ok(String::from_utf8_lossy(&out.stdout).into_owned()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()) }
    };
    let file_text = file.to_string_lossy().into_owned();
    // (One left from before would refuse the name.)
    let _ = call("/Scripting", "org.kde.kwin.Scripting.unloadScript", &[&name]);
    let result = call("/Scripting", "org.kde.kwin.Scripting.loadScript", &[&file_text, &name]).and_then(|id| {
        let id: String = id.chars().filter(char::is_ascii_digit).collect();
        call(&format!("/Scripting/Script{id}"), "org.kde.kwin.Script.run", &[]).map(|_| ())
    });
    let _ = call("/Scripting", "org.kde.kwin.Scripting.unloadScript", &[&name]);
    let _ = std::fs::remove_file(&file);
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_script_names_only_this_window() {
        let s = super::script(42, "harlo — mini \"chat\"", true);
        assert!(s.contains("w.pid === 42 && w.caption === \"harlo — mini \\\"chat\\\"\") w.keepAbove = true;"), "{s}");
    }
}
