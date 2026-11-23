//! Harlo's side of a relay: one WebSocket to the relay carries every phone
//! connected through it, so phones reach Harlo from anywhere.
//!
//! Harlo opens `/host` and proves it owns its relay ID with a secret. The
//! relay then announces each phone (`open`), forwards its frames (`data`)
//! and its hanging up (`close`), all tagged with a connection number. Each
//! phone is served exactly like a direct one, so the relay only ever sees
//! sealed frames.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use super::conn::{Link, connection};
use super::{Hub, relay_id};

/// How long the relay has to answer.
const ANSWER_WAIT: Duration = Duration::from_secs(20);

/// How often Harlo pings the relay, so proxies in between keep the
/// connection open.
const PING_EVERY: Duration = Duration::from_secs(25);

/// The longest wait between attempts to reach the relay.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Where Harlo's connection to the relay is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayStatus {
    /// Reaching the relay.
    Connecting,
    /// Connected; phones reach Harlo at this relay ID.
    Online(String),
    /// The last attempt failed, and why; Harlo tries again.
    Failed(String),
}

/// A connection to a relay, kept up until dropped.
#[derive(Debug)]
pub struct Relay {
    status: watch::Receiver<RelayStatus>,
    task: JoinHandle<()>,
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Relay {
    /// Returns a receiver of the connection's status.
    #[must_use]
    pub fn status(&self) -> watch::Receiver<RelayStatus> {
        self.status.clone()
    }
}

/// Connects to the relay at `ws_base` and keeps connecting until the
/// returned [`Relay`] is dropped.
pub(crate) fn start(hub: Arc<Hub>, ws_base: String, secret: String) -> Relay {
    let (status_tx, status) = watch::channel(RelayStatus::Connecting);
    let task = tokio::spawn(async move {
        let mut backoff = Duration::from_secs(1);
        loop {
            status_tx.send_replace(RelayStatus::Connecting);
            let (why, was_online) = session(&hub, &ws_base, &secret, &status_tx).await;
            if was_online {
                backoff = Duration::from_secs(1);
            }
            status_tx.send_replace(RelayStatus::Failed(why));
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    });
    Relay { status, task }
}

/// One connection to the relay, until it fails: why, and whether it had been online.
async fn session(
    hub: &Arc<Hub>,
    ws_base: &str,
    secret: &str,
    status: &watch::Sender<RelayStatus>,
) -> (String, bool) {
    let url = format!("{ws_base}/host");
    let connect = tokio::time::timeout(ANSWER_WAIT, tokio_tungstenite::connect_async(url));
    let (socket, _) = match connect.await {
        Ok(Ok(ok)) => ok,
        Ok(Err(e)) => return (format!("Can't reach the relay: {e}"), false),
        Err(_) => return ("The relay didn't answer.".to_owned(), false),
    };
    let (mut sink, mut stream) = socket.split();
    let hello = json!({ "type": "host", "secret": secret }).to_string();
    if sink.send(Message::Text(hello.into())).await.is_err() {
        return ("The relay closed the connection.".to_owned(), false);
    }
    let id = relay_id(secret);
    let Ok(Some(Ok(Message::Text(text)))) = tokio::time::timeout(ANSWER_WAIT, stream.next()).await
    else {
        return ("The relay didn't answer.".to_owned(), false);
    };
    let answer: Value = serde_json::from_str(&text).unwrap_or_default();
    if answer["type"] != "hosting" || answer["id"] != id.as_str() {
        let why = answer["error"]
            .as_str()
            .unwrap_or("it isn't a Harlo relay, or a different version");
        return (format!("The relay turned Harlo away: {why}"), false);
    }
    status.send_replace(RelayStatus::Online(id));

    // Frames for the relay, from every phone's connection.
    let (to_relay, mut for_relay) = mpsc::channel::<String>(256);
    let mut phones: HashMap<u64, mpsc::Sender<String>> = HashMap::new();
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    let why = loop {
        tokio::select! {
            frame = for_relay.recv() => {
                let Some(frame) = frame else { break String::new() };
                if sink.send(Message::Text(frame.into())).await.is_err() {
                    break "The relay closed the connection.".to_owned();
                }
            }
            _ = ping.tick() => {
                if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break "The relay closed the connection.".to_owned();
                }
            }
            message = stream.next() => {
                let text = match message {
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(Message::Close(_))) | None => break "The relay closed the connection.".to_owned(),
                    Some(Err(e)) => break format!("Lost the relay: {e}"),
                    Some(Ok(_)) => continue,
                };
                let v: Value = serde_json::from_str(&text).unwrap_or_default();
                let Some(c) = v["c"].as_u64() else { continue };
                match v["type"].as_str() {
                    Some("open") => {
                        let (in_tx, incoming) = mpsc::channel(64);
                        let (outgoing, out_rx) = mpsc::channel::<String>(64);
                        phones.insert(c, in_tx);
                        tokio::spawn(phone(Arc::clone(hub), c, Link { incoming, outgoing }, out_rx, to_relay.clone()));
                    }
                    Some("data") => {
                        let data = v["d"].as_str().unwrap_or_default().to_owned();
                        if let Some(tx) = phones.get(&c)
                            && tx.send(data).await.is_err()
                        {
                            phones.remove(&c);
                        }
                    }
                    // Dropping its sender ends that phone's connection.
                    Some("close") => {
                        phones.remove(&c);
                    }
                    _ => {}
                }
            }
        }
    };
    // Every phone on this connection goes with it.
    drop(phones);
    let _ = sink.close().await;
    (why, true)
}

/// Serves phone `c`, forwarding its frames to the relay.
async fn phone(
    hub: Arc<Hub>,
    c: u64,
    link: Link,
    mut out_rx: mpsc::Receiver<String>,
    to_relay: mpsc::Sender<String>,
) {
    let forward = async {
        while let Some(d) = out_rx.recv().await {
            let frame = json!({ "type": "data", "c": c, "d": d }).to_string();
            if to_relay.send(frame).await.is_err() {
                return;
            }
        }
    };
    tokio::join!(connection(link, hub), forward);
    // Harlo ended it (turned away, removed): the relay hangs up on the phone.
    let _ = to_relay
        .send(json!({ "type": "close", "c": c }).to_string())
        .await;
}
