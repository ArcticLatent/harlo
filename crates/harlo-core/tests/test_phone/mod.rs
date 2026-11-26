//! A phone app stand-in for tests: it pairs, opens the sealed channel,
//! sends requests and keeps a copy of the open session's rows, as the
//! phone app does.

// Rust guideline compliant 2026-10-09

#![allow(dead_code, reason = "each user takes part of it")]

use std::net::SocketAddr;
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use harlo_core::phone::{Identity, Sealed, key_bytes};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

/// How long a phone waits for an answer or an event.
pub const WAIT: Duration = Duration::from_secs(30);

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A phone app stand-in: its key, its socket and its copy of the open
/// session's rows.
pub struct TestPhone {
    socket: Socket,
    sealed: Sealed,
    next_id: u64,
    pub rows: Vec<Value>,
    /// Events that arrived while waiting for something else.
    seen: Vec<Value>,
}

/// Connects as the phone with `key`, saying hello (with `code` when it's
/// pairing); returns the phone, or Harlo's plain answer when it was turned away.
pub async fn connect(
    address: SocketAddr,
    key: &Identity,
    harlo: &str,
    code: &str,
) -> Result<TestPhone, Value> {
    connect_to(&format!("ws://{address}/ws"), key, harlo, code).await
}

/// Connects as the phone with `key` at the WebSocket address `url`, as
/// [`connect`] does.
pub async fn connect_to(
    url: &str,
    key: &Identity,
    harlo: &str,
    code: &str,
) -> Result<TestPhone, Value> {
    let (mut socket, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("the phone server answers");
    let mut hello = json!({ "type": "hello", "key": key.public_b64(), "features": [] });
    if !code.is_empty() {
        hello["code"] = json!(code);
        hello["name"] = json!("Test phone");
    }
    socket
        .send(Message::Text(hello.to_string().into()))
        .await
        .expect("hello");
    let answer = tokio::time::timeout(WAIT, socket.next())
        .await
        .expect("an answer in time")
        .expect("an answer")
        .expect("a frame");
    let answer: Value = serde_json::from_str(answer.to_text().expect("text")).expect("JSON");
    if answer["type"] != "ready" {
        return Err(answer);
    }
    let channel = key.channel(&key_bytes(harlo).expect("Harlo's key"));
    let id = answer["conn"].as_str().expect("a connection ID").to_owned();
    Ok(TestPhone {
        socket,
        sealed: Sealed::new(channel, id),
        next_id: 0,
        rows: Vec::new(),
        seen: Vec::new(),
    })
}

impl TestPhone {
    /// The next sealed message; `Null` when Harlo hung up.
    pub async fn next(&mut self) -> Value {
        loop {
            match tokio::time::timeout(WAIT, self.socket.next())
                .await
                .expect("a message in time")
            {
                Some(Ok(Message::Text(text))) => {
                    let message = self.sealed.open(&text).expect("a sealed frame opens");
                    self.keep(&message);
                    return message;
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => return Value::Null,
                Some(Ok(_)) => {}
            }
        }
    }

    /// Applies row events to the phone's copy, as the app does.
    fn keep(&mut self, message: &Value) {
        match message["ev"].as_str() {
            Some("reset") => {
                self.rows = message["rows"].as_array().cloned().unwrap_or_default();
            }
            Some("rows") => {
                for change in message["changes"].as_array().into_iter().flatten() {
                    let at = usize::try_from(change["at"].as_u64().unwrap_or(0)).unwrap_or(0);
                    match change["op"].as_str() {
                        Some("update") => self.rows[at] = change["row"].clone(),
                        Some("remove") => {
                            let count =
                                usize::try_from(change["count"].as_u64().unwrap_or(0)).unwrap_or(0);
                            self.rows.drain(at..at + count);
                        }
                        Some("insert") => {
                            let new = change["rows"].as_array().cloned().unwrap_or_default();
                            self.rows.splice(at..at, new);
                        }
                        other => panic!("an unknown change {other:?}"),
                    }
                }
            }
            _ => {}
        }
    }

    /// Sends a request without waiting for its reply; returns its ID.
    pub async fn send(&mut self, mut request: Value) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        request["id"] = json!(id);
        let frame = self.sealed.seal(&request);
        self.socket
            .send(Message::Text(frame.into()))
            .await
            .expect("send");
        id
    }

    /// The reply to request `id`, sent earlier; events on the way are kept.
    pub async fn reply(&mut self, id: u64) -> Value {
        if let Some(at) = self.seen.iter().position(|m| m["id"] == id) {
            return self.seen.remove(at);
        }
        loop {
            let message = self.next().await;
            assert!(!message.is_null(), "Harlo hung up before replying to {id}");
            if message["id"] == id {
                return message;
            }
            self.seen.push(message);
        }
    }

    /// Sends a request and returns its reply; events on the way are kept.
    pub async fn call(&mut self, mut request: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        request["id"] = json!(id);
        let frame = self.sealed.seal(&request);
        self.socket
            .send(Message::Text(frame.into()))
            .await
            .expect("send");
        loop {
            let message = self.next().await;
            assert!(!message.is_null(), "Harlo hung up during {request}");
            if message["id"] == id {
                return message;
            }
            self.seen.push(message);
        }
    }

    /// Waits for an event that `pick` accepts, looking at kept ones first.
    pub async fn until<T>(
        &mut self,
        what: &str,
        mut pick: impl FnMut(&Self, &Value) -> Option<T>,
    ) -> T {
        // Messages passed over stay kept, for a later wait.
        let kept = std::mem::take(&mut self.seen);
        for (i, message) in kept.iter().enumerate() {
            if let Some(found) = pick(self, message) {
                self.seen = kept[i + 1..].to_vec();
                return found;
            }
        }
        self.seen = kept;
        // What a pick reads off the phone itself (its rows) may already be
        // so, with no message left to say it.
        if let Some(found) = pick(self, &Value::Null) {
            return found;
        }
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            assert!(
                tokio::time::Instant::now() < deadline,
                "waited too long for {what}"
            );
            let message = self.next().await;
            assert!(!message.is_null(), "Harlo hung up while waiting for {what}");
            if let Some(found) = pick(self, &message) {
                return found;
            }
            self.seen.push(message);
        }
    }

    /// The text of the phone's rows, one per line, to search.
    pub fn text(&self) -> String {
        self.rows
            .iter()
            .map(|r| {
                format!(
                    "{} {}",
                    r["kind"].as_str().unwrap_or_default(),
                    r["text"].as_str().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
