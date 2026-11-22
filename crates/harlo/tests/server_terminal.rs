//! Terminals whose shells run on the server, as for a server on another
//! computer: the terminal connects once the shell has started, shows what it
//! prints, and closing the tab removes the shell from the server.
//!
//! Runs against a private `opencode serve` with scratch `HOME`/`XDG_*`
//! folders, never the shared service. The app's tasks run here, feeding their
//! messages back into it as the runtime would.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
#[expect(
    dead_code,
    reason = "this file uses only the demo data of the shared helpers"
)]
mod support;

use std::sync::Arc;

use futures_util::StreamExt;
use futures_util::stream::SelectAll;
use harlo::{App, DockKey, Message, TerminalEvent};
use harlo_core::server::{Connected, Location, Update};
use iced::Task;
use iced_term::BackendCommand;
use iced_test::runtime::{Action, task};
use server::{Sandbox, WAIT};
use support::demo;

/// Runs the app's tasks, feeding their messages back, until `done` holds.
struct Driver {
    app: App,
    running: SelectAll<futures_util::stream::BoxStream<'static, Action<Message>>>,
}

impl Driver {
    /// The demo app connected to `sandbox`, with shells on the server.
    fn new(sandbox: &Sandbox) -> Self {
        let project = sandbox.project_str();
        let mut app = App::new(Location::Shared, vec![Location::Shared], demo::layout())
            .terminal_shell("/bin/sh", project.clone())
            .terminals_on_server();
        let _ = app.update(Message::Tick(demo::now()));
        let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
            client: sandbox.client(),
            version: harlo_core::OPENCODE_VERSION.to_owned(),
            snapshot: demo::snapshot(),
        }))));
        app.new_folder = Some(project);
        Self {
            app,
            running: SelectAll::new(),
        }
    }

    /// Sends `message` and keeps what it starts running.
    fn send(&mut self, message: Message) {
        let started = self.app.update(message);
        self.start(started);
    }

    fn start(&mut self, started: Task<Message>) {
        if let Some(stream) = task::into_stream(started) {
            self.running.push(stream);
        }
    }

    /// Types `text` into terminal `id`.
    fn type_into(&mut self, id: u64, text: &str) {
        let call =
            iced_term::Event::BackendCall(id, BackendCommand::Write(text.as_bytes().to_vec()));
        self.send(Message::Terminal(TerminalEvent::Backend(call)));
    }

    /// Runs the tasks until `done` holds, or they all finish, or time's up.
    async fn until(&mut self, done: impl Fn(&App) -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + WAIT;
        while !done(&self.app) {
            let next = tokio::time::timeout_at(deadline, self.running.next()).await;
            match next {
                Ok(Some(Action::Output(message))) => self.send(message),
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => return false,
            }
        }
        true
    }
}

/// Whether terminal `id` shows a line with `text`.
fn shows(app: &App, id: u32, text: &str) -> bool {
    app.terminals
        .get(id)
        .is_some_and(|shell| shell.term.text().iter().any(|line| line.contains(text)))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_shell_answers_and_closing_its_tab_removes_it() {
    let sandbox = Sandbox::start().await;
    let project = sandbox.project_str();
    let mut driver = Driver::new(&sandbox);

    driver.send(Message::Terminal(TerminalEvent::Toggle));
    assert!(driver.app.dock.keys().contains(&DockKey::Terminal(0)));
    // Typed before the shell has started; it arrives once connected.
    driver.type_into(0, "echo $((6 * 7)); pwd\n");
    assert!(driver.until(|app| shows(app, 0, "42")).await, "it answers");
    assert!(driver.until(|app| shows(app, 0, &project)).await);
    let pty = driver
        .app
        .terminals
        .get(0)
        .and_then(|s| s.pty())
        .map(str::to_owned);
    let pty = pty.expect("the server's shell");

    driver.send(Message::CloseDocked(DockKey::Terminal(0)));
    assert!(driver.app.terminals.get(0).is_none());
    let _ = driver.until(|_| false).await;
    let client = sandbox.client();
    assert!(
        client.connect_pty(&project, &pty).await.is_err(),
        "the shell is gone from the server"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_shell_that_exits_closes_its_tab() {
    let sandbox = Sandbox::start().await;
    let mut driver = Driver::new(&sandbox);
    driver.send(Message::Terminal(TerminalEvent::Toggle));
    driver.type_into(0, "exit\n");
    let closed = driver.until(|app| app.terminals.get(0).is_none()).await;
    assert!(closed, "the tab closes with the shell");
    assert!(!driver.app.dock.keys().contains(&DockKey::Terminal(0)));
    assert_eq!(driver.app.send_error, None, "an exit isn't an error");
}
