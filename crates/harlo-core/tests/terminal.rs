//! A terminal whose shell runs on the server: output, input and size go both
//! ways, and the stream ends with the shell.
//!
//! Runs against a private `harlo-server` with scratch `HOME`/`XDG_*` folders,
//! never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod support;

use futures_util::{Stream, StreamExt};
use harlo_core::terminal::{self, Input, Update};
use support::{Sandbox, WAIT};
use tokio::sync::mpsc;

/// Reads updates until the output holds `text`.
async fn read_until(updates: &mut (impl Stream<Item = Update> + Unpin), text: &str) -> String {
    let mut seen = String::new();
    tokio::time::timeout(WAIT, async {
        while !seen.contains(text) {
            match updates.next().await {
                Some(Update::Output(bytes)) => seen.push_str(&String::from_utf8_lossy(&bytes)),
                other => panic!("ended before {text:?}: {other:?} after {seen:?}"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no {text:?} in {seen:?}"));
    seen
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_shell_follows_input_and_size_then_ends() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project_str();
    let pty = client
        .create_pty(&project, &project, Some("/bin/sh"))
        .await
        .expect("a shell");

    let (typed, received) = mpsc::unbounded_channel();
    let input = futures_util::stream::unfold(received, |mut received| async move {
        received.recv().await.map(|item| (item, received))
    });
    let mut updates = Box::pin(terminal::follow(
        client.clone(),
        project.clone(),
        pty.id.clone(),
        input,
    ));

    // Sizes repeat as the window settles; the shell gets the last one.
    for (columns, lines) in [(80, 24), (80, 24), (120, 40)] {
        typed.send(Input::Resize { columns, lines }).expect("sent");
    }
    typed
        .send(Input::Write(b"echo $((6 * 7)); stty size\n".to_vec()))
        .expect("sent");
    let _ = read_until(&mut updates, "42").await;
    let _ = read_until(&mut updates, "40 120").await;

    typed.send(Input::Write(b"exit\n".to_vec())).expect("sent");
    let ended = tokio::time::timeout(WAIT, async {
        loop {
            match updates.next().await {
                Some(Update::Output(_)) => {}
                Some(Update::Ended(reason)) => return reason,
                None => panic!("the stream stopped without saying the shell ended"),
            }
        }
    })
    .await
    .expect("the shell ends");
    assert_eq!(ended, None, "it ended by itself, not by an error");
    assert!(updates.next().await.is_none(), "nothing after the end");
}
