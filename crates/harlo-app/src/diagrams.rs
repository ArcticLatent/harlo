//! `Diagrams` (QML singleton): Mermaid diagrams in the chat. A ```mermaid
//! block an agent writes is drawn as a picture, in the theme's colours.
//!
//! harlo draws them itself with merman, a Rust implementation of Mermaid
//! (no browser engine), in its bundled Geist font, and keeps the PNGs in
//! its cache folder (`diagrams/`). What merman can't draw keeps its code,
//! and "Open in browser" draws it there with the bundled mermaid.js
//! (assets/mermaid).

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
        /// Bumped when a diagram is drawn (or can't be).
        #[qproperty(i32, revision)]
        type Diagrams = super::DiagramsRust;
    }

    impl cxx_qt::Threading for Diagrams {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// A diagram's picture: `{ state: "ready" | "drawing" | "failed",
        /// url, width, height (in pixels at 1x), error }`. `colors`: the
        /// theme's, as JSON `{ background, surface, text, muted, accent }`.
        /// Draws it in the background when needed.
        #[qinvokable]
        fn diagram(self: Pin<&mut Diagrams>, source: &QString, light: bool, colors: &QString) -> QString;
        /// Opens the diagram in the browser (a page in the cache folder).
        #[qinvokable]
        fn open_in_browser(self: &Diagrams, source: &QString, light: bool);
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::OnceLock;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use merman::svg::export::{NativeFontContext, NativeFontLimits, RasterFitBox, RasterOptions};
use merman::{Engine, MermaidConfig, OperationControl, PngRequest, RenderOutput, RenderRequest, Renderer, SvgEnvironment, SvgRequest};
use serde_json::{Value, json};

use qobject::Diagrams;

/// For "Open in browser".
const MERMAID: &[u8] = include_bytes!("../assets/mermaid/mermaid.min.js");

/// The faces diagrams are drawn in (harlo's interface font).
const GEIST: [&[u8]; 4] = [
    include_bytes!("../fonts/Geist-Regular.ttf"),
    include_bytes!("../fonts/Geist-Medium.ttf"),
    include_bytes!("../fonts/Geist-SemiBold.ttf"),
    include_bytes!("../fonts/Geist-Bold.ttf"),
];

/// Bumped when the drawing changes (its settings), so cached pictures go.
const DRAWING: u32 = 3;

/// Section colours (pie slices, mindmap branches), readable in both modes.
const PALETTE: [&str; 12] = ["#d97757", "#6c8ebf", "#82b366", "#b85450", "#9673a6", "#c9a83e", "#4a9a9a", "#c47a9c", "#7a8a5a", "#5a7ac4", "#c4905a", "#8a8a8a"];

#[derive(Default)]
pub struct DiagramsRust {
    revision: i32,
    /// Drawn (or failed) this run, by key.
    known: HashMap<String, Value>,
    /// Waiting to be drawn: key → (source, light, colors).
    pending: HashMap<String, (String, bool, Value)>,
    /// A batch is being drawn.
    running: bool,
}

fn cache() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("harlo/diagrams")
}

fn key_of(source: &str, light: bool, colors: &Value) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (source.trim(), light, colors.to_string(), DRAWING).hash(&mut h);
    format!("{:016x}", h.finish())
}

/// A PNG's size, from its header.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    (bytes.len() >= 24 && &bytes[1..4] == b"PNG").then(|| (u32::from_be_bytes(bytes[16..20].try_into().unwrap()), u32::from_be_bytes(bytes[20..24].try_into().unwrap())))
}

fn ready(file: &std::path::Path) -> Option<Value> {
    let mut head = [0u8; 24];
    use std::io::Read;
    std::fs::File::open(file).ok()?.read_exact(&mut head).ok()?;
    let (w, h) = png_size(&head)?;
    // Drawn at twice the size, for sharp text.
    Some(json!({ "state": "ready", "url": format!("file://{}", file.display()), "width": w / 2, "height": h / 2 }))
}

/// Mermaid's settings for the theme: its "base" theme in harlo's colours
/// and font. Labels are SVG text (what a picture can hold), wrapping at
/// 240 px (Mermaid's 120 splits words like domain names).
fn mermaid_config(light: bool, colors: &Value) -> Value {
    let c = |k: &str, or: &str| colors[k].as_str().filter(|v| !v.is_empty()).unwrap_or(or).to_owned();
    let (bg, surface, text, muted, accent) = if light {
        (c("background", "#ffffff"), c("surface", "#f3f1ee"), c("text", "#1f1d1b"), c("muted", "#6b645e"), c("accent", "#c96442"))
    } else {
        (c("background", "#1b1a19"), c("surface", "#2b2825"), c("text", "#ece6df"), c("muted", "#9a918a"), c("accent", "#d97757"))
    };
    let mut vars = json!({
        "darkMode": !light, "background": bg, "fontFamily": "Geist",
        "primaryColor": surface, "primaryTextColor": text, "primaryBorderColor": accent,
        "secondaryColor": surface, "tertiaryColor": bg, "lineColor": muted, "textColor": text,
        "edgeLabelBackground": bg, "noteBkgColor": surface, "noteTextColor": text, "noteBorderColor": muted,
        "actorBkg": surface, "actorBorder": accent, "actorTextColor": text, "actorLineColor": muted,
        "signalColor": muted, "signalTextColor": text, "labelTextColor": text, "titleColor": text,
        "pieTitleTextColor": text, "pieLegendTextColor": text, "pieSectionTextColor": "#ffffff", "pieStrokeColor": bg,
    });
    for (i, colour) in PALETTE.iter().enumerate() {
        vars[format!("pie{}", i + 1)] = json!(colour);
        vars[format!("cScale{i}")] = json!(colour);
        vars[format!("cScaleLabel{i}")] = json!("#ffffff");
    }
    json!({
        "theme": "base", "fontFamily": "Geist", "htmlLabels": false, "securityLevel": "strict",
        "flowchart": { "htmlLabels": false, "wrappingWidth": 240 },
        "state": { "wrappingWidth": 240 },
        "gantt": { "axisFormat": "%b %d" },
        "themeVariables": vars,
    })
}

/// harlo's fonts for merman (loaded once).
fn fonts() -> Result<NativeFontContext, String> {
    static FONTS: OnceLock<Result<NativeFontContext, String>> = OnceLock::new();
    FONTS
        .get_or_init(|| NativeFontContext::from_font_bytes(GEIST.iter().map(|f| f.to_vec()), NativeFontLimits::default(), &OperationControl::new()).map_err(|e| e.to_string()))
        .clone()
}

/// Draws a diagram (blocking): the PNG, at twice its size.
fn draw(source: &str, light: bool, colors: &Value) -> Result<Vec<u8>, String> {
    let environment = SvgEnvironment::deterministic().with_native_font_context(fonts()?).map_err(|e| e.to_string())?;
    let renderer = Renderer::new().with_engine(Engine::new().with_site_config(MermaidConfig::from_value(mermaid_config(light, colors))));
    let raster = RasterOptions::default().with_scale(2.0).with_fit_to(RasterFitBox::contain(6000, 6000));
    let request = RenderRequest::png(source, OperationControl::new(), PngRequest { svg: SvgRequest { environment, ..Default::default() }, options: raster });
    // merman is new: a failure in it stays a failure of this diagram.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| renderer.render(request))).map_err(|_| "merman couldn't draw this diagram.".to_owned())?;
    match outcome.map_err(|e| first_line(&e.to_string()))? {
        RenderOutput::Png(Some(png)) => Ok(png.bytes),
        _ => Err("That isn't a diagram merman knows.".into()),
    }
}

fn first_line(e: &str) -> String {
    e.lines().map(str::trim).filter(|l| !l.is_empty()).take(2).collect::<Vec<_>>().join(" ")
}

/// mermaid.js in the cache folder, for "Open in browser".
fn put_mermaid() -> std::io::Result<()> {
    std::fs::create_dir_all(cache())?;
    let js = cache().join("mermaid.min.js");
    if std::fs::metadata(&js).map(|m| m.len()).ok() != Some(MERMAID.len() as u64) {
        std::fs::write(&js, MERMAID)?;
    }
    Ok(())
}

/// Escapes JSON for a `<script>`.
fn script_json(v: &Value) -> String {
    v.to_string().replace("</", "<\\/")
}

impl Diagrams {
    pub fn diagram(mut self: Pin<&mut Self>, source: &QString, light: bool, colors: &QString) -> QString {
        let source = source.to_string();
        let colors: Value = serde_json::from_str(&colors.to_string()).unwrap_or_default();
        let key = key_of(&source, light, &colors);
        if let Some(v) = self.known.get(&key) {
            return QString::from(&v.to_string());
        }
        if let Some(v) = ready(&cache().join(format!("{key}.png"))) {
            self.as_mut().rust_mut().known.insert(key, v.clone());
            return QString::from(&v.to_string());
        }
        if source.trim().is_empty() {
            return QString::from(&json!({ "state": "failed", "error": "It's empty." }).to_string());
        }
        self.as_mut().rust_mut().pending.insert(key, (source, light, colors));
        self.as_mut().start();
        QString::from(&json!({ "state": "drawing" }).to_string())
    }

    /// Draws what's waiting, after a moment (a reply still streaming asks
    /// for each version of its diagram; the latest wins).
    fn start(mut self: Pin<&mut Self>) {
        if self.running {
            return;
        }
        self.as_mut().rust_mut().running = true;
        let thread = self.qt_thread();
        crate::runtime().spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _ = thread.clone().queue(move |mut this| {
                let jobs: Vec<(String, String, bool, Value)> = this.as_mut().rust_mut().pending.drain().map(|(k, (s, l, c))| (k, s, l, c)).collect();
                if jobs.is_empty() {
                    this.as_mut().rust_mut().running = false;
                    return;
                }
                let thread = this.qt_thread();
                crate::runtime().spawn_blocking(move || {
                    let _ = std::fs::create_dir_all(cache());
                    let results: Vec<(String, Value)> = jobs
                        .into_iter()
                        .map(|(key, source, light, colors)| {
                            let file = cache().join(format!("{key}.png"));
                            let v = match draw(&source, light, &colors).and_then(|png| std::fs::write(&file, png).map_err(|e| e.to_string())) {
                                Ok(()) => ready(&file).unwrap_or_else(|| json!({ "state": "failed", "error": "The picture was empty." })),
                                Err(e) => json!({ "state": "failed", "error": e }),
                            };
                            (key, v)
                        })
                        .collect();
                    let _ = thread.queue(move |mut this| {
                        for (key, v) in results {
                            this.as_mut().rust_mut().known.insert(key, v);
                        }
                        this.as_mut().rust_mut().running = false;
                        let n = this.revision + 1;
                        this.as_mut().set_revision(n);
                        if !this.pending.is_empty() {
                            this.start();
                        }
                    });
                });
            });
        });
    }

    pub fn open_in_browser(&self, source: &QString, light: bool) {
        if put_mermaid().is_err() {
            return;
        }
        let source = source.to_string();
        let page = cache().join(format!("view-{}.html", key_of(&source, light, &Value::Null)));
        let config = json!({ "startOnLoad": true, "securityLevel": "strict", "theme": if light { "default" } else { "dark" } });
        let bg = if light { "#ffffff" } else { "#1e1e1e" };
        // The source as text in the page (escaped), drawn by mermaid.js.
        let mut escaped = String::new();
        harlo_core::markdown::escape_into(&mut escaped, &source);
        let html = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>Diagram</title><style>body{{background:{bg};margin:24px}}</style><script src=\"mermaid.min.js\"></script></head><body><pre class=\"mermaid\">{escaped}</pre><script>mermaid.initialize({});</script></body></html>",
            script_json(&config)
        );
        if std::fs::write(&page, html).is_ok() {
            let _ = std::process::Command::new("xdg-open").arg(&page).stdin(std::process::Stdio::null()).spawn();
        }
    }
}

/// `harlo --check-diagram`: draws a small diagram and says how it went
/// (the release checks it on each system).
pub fn check() -> i32 {
    match draw("flowchart LR\n  A[harlo] --> B[diagram]", false, &Value::Null) {
        Ok(png) => {
            let (w, h) = png_size(&png).unwrap_or_default();
            println!("check-diagram: ok ({}x{})", w / 2, h / 2);
            0
        }
        Err(e) => {
            println!("check-diagram: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_settings() {
        let colors = json!({ "text": "#eee", "surface": "#222" });
        assert_eq!(key_of("graph LR\nA-->B\n", true, &colors), key_of("  graph LR\nA-->B", true, &colors));
        assert_ne!(key_of("graph LR", true, &colors), key_of("graph LR", false, &colors));
        let cfg = mermaid_config(false, &colors);
        assert_eq!(cfg["themeVariables"]["primaryTextColor"], "#eee");
        assert_eq!(cfg["themeVariables"]["background"], "#1b1a19");
        assert_eq!(cfg["themeVariables"]["pie3"], PALETTE[2]);
        assert_eq!(script_json(&json!({ "s": "</script><b>" })), r#"{"s":"<\/script><b>"}"#);
        let png = [0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 2, 0, 0, 0, 1, 0];
        assert_eq!(png_size(&png), Some((512, 256)));
    }

    #[test]
    fn draws_diagrams() {
        for source in [
            "flowchart TD\n  A([Visitor opens comfypilot.arcticlatent.ai]) --> B{Pages}\n  B -->|/| C[index.html]",
            "sequenceDiagram\n  You->>harlo: prompt\n  harlo-->>You: reply",
            "pie title Time\n  \"Code\" : 60\n  \"Tests\" : 40",
            "mindmap\n  root((harlo))\n    Agents\n      pi",
        ] {
            let png = draw(source, false, &Value::Null).unwrap_or_else(|e| panic!("{source}: {e}"));
            let (w, h) = png_size(&png).unwrap();
            assert!(w > 100 && h > 50, "{source}: {w}x{h}");
        }
        assert!(draw("flowchart LR\n  A --> ", true, &Value::Null).is_err());
        assert!(draw("not a diagram at all", true, &Value::Null).is_err());
    }
}
