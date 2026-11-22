//! The terminals, in a strip under the center: shells opened in the current
//! session's folder.
//!
//! The rail's terminal button shows the terminals, starting one when there
//! are none, and hides them again. Hidden shells keep running; closing a
//! terminal's tab ends its shell, as does the shell exiting by itself.
//!
//! On a server on this computer the shell runs here. On a server elsewhere
//! the server runs it and the terminal connects to it; closing the tab
//! removes it from the server.

// Rust guideline compliant 2026-10-09

use futures_util::StreamExt as _;
use harlo_core::server::{Client, Pty};
use harlo_core::terminal::{self as remote, Input, Update};
use tokio::sync::mpsc::UnboundedReceiver;

use super::{App, Message};
use crate::task::{Effect, Handle, Task};
use crate::term::{Screen, Signal};

/// The shell started when `$SHELL` isn't set.
const FALLBACK_SHELL: &str = "/bin/sh";

/// The terminal view's name, for focus requests.
pub const VIEW: &str = "terminal";

/// Something that happened to the terminals.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Show the terminals, starting one if there's none, or hide them.
    Toggle,
    /// Start another terminal and show it.
    New,
    /// Show terminal `id` in the strip.
    Select(u32),
    /// End terminal `id`.
    Close(u32),
    /// Terminal `id`'s screen or shell reported something.
    Signal(u32, Signal),
    /// The server started terminal `id`'s shell in the project `directory`,
    /// or couldn't.
    Started(u32, String, Result<Pty, String>),
    /// Something happened to terminal `id`'s shell on the server.
    Remote(u32, Update),
    /// The user typed these bytes into the shown terminal.
    Typed(Vec<u8>),
    /// The user pasted this text into the shown terminal.
    Paste(String),
    /// The strip has room for this many columns and lines.
    Resized(usize, usize),
    /// The user scrolled the shown terminal by this many lines, up when
    /// positive.
    Scroll(i32),
    /// The user pressed at a visible line and fractional column of the
    /// shown terminal; by word when it was a double click.
    SelectFrom {
        /// The visible line.
        line: usize,
        /// The column, with where in the cell.
        column: f32,
        /// Whether to select whole words.
        words: bool,
    },
    /// The user dragged to a visible line and fractional column.
    SelectTo(usize, f32),
}

/// One terminal and the shell in it.
#[derive(Debug)]
pub struct Shell {
    /// The terminal's number, which its tab refers to.
    pub id: u32,
    /// The title the shell gave itself, or its folder's name.
    pub title: String,
    /// The folder it started in.
    pub folder: String,
    /// The screen and the shell behind it.
    pub screen: Screen,
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

/// A shell the server runs.
#[derive(Debug)]
struct Remote {
    /// The project folder the server keeps it under.
    directory: String,
    /// The server's ID for it, once started.
    pty: Option<String>,
    /// What the user typed, until the connection takes it over.
    input: Option<UnboundedReceiver<Input>>,
    /// The connection; dropping it closes the connection.
    connection: Option<Handle>,
}

/// The open terminals, in the order they were started.
#[derive(Debug, Default)]
pub struct Terminals {
    shells: Vec<Shell>,
    /// The number the next terminal gets; numbers aren't reused.
    next: u32,
    /// The program to start instead of `$SHELL`, and the folder to start it
    /// in when no session is open, instead of the home folder.
    instead: Option<(String, String)>,
    /// Whether shells run on the server even when it's on this computer.
    on_server: bool,
    /// Whether the strip is shown.
    shown: bool,
    /// The terminal the strip shows.
    active: Option<u32>,
    /// The columns and lines the strip has room for, once known.
    grid: Option<(usize, usize)>,
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

    /// The terminal the strip shows.
    #[must_use]
    pub fn active(&self) -> Option<&Shell> {
        self.active.and_then(|id| self.get(id))
    }

    fn active_mut(&mut self) -> Option<&mut Shell> {
        let id = self.active?;
        self.get_mut(id)
    }

    /// Adds a terminal, sized to the strip, and shows it.
    fn add(&mut self, mut shell: Shell) {
        if let Some((columns, lines)) = self.grid {
            shell.screen.resize(columns, lines);
        }
        self.active = Some(shell.id);
        self.shells.push(shell);
        self.shown = true;
    }
}

/// A folder's last part, for a terminal's first title.
fn folder_name(folder: &str) -> String {
    std::path::Path::new(folder)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(folder)
        .to_owned()
}

/// Turns terminal `id`'s signals into messages.
fn signals(id: u32, receiver: UnboundedReceiver<Signal>) -> Task<Message> {
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
        let signal = receiver.recv().await?;
        Some((signal, receiver))
    });
    Task::stream(stream.map(move |signal| Message::Terminal(Event::Signal(id, signal))))
}

impl App {
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

    /// Whether the terminals are on screen.
    #[must_use]
    pub fn terminals_shown(&self) -> bool {
        self.terminals.shown && !self.terminals.shells.is_empty() && self.maximized.is_none()
    }

    /// Applies something that happened to the terminals.
    pub(super) fn terminal_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Toggle if self.maximized.is_some() => {
                // The terminals were under the maximized panel: bring them up.
                self.maximized = None;
                self.terminals.shown = true;
                if self.terminals.shells.is_empty() {
                    return self.new_terminal();
                }
                return Task::effect(Effect::Focus(VIEW));
            }
            Event::Toggle if self.terminals.shells.is_empty() => return self.new_terminal(),
            Event::Toggle => {
                self.terminals.shown = !self.terminals.shown;
                if self.terminals.shown {
                    return Task::effect(Effect::Focus(VIEW));
                }
            }
            Event::New => return self.new_terminal(),
            Event::Select(id) if self.terminals.get(id).is_some() => {
                self.terminals.active = Some(id);
                return Task::effect(Effect::Focus(VIEW));
            }
            Event::Select(_) => {}
            Event::Close(id) => return self.end_terminal(id),
            Event::Signal(id, signal) => return self.terminal_signal(id, signal),
            Event::Started(id, directory, started) => return self.started(id, directory, started),
            Event::Remote(id, Update::Output(bytes)) => {
                if let Some(shell) = self.terminals.get_mut(id) {
                    shell.screen.feed(&bytes);
                }
            }
            Event::Remote(id, Update::Ended(reason)) => {
                if let Some(reason) = reason {
                    self.send_error = Some(format!("The terminal's connection ended: {reason}"));
                }
                return self.end_terminal(id);
            }
            Event::Typed(bytes) => {
                if let Some(shell) = self.terminals.active_mut() {
                    shell.screen.typed(bytes);
                }
            }
            Event::Paste(text) => {
                if let Some(shell) = self.terminals.active_mut() {
                    shell.screen.paste(&text);
                }
            }
            Event::Resized(columns, lines) => {
                self.terminals.grid = Some((columns, lines));
                for shell in &mut self.terminals.shells {
                    shell.screen.resize(columns, lines);
                }
            }
            Event::Scroll(lines) => {
                if let Some(shell) = self.terminals.active_mut() {
                    shell.screen.scroll(lines);
                }
            }
            Event::SelectFrom {
                line,
                column,
                words,
            } => {
                if let Some(shell) = self.terminals.active_mut() {
                    shell.screen.select_from(line, column, words);
                }
            }
            Event::SelectTo(line, column) => {
                if let Some(shell) = self.terminals.active_mut() {
                    shell.screen.select_to(line, column);
                }
            }
        }
        Task::none()
    }

    /// Acts on what terminal `id` reported.
    fn terminal_signal(&mut self, id: u32, signal: Signal) -> Task<Message> {
        let Some(shell) = self.terminals.get_mut(id) else {
            return Task::none();
        };
        match signal {
            // The window shows the change when it shows the app next.
            Signal::Changed => {}
            Signal::Title(title) if title.trim().is_empty() => {
                shell.title = folder_name(&shell.folder);
            }
            Signal::Title(title) => shell.title = title,
            Signal::Reply(text) => shell.screen.write(text.into_bytes()),
            Signal::Exited => return self.end_terminal(id),
        }
        Task::none()
    }

    /// The folder a new terminal starts in: the open session's, else the
    /// test folder or the home folder.
    fn terminal_folder(&self) -> String {
        self.active
            .as_deref()
            .and_then(|s| self.session_folder(s))
            .or_else(|| self.terminals.instead.as_ref().map(|(_, f)| f.clone()))
            .unwrap_or_else(super::home_folder)
    }

    /// Starts a shell in a new terminal and shows it: here, or on the
    /// server when it runs elsewhere.
    fn new_terminal(&mut self) -> Task<Message> {
        if self.terminals.on_server || !self.location.is_local() {
            return self.new_remote_terminal();
        }
        let program = match &self.terminals.instead {
            Some((program, _)) => program.clone(),
            None => std::env::var("SHELL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| FALLBACK_SHELL.to_owned()),
        };
        let folder = self.terminal_folder();
        let id = self.terminals.next;
        let (screen, receiver) = match Screen::local(&program, &folder, id) {
            Ok(started) => started,
            Err(error) => {
                self.send_error = Some(format!("Couldn't start a terminal: {error}"));
                return Task::none();
            }
        };
        self.terminals.next += 1;
        self.terminals.add(Shell {
            id,
            title: folder_name(&folder),
            folder,
            screen,
            remote: None,
        });
        Task::batch([signals(id, receiver), Task::effect(Effect::Focus(VIEW))])
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
        let (screen, receiver, input) = Screen::remote();
        self.terminals.add(Shell {
            id,
            title: folder_name(&directory),
            folder: directory.clone(),
            screen,
            remote: Some(Remote {
                directory: directory.clone(),
                pty: None,
                input: Some(input),
                connection: None,
            }),
        });
        let start = Task::future(async move {
            let started = client
                .create_pty(&directory, &directory, program.as_deref())
                .await
                .map_err(|e| e.to_string());
            Message::Terminal(Event::Started(id, directory, started))
        });
        Task::batch([
            start,
            signals(id, receiver),
            Task::effect(Effect::Focus(VIEW)),
        ])
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
            let item = typed.recv().await?;
            Some((item, typed))
        });
        let updates = remote::follow(client, directory, pty.id.clone(), input);
        let (task, handle) =
            Task::stream(updates.map(move |update| Message::Terminal(Event::Remote(id, update))))
                .abortable();
        state.pty = Some(pty.id);
        state.connection = Some(handle.abort_on_drop());
        task
    }

    /// Ends a terminal: its tab closes and its shell stops, here or on the
    /// server.
    fn end_terminal(&mut self, id: u32) -> Task<Message> {
        let Some(index) = self.terminals.shells.iter().position(|s| s.id == id) else {
            return Task::none();
        };
        let shell = self.terminals.shells.remove(index);
        if self.terminals.active == Some(id) {
            // The neighbour takes its place.
            let next = index.min(self.terminals.shells.len().saturating_sub(1));
            self.terminals.active = self.terminals.shells.get(next).map(|s| s.id);
        }
        if self.terminals.shells.is_empty() {
            self.terminals.shown = false;
        }
        let ended = shell
            .remote
            .as_ref()
            .and_then(|state| Some((state.directory.clone(), state.pty.clone()?)));
        match (ended, self.client.clone()) {
            (Some((directory, pty)), Some(client)) => remove(client, directory, pty),
            _ => Task::none(),
        }
    }
}

/// Removes a shell from the server. One that's already gone is fine.
fn remove(client: Client, directory: String, pty: String) -> Task<Message> {
    Task::future(async move {
        let _ = client.remove_pty(&directory, &pty).await;
    })
    .discard()
}
