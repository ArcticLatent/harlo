//! A later launch handing its command to the running Harlo, over sockets in
//! scratch folders.

// Rust guideline compliant 2026-10-09

use std::io;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use harlo::instance::{self, Command};

/// How long a command may take to arrive.
const WAIT: Duration = Duration::from_secs(5);

#[test]
fn a_later_launch_hands_its_command_to_the_running_one() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let path = scratch.path().join("harlo.sock");
    let (sent, received) = mpsc::channel();
    let listener = instance::listen(&path, move |command| {
        let _ = sent.send(command);
    })
    .expect("listening");

    let pid = instance::forward(&path, Command::Show).expect("handed over");
    assert_eq!(pid, Some(std::process::id()), "it answers with its process");
    assert_eq!(received.recv_timeout(WAIT), Ok(Command::Show));

    // Quitting removes the socket, so the next launch starts afresh.
    drop(listener);
    assert!(!path.exists());
    assert_eq!(
        instance::forward(&path, Command::Show).expect("no one there"),
        None
    );
}

#[test]
fn a_socket_left_behind_is_replaced_and_a_live_one_kept() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let path = scratch.path().join("harlo.sock");
    // A Harlo that ended without cleaning up.
    drop(UnixListener::bind(&path).expect("a socket"));
    assert!(path.exists());
    assert_eq!(
        instance::forward(&path, Command::Show).expect("no one answers"),
        None
    );

    let _first = instance::listen(&path, |_| {}).expect("replaces it");
    let second = instance::listen(&path, |_| {}).expect_err("one listens already");
    assert_eq!(second.kind(), io::ErrorKind::AddrInUse);
}

#[test]
fn each_state_folder_has_its_own_socket() {
    let runtime = Path::new("/run/user/1000");
    let mine = instance::socket_in(runtime, Path::new("/home/me/.local/state"));
    let scratch = instance::socket_in(runtime, Path::new("/tmp/test/state"));
    assert_ne!(mine, scratch);
    assert_eq!(
        mine,
        instance::socket_in(runtime, Path::new("/home/me/.local/state")),
        "the same every time"
    );
    assert!(mine.starts_with(runtime));
}
