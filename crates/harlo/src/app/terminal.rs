//! Terminals in the dock: shells that open below the conversation.
//!
//! The rail's terminal button shows the terminals, starting one in the
//! current session's folder when there are none, and hides them again. Hidden
//! shells keep running; closing a terminal's tab ends its shell, as does
//! the shell exiting by itself.
//!
//! On a server on this computer the shell runs here. On a server elsewhere
//! the server runs it, and the terminal connects to it; closing the tab
//! removes it from the server.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::path::PathBuf;

use futures_util::StreamExt;
use harlo_core::server::Pty;
use harlo_core::terminal::{self as remote, Input, Update};
use harlo_core::theme::{Rgba, Tokens};
use iced::{Subscription, Task};
use iced_term::settings::{BackendSettings, FontSettings, Settings, ThemeSettings};
use iced_term::{BackendCommand, ColorPalette, Command, RemoteInput};
use tokio::sync::mpsc;

use super::{App, DockKey, Message};
use crate::style::{self, size};

/// The shell started when `$SHELL` isn't set.
const FALLBACK_SHELL: &str = "/bin/sh";

/// Something that happened to the terminals.
#[derive(Debug, Clone)]
pub enum Event {
    /// Show the terminals, or hide them if they're shown.
    Toggle,
    /// Start another terminal beside the others.
    New,
    /// A terminal's screen changed or it asked for something.
    Backend(iced_term::Event),
    /// The server started terminal `id`'s shell, in the project `directory`,
    /// or couldn't.
    Started(u32, String, Result<Pty, String>),
    /// Something happened to terminal `id`'s shell on the server.
    Remote(u32, Update),
}

/// One terminal and the shell in it.
pub struct Shell {
    /// The terminal's number, which its dock tab refers to.
    pub id: u32,
    /// The screen and the shell behind it.
    pub term: iced_term::Terminal,
    /// The title the shell gave itself, or its folder's name.
    pub title: String,
    /// The folder it started in.
    pub folder: String,
    /// The shell on the server, when it runs there.
    remote: Option<Remote>,
}

impl Shell {
    /// The server's ID for the shell, once a shell on the server started.
    #[must_use]
    pub fn pty(&self) -> Option<&str> {
        self.remote.as_ref()?.pty.as_deref()
    }
}

impl std::fmt::Debug for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shell")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("folder", &self.folder)
            .field("remote", &self.remote)
            .finish_non_exhaustive()
    }
}

/// A shell the server runs.
#[derive(Debug)]
struct Remote {
    /// The project folder the server keeps it under.
    directory: String,
    /// The server's ID for it, once started.
    pty: Option<String>,
    /// What the terminal sends it, until the connection takes it over.
    input: Option<mpsc::UnboundedReceiver<RemoteInput>>,
    /// The connection; dropping it closes the connection.
    connection: Option<iced::task::Handle>,
}

/// The open terminals, in the order they were started.
#[derive(Debug, Default)]
pub struct Terminals {
    shells: Vec<Shell>,
    /// The number the next terminal gets; numbers aren't reused.
    next: u32,
    /// The shell to start instead of `$SHELL`, and the folder to start it in
    /// when no session is open, instead of the home folder.
    instead: Option<(String, String)>,
    /// Whether shells run on the server even when it's on this computer.
    on_server: bool,
}

impl Terminals {
    /// The open terminals.
    #[must_use]
    pub fn shells(&self) -> &[Shell] {
        &self.shells
    }

    /// The terminal with number `id`.
    #[must_use]
    pub fn get(&self, id: u32) -> Option<&Shell> {
        self.shells.iter().find(|s| s.id == id)
    }

    fn get_mut(&mut self, id: u32) -> Option<&mut Shell> {
        self.shells.iter_mut().find(|s| s.id == id)
    }

    /// Ends a terminal's shell.
    fn remove(&mut self, id: u32) {
        self.shells.retain(|s| s.id != id);
    }

    /// Each terminal's events: its screen changing, its title, its exit.
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(self.shells.iter().map(|s| {
            s.term
                .subscription()
                .map(|e| Message::Terminal(Event::Backend(e)))
        }))
    }
}

/// The settings for `program` in `folder`, drawn in the theme's colours.
fn settings(program: String, folder: &str, tokens: &Tokens) -> Settings {
    let env = HashMap::from([
        ("TERM".to_owned(), "xterm-256color".to_owned()),
        ("COLORTERM".to_owned(), "truecolor".to_owned()),
    ]);
    Settings {
        font: FontSettings {
            size: size::CODE,
            scale_factor: 1.3,
            font_type: style::MONO,
        },
        theme: ThemeSettings::new(Box::new(palette(tokens))),
        backend: BackendSettings {
            program,
            args: Vec::new(),
            env,
            working_directory: Some(PathBuf::from(folder)),
        },
    }
}

/// The terminal's colours: text and background from the theme, the four
/// status colours for red, green, yellow and blue, and the widget's own
/// choices for the rest.
#[must_use]
pub fn palette(t: &Tokens) -> ColorPalette {
    let back = t.background_base;
    let hex = |c: Rgba| hex_over(c, back);
    ColorPalette {
        foreground: hex(t.text_base),
        background: hex(back),
        red: hex(t.icon_critical_base),
        green: hex(t.icon_success_base),
        yellow: hex(t.icon_warning_base),
        blue: hex(t.icon_info_base),
        bright_black: hex(t.text_weaker),
        dim_foreground: hex(t.text_weak),
        ..ColorPalette::default()
    }
}

/// `color` laid over `back`, as `#rrggbb`.
fn hex_over(color: Rgba, back: Rgba) -> String {
    let channel = |front: f32, under: f32| {
        let mixed = front * color.a + under * (1.0 - color.a);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a channel clamped to 0..=1 scales into a byte"
        )]
        let byte = (mixed.clamp(0.0, 1.0) * 255.0).round() as u8;
        byte
    };
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(color.r, back.r),
        channel(color.g, back.g),
        channel(color.b, back.b),
    )
}

/// A folder's last part, for a terminal's first title.
fn folder_name(folder: &str) -> String {
    std::path::Path::new(folder)
        .file_name()
        .map_or_else(|| folder.to_owned(), |n| n.to_string_lossy().into_owned())
}

impl App {
    /// Applies something that happened to the terminals.
    pub(super) fn terminal_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Toggle => self.toggle_terminals(),
            Event::New => self.new_terminal(),
            Event::Backend(iced_term::Event::BackendCall(id, call)) => match u32::try_from(id) {
                Ok(id) => self.terminal_call(id, call),
                Err(_) => Task::none(),
            },
            Event::Started(id, directory, started) => self.started(id, directory, started),
            Event::Remote(id, Update::Output(bytes)) => {
                self.terminal_call(id, BackendCommand::Feed(bytes))
            }
            Event::Remote(id, Update::Ended(reason)) => {
                if let Some(reason) = reason {
                    self.send_error = Some(format!("The terminal's connection ended: {reason}"));
                }
                self.end_terminal(id)
            }
        }
    }

    /// Passes `call` to terminal `id`, and acts on what it asks for.
    fn terminal_call(&mut self, id: u32, call: BackendCommand) -> Task<Message> {
        let Some(shell) = self.terminals.get_mut(id) else {
            return Task::none();
        };
        match shell.term.handle(Command::ProxyToBackend(call)) {
            iced_term::actions::Action::Shutdown => return self.end_terminal(id),
            iced_term::actions::Action::ChangeTitle(title) => {
                if !title.trim().is_empty() {
                    shell.title = title;
                }
            }
            iced_term::actions::Action::Ignore => {}
        }
        Task::none()
    }

    /// Connects terminal `id` to the shell the server started for it; removes
    /// the shell when the terminal closed in the meantime.
    fn started(
        &mut self,
        id: u32,
        directory: String,
        started: Result<Pty, String>,
    ) -> Task<Message> {
        let pty = match started {
            Ok(pty) => pty,
            Err(error) => {
                self.send_error = Some(format!("Couldn't start a terminal: {error}"));
                return self.end_terminal(id);
            }
        };
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        let Some(state) = self
            .terminals
            .get_mut(id)
            .and_then(|shell| shell.remote.as_mut())
        else {
            return remove(client, directory, pty.id);
        };
        let Some(typed) = state.input.take() else {
            return Task::none();
        };
        let input = futures_util::stream::unfold(typed, |mut typed| async move {
            let item = match typed.recv().await? {
                RemoteInput::Write(bytes) => Input::Write(bytes),
                RemoteInput::Resize { columns, lines } => Input::Resize { columns, lines },
            };
            Some((item, typed))
        });
        let updates = remote::follow(client, directory, pty.id.clone(), input);
        let (task, handle) = Task::run(updates.boxed(), move |update| {
            Message::Terminal(Event::Remote(id, update))
        })
        .abortable();
        state.pty = Some(pty.id);
        state.connection = Some(handle.abort_on_drop());
        task
    }

    /// Starts `program` in terminals instead of the user's shell, in
    /// `folder` when no session is open; for tests, which must not run the
    /// user's shell or its startup files.
    #[must_use]
    pub fn terminal_shell(mut self, program: impl Into<String>, folder: impl Into<String>) -> Self {
        self.terminals.instead = Some((program.into(), folder.into()));
        self
    }

    /// Runs terminals' shells on the server even when it's on this
    /// computer, as for a server elsewhere; for tests.
    #[must_use]
    pub fn terminals_on_server(mut self) -> Self {
        self.terminals.on_server = true;
        self
    }

    /// Whether any terminal is on screen.
    #[must_use]
    pub fn terminals_shown(&self) -> bool {
        self.terminals
            .shells
            .iter()
            .any(|s| self.dock.is_shown(DockKey::Terminal(s.id)))
    }

    /// Shows the terminals, starting one if there are none, or hides them
    /// if any is shown. Hidden shells keep running.
    fn toggle_terminals(&mut self) -> Task<Message> {
        if self.terminals_shown() {
            for id in self
                .terminals
                .shells
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>()
            {
                self.dock.close(DockKey::Terminal(id));
            }
            return Task::none();
        }
        let Some(last) = self.terminals.shells.last().map(|s| s.id) else {
            return self.new_terminal();
        };
        let mut beside = None;
        for id in self
            .terminals
            .shells
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>()
        {
            let key = DockKey::Terminal(id);
            if !self.dock.has(key) {
                self.dock.open_terminal(key, beside);
            }
            beside = Some(key);
        }
        self.focus_terminal(last)
    }

    /// Starts a shell in the current session's folder (or the home folder)
    /// in a new terminal beside the others: here, or on the server when it
    /// runs elsewhere.
    fn new_terminal(&mut self) -> Task<Message> {
        if self.terminals.on_server || !self.location.is_local() {
            return self.new_remote_terminal();
        }
        let (program, home) = match &self.terminals.instead {
            Some((program, folder)) => (program.clone(), Some(folder.clone())),
            None => (
                std::env::var("SHELL")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| FALLBACK_SHELL.to_owned()),
                std::env::home_dir().map(|h| h.to_string_lossy().into_owned()),
            ),
        };
        let folder = self
            .active
            .as_deref()
            .and_then(|id| self.session_folder(id))
            .or(home)
            .unwrap_or_else(|| "/".to_owned());
        let id = self.terminals.next;
        self.terminals.next += 1;
        let settings = settings(program, &folder, &self.tokens);
        let term = match iced_term::Terminal::new(u64::from(id), settings) {
            Ok(term) => term,
            Err(error) => {
                self.send_error = Some(format!("Couldn't start a terminal: {error}"));
                return Task::none();
            }
        };
        let beside = self.last_docked_terminal();
        self.terminals.shells.push(Shell {
            id,
            term,
            title: folder_name(&folder),
            folder,
            remote: None,
        });
        self.dock.open_terminal(DockKey::Terminal(id), beside);
        self.focus_terminal(id)
    }

    /// Asks the server for a shell in the current folder, and opens its
    /// terminal, which connects once the shell has started.
    fn new_remote_terminal(&mut self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            self.send_error = Some("Couldn't start a terminal: not connected.".to_owned());
            return Task::none();
        };
        let directory = self.folder();
        let program = self.terminals.instead.as_ref().map(|(p, _)| p.clone());
        let id = self.terminals.next;
        self.terminals.next += 1;
        let (sender, input) = mpsc::unbounded_channel();
        let settings = settings(String::new(), &directory, &self.tokens);
        let term = iced_term::Terminal::remote(u64::from(id), settings, sender);
        let beside = self.last_docked_terminal();
        self.terminals.shells.push(Shell {
            id,
            term,
            title: folder_name(&directory),
            folder: directory.clone(),
            remote: Some(Remote {
                directory: directory.clone(),
                pty: None,
                input: Some(input),
                connection: None,
            }),
        });
        self.dock.open_terminal(DockKey::Terminal(id), beside);
        let start = Task::perform(
            async move {
                let started = client
                    .create_pty(&directory, &directory, program.as_deref())
                    .await
                    .map_err(|e| e.to_string());
                (directory, started)
            },
            move |(directory, started)| Message::Terminal(Event::Started(id, directory, started)),
        );
        Task::batch([start, self.focus_terminal(id)])
    }

    /// The newest terminal that has a tab, to open the next one beside.
    fn last_docked_terminal(&self) -> Option<DockKey> {
        self.terminals
            .shells
            .iter()
            .rev()
            .map(|s| DockKey::Terminal(s.id))
            .find(|&key| self.dock.has(key))
    }

    /// Ends a terminal: its tab closes and its shell stops, here or on the
    /// server.
    pub(super) fn end_terminal(&mut self, id: u32) -> Task<Message> {
        self.dock.close(DockKey::Terminal(id));
        let ended = self.terminals.get(id).and_then(|shell| {
            let state = shell.remote.as_ref()?;
            Some((state.directory.clone(), state.pty.clone()?))
        });
        self.terminals.remove(id);
        match (ended, self.client.clone()) {
            (Some((directory, pty)), Some(client)) => remove(client, directory, pty),
            _ => Task::none(),
        }
    }

    /// Shows a terminal and puts the keyboard in it.
    fn focus_terminal(&self, id: u32) -> Task<Message> {
        self.dock.select(DockKey::Terminal(id));
        match self.terminals.get(id) {
            Some(shell) => iced::widget::operation::focus(shell.term.widget_id().clone()),
            None => Task::none(),
        }
    }
}

/// Removes a shell from the server. One that's already gone is fine.
fn remove(client: harlo_core::server::Client, directory: String, pty: String) -> Task<Message> {
    Task::future(async move {
        let _ = client.remove_pty(&directory, &pty).await;
    })
    .discard()
}
