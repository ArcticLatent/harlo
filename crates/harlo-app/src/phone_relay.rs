//! harlo's side of a harlo relay (see the `harlo-relay` crate): one WebSocket
//! to the relay carries every phone connected through it. Each phone gets a
//! `Link` and is handled by `phone_server::connection` exactly like a
//! direct one, so the relay only ever sees sealed frames.
//!
//! harlo keeps the connection up while the relay is on, reconnecting with
//! backoff, and reports how it's going.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use crate::phone_server::{self, Link, Shared};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Connecting,
    /// Connected; phones can reach harlo at this relay id.
    Online(String),
    /// The last attempt failed (harlo tries again).
    Failed(String),
}

/// A relay connection; dropping it disconnects.
pub struct Client {
    stop: Option<oneshot::Sender<()>>,
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

/// Connects to the relay at `ws_base` (`wss://relay.example.com`) and keeps
/// connecting until dropped. `status` hears every change.
pub fn start(ws_base: String, secret: String, shared: Arc<Shared>, status: impl Fn(Status) + Send + Sync + 'static) -> Client {
    let (stop, mut stopped) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let mut backoff = Duration::from_secs(1);
        loop {
            status(Status::Connecting);
            let run = session(&ws_base, &secret, &shared, &status);
            let failure = tokio::select! {
                r = run => r,
                _ = &mut stopped => return,
            };
            let online_for_a_while = failure.1;
            if online_for_a_while {
                backoff = Duration::from_secs(1);
            }
            status(Status::Failed(failure.0));
            tokio::select! {
                _ = tokio::time::sleep(backoff) => {}
                _ = &mut stopped => return,
            }
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    });
    Client { stop: Some(stop) }
}

/// One connection to the relay, until it fails: why, and whether it had
/// been online.
async fn session(ws_base: &str, secret: &str, shared: &Arc<Shared>, status: &(impl Fn(Status) + Sync)) -> (String, bool) {
    let url = format!("{ws_base}/host");
    let connect = tokio::time::timeout(Duration::from_secs(20), tokio_tungstenite::connect_async(url.as_str()));
    let (ws, _) = match connect.await {
        Ok(Ok(ok)) => ok,
        Ok(Err(e)) => return (format!("Can't reach the relay: {e}"), false),
        Err(_) => return ("The relay didn't answer.".to_owned(), false),
    };
    let (mut sink, mut stream) = ws.split();
    if sink.send(Message::Text(json!({ "type": "host", "secret": secret }).to_string().into())).await.is_err() {
        return ("The relay closed the connection.".to_owned(), false);
    }
    let answer = tokio::time::timeout(Duration::from_secs(20), stream.next()).await;
    let Ok(Some(Ok(Message::Text(text)))) = answer else { return ("The relay didn't answer.".to_owned(), false) };
    let v: Value = serde_json::from_str(&text).unwrap_or_default();
    let id = harlo_relay::relay_id(secret);
    if v["type"] != "hosting" || v["id"] != id.as_str() {
        let why = v["error"].as_str().unwrap_or("it isn't a harlo relay, or a different version");
        return (format!("The relay refused harlo: {why}"), false);
    }
    status(Status::Online(id));

    // Frames for the relay, from every phone's connection.
    let (to_relay, mut for_relay) = mpsc::channel::<String>(256);
    let mut phones: HashMap<u64, mpsc::Sender<String>> = HashMap::new();
    let mut ping = tokio::time::interval(Duration::from_secs(25));
    ping.tick().await;
    let why = loop {
        tokio::select! {
            frame = for_relay.recv() => {
                let Some(frame) = frame else { break "".to_owned() };
                if sink.send(Message::Text(frame.into())).await.is_err() {
                    break "The relay closed the connection.".to_owned();
                }
            }
            _ = ping.tick() => {
                // Keeps proxies (Cloudflare and the like) from timing it out.
                if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break "The relay closed the connection.".to_owned();
                }
            }
            msg = stream.next() => {
                let text = match msg {
                    Some(Ok(Message::Text(t))) => t,
                    Some(Ok(Message::Close(_))) | None => break "The relay closed the connection.".to_owned(),
                    Some(Err(e)) => break format!("Lost the relay: {e}"),
                    Some(Ok(_)) => continue,
                };
                let v: Value = serde_json::from_str(&text).unwrap_or_default();
                let Some(c) = v["c"].as_u64() else { continue };
                match v["type"].as_str() {
                    Some("open") => {
                        let (in_tx, incoming) = mpsc::channel(64);
                        let (outgoing, mut out_rx) = mpsc::channel::<String>(64);
                        phones.insert(c, in_tx);
                        let shared = shared.clone();
                        let to_relay = to_relay.clone();
                        tokio::spawn(async move {
                            let forward = async {
                                while let Some(d) = out_rx.recv().await {
                                    if to_relay.send(json!({ "type": "data", "c": c, "d": d }).to_string()).await.is_err() {
                                        return;
                                    }
                                }
                            };
                            tokio::join!(phone_server::connection(Link { incoming, outgoing }, shared), forward);
                            // harlo ended it (denied, revoked): the relay closes the phone.
                            let _ = to_relay.send(json!({ "type": "close", "c": c }).to_string()).await;
                        });
                    }
                    Some("data") => {
                        let d = v["d"].as_str().unwrap_or_default().to_owned();
                        if let Some(tx) = phones.get(&c)
                            && tx.send(d).await.is_err()
                        {
                            phones.remove(&c);
                        }
                    }
                    Some("close") => {
                        // Dropping its sender ends that connection.
                        phones.remove(&c);
                    }
                    _ => {}
                }
            }
        }
    };
    // Every phone on this relay connection goes with it.
    drop(phones);
    let _ = sink.close().await;
    (why, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phone_server::tests::{Phone, shared as shared_in};
    use crypto_box::aead::OsRng;

    #[tokio::test]
    async fn phones_reach_harlo_through_a_relay() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, harlo_relay::router()).await.unwrap() });

        let dir = std::env::temp_dir().join(format!("harlo-phone-relay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let shared = shared_in(&dir);
        let harlo_public = shared.identity.public_b64();
        let secret = harlo_core::phone::random_id(32);
        let (status_tx, mut statuses) = mpsc::unbounded_channel();
        let client = start(format!("ws://{addr}"), secret.clone(), shared.clone(), move |s| {
            let _ = status_tx.send(s);
        });
        let id = loop {
            match tokio::time::timeout(Duration::from_secs(5), statuses.recv()).await.unwrap().unwrap() {
                Status::Online(id) => break id,
                Status::Failed(e) => panic!("{e}"),
                Status::Connecting => {}
            }
        };
        assert_eq!(id, harlo_relay::relay_id(&secret));

        // A phone pairs through the relay, end to end encrypted.
        let url = format!("ws://{addr}/ws?id={id}");
        let key = crypto_box::SecretKey::generate(&mut OsRng);
        let (mut phone, ready) = Phone::connect_url(&url, &key, &harlo_public, &shared.new_code()).await;
        assert_eq!(ready["type"], "ready");
        assert_eq!(shared.devices.lock().unwrap().list.len(), 1);
        assert_eq!(phone.call(json!({ "id": 1, "op": "ping" })).await["pong"], true);
        // Another at the same time, with a wrong code: denied; the first
        // carries on.
        let stranger = crypto_box::SecretKey::generate(&mut OsRng);
        assert_eq!(Phone::connect_url(&url, &stranger, &harlo_public, "nope").await.1["type"], "denied");
        assert_eq!(phone.call(json!({ "id": 2, "op": "ping" })).await["pong"], true);
        // Paired: back in without a code.
        let (mut again, ready) = Phone::connect_url(&url, &key, &harlo_public, "").await;
        assert_eq!(ready["type"], "ready");
        assert_eq!(again.call(json!({ "id": 3, "op": "ping" })).await["pong"], true);

        // harlo leaves the relay: its phones are disconnected.
        drop(client);
        assert_eq!(phone.next().await, Value::Null);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
