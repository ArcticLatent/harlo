//! M0 spike: a 5,000-row live transcript while a reply streams in.
//!
//! Usage: `stress <naive|virtual|slots> <results.json>`
//!
//! Loads 5,000 rows (wrapped prose, highlighted code blocks, tool output),
//! streams Markdown into the last row every 10 ms, and runs three phases:
//! scrolling, typing into the composer, then both. It records the interval
//! between frames, the time `view` takes, and how long a keystroke waits for
//! the next frame, then writes them to `results.json` and exits.
//!
//! `naive` builds every row on every view; `virtual` builds only the rows near
//! the viewport, with spacers sized from per-kind height estimates. `slots`
//! also builds only nearby rows, but pins row `i` at its measured offset in
//! stack slot `i % SLOTS`, so a row keeps its widget state (shaped text) for
//! as long as it stays near the viewport; `sensor` reports each row's real
//! height, and rows above the viewport that change size adjust the scroll
//! position so the content doesn't jump.

// Rust guideline compliant 2026-10-09

use std::cell::RefCell;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use iced::widget::{
    column, container, markdown, pin, scrollable, sensor, space, stack, text, text_input,
};
use iced::{Element, Fill, Font, Size, Subscription, Task, Theme, time, window};

/// Rows in the transcript.
const ROWS: usize = 5_000;

/// How often the streaming reply grows.
const STREAM_TICK: Duration = Duration::from_millis(10);

/// Characters added per stream tick (about 2,400 per second, far above a real model).
const STREAM_CHUNK: usize = 24;

/// How often a synthetic keystroke reaches the composer (about 16 keys per second).
const KEY_TICK: Duration = Duration::from_millis(60);

/// Pixels scrolled per frame while scrolling (about 5,800 px/s at 144 Hz).
const SCROLL_STEP: f32 = 40.0;

/// Time before measuring, so the first layout and shaping settle.
const WARMUP: Duration = Duration::from_secs(3);

/// Length of each measured phase.
const PHASE: Duration = Duration::from_secs(10);

/// Rows built above and below the visible range in virtual mode.
const OVERSCAN: usize = 6;

/// Stack slots for rows near the viewport in slots mode.
const SLOTS: usize = 64;

/// Pixels above and below the viewport that slots mode keeps built.
const OVERSCAN_PX: f32 = 900.0;

/// Gap between rows.
const SPACING: f32 = 12.0;

/// Estimated row heights per kind, for virtual mode's spacers.
const HEIGHTS: [f32; 3] = [96.0, 190.0, 230.0];

const PROSE: &str = "The agent read the session's configuration, compared the two branches and \
found that the retry loop gives up one attempt early when the server answers with a rate limit. \
It explains the fix in plain words so a reader who never saw this code can follow along, and \
wraps long sentences like this one across several lines of the transcript.";

const CODE: &str = "```rust\nfn retry<T>(mut attempt: impl FnMut() -> Result<T, Error>) -> Result<T, Error> {\n    \
let mut tries = 0;\n    loop {\n        match attempt() {\n            Ok(value) => return Ok(value),\n            \
Err(e) if e.is_rate_limit() && tries < MAX => tries += 1,\n            Err(e) => return Err(e),\n        }\n    }\n}\n```";

#[derive(Debug, Clone)]
enum Message {
    Frame(Instant),
    Stream,
    Key,
    Composer(String),
    Scrolled(scrollable::Viewport),
    Measured(usize, Size),
    Link,
}

enum Row {
    Markdown {
        kind: usize,
        content: Box<markdown::Content>,
    },
    Tool(String),
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    Warmup,
    Scroll,
    Type,
    Both,
    Done,
}

#[derive(Default)]
struct Samples {
    frames: Vec<f64>,
    keys: Vec<f64>,
    views: Vec<f64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Naive,
    Virtual,
    Slots,
}

struct Stress {
    mode: Mode,
    out: String,
    rows: Vec<Row>,
    heights: Vec<f32>,
    measured: usize,
    stream_source: String,
    streamed: usize,
    composer: String,
    offset: f32,
    viewport_height: f32,
    started: Option<Instant>,
    last_frame: Option<Instant>,
    pending_key: Option<Instant>,
    phase: Phase,
    samples: [Samples; 3],
    view_time: RefCell<Duration>,
}

fn scroller_id() -> iced::widget::Id {
    iced::widget::Id::new("transcript")
}

impl Stress {
    fn new(mode: Mode, out: String) -> Self {
        let mut rows = Vec::with_capacity(ROWS);
        for i in 0..ROWS - 1 {
            rows.push(match i % 3 {
                0 => Row::Markdown {
                    kind: 0,
                    content: Box::new(markdown::Content::parse(&format!("**Step {i}.** {PROSE}"))),
                },
                1 => Row::Markdown {
                    kind: 1,
                    content: Box::new(markdown::Content::parse(CODE)),
                },
                _ => Row::Tool((0..12).fold(String::new(), |mut out, l| {
                    let _ = writeln!(
                        out,
                        "src/module_{i}/file_{l}.rs:{l}: warning: unused variable `x{l}`"
                    );
                    out
                })),
            });
        }
        rows.push(Row::Markdown {
            kind: 0,
            content: Box::default(),
        });
        let stream_source = (0..400).fold(String::new(), |mut out, i| {
            let _ = write!(out, "Streaming paragraph {i}: {PROSE}\n\n{CODE}\n\n");
            out
        });
        let heights = rows.iter().map(Self::height).collect();
        Self {
            mode,
            out,
            rows,
            heights,
            measured: 0,
            stream_source,
            streamed: 0,
            composer: String::new(),
            offset: 0.0,
            viewport_height: 900.0,
            started: None,
            last_frame: None,
            pending_key: None,
            phase: Phase::Warmup,
            samples: Default::default(),
            view_time: RefCell::new(Duration::ZERO),
        }
    }

    fn phase_at(elapsed: Duration) -> Phase {
        match elapsed.checked_sub(WARMUP) {
            None => Phase::Warmup,
            Some(t) if t < PHASE => Phase::Scroll,
            Some(t) if t < PHASE * 2 => Phase::Type,
            Some(t) if t < PHASE * 3 => Phase::Both,
            Some(_) => Phase::Done,
        }
    }

    fn slot(&self) -> Option<usize> {
        match self.phase {
            Phase::Scroll => Some(0),
            Phase::Type => Some(1),
            Phase::Both => Some(2),
            Phase::Warmup | Phase::Done => None,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Frame(at) => {
                let started = *self.started.get_or_insert(at);
                self.phase = Self::phase_at(at.duration_since(started));
                if let Some(slot) = self.slot() {
                    if let Some(last) = self.last_frame {
                        self.samples[slot]
                            .frames
                            .push(at.duration_since(last).as_secs_f64() * 1000.0);
                    }
                    self.samples[slot]
                        .views
                        .push(self.view_time.borrow().as_secs_f64() * 1000.0);
                    if let Some(key) = self.pending_key.take() {
                        self.samples[slot]
                            .keys
                            .push(at.duration_since(key).as_secs_f64() * 1000.0);
                    }
                }
                self.last_frame = Some(at);
                if self.phase == Phase::Done {
                    self.write_results();
                    return iced::exit();
                }
                if matches!(self.phase, Phase::Scroll | Phase::Both) {
                    return iced::widget::operation::scroll_by(
                        scroller_id(),
                        scrollable::AbsoluteOffset {
                            x: 0.0,
                            y: SCROLL_STEP,
                        },
                    );
                }
                Task::none()
            }
            Message::Stream => {
                let end = (self.streamed + STREAM_CHUNK).min(self.stream_source.len());
                let end = (end..=self.stream_source.len())
                    .find(|&e| self.stream_source.is_char_boundary(e))
                    .unwrap_or(end);
                if let Some(Row::Markdown { content, .. }) = self.rows.last_mut() {
                    content.push_str(&self.stream_source[self.streamed..end]);
                }
                self.streamed = if end >= self.stream_source.len() {
                    0
                } else {
                    end
                };
                Task::none()
            }
            Message::Key => {
                if matches!(self.phase, Phase::Type | Phase::Both) {
                    self.composer.push('k');
                    if self.composer.len() > 200 {
                        self.composer.clear();
                    }
                    self.pending_key.get_or_insert_with(Instant::now);
                }
                Task::none()
            }
            Message::Composer(value) => {
                self.composer = value;
                Task::none()
            }
            Message::Scrolled(viewport) => {
                self.offset = viewport.absolute_offset().y;
                self.viewport_height = viewport.bounds().height;
                Task::none()
            }
            Message::Measured(row, size) => {
                let old = self.heights[row];
                if (size.height - old).abs() < 0.5 {
                    return Task::none();
                }
                self.heights[row] = size.height;
                self.measured += 1;
                // A row above the viewport changed size: move with it so the content stays put.
                let top: f32 = self.heights[..row].iter().map(|h| h + SPACING).sum();
                if top + old <= self.offset {
                    return iced::widget::operation::scroll_by(
                        scroller_id(),
                        scrollable::AbsoluteOffset {
                            x: 0.0,
                            y: size.height - old,
                        },
                    );
                }
                Task::none()
            }
            Message::Link => Task::none(),
        }
    }

    fn row_view<'a>(row: &'a Row, theme: &Theme) -> Element<'a, Message> {
        match row {
            Row::Markdown { content, .. } => {
                markdown::view(content.items(), theme).map(|_| Message::Link)
            }
            Row::Tool(out) => container(text(out).font(Font::MONOSPACE).size(13))
                .padding(8)
                .into(),
        }
    }

    fn height(row: &Row) -> f32 {
        match row {
            Row::Markdown { kind, .. } => HEIGHTS[*kind],
            Row::Tool(_) => HEIGHTS[2],
        }
    }

    /// Builds rows near the viewport between two spacers sized from estimates.
    fn virtual_view(&self, theme: &Theme) -> Element<'_, Message> {
        let mut top = 0.0;
        let mut first = 0;
        while first < self.rows.len() && top + Self::height(&self.rows[first]) < self.offset {
            top += Self::height(&self.rows[first]);
            first += 1;
        }
        let start = first.saturating_sub(OVERSCAN);
        let top: f32 = self.rows[..start].iter().map(Self::height).sum();
        let mut shown = 0.0;
        let mut end = first;
        while end < self.rows.len() && shown < self.viewport_height {
            shown += Self::height(&self.rows[end]);
            end += 1;
        }
        let end = (end + OVERSCAN).min(self.rows.len());
        let bottom: f32 = self.rows[end..].iter().map(Self::height).sum();
        let mut col = column![space().height(top)].spacing(SPACING);
        for row in &self.rows[start..end] {
            col = col.push(Self::row_view(row, theme));
        }
        // The streaming row stays built so its parsing cost is measured in every mode.
        if end < self.rows.len() {
            col = col.push(space().height(bottom));
            if let Some(last) = self.rows.last() {
                col = col.push(Self::row_view(last, theme));
            }
        }
        col.into()
    }

    /// Pins rows near the viewport at measured offsets, row `i` always in slot `i % SLOTS`.
    fn slots_view(&self, theme: &Theme) -> Element<'_, Message> {
        let last = self.rows.len() - 1;
        let mut slots: Vec<Option<Element<'_, Message>>> = (0..SLOTS).map(|_| None).collect();
        let (low, high) = (
            self.offset - OVERSCAN_PX,
            self.offset + self.viewport_height + OVERSCAN_PX,
        );
        let mut top = 0.0;
        let mut last_top = 0.0;
        for (i, (row, height)) in self.rows.iter().zip(&self.heights).enumerate() {
            if i == last {
                last_top = top;
            } else if top + height >= low && top <= high {
                slots[i % SLOTS] = Some(Self::measured_row(i, row, top, theme));
            }
            top += height + SPACING;
        }
        let mut layers: Vec<Element<'_, Message>> = Vec::with_capacity(SLOTS + 2);
        layers.push(space().width(Fill).height(top).into());
        layers.extend(
            slots
                .into_iter()
                .map(|slot| slot.unwrap_or_else(|| space().into())),
        );
        // The streaming row has a slot of its own and is always built.
        layers.push(Self::measured_row(last, &self.rows[last], last_top, theme));
        stack(layers).width(Fill).into()
    }

    fn measured_row<'a>(i: usize, row: &'a Row, top: f32, theme: &Theme) -> Element<'a, Message> {
        let content = sensor(Self::row_view(row, theme))
            .key(i)
            .anticipate(OVERSCAN_PX * 2.0)
            .on_show(move |size| Message::Measured(i, size))
            .on_resize(move |size| Message::Measured(i, size));
        pin(content).y(top).into()
    }

    fn view(&self) -> Element<'_, Message> {
        let started = Instant::now();
        let theme = Theme::Dark;
        let transcript: Element<'_, Message> = match self.mode {
            Mode::Naive => column(self.rows.iter().map(|row| Self::row_view(row, &theme)))
                .spacing(SPACING)
                .into(),
            Mode::Virtual => self.virtual_view(&theme),
            Mode::Slots => self.slots_view(&theme),
        };
        let content = column![
            scrollable(container(transcript).padding(16).width(Fill))
                .id(scroller_id())
                .on_scroll(Message::Scrolled)
                .height(Fill),
            container(
                text_input("Ask, or describe what to build.", &self.composer)
                    .on_input(Message::Composer)
                    .padding(10)
            )
            .padding(12),
        ];
        *self.view_time.borrow_mut() = started.elapsed();
        content.into()
    }

    #[expect(clippy::unused_self, reason = "iced calls it with the state")]
    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            window::frames().map(Message::Frame),
            time::every(STREAM_TICK).map(|_| Message::Stream),
            time::every(KEY_TICK).map(|_| Message::Key),
        ])
    }

    fn write_results(&self) {
        let names = ["scroll+stream", "type+stream", "scroll+type+stream"];
        let mode = match self.mode {
            Mode::Naive => "naive",
            Mode::Virtual => "virtual",
            Mode::Slots => "slots",
        };
        let mut out = format!("{{\"mode\":\"{mode}\",\"rows\":{ROWS},\"phases\":{{");
        for (i, (name, s)) in names.iter().zip(&self.samples).enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "\"{name}\":{{\"frame_ms\":{},\"view_ms\":{},\"key_to_frame_ms\":{}}}",
                stats(&s.frames),
                stats(&s.views),
                stats(&s.keys)
            );
        }
        let _ = write!(out, "}},\"rows_measured\":{}}}", self.measured);
        if let Err(e) = std::fs::write(&self.out, out) {
            eprintln!("could not write results: {e}");
        }
    }
}

fn stats(values: &[f64]) -> String {
    if values.is_empty() {
        return "null".into();
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let pick = |q: f64| {
        // Index from a quantile in [0, 1]; values are few thousand at most.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "small positive index"
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

fn main() -> iced::Result {
    let mut args = std::env::args().skip(1);
    let mode = match args.next().as_deref() {
        Some("virtual") => Mode::Virtual,
        Some("slots") => Mode::Slots,
        _ => Mode::Naive,
    };
    let out = args.next().unwrap_or_else(|| "stress.json".into());
    iced::application(
        move || Stress::new(mode, out.clone()),
        Stress::update,
        Stress::view,
    )
    .title("Harlo stress spike")
    .subscription(Stress::subscription)
    .theme(|_: &Stress| Theme::Dark)
    .window_size((1280.0, 900.0))
    .run()
}
