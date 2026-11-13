//! `Visuals` (QML singleton): the options and mockups in the chat, besides
//! its diagrams (harlo_core::visuals reads the blocks). An ```options
//! block is drawn as cards; a ```mockup block names an HTML file the agent
//! wrote, which harlo shows as a picture, taken by a browser on this
//! computer run without a window (the first that works: Firefox, Chromium
//! and the browsers made from them; `HARLO_BROWSER` names one), each in a
//! profile of its own in the cache folder, never yours. A picture is
//! keyed by its file's time and size, so a mockup the agent edits is
//! taken again the next time it's looked at. A mockup on another machine
//! is copied here first, once.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Bumped when a mockup's picture is taken (or can't be).
        #[qproperty(i32, revision)]
        type Visuals = super::VisualsRust;
    }

    impl cxx_qt::Threading for Visuals {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// An ```options block: `{ choices: [{ name, recommended, about,
        /// pros, cons }] }`, or `{ error }`.
        #[qinvokable]
        fn options(self: &Visuals, source: &QString) -> QString;
        /// A ```mockup block of a session in `cwd`: `{ title, width,
        /// height, after, before }` (before null when it has none), each a
        /// picture `{ state: "ready" | "taking" | "failed", url, width,
        /// height, error }`, or `{ error }`. Takes them in the background
        /// when needed.
        #[qinvokable]
        fn mockup(self: Pin<&mut Visuals>, source: &QString, cwd: &QString) -> QString;
        /// Opens a mockup's file (`before`: the one before) in the browser.
        #[qinvokable]
        fn open_mockup(self: &Visuals, source: &QString, cwd: &QString, before: bool);
    }
}

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::visuals;
use serde_json::{Value, json};

use qobject::Visuals;

#[derive(Default)]
pub struct VisualsRust {
    revision: i32,
    /// Taken (or failed) this run, by key.
    known: HashMap<String, Value>,
    /// Being taken.
    taking: HashSet<String>,
}

fn cache() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("harlo/mockups")
}

fn hash(parts: impl std::hash::Hash) -> String {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// A PNG's size, from its header.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    (bytes.len() >= 24 && &bytes[1..4] == b"PNG").then(|| (u32::from_be_bytes(bytes[16..20].try_into().unwrap()), u32::from_be_bytes(bytes[20..24].try_into().unwrap())))
}

fn ready(file: &Path) -> Option<Value> {
    let mut head = [0u8; 24];
    use std::io::Read;
    std::fs::File::open(file).ok()?.read_exact(&mut head).ok()?;
    let (w, h) = png_size(&head)?;
    Some(json!({ "state": "ready", "url": format!("file://{}", file.display()), "width": w, "height": h }))
}

/// A mockup's file and the key of its picture now (its time and size on
/// this computer; on another machine, its path: taken once).
fn picture_key(file: &Path, size: (u32, u32)) -> String {
    if harlo_core::remote::is_remote(file) {
        return hash((file, size, "remote"));
    }
    let stamp = std::fs::metadata(file).ok().map(|m| (m.len(), m.modified().ok()));
    hash((file, size, stamp))
}

/// The browsers that may take pictures: `HARLO_BROWSER`, else the ones
/// found on PATH, the one that worked last first.
fn browsers() -> Vec<PathBuf> {
    if let Some(b) = std::env::var_os("HARLO_BROWSER").filter(|b| !b.is_empty()) {
        return vec![PathBuf::from(b)];
    }
    const NAMES: [&str; 16] = [
        "firefox", "firefox-developer-edition", "firefox-esr", "librewolf", "floorp", "waterfox", "zen-browser", "chromium", "chromium-browser", "google-chrome-stable", "google-chrome", "brave",
        "brave-browser", "brave-origin", "microsoft-edge-stable", "vivaldi-stable",
    ];
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut found: Vec<PathBuf> = NAMES.iter().filter_map(|n| std::env::split_paths(&path).map(|d| d.join(n)).find(|p| p.is_file())).collect();
    if let Some(good) = WORKED.get().and_then(|w| w.lock().ok()?.clone())
        && let Some(i) = found.iter().position(|b| *b == good)
    {
        let b = found.remove(i);
        found.insert(0, b);
    }
    found
}

static WORKED: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
/// One picture at a time (a browser's profile is used by one at once).
static TAKING: Mutex<()> = Mutex::new(());

/// The longest a browser may take for a picture.
const TAKES: Duration = Duration::from_secs(25);

/// Takes a picture of `html` in a `size` window into `out` (blocking).
pub fn take(html: &Path, size: (u32, u32), out: &Path) -> Result<(), String> {
    let _one = TAKING.lock().unwrap_or_else(|e| e.into_inner());
    let found = browsers();
    if found.is_empty() {
        return Err("No browser to show it with: harlo uses Firefox or Chromium (or one made from them), without a window.".into());
    }
    let mut why = String::new();
    for b in found {
        match shoot(&b, html, size, out) {
            Ok(()) => {
                *WORKED.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner()) = Some(b);
                return Ok(());
            }
            Err(e) => why = e,
        }
    }
    Err(why)
}

fn shoot(browser: &Path, html: &Path, (w, h): (u32, u32), out: &Path) -> Result<(), String> {
    let name = browser.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let firefox = ["fox", "wolf", "floorp", "zen"].iter().any(|f| name.contains(f));
    let profile = cache().join("profiles").join(&name);
    std::fs::create_dir_all(&profile).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(out);
    let url = format!("file://{}", html.display());
    let mut cmd = Command::new(browser);
    if firefox {
        cmd.args(["--headless", "--no-remote", "--profile"]).arg(&profile).arg("--screenshot").arg(out).arg(format!("--window-size={w},{h}")).arg(&url);
    } else {
        cmd.args(["--headless=new", "--disable-gpu", "--hide-scrollbars", "--no-first-run", "--no-default-browser-check", "--disable-extensions"])
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(format!("--window-size={w},{h}"))
            .arg(format!("--screenshot={}", out.display()))
            .arg(&url);
    }
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| format!("{name} didn't start: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > TAKES => {
                // (It and what it started.)
                let _ = Command::new("kill").args(["-9", "--", &format!("-{}", child.id())]).status();
                let _ = child.wait();
                return Err(format!("{name} took too long to show it."));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let bytes = std::fs::read(out).map_err(|_| format!("{name} didn't take a picture of it."))?;
    png_size(&bytes).map(|_| ()).ok_or_else(|| format!("{name} didn't take a picture of it."))
}

/// A mockup's file, here: as it is on this computer, else copied from its
/// machine into the cache.
fn local_file(file: &Path) -> Result<PathBuf, String> {
    if !harlo_core::remote::is_remote(file) {
        return if file.is_file() { Ok(file.to_path_buf()) } else { Err(format!("{} isn't there.", file.display())) };
    }
    let bytes = harlo_core::remote::read(file).map_err(|_| format!("{} isn't there.", harlo_core::remote::display(file)))?;
    let copy = cache().join(format!("copy-{}.html", hash(file)));
    std::fs::create_dir_all(cache()).map_err(|e| e.to_string())?;
    std::fs::write(&copy, bytes).map_err(|e| e.to_string())?;
    Ok(copy)
}

/// A mockup's picture, taken now (blocking): the PNG's file.
fn picture(file: &Path, size: (u32, u32)) -> Result<PathBuf, String> {
    let png = cache().join(format!("{}.png", picture_key(file, size)));
    if ready(&png).is_some() {
        return Ok(png);
    }
    std::fs::create_dir_all(cache()).map_err(|e| e.to_string())?;
    take(&local_file(file)?, size, &png)?;
    Ok(png)
}

/// The most a phone's picture of a mockup may be.
const PHONE_BYTES: usize = 2 << 20;

/// A mockup for a phone (blocking): `{ title, after: { png (base64),
/// width, height }, before }`, or `{ error }`.
pub fn for_phone(source: &str, cwd: &Path) -> Value {
    use base64::Engine;
    let m = match visuals::mockup(source) {
        Ok(m) => m,
        Err(e) => return json!({ "error": format!("This mockup can't be shown: {e}.") }),
    };
    let one = |path: &str| -> Value {
        match picture(&visuals::file_in(cwd, path), (m.width, m.height)).and_then(|p| std::fs::read(p).map_err(|e| e.to_string())) {
            Ok(png) if png.len() > PHONE_BYTES => json!({ "error": "This picture is too big to send to the phone." }),
            Ok(png) => {
                let (w, h) = png_size(&png).unwrap_or((0, 0));
                json!({ "png": base64::engine::general_purpose::STANDARD.encode(&png), "width": w, "height": h })
            }
            Err(e) => json!({ "error": e }),
        }
    };
    json!({ "title": m.title, "path": m.path, "after": one(&m.path), "before": m.before.as_deref().map(one) })
}

impl Visuals {
    pub fn options(&self, source: &QString) -> QString {
        let v = match visuals::options(&source.to_string()) {
            // (Each line also as rich text, its Markdown drawn: `code`,
            // **bold**, links; the name plain too, to read aloud.)
            Ok(choices) => {
                let html = |t: &str| harlo_core::markdown::inline_html(t);
                let all = |l: &[String]| l.iter().map(|t| html(t)).collect::<Vec<_>>();
                json!({ "choices": choices.iter().map(|c| json!({
                    "name": c.name, "recommended": c.recommended, "about": c.about, "pros": c.pros, "cons": c.cons,
                    "plain": harlo_core::markdown::plain_text(&html(&c.name)), "nameHtml": html(&c.name), "aboutHtml": html(&c.about),
                    "prosHtml": all(&c.pros), "consHtml": all(&c.cons) })).collect::<Vec<_>>() })
            }
            Err(e) => json!({ "error": format!("These options can't be drawn: {e}.") }),
        };
        QString::from(&v.to_string())
    }

    pub fn mockup(mut self: Pin<&mut Self>, source: &QString, cwd: &QString) -> QString {
        let m = match visuals::mockup(&source.to_string()) {
            Ok(m) => m,
            Err(e) => return QString::from(&json!({ "error": format!("This mockup can't be shown: {e}.") }).to_string()),
        };
        let cwd = PathBuf::from(cwd.to_string());
        let size = (m.width, m.height);
        let after = self.as_mut().picture_now(visuals::file_in(&cwd, &m.path), size);
        let before = m.before.as_deref().map(|b| self.as_mut().picture_now(visuals::file_in(&cwd, b), size));
        QString::from(&json!({ "title": m.title, "path": m.path, "width": m.width, "height": m.height, "after": after, "before": before }).to_string())
    }

    /// A picture as it is now, taken in the background when it isn't yet.
    fn picture_now(mut self: Pin<&mut Self>, file: PathBuf, size: (u32, u32)) -> Value {
        let key = picture_key(&file, size);
        if let Some(v) = self.known.get(&key) {
            return v.clone();
        }
        if let Some(v) = ready(&cache().join(format!("{key}.png"))) {
            self.as_mut().rust_mut().known.insert(key, v.clone());
            return v;
        }
        if self.as_mut().rust_mut().taking.insert(key.clone()) {
            let thread = self.qt_thread();
            crate::runtime().spawn_blocking(move || {
                let v = match picture(&file, size) {
                    Ok(png) => ready(&png).unwrap_or_else(|| json!({ "state": "failed", "error": "The picture was empty." })),
                    Err(e) => json!({ "state": "failed", "error": e }),
                };
                let _ = thread.queue(move |mut this| {
                    this.as_mut().rust_mut().taking.remove(&key);
                    this.as_mut().rust_mut().known.insert(key, v);
                    let n = this.revision + 1;
                    this.as_mut().set_revision(n);
                });
            });
        }
        json!({ "state": "taking" })
    }

    pub fn open_mockup(&self, source: &QString, cwd: &QString, before: bool) {
        let Ok(m) = visuals::mockup(&source.to_string()) else { return };
        let Some(path) = (if before { m.before } else { Some(m.path) }) else { return };
        let file = visuals::file_in(Path::new(&cwd.to_string()), &path);
        std::thread::spawn(move || {
            if let Ok(here) = local_file(&file) {
                let _ = Command::new("xdg-open").arg(here).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in browser: Chromium's arguments, a PNG of the window's
    /// size whose colour comes from the file.
    const FAKE: &str = r#"#!/usr/bin/env python3
import hashlib, struct, sys, zlib
size, out, url = (1280, 800), None, ""
for a in sys.argv[1:]:
    if a.startswith("--window-size="): size = tuple(int(x) for x in a.split("=", 1)[1].split(","))
    elif a.startswith("--screenshot="): out = a.split("=", 1)[1]
    elif not a.startswith("--"): url = a
r, g, b = hashlib.sha256(open(url[7:], "rb").read()).digest()[:3]
w, h = size
raw = b"".join(b"\x00" + bytes([r, g, b]) * w for _ in range(h))
chunk = lambda k, d: struct.pack(">I", len(d)) + k + d + struct.pack(">I", zlib.crc32(k + d) & 0xFFFFFFFF)
open(out, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))
"#;

    /// A picture taken by a stand-in browser, taken again when its file
    /// changes.
    #[test]
    fn takes_pictures() {
        let dir = std::env::temp_dir().join(format!("harlo-mockups-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake-browser");
        std::fs::write(&fake, FAKE).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        // SAFETY: this test alone reads these.
        unsafe {
            std::env::set_var("HARLO_BROWSER", &fake);
            std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));
        }
        let html = dir.join("a.html");
        std::fs::write(&html, "<p>one</p>").unwrap();
        let first = picture(&html, (390, 844)).unwrap();
        assert_eq!(png_size(&std::fs::read(&first).unwrap()), Some((390, 844)));
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&html, "<p>two, longer</p>").unwrap();
        assert_ne!(picture(&html, (390, 844)).unwrap(), first);
        assert!(picture(&dir.join("none.html"), (390, 844)).unwrap_err().contains("isn't there"));
        let phone = for_phone("path: a.html\nbefore: none.html\nsize: 390x844", &dir);
        assert_eq!(phone["after"]["width"], 390);
        assert!(phone["before"]["error"].as_str().unwrap().contains("isn't there"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
