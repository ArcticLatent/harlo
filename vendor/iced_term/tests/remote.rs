//! Harlo: a remote terminal shows what it's fed, and sends what's typed and
//! its size toward the program.

use iced::Size;
use iced_term::settings::Settings;
use iced_term::{BackendCommand, Command, RemoteInput, Terminal};
use tokio::sync::mpsc;

#[test]
fn a_remote_terminal_shows_output_and_sends_input() {
    let (input, mut sent) = mpsc::unbounded_channel();
    let mut term = Terminal::remote(7, Settings::default(), input);

    let _ = term.handle(Command::ProxyToBackend(BackendCommand::Feed(
        b"\x1b[1mhello\x1b[0m from afar\r\n$ ".to_vec(),
    )));
    let text = term.text();
    assert!(
        text.iter().any(|line| line == "hello from afar"),
        "{text:?}"
    );

    let _ = term.handle(Command::ProxyToBackend(BackendCommand::Write(
        b"ls\r".to_vec(),
    )));
    let _ = term.handle(Command::ProxyToBackend(BackendCommand::Resize(
        Some(Size::new(800.0, 400.0)),
        None,
    )));
    let mut writes = Vec::new();
    let mut sizes = Vec::new();
    while let Ok(message) = sent.try_recv() {
        match message {
            RemoteInput::Write(bytes) => writes.push(bytes),
            RemoteInput::Resize { columns, lines } => {
                sizes.push((columns, lines))
            },
        }
    }
    assert_eq!(writes, [b"ls\r".to_vec()]);
    assert!(!sizes.is_empty(), "the size goes to the program");
    assert!(sizes.iter().all(|&(c, l)| c > 0 && l > 0), "{sizes:?}");
}
