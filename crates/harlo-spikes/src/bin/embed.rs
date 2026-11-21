//! M0 spike: a code editor and a terminal embedded in one window.
//!
//! Usage: `embed <empty|editor|terminal|both> <file-to-open> <results.json>`
//!
//! Opens one window with the chosen parts. The editor loads the given file;
//! the terminal runs a command that floods it with two million lines
//! (`HARLO_FLOOD_LINES` changes that).
//! After a short warm-up it records frame intervals for ten seconds, writes
//! them to `results.json` and exits. Peak memory comes from running it under
//! `tools/peak-rss.sh`.

// Rust guideline compliant 2026-10-09

use std::time::{Duration, Instant};

use iced::widget::{container, row, rule, space, text};
use iced::{Element, Fill, Subscription, Task, Theme, time, window};
use iced_code_editor::{CodeEditor, Message as EditorMessage};

/// Time before measuring, so the first layout and the editor's highlighting settle.
const WARMUP: Duration = Duration::from_secs(3);

/// Length of the measured run.
const MEASURE: Duration = Duration::from_secs(10);

/// Lines the terminal prints unless `HARLO_FLOOD_LINES` says otherwise.
const FLOOD_LINES: u64 = 2_000_000;

/// What the terminal runs: a fast flood of `{lines}` lines, then it stays open.
fn flood() -> String {
    let lines = std::env::var("HARLO_FLOOD_LINES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(FLOOD_LINES);
    format!(
        "seq -f 'line %g: the quick brown fox jumps over the lazy dog, again and again' 1 {lines}; sleep 600"
    )
}

#[derive(Debug, Clone)]
enum Message {
    Frame(Instant),
    Editor(EditorMessage),
    Terminal(iced_term::Event),
    Blink,
}

struct Embed {
    label: String,
    out: String,
    editor: Option<CodeEditor>,
    terminal: Option<iced_term::Terminal>,
    started: Option<Instant>,
    last_frame: Option<Instant>,
    frames: Vec<f64>,
}

impl Embed {
    fn new(label: String, file: &str, out: String) -> Self {
        let wants = |part: &str| label == part || label == "both";
        let editor = wants("editor").then(|| {
            let code = std::fs::read_to_string(file)
                .unwrap_or_else(|e| format!("// could not read {file}: {e}\n"));
            CodeEditor::new(&code, "rust")
        });
        let terminal = wants("terminal").then(|| {
            let settings = iced_term::settings::Settings {
                backend: iced_term::settings::BackendSettings {
                    program: "/bin/sh".into(),
                    args: vec!["-c".into(), flood()],
                    ..Default::default()
                },
                ..Default::default()
            };
            iced_term::Terminal::new(0, settings)
        });
        let terminal = match terminal {
            Some(Ok(t)) => Some(t),
            Some(Err(e)) => {
                eprintln!("could not start the terminal: {e}");
                None
            }
            None => None,
        };
        Self {
            label,
            out,
            editor,
            terminal,
            started: None,
            last_frame: None,
            frames: Vec::new(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Frame(at) => {
                let started = *self.started.get_or_insert(at);
                let elapsed = at.duration_since(started);
                if elapsed >= WARMUP
                    && let Some(last) = self.last_frame
                {
                    self.frames
                        .push(at.duration_since(last).as_secs_f64() * 1000.0);
                }
                self.last_frame = Some(at);
                if elapsed >= WARMUP + MEASURE {
                    self.write_results();
                    return iced::exit();
                }
                Task::none()
            }
            Message::Editor(event) => self
                .editor
                .as_mut()
                .map_or_else(Task::none, |e| e.update(&event).map(Message::Editor)),
            Message::Blink => self.editor.as_mut().map_or_else(Task::none, |e| {
                e.update(&EditorMessage::Tick).map(Message::Editor)
            }),
            Message::Terminal(iced_term::Event::BackendCall(_, cmd)) => {
                if let Some(term) = &mut self.terminal {
                    // A shutdown here means the flood command exited early; the run still measures.
                    let _action = term.handle(iced_term::Command::ProxyToBackend(cmd));
                }
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let editor: Element<'_, Message> = match &self.editor {
            Some(editor) => editor.view().map(Message::Editor),
            None => space().into(),
        };
        let terminal: Element<'_, Message> = match &self.terminal {
            Some(term) => iced_term::TerminalView::show(term).map(Message::Terminal),
            None => space().into(),
        };
        let body: Element<'_, Message> = match (&self.editor, &self.terminal) {
            (None, None) => container(text("empty window")).center(Fill).into(),
            (Some(_), None) => editor,
            (None, Some(_)) => terminal,
            (Some(_), Some(_)) => row![
                container(editor).width(Fill),
                rule::vertical(1),
                container(terminal).width(Fill)
            ]
            .into(),
        };
        container(body).width(Fill).height(Fill).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![window::frames().map(Message::Frame)];
        if self.editor.is_some() {
            subs.push(time::every(Duration::from_millis(500)).map(|_| Message::Blink));
        }
        if let Some(term) = &self.terminal {
            subs.push(term.subscription().map(Message::Terminal));
        }
        Subscription::batch(subs)
    }

    fn write_results(&self) {
        let mut v = self.frames.clone();
        v.sort_by(f64::total_cmp);
        let pick = |q: f64| {
            if v.is_empty() {
                return 0.0;
            }
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
        let out = format!(
            "{{\"mode\":\"{}\",\"frames\":{},\"p50\":{:.2},\"p95\":{:.2},\"p99\":{:.2},\"max\":{:.2},\"over_16ms\":{},\"over_33ms\":{}}}",
            self.label,
            v.len(),
            pick(0.5),
            pick(0.95),
            pick(0.99),
            pick(1.0),
            over(16.7),
            over(33.4)
        );
        if let Err(e) = std::fs::write(&self.out, out) {
            eprintln!("could not write results: {e}");
        }
    }
}

fn main() -> iced::Result {
    let mut args = std::env::args().skip(1);
    let label = args.next().unwrap_or_else(|| "both".into());
    let file = args.next().unwrap_or_default();
    let out = args.next().unwrap_or_else(|| "embed.json".into());
    iced::application(
        move || Embed::new(label.clone(), &file, out.clone()),
        Embed::update,
        Embed::view,
    )
    .title("Harlo embed spike")
    .subscription(Embed::subscription)
    .theme(|_: &Embed| Theme::Dark)
    .window_size((1280.0, 900.0))
    .run()
}
