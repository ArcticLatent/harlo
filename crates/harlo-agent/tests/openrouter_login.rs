//! `harlo-agent login --openrouter --paste` against a stand-in OpenRouter:
//! the browser's address carries a PKCE challenge, the pasted callback's
//! code is exchanged with its verifier, and the key it gives is saved
//! privately. Nothing reaches openrouter.ai.

// Rust guideline compliant 2026-10-09

use std::io::{BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use base64::Engine as _;
use sha2::{Digest, Sha256};

/// A stand-in for OpenRouter's key exchange: it gives a key only for the
/// code it handed out and a verifier matching the challenge (read from the
/// first line of `challenge`'s receiver).
fn stand_in(challenge: std::sync::mpsc::Receiver<String>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut req = Vec::new();
        let mut buf = [0u8; 4096];
        // (The head, then as much body as it says.)
        loop {
            let n = sock.read(&mut buf).unwrap();
            req.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&req);
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let len = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap())).unwrap_or(0);
                if body.len() >= len {
                    break;
                }
            }
        }
        let text = String::from_utf8_lossy(&req).to_string();
        let body: serde_json::Value = serde_json::from_str(text.split_once("\r\n\r\n").unwrap().1).unwrap();
        let verifier = body["code_verifier"].as_str().unwrap();
        let made = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let ok = text.starts_with("POST /api/v1/auth/keys ") && body["code"] == "the-code" && body["code_challenge_method"] == "S256" && made == challenge.recv().unwrap();
        let (status, answer) = if ok { ("200 OK", r#"{"key":"sk-or-v1-made"}"#) } else { ("400 Bad Request", r#"{"error":{"message":"bad exchange"}}"#) };
        let _ = write!(sock, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len());
    });
    base
}

#[test]
fn an_openrouter_sign_in_saves_its_key() {
    let dir = std::env::temp_dir().join(format!("harlo-agent-openrouter-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let opener = dir.join("bin/xdg-open");
    std::fs::write(&opener, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&opener, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let site = stand_in(rx);
    let mut child = Command::new(env!("CARGO_BIN_EXE_harlo-agent"))
        .args(["login", "--openrouter", "--paste"])
        .env_clear()
        .env("HOME", &dir)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("PATH", format!("{}:/usr/bin:/bin", dir.join("bin").display()))
        .env("HARLO_AGENT_OPENROUTER_URL", &site)
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
            let _ = child.wait();
            let _ = std::fs::remove_dir_all(&dir);
            // (Port 1455 taken: a sign-in going on here; nothing checked.)
            assert!(err.contains("in use"), "{text}{err}");
            return;
        }
        text.push(byte[0] as char);
    }
    let link = text.lines().find(|l| l.contains("/auth?")).unwrap().trim().to_owned();
    let url = reqwest::Url::parse(&link).unwrap();
    let q = |k: &str| url.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned()).unwrap_or_default();
    assert!(link.starts_with(&format!("{site}/auth?")), "{link}");
    assert_eq!((q("callback_url").as_str(), q("code_challenge_method").as_str()), ("http://localhost:1455/auth/callback", "S256"));
    tx.send(q("code_challenge")).unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "http://localhost:1455/auth/callback?code=the-code&state={}", q("state")).unwrap();
    stdin.flush().unwrap();
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        assert!(start.elapsed() < Duration::from_secs(10), "the sign-in didn't end");
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut rest = String::new();
    let _ = out.read_to_string(&mut rest);
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    assert!(child.wait().unwrap().success(), "{rest}{err}");
    assert!(rest.contains("Signed in to OpenRouter"), "{rest}");
    let keys = dir.join("config/harlo-agent/keys.json");
    let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&keys).unwrap()).unwrap();
    assert_eq!(saved["openrouter"], "sk-or-v1-made");
    assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&keys).unwrap().permissions()) & 0o777, 0o600);
    drop(stdin);
    let _ = std::fs::remove_dir_all(&dir);
}
