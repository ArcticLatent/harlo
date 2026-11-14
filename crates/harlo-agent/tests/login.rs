//! `harlo-agent login --paste` (how harlo signs in on another machine) ends
//! as soon as the sign-in does, though its input stays open: harlo keeps it
//! open for a pasted address, and a program waiting on it never returned.
//! Nothing reaches OpenAI: the sign-in is declined on the callback.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn scratch() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("harlo-agent-login-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    // (No browser opens.)
    let opener = dir.join("bin/xdg-open");
    std::fs::write(&opener, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&opener, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    dir
}

/// Runs a login; `finish` ends it (given its state and stdin); whether it
/// ended within a few seconds, its input still open. None: port 1455 was
/// taken (a sign-in going on here), so nothing was checked.
fn ends(finish: impl FnOnce(&str, &mut std::process::ChildStdin)) -> Option<bool> {
    let dir = scratch();
    let mut child = Command::new(env!("CARGO_BIN_EXE_harlo-agent"))
        .args(["login", "--codex", "--paste"])
        .env_clear()
        .env("HOME", &dir)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("PATH", format!("{}:/usr/bin:/bin", dir.join("bin").display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut text = String::new();
    let mut byte = [0u8; 1];
    while !text.contains("paste the address") {
        if out.read(&mut byte).unwrap() == 0 {
            let mut err = String::new();
            child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
            let _ = std::fs::remove_dir_all(&dir);
            assert!(err.contains("in use"), "{text}{err}");
            return None;
        }
        text.push(byte[0] as char);
    }
    let state = text.split("state=").nth(1).unwrap().split(['&', '\n', ' ']).next().unwrap().to_owned();
    let mut stdin = child.stdin.take().unwrap();
    finish(&state, &mut stdin);
    let start = Instant::now();
    let ended = loop {
        if child.try_wait().unwrap().is_some() {
            break true;
        }
        if start.elapsed() > Duration::from_secs(5) {
            break false;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let _ = child.kill();
    let _ = child.wait();
    drop(stdin);
    let _ = std::fs::remove_dir_all(&dir);
    Some(ended)
}

#[test]
fn a_sign_in_ends_with_its_input_open() {
    // The browser's return.
    let by_browser = ends(|state, _| {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", 1455)).unwrap();
        // (The server may answer from the first line and close before the
        // rest is written: what's tested is that it ends, not the write.)
        let _ = write!(s, "GET /auth/callback?error=access_denied&state={state} HTTP/1.1\r\nHost: localhost\r\n\r\n");
        let _ = BufReader::new(s).lines().next();
    });
    // The address pasted.
    let pasted = ends(|state, stdin| {
        writeln!(stdin, "http://localhost:1455/auth/callback?error=access_denied&state={state}").unwrap();
        stdin.flush().unwrap();
    });
    for (how, ended) in [("by the browser", by_browser), ("pasted", pasted)] {
        if let Some(ended) = ended {
            assert!(ended, "the sign-in ended {how}, but harlo-agent didn't");
        }
    }
}
