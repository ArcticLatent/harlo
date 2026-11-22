//! The long-session stress test, on the real window.
//!
//! Usage: `stress [results.json]`
//!
//! Opens Harlo's window on the desktop with a 5,000-entry session (answers
//! of prose, answers with a highlighted code block, and edits with 12-line
//! diffs) and streams a reply into it every 10 ms, through the same events,
//! reducer and presenter the app uses. No OpenCode server is involved. After
//! a warm-up it runs three phases of 10 s: scrolling, typing into the
//! composer, then both. Scrolling sends a wheel event after every frame;
//! typing sends a key every 60 ms. It records the time between frames, how
//! long each update and present takes, and how long a key waits for the next
//! frame, writes them to `results.json`, and exits.
//!
//! The conditions match the M0 test of the iced timeline, so the numbers
//! compare.

// Rust guideline compliant 2026-10-09

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use harlo::Driver;
use harlo::app::{App, Message};
use harlo::ui::{AppWindow, Content, State};
use harlo_core::layout::Layout;
use harlo_core::server::{Client, Connected, Endpoint, Location, Update};
use harlo_core::timeline::Entry;
use harlo_core::workspace::{
    Event, EventKind, FileDiff, ServerProject, Session, Snapshot, Tokens, ToolStatus,
};
use slint::platform::WindowEvent;
use slint::{ComponentHandle as _, LogicalPosition, RenderingState, Timer, TimerMode};

/// Entries in the session, unless `STRESS_ROWS` says otherwise.
const ROWS: usize = 5_000;

/// Entries in the session.
fn rows() -> usize {
    std::env::var("STRESS_ROWS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(ROWS)
}

/// Whether the reply streams; `STRESS_STREAM=0` stops it, to tell costs apart.
fn streams() -> bool {
    std::env::var("STRESS_STREAM").map_or(true, |v| v != "0")
}

/// How often the streaming reply grows.
const STREAM_TICK: Duration = Duration::from_millis(10);

/// Characters added per stream tick (about 2,400 per second, far above a real model).
const STREAM_CHUNK: usize = 24;

/// How often a key reaches the composer (about 16 keys per second).
const KEY_TICK: Duration = Duration::from_millis(60);

/// Pixels scrolled per frame while scrolling (about 5,800 px/s at 144 Hz).
const SCROLL_STEP: f32 = 40.0;

/// Time before measuring, so the first layout and shaping settle.
const WARMUP: Duration = Duration::from_secs(3);

/// Length of each measured phase.
const PHASE: Duration = Duration::from_secs(10);

/// The window's size.
const WINDOW: (f32, f32) = (1280.0, 900.0);

/// The project folder and the session.
const FOLDER: &str = "/tmp/harlo-stress";
const SESSION: &str = "ses_stress";

const PROSE: &str = "The agent read the session's configuration, compared the two branches and \
found that the retry loop gives up one attempt early when the server answers with a rate limit. \
It explains the fix in plain words so a reader who never saw this code can follow along, and \
wraps long sentences like this one across several lines of the transcript.";

const CODE: &str = "```rust\nfn retry<T>(mut attempt: impl FnMut() -> Result<T, Error>) -> Result<T, Error> {\n    \
let mut tries = 0;\n    loop {\n        match attempt() {\n            Ok(value) => return Ok(value),\n            \
Err(e) if e.is_rate_limit() && tries < MAX => tries += 1,\n            Err(e) => return Err(e),\n        }\n    }\n}\n```";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Warmup,
    Scroll,
    Type,
    Both,
    Done,
}

impl Phase {
    fn at(elapsed: Duration) -> Self {
        match elapsed.checked_sub(WARMUP) {
            None => Self::Warmup,
            Some(t) if t < PHASE => Self::Scroll,
            Some(t) if t < PHASE * 2 => Self::Type,
            Some(t) if t < PHASE * 3 => Self::Both,
            Some(_) => Self::Done,
        }
    }

    fn slot(self) -> Option<usize> {
        match self {
            Self::Scroll => Some(0),
            Self::Type => Some(1),
            Self::Both => Some(2),
            Self::Warmup | Self::Done => None,
        }
    }

    fn scrolls(self) -> bool {
        matches!(self, Self::Scroll | Self::Both)
    }

    fn types(self) -> bool {
        matches!(self, Self::Type | Self::Both)
    }
}

#[derive(Default)]
struct Samples {
    frames: Vec<f64>,
    keys: Vec<f64>,
    updates: Vec<f64>,
}

struct Bench {
    out: String,
    started: Instant,
    phase: Phase,
    last_frame: Option<Instant>,
    pending_key: Option<Instant>,
    samples: [Samples; 3],
    stream: String,
    streamed: usize,
    typed: usize,
    finished: bool,
    /// Every stream tick's update and present, for the progress line.
    stream_ms: Vec<f64>,
}

impl Bench {
    fn record_update(&mut self, took: Duration) {
        if let Some(slot) = self.phase.slot() {
            self.samples[slot].updates.push(ms(took));
        }
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "stress-slint.json".into());
    let window = AppWindow::new()?;
    let app = App::new(Location::Shared, vec![Location::Shared], Layout::default());
    let driver = Driver::new(&window, app, WINDOW);
    let loaded = Instant::now();
    driver.send(Message::Tick(SystemTime::now()));
    driver.send(connected(snapshot()));
    driver.send(Message::OpenSession(SESSION.into()));
    driver.send(Message::History(SESSION.into(), Ok(history())));
    eprintln!("history shown in {:.0} ms", ms(loaded.elapsed()));
    window.show()?;

    let stream = (0..400).fold(String::new(), |mut out, i| {
        let _ = write!(out, "Streaming paragraph {i}: {PROSE}\n\n{CODE}\n\n");
        out
    });
    let bench = Rc::new(RefCell::new(Bench {
        out,
        started: Instant::now(),
        phase: Phase::Warmup,
        last_frame: None,
        pending_key: None,
        samples: Default::default(),
        stream,
        streamed: 0,
        typed: 0,
        finished: false,
        stream_ms: Vec::new(),
    }));

    // Starts at the top, as the iced test did, and reads down.
    let weak = window.as_weak();
    Timer::single_shot(Duration::from_millis(500), move || {
        if let Some(window) = weak.upgrade() {
            scroll(&window, 1.0e7);
        }
    });

    watch_frames(&window, &bench)?;
    let _stream = stream_reply(&driver, &bench);
    let _keys = type_keys(&window, &bench);
    let _progress = progress(&bench);

    slint::run_event_loop()?;
    let typed = bench.borrow().typed;
    let composer = driver.app().composer.text().len();
    eprintln!("typed {typed} keys; the composer holds {composer} characters");
    Ok(())
}

/// Measures every frame, then scrolls on while a phase scrolls.
fn watch_frames(window: &AppWindow, bench: &Rc<RefCell<Bench>>) -> Result<(), String> {
    let frames = Rc::clone(bench);
    let weak = window.as_weak();
    window
        .window()
        .set_rendering_notifier(move |state, _| {
            if !matches!(state, RenderingState::AfterRendering) {
                return;
            }
            let now = Instant::now();
            let mut bench = frames.borrow_mut();
            let phase = Phase::at(now.duration_since(bench.started));
            if phase != bench.phase {
                // A phase starts timing from its own first frame: Slint draws
                // only what changed, so the gap before it is idle, not slow.
                bench.last_frame = None;
                bench.pending_key = None;
            }
            bench.phase = phase;
            if let Some(slot) = bench.phase.slot() {
                if let Some(last) = bench.last_frame {
                    bench.samples[slot]
                        .frames
                        .push(ms(now.duration_since(last)));
                }
                if let Some(key) = bench.pending_key.take() {
                    bench.samples[slot].keys.push(ms(now.duration_since(key)));
                }
            }
            bench.last_frame = Some(now);
            if bench.phase == Phase::Done && !bench.finished {
                bench.finished = true;
                write_results(&bench);
                let _ = slint::quit_event_loop();
                return;
            }
            if bench.phase.scrolls() {
                let weak = weak.clone();
                Timer::single_shot(Duration::ZERO, move || {
                    if let Some(window) = weak.upgrade() {
                        scroll(&window, -SCROLL_STEP);
                    }
                });
            }
        })
        .map_err(|e| format!("can't watch frames: {e:?}"))
}

/// Streams the reply in, the whole time.
fn stream_reply(driver: &Driver, bench: &Rc<RefCell<Bench>>) -> Timer {
    let streaming = Rc::clone(bench);
    let stream_driver = driver.clone();
    let stream_timer = Timer::default();
    stream_timer.start(TimerMode::Repeated, STREAM_TICK, move || {
        if !streams() {
            return;
        }
        let delta = {
            let mut bench = streaming.borrow_mut();
            let start = bench.streamed;
            let mut end = (start + STREAM_CHUNK).min(bench.stream.len());
            while !bench.stream.is_char_boundary(end) {
                end += 1;
            }
            bench.streamed = if end >= bench.stream.len() { 0 } else { end };
            bench.stream[start..end].to_owned()
        };
        let began = Instant::now();
        stream_driver.send(Message::Server(Update::Event(Event {
            id: "evt_stream".into(),
            seq: None,
            time: now_ms(),
            kind: EventKind::TextDelta {
                session_id: SESSION.into(),
                message_id: "msg_stream".into(),
                ordinal: 0,
                delta,
            },
        })));
        let took = began.elapsed();
        let mut bench = streaming.borrow_mut();
        bench.record_update(took);
        bench.stream_ms.push(ms(took));
    });
    stream_timer
}

/// Sends keys to the composer while a phase types.
fn type_keys(window: &AppWindow, bench: &Rc<RefCell<Bench>>) -> Timer {
    let typing = Rc::clone(bench);
    let weak = window.as_weak();
    let key_timer = Timer::default();
    key_timer.start(TimerMode::Repeated, KEY_TICK, move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if !typing.borrow().phase.types() {
            return;
        }
        let began = Instant::now();
        let w = window.window();
        let _ = w.dispatch_event_with_result(WindowEvent::KeyPressed { text: "k".into() });
        let _ = w.dispatch_event_with_result(WindowEvent::KeyReleased { text: "k".into() });
        let mut bench = typing.borrow_mut();
        bench.typed += 1;
        bench.pending_key.get_or_insert(began);
        bench.record_update(began.elapsed());
    });
    key_timer
}

/// Prints a line a second on how it goes.
fn progress(bench: &Rc<RefCell<Bench>>) -> Timer {
    let progress = Rc::clone(bench);
    let progress_timer = Timer::default();
    progress_timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
    let mut bench = progress.borrow_mut();
    let frames: usize = bench.samples.iter().map(|s| s.frames.len()).sum();
    let ticks = std::mem::take(&mut bench.stream_ms);
    #[expect(clippy::cast_precision_loss, reason = "a few hundred ticks")]
    let mean = ticks.iter().sum::<f64>() / ticks.len().max(1) as f64;
    eprintln!(
        "{:>4.1} s: {} frames measured, {} typed, {} characters streamed, {} stream ticks at {:.1} ms each",
        bench.started.elapsed().as_secs_f64(),
        frames,
        bench.typed,
        bench.streamed,
        ticks.len(),
        mean
    );
});
    progress_timer
}

/// A wheel turn over the session's timeline: positive goes up.
fn scroll(window: &AppWindow, delta: f32) {
    let state = window.global::<State>();
    if state.get_content() != Content::Session {
        return;
    }
    // The middle of the center, between the sidebar and the right panel.
    let size = window
        .window()
        .size()
        .to_logical(window.window().scale_factor());
    let left = harlo::fit::RAIL
        + if state.get_sidebar_shown() {
            state.get_sidebar_width()
        } else {
            0.0
        };
    let right = harlo::fit::RAIL
        + if state.get_panel() >= 0 {
            state.get_panel_width()
        } else {
            0.0
        };
    let position =
        LogicalPosition::new(left + (size.width - left - right) / 2.0, size.height / 2.0);
    let _ = window
        .window()
        .dispatch_event_with_result(WindowEvent::PointerScrolled {
            position,
            delta_x: 0.0,
            delta_y: delta,
        });
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// One project with the one long session.
fn snapshot() -> Snapshot {
    let now = now_ms();
    Snapshot {
        projects: vec![ServerProject {
            id: "p_stress".into(),
            directory: FOLDER.into(),
            name: None,
            git: true,
        }],
        worktrees: HashMap::from([("p_stress".to_owned(), vec![FOLDER.to_owned()])]),
        sessions: vec![Session {
            id: SESSION.into(),
            title: Some("A long session".into()),
            directory: FOLDER.into(),
            project_id: "p_stress".into(),
            parent_id: None,
            agent: Some("build".into()),
            model: None,
            created: now,
            updated: now,
            revert: None,
            permissions: Vec::new(),
            cost: 0.0,
            tokens: Tokens::default(),
        }],
        active: Vec::new(),
        permissions: Vec::new(),
        forms: Vec::new(),
    }
}

/// The update a connection sends once it has loaded `snapshot`; nothing listens at its address.
fn connected(snapshot: Snapshot) -> Message {
    Message::Server(Update::Connected(Arc::new(Connected {
        client: Client::new(Endpoint::new("http://127.0.0.1:9", None)),
        version: harlo_core::OPENCODE_VERSION.to_owned(),
        snapshot,
    })))
}

/// The session's history: prose, code and edits in turn.
fn history() -> Vec<Entry> {
    (0..rows())
        .map(|i| match i % 3 {
            0 => Entry::Answer {
                key: format!("msg_{i}/text/0"),
                text: format!("**Step {i}.** {PROSE}"),
            },
            1 => Entry::Answer {
                key: format!("msg_{i}/text/0"),
                text: CODE.to_owned(),
            },
            _ => {
                let file = format!("src/module_{i}/lib.rs");
                let mut patch = format!("--- a/{file}\n+++ b/{file}\n@@ -1,0 +1,12 @@\n");
                for l in 0..12 {
                    let _ = writeln!(
                        patch,
                        "+src/module_{i}/file_{l}.rs:{l}: warning: unused variable `x{l}`"
                    );
                }
                Entry::Tool {
                    key: format!("msg_{i}/tool/0"),
                    name: "edit".into(),
                    label: "Edit".into(),
                    detail: file.clone(),
                    status: ToolStatus::Completed,
                    took: Some(Duration::from_millis(1_200)),
                    files: vec![FileDiff {
                        file,
                        patch,
                        additions: 12,
                        deletions: 0,
                        status: "modified".into(),
                    }],
                    todos: None,
                }
            }
        })
        .collect()
}

fn write_results(bench: &Bench) {
    let names = ["scroll+stream", "type+stream", "scroll+type+stream"];
    let mut out = format!("{{\"mode\":\"slint\",\"rows\":{},\"phases\":{{", rows());
    for (i, (name, s)) in names.iter().zip(&bench.samples).enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "\"{name}\":{{\"frame_ms\":{},\"view_ms\":{},\"key_to_frame_ms\":{}}}",
            stats(&s.frames),
            stats(&s.updates),
            stats(&s.keys)
        );
    }
    out.push_str("}}");
    if let Err(e) = std::fs::write(&bench.out, out) {
        eprintln!("could not write results: {e}");
    }
}

fn stats(values: &[f64]) -> String {
    if values.is_empty() {
        return "null".into();
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let pick = |q: f64| {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "a small positive index from a quantile"
        )]
        let i = ((v.len() - 1) as f64 * q).round() as usize;
        v[i]
    };
    let over = |limit: f64| v.iter().filter(|&&x| x > limit).count();
    format!(
        "{{\"n\":{},\"p50\":{:.2},\"p95\":{:.2},\"p99\":{:.2},\"max\":{:.2},\"over_7ms\":{},\"over_16ms\":{},\"over_33ms\":{}}}",
        v.len(),
        pick(0.5),
        pick(0.95),
        pick(0.99),
        pick(1.0),
        over(7.0),
        over(16.7),
        over(33.4)
    )
}
