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
        /// Where a flowchart's boxes are in its picture: `[{ id, x, y, w,
        /// h }]`, as fractions of its width and height (to click on them).
        #[qinvokable]
        fn boxes(self: &Diagrams, source: &QString) -> QString;
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
    draw_within(source, light, colors, (6000, 6000))
}

/// Draws a diagram at twice its size, made smaller to fit `fit` (in
/// pixels before it's doubled).
fn draw_within(source: &str, light: bool, colors: &Value, fit: (u32, u32)) -> Result<Vec<u8>, String> {
    let environment = SvgEnvironment::deterministic().with_native_font_context(fonts()?).map_err(|e| e.to_string())?;
    let renderer = Renderer::new().with_engine(Engine::new().with_site_config(MermaidConfig::from_value(mermaid_config(light, colors))));
    let raster = RasterOptions::default().with_scale(2.0).with_fit_to(RasterFitBox::contain(fit.0, fit.1));
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

    pub fn boxes(&self, source: &QString) -> QString {
        QString::from(&Value::Array(node_boxes(&source.to_string())).to_string())
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

/// The ```mermaid blocks of a reply, in order.
/// A flowchart's boxes in its picture (see `boxes`), laid out as merman
/// lays out the picture, from its SVG: each node's place and its box's size,
/// over the drawing's view box.
pub fn node_boxes(source: &str) -> Vec<Value> {
    static SEEN: OnceLock<std::sync::Mutex<HashMap<String, Vec<Value>>>> = OnceLock::new();
    let seen = SEEN.get_or_init(Default::default);
    if let Some(b) = seen.lock().unwrap().get(source) {
        return b.clone();
    }
    let svg = (|| {
        let environment = SvgEnvironment::deterministic().with_native_font_context(fonts().ok()?).ok()?;
        let renderer = Renderer::new().with_engine(Engine::new().with_site_config(MermaidConfig::from_value(mermaid_config(false, &json!({})))));
        let request = RenderRequest::svg(source, OperationControl::new(), SvgRequest { environment, ..Default::default() });
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| renderer.render(request))).ok()?.ok()? {
            RenderOutput::Svg(Some(out)) => Some(out.svg().to_owned()),
            _ => None,
        }
    })();
    let found = svg.map(|svg| svg_boxes(&svg)).unwrap_or_default();
    seen.lock().unwrap().insert(source.to_owned(), found.clone());
    found
}

/// The boxes in a merman flowchart's SVG.
fn svg_boxes(svg: &str) -> Vec<Value> {
    let attr = |tag: &str, name: &str| -> Option<String> {
        let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        Some(tag[at..].split('"').next()?.to_owned())
    };
    let nums = |s: &str| s.split(|c: char| c == ' ' || c == ',').filter(|t| !t.is_empty()).filter_map(|t| t.parse::<f64>().ok()).collect::<Vec<_>>();
    let Some(view) = svg.find("<svg").and_then(|i| attr(&svg[i..svg[i..].find('>').map_or(svg.len(), |e| i + e)], "viewBox")).map(|v| nums(&v)) else { return vec![] };
    let [vx, vy, vw, vh] = view[..] else { return vec![] };
    if vw <= 0.0 || vh <= 0.0 {
        return vec![];
    }
    let mut out = vec![];
    let mut rest = svg;
    while let Some(i) = rest.find("<g class=\"node") {
        rest = &rest[i..];
        let tag = &rest[..rest.find('>').unwrap_or(rest.len())];
        let id = attr(tag, "id").and_then(|id| {
            let core = id.rsplit_once('-')?.0.to_owned();
            Some(core.rsplit_once("flowchart-")?.1.to_owned())
        });
        let at = attr(tag, "transform").and_then(|t| Some(nums(t.strip_prefix("translate(")?.trim_end_matches(')'))));
        let next = rest[1..].find("<g class=\"node").map_or(rest.len(), |n| n + 1);
        let body = &rest[..next];
        let rect = body.find("label-container").and_then(|r| {
            let start = body[..r].rfind('<')?;
            let tag = &body[start..start + body[start..].find('>')?];
            Some([attr(tag, "x")?, attr(tag, "y")?, attr(tag, "width")?, attr(tag, "height")?].map(|v| v.parse::<f64>().unwrap_or(0.0)))
        });
        if let (Some(id), Some(at), Some([x, y, w, h])) = (id, at, rect)
            && at.len() >= 2
        {
            out.push(json!({ "id": id, "x": (at[0] + x - vx) / vw, "y": (at[1] + y - vy) / vh, "w": w / vw, "h": h / vh }));
        }
        rest = &rest[next..];
    }
    out
}

pub fn blocks(text: &str) -> Vec<String> {
    harlo_core::visuals::blocks(text, "mermaid")
}

/// What's wrong in a diagram's source, as Mermaid would see it: None when
/// it parses, or is a kind merman doesn't draw (the browser does).
pub fn problem(source: &str) -> Option<String> {
    if source.trim().is_empty() {
        return Some("it's empty".into());
    }
    let engine = Engine::new();
    // (merman is new: a failure in it isn't the diagram's.)
    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.parse_diagram_sync(source, merman::ParseOptions::strict()))).ok()?;
    match parsed {
        Ok(_) => None,
        Err(merman::Error::DiagramParse { diagnostic, .. }) => {
            let at = diagnostic.span().map(|s| source[..s.start.min(source.len())].matches('\n').count() + 1);
            let said = first_line(diagnostic.message());
            Some(match at {
                Some(n) => format!("line {n}: {said}"),
                None => said,
            })
        }
        Err(merman::Error::DetectType(_)) => Some("no diagram type: its first line should name one (flowchart TD, sequenceDiagram, classDiagram, …)".into()),
        Err(e @ merman::Error::MalformedFrontMatter) => Some(first_line(&e.to_string())),
        Err(_) => None,
    }
}

/// What harlo tells an agent whose reply has blocks it can't show: a
/// diagram that doesn't parse, an options block without choices, a
/// mockup of a file that isn't there (looked for in `cwd`). None: all
/// can be.
pub fn fix_message(reply: &str, cwd: Option<&std::path::Path>) -> Option<String> {
    let found = blocks(reply);
    let mut bad: Vec<String> = found
        .iter()
        .enumerate()
        .filter_map(|(i, b)| problem(b).map(|p| if found.len() > 1 { format!("diagram {} of {} doesn't parse ({p})", i + 1, found.len()) } else { format!("a diagram doesn't parse ({p})") }))
        .collect();
    bad.extend(harlo_core::visuals::problems(reply, cwd));
    bad.truncate(3);
    if bad.is_empty() {
        return None;
    }
    let one = bad.len() == 1;
    Some(format!(
        "[harlo] In your last reply, {}, so it shows as code. Send {} again, fixed (only {}; no need to repeat the rest).",
        bad.join("; "),
        if one { "it" } else { "them" },
        if one { "that block" } else { "those blocks" }
    ))
}

/// The most a phone's picture of a diagram may be (a relay carries 4 MB a
/// message, in base64 twice).
const PHONE_BYTES: usize = 2 << 20;

/// A diagram for a phone (blocking): its PNG and size in points (drawn at
/// twice that, at most 2400 × 4800 pixels), from the cache when it was
/// drawn before. `colors` as `diagram` takes them.
pub fn for_phone(source: &str, light: bool, colors: &Value) -> Result<(Vec<u8>, u32, u32), String> {
    if source.trim().is_empty() {
        return Err("It's empty.".into());
    }
    let key = key_of(&format!("{source}\n%% phone"), light, colors);
    let file = cache().join(format!("{key}.png"));
    let png = match std::fs::read(&file) {
        Ok(png) if png_size(&png).is_some() => png,
        _ => {
            let png = draw_within(source, light, colors, (1200, 2400))?;
            let _ = std::fs::create_dir_all(cache());
            let _ = std::fs::write(&file, &png);
            png
        }
    };
    if png.len() > PHONE_BYTES {
        return Err("This diagram is too big to send to the phone.".into());
    }
    let (w, h) = png_size(&png).ok_or("The picture was empty.")?;
    Ok((png, w / 2, h / 2))
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
    fn finds_a_flowcharts_boxes() {
        let b = node_boxes("flowchart TD\n  s0[\"Notes app\"]\n  s1[\"Sidebar\"]\n  s2[\"Store\"]\n  s0 --> s1\n  s0 --> s2\n  s1 -.->|depends on| s2\n");
        let ids: Vec<&str> = b.iter().map(|v| v["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["s0", "s1", "s2"], "{b:?}");
        for v in &b {
            let (x, y, w, h) = (v["x"].as_f64().unwrap(), v["y"].as_f64().unwrap(), v["w"].as_f64().unwrap(), v["h"].as_f64().unwrap());
            assert!(x >= 0.0 && y >= 0.0 && w > 0.05 && h > 0.05 && x + w <= 1.001 && y + h <= 1.001, "inside the picture: {v}");
        }
        // Top to bottom: the root above the others.
        assert!(b[0]["y"].as_f64() < b[1]["y"].as_f64() && b[1]["y"].as_f64() < b[2]["y"].as_f64(), "{b:?}");
    }

    #[test]
    fn checks_diagrams_in_replies() {
        let reply = "Here:\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nand\n\n```mermaid\nflowchart TD\n  A --> B\n  C -->\n```\n\n```rust\nlet x = 1;\n```\n";
        assert_eq!(blocks(reply), ["flowchart TD\n  A --> B", "flowchart TD\n  A --> B\n  C -->"]);
        assert_eq!(problem("flowchart TD\n  A --> B"), None);
        let bad = problem("flowchart TD\n  A --> B\n  C -->").expect("a broken diagram");
        assert!(bad.starts_with("line 3: ") || bad.starts_with("line 4: "), "{bad}");
        assert!(problem("flowchar TD\n A --> B").unwrap().starts_with("no diagram type"));
        assert_eq!(problem("  "), Some("it's empty".into()));
        let said = fix_message(reply, None).unwrap();
        assert!(said.starts_with("[harlo] In your last reply, diagram 2 of 2 doesn't parse (line "), "{said}");
        assert!(said.ends_with("), so it shows as code. Send it again, fixed (only that block; no need to repeat the rest)."), "{said}");
        assert_eq!(fix_message("No diagrams here.\n```mermaid\nsequenceDiagram\n  A->>B: hi\n```", None), None);
        let two = fix_message("```options\n## Only\n```\n```mermaid\nflowchar TD\n```", None).unwrap();
        assert!(two.contains("a diagram doesn't parse (no diagram type") && two.contains("; an options block can't be drawn: it has only one choice") && two.contains("Send them again, fixed (only those blocks;"), "{two}");
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
        let png = draw_within("flowchart LR\n  A[one] --> B[two] --> C[three] --> D[four] --> E[five] --> F[six] --> G[seven]", false, &Value::Null, (400, 400)).unwrap();
        let (w, h) = png_size(&png).unwrap();
        assert!(w <= 800 && h <= 800 && w > 200, "made to fit, then doubled: {w}x{h}");
        assert!(draw("flowchart LR\n  A --> ", true, &Value::Null).is_err());
        assert!(draw("not a diagram at all", true, &Value::Null).is_err());
    }
}

