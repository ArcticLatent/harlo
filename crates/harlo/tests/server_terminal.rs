//! Terminals whose shells run on the server, as for a server on another
//! computer: the terminal connects once the shell has started, shows what it
//! prints, and closing the tab removes the shell from the server.
//!
//! Runs against a private `opencode serve` with scratch `HOME`/`XDG_*`
//! folders, never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;

use harlo::app::{Message, TerminalEvent};
use harlo_core::server::{Connected, Update};
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// The window connected to `sandbox`, with shells on the server.
fn window(sandbox: &Sandbox) -> Ui {
    let project = sandbox.project_str();
    let mut app = demo::new_app()
        .terminal_shell("/bin/sh", project.clone())
        .terminals_on_server();
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        version: harlo_core::OPENCODE_VERSION.to_owned(),
        snapshot: demo::snapshot(),
    }))));
    app.new_folder = Some(project);
    ui(app)
}

/// Whether terminal `id` shows a line with `text`.
fn shows(ui: &Ui, id: u32, text: &str) -> bool {
    ui.app()
        .terminals
        .get(id)
        .is_some_and(|shell| shell.screen.text().iter().any(|line| line.contains(text)))
}

/// Types `text` into the shown terminal.
fn type_text(ui: &Ui, text: &str) {
    ui.send(Message::Terminal(TerminalEvent::Typed(
        text.as_bytes().to_vec(),
    )));
}

#[test]
fn a_server_shell_answers_and_closing_its_tab_removes_it() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let project = sandbox.project_str();
    let ui = window(&sandbox);

    ui.open("Terminal");
    assert!(ui.app().terminals_shown());
    // Typed before the shell has started; it arrives once connected.
    type_text(&ui, "echo $((6 * 7)); pwd\n");
    assert!(
        tasks.until(&ui, WAIT, |ui| shows(ui, 0, "42")),
        "it answers"
    );
    assert!(tasks.until(&ui, WAIT, |ui| shows(ui, 0, &project)));
    assert!(ui.shows("42"), "the window draws it");
    let pty = ui
        .app()
        .terminals
        .get(0)
        .and_then(|s| s.pty())
        .map(str::to_owned)
        .expect("the server's shell");

    ui.send(Message::Terminal(TerminalEvent::Close(0)));
    assert!(ui.app().terminals.get(0).is_none());
    // Lets the removal reach the server.
    let _ = tasks.until(&ui, std::time::Duration::from_secs(2), |_| false);
    let client = sandbox.client();
    assert!(
        tasks.block_on(client.connect_pty(&project, &pty)).is_err(),
        "the shell is gone from the server"
    );
}

#[test]
fn a_server_shell_that_exits_closes_its_tab() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let ui = window(&sandbox);
    ui.open("Terminal");
    type_text(&ui, "exit\n");
    let closed = tasks.until(&ui, WAIT, |ui| ui.app().terminals.get(0).is_none());
    assert!(closed, "the tab closes with the shell");
    assert!(!ui.app().terminals_shown());
    assert_eq!(ui.app().send_error, None, "an exit isn't an error");
}
