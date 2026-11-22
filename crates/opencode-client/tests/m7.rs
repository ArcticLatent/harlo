//! M7 endpoints against a private server: terminals that run on the server.
//!
//! Each test runs its own `opencode serve` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use opencode_client::PtyOutput;
use support::{Sandbox, WAIT};

/// Reads output until it holds `text`, returning everything read.
async fn read_until(output: &mut PtyOutput, text: &str) -> String {
    let mut seen = String::new();
    tokio::time::timeout(WAIT, async {
        while !seen.contains(text) {
            match output.next().await {
                Some(Ok(bytes)) => seen.push_str(&String::from_utf8_lossy(&bytes)),
                Some(Err(error)) => panic!("the connection broke: {error}"),
                None => panic!("the shell ended before {text:?}: {seen:?}"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no {text:?} in {seen:?}"));
    seen
}

/// A shell started on the server runs in the folder asked for, answers what's
/// typed, takes a size, replays its output to a new connection, and ends
/// when removed.
#[tokio::test(flavor = "multi_thread")]
async fn a_shell_on_the_server_answers_and_ends() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project_str();
    let pty = client
        .create_pty(&project, &project, Some("/bin/sh"))
        .await
        .expect("a shell");
    assert_eq!(pty.cwd, project);

    let (mut output, mut input) = client
        .connect_pty(&project, &pty.id)
        .await
        .expect("connected");
    input.send(b"echo $((6 * 7)); pwd\n").await.expect("typed");
    let seen = read_until(&mut output, "42").await;
    let seen = seen + &read_until(&mut output, &project).await;
    assert!(seen.contains(&project), "{seen:?}");

    client
        .resize_pty(&project, &pty.id, 100, 30)
        .await
        .expect("resized");
    input.send(b"stty size\n").await.expect("typed");
    let _ = read_until(&mut output, "30 100").await;

    // A second connection starts with what came before.
    input.close().await.expect("closed");
    let (mut again, _input) = client
        .connect_pty(&project, &pty.id)
        .await
        .expect("reconnected");
    let _ = read_until(&mut again, "42").await;

    client.remove_pty(&project, &pty.id).await.expect("removed");
    let ended = tokio::time::timeout(WAIT, async {
        loop {
            match again.next().await {
                Some(Ok(_)) => {}
                Some(Err(_)) | None => return,
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "the output ends with the shell");
}
