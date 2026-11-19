//! The Codex backend's Responses API over a WebSocket: one connection per
//! session, kept between requests, and each request that only extends the
//! last one sends just its new input with `previous_response_id`.
//!
//! On separate HTTPS requests about a quarter of a session's later requests
//! missed the prompt cache; requests on one connection reach the server that
//! holds their start. The events are the SSE ones, as JSON messages, read by
//! [`crate::responses::handle`]. A connection that can't be opened or used
//! before the reply starts sends the session back to HTTPS ([`Unusable`]).

// Rust guideline compliant 2026-10-09

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::Result;
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Map, Value, json};
use tokio_tungstenite::tungstenite::{self, Message, client::IntoClientRequest as _, http::HeaderValue};

use crate::responses::{Dropped, Event};

/// The beta the backend's WebSocket transport is behind (the Codex CLI's).
const BETA: &str = "responses_websockets=2026-02-06";

/// A connection unused this long is opened anew: the backend drops idle
/// ones (5 minutes, as the Codex CLI keeps them).
const IDLE: Duration = Duration::from_mins(5);

/// A connection this old is opened anew: the backend ends connections at
/// an hour (55 minutes leaves a request room to finish).
const MAX_AGE: Duration = Duration::from_mins(55);

/// How long opening a connection may take.
const CONNECT: Duration = Duration::from_secs(15);

/// Whether Codex requests go over a WebSocket (`HARLO_AGENT_CODEX_WS=off`:
/// over HTTPS, one request each).
pub fn on() -> bool {
    std::env::var("HARLO_AGENT_CODEX_WS").map_or(true, |v| v.trim() != "off")
}

/// Where a request goes: the provider's address, its token, the session's
/// id, the provider's own headers.
pub struct Wire<'a> {
    pub base: &'a str,
    pub token: Option<&'a str>,
    pub session: &'a str,
    pub headers: &'a [(&'static str, String)],
}

/// The WebSocket couldn't be used for this request, and nothing of the
/// reply came: the request can go over HTTPS instead.
#[derive(Debug)]
pub struct Unusable(pub String);

impl std::fmt::Display for Unusable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "The WebSocket couldn't be used: {}", self.0)
    }
}

impl std::error::Error for Unusable {}

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A session's connection, between requests.
struct Kept {
    socket: Socket,
    base: String,
    opened: Instant,
    used: Instant,
    last: Option<Last>,
}

/// The last request on a connection, and what the next may build on.
struct Last {
    /// Its body without `input` (and `previous_response_id`).
    rest: Map<String, Value>,
    /// Its whole input.
    input: Vec<Value>,
    /// The reply's id.
    id: String,
    /// The reply as the next request's input gives it back ([`replied`]):
    /// until then nothing builds on it.
    reply: Option<Vec<Value>>,
}

/// Each session's connection while no request uses it.
static KEPT: LazyLock<Mutex<HashMap<String, Kept>>> = LazyLock::new(Default::default);

/// Sessions whose WebSocket failed: they stay on HTTPS.
static FELL_BACK: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// Whether `session` went back to HTTPS.
pub fn fell_back(session: &str) -> bool {
    FELL_BACK.lock().unwrap_or_else(PoisonError::into_inner).contains(session)
}

/// Sends `session`'s requests over HTTPS from now on.
pub fn fall_back(session: &str, why: &anyhow::Error) {
    trace(&format!("back to HTTPS: {why}"));
    FELL_BACK.lock().unwrap_or_else(PoisonError::into_inner).insert(session.to_owned());
}

/// The last reply on `session`'s connection, as the next request's input
/// will give it back (`reply`): a request whose input starts with the last
/// one's and then this sends only what follows.
pub fn replied(session: &str, reply: Vec<Value>) {
    if let Some(last) = KEPT.lock().unwrap_or_else(PoisonError::into_inner).get_mut(session).and_then(|k| k.last.as_mut()) {
        last.reply = Some(reply);
    }
}

/// Sends one request over `session`'s connection (opened when it has none,
/// or an old one) and calls `on` with each event until it completes.
///
/// # Errors
///
/// [`Unusable`] when no connection could be opened or it broke before the
/// reply started; [`Dropped`] when it broke during the reply; the
/// backend's refusals and failures as [`crate::responses::handle`] gives them.
pub async fn stream(to: Wire<'_>, body: &Value, on: &mut impl FnMut(Event)) -> Result<()> {
    // (A kept connection may have closed meanwhile, or the backend may no
    // longer have the reply a continuation builds on: one more try, on a
    // new connection, which sends the whole input.)
    let mut fresh = false;
    loop {
        let kept = if fresh { None } else { take(to.session, to.base) };
        let reused = kept.is_some();
        let mut kept = match kept {
            Some(k) => k,
            None => open(&to, on).await?,
        };
        let last = kept.last.take();
        let (request, continued) = match last.as_ref().and_then(|l| continuation(l, body)) {
            Some(r) => (r, true),
            None => (body.clone(), false),
        };
        trace(&if continued { format!("{} new input items", request["input"].as_array().map_or(0, Vec::len)) } else { "whole input".to_owned() });
        match exchange(&mut kept.socket, request, on).await {
            Ok(id) => {
                kept.used = Instant::now();
                kept.last = id.map(|id| Last { rest: rest(body), input: body["input"].as_array().cloned().unwrap_or_default(), id, reply: None });
                KEPT.lock().unwrap_or_else(PoisonError::into_inner).insert(to.session.to_owned(), kept);
                return Ok(());
            }
            Err(Ended::Before(why)) if reused && !fresh => {
                trace(&format!("kept connection unusable ({why}), opening another"));
                fresh = true;
            }
            Err(Ended::Before(why)) => return Err(Unusable(why).into()),
            Err(Ended::Again(code)) if !fresh => {
                trace(&format!("{code}: once more on a new connection"));
                fresh = true;
            }
            Err(Ended::Again(code)) => return Err(Unusable(code).into()),
            Err(Ended::With(e)) => return Err(e),
        }
    }
}

/// How a request on a connection ended, short of its reply.
enum Ended {
    /// The connection broke before anything of the reply came.
    Before(String),
    /// The backend asks for the request again (on a new connection, or
    /// whole): its error code.
    Again(String),
    /// The reply ended so (a refusal, a failure, a break during it).
    With(anyhow::Error),
}

/// `session`'s kept connection, when it's to `base` and neither idle nor old.
fn take(session: &str, base: &str) -> Option<Kept> {
    let kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner).remove(session)?;
    (kept.base == base && kept.used.elapsed() < IDLE && kept.opened.elapsed() < MAX_AGE).then_some(kept)
}

/// Opens a connection to the backend's responses address (`wss://…/responses`)
/// with the request headers HTTPS would send, and the WebSocket beta.
async fn open(to: &Wire<'_>, on: &mut impl FnMut(Event)) -> Result<Kept> {
    let url = format!("{}/responses", to.base.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1));
    let mut request = url.as_str().into_client_request().map_err(|e| Unusable(e.to_string()))?;
    let headers = request.headers_mut();
    let mut set = |k: &str, v: &str| {
        if let (Ok(k), Ok(v)) = (tungstenite::http::HeaderName::try_from(k), HeaderValue::from_str(v)) {
            headers.insert(k, v);
        }
    };
    for (k, v) in to.headers {
        set(k, v);
    }
    if let Some(t) = to.token {
        set("authorization", &format!("Bearer {t}"));
    }
    set("openai-beta", BETA);
    set("session-id", to.session);
    set("x-client-request-id", to.session);
    set("user-agent", concat!("harlo-agent/", env!("CARGO_PKG_VERSION")));
    let opened = match tokio::time::timeout(CONNECT, tokio_tungstenite::connect_async(request)).await {
        Err(_) => return Err(Unusable(format!("no connection in {}", crate::responses::span(CONNECT))).into()),
        Ok(Err(tungstenite::Error::Http(resp))) => return Err(refusal(&resp)),
        Ok(Err(e)) => return Err(Unusable(e.to_string()).into()),
        Ok(Ok(o)) => o,
    };
    let (socket, resp) = opened;
    let windows = crate::responses::limits(resp.headers());
    if !windows.is_empty() {
        on(Event::Limits(windows));
    }
    trace("connection opened");
    let now = Instant::now();
    Ok(Kept { socket, base: to.base.to_owned(), opened: now, used: now, last: None })
}

/// A refused upgrade: the sign-in or the plan refused (as over HTTPS), or
/// else no WebSocket here ([`Unusable`]).
fn refusal(resp: &tungstenite::http::Response<Option<Vec<u8>>>) -> anyhow::Error {
    let status = resp.status().as_u16();
    let body: Value = resp.body().as_deref().and_then(|b| serde_json::from_slice(b).ok()).unwrap_or(Value::Null);
    let plan = body["error"]["type"].as_str().is_some_and(|t| t.starts_with("usage_"));
    if status == 401 || status == 429 || plan {
        return crate::responses::refused(status, &body, crate::retry::retry_after(resp.headers())).into();
    }
    Unusable(format!("the upgrade was refused ({status})")).into()
}

/// Sends `request` (a `response.create`) and reads the reply's events into
/// `on` until it ends: the reply's id when it completed.
async fn exchange(socket: &mut Socket, mut request: Value, on: &mut impl FnMut(Event)) -> Result<Option<String>, Ended> {
    request["type"] = json!("response.create");
    if let Err(e) = socket.send(Message::text(request.to_string())).await {
        return Err(Ended::Before(e.to_string()));
    }
    let idle = crate::responses::idle();
    let mut started = false;
    let mut id = None;
    loop {
        let broke = |why: String| if started { Ended::With(Dropped(format!("The connection broke during the reply: {why}"), Value::Null).into()) } else { Ended::Before(why) };
        let text = match tokio::time::timeout(idle, socket.next()).await {
            Err(_) => return Err(Ended::With(Dropped(format!("The server sent nothing for {} during the reply", crate::responses::span(idle)), Value::Null).into())),
            Ok(None) => return Err(broke("the connection closed".into())),
            Ok(Some(Err(e))) => return Err(broke(e.to_string())),
            Ok(Some(Ok(Message::Close(frame)))) => return Err(broke(frame.map_or_else(|| "the server closed it".to_owned(), |f| format!("closed: {} {}", f.code, f.reason)))),
            Ok(Some(Ok(Message::Text(t)))) => t,
            Ok(Some(Ok(_))) => continue,
        };
        let Ok(ev) = serde_json::from_str::<Value>(text.as_str()) else { continue };
        started = true;
        match ev["type"].as_str().unwrap_or_default() {
            "response.created" => id = ev["response"]["id"].as_str().map(str::to_owned),
            "codex.rate_limits" => {
                let windows = rate_limits(&ev);
                if !windows.is_empty() {
                    on(Event::Limits(windows));
                }
                continue;
            }
            "error" => {
                let code = ev["error"]["code"].as_str().or_else(|| ev["code"].as_str()).unwrap_or_default();
                if matches!(code, "previous_response_not_found" | "websocket_connection_limit_reached") {
                    return Err(Ended::Again(code.to_owned()));
                }
                // (A refusal comes wrapped, with the status HTTPS would have.)
                if let Some(status) = ev["status"].as_u64().or_else(|| ev["status_code"].as_u64()).and_then(|s| u16::try_from(s).ok()).filter(|s| *s >= 400) {
                    return Err(Ended::With(crate::responses::refused(status, &json!({ "error": ev["error"] }), None).into()));
                }
            }
            _ => {}
        }
        if let Some(end) = crate::responses::handle(&ev, on) {
            let id = ev["response"]["id"].as_str().map(str::to_owned).or(id);
            return end.map(|()| id).map_err(Ended::With);
        }
    }
}

/// The plan's windows in a `codex.rate_limits` event, as
/// [`crate::responses::limits`] gives them.
fn rate_limits(ev: &Value) -> Vec<Value> {
    ["primary", "secondary"]
        .into_iter()
        .filter_map(|w| {
            let w = &ev["rate_limits"][w];
            let used = w["used_percent"].as_f64()?;
            Some(json!({ "windowMinutes": w["window_minutes"].as_i64(), "usedPercent": used, "resetsAt": w["reset_at"].as_i64() }))
        })
        .collect()
}

/// A body without its input: what must match for a request to build on
/// the last.
fn rest(body: &Value) -> Map<String, Value> {
    body.as_object().into_iter().flatten().filter(|(k, _)| *k != "input" && *k != "previous_response_id").map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// `body` as a continuation of `last`, when it is one: the same but for
/// its input, which starts with the last input and its reply; then only
/// what follows is sent, with the reply's id.
fn continuation(last: &Last, body: &Value) -> Option<Value> {
    let reply = last.reply.as_ref()?;
    let input = body["input"].as_array()?;
    let known = last.input.len() + reply.len();
    let extends = input.len() > known && input[..last.input.len()] == last.input[..] && input[last.input.len()..known] == reply[..];
    if !extends || rest(body) != last.rest {
        return None;
    }
    let mut request = Value::Object(rest(body));
    request["previous_response_id"] = json!(last.id);
    request["input"] = Value::Array(input[known..].to_vec());
    Some(request)
}

/// What the transport did, on stderr, with `HARLO_AGENT_SOCKET_TRACE` set.
fn trace(what: &str) {
    static ON: LazyLock<bool> = LazyLock::new(|| std::env::var_os("HARLO_AGENT_SOCKET_TRACE").is_some());
    if *ON {
        eprintln!("[socket] {what}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in backend on a WebSocket: it records each `response.create`
    /// (with the upgrade's beta header first) and answers each with a reply
    /// whose id counts up. `forget`: the first continuation is refused as
    /// the backend does when it no longer has the reply.
    #[expect(clippy::result_large_err, reason = "the handshake callback's error type is tungstenite's")]
    async fn backend(forget: bool) -> (String, tokio::sync::mpsc::UnboundedReceiver<Value>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut n = 0;
            let mut forget = forget;
            while let Ok((tcp, _)) = listener.accept().await {
                let seen = tx.clone();
                let beta = std::sync::Arc::new(Mutex::new(String::new()));
                let keep = std::sync::Arc::clone(&beta);
                let mut ws = tokio_tungstenite::accept_hdr_async(tcp, move |req: &tungstenite::handshake::server::Request, resp| {
                    *keep.lock().unwrap() = req.headers().get("openai-beta").and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
                    Ok(resp)
                })
                .await
                .unwrap();
                let _ = seen.send(json!({ "connected": *beta.lock().unwrap() }));
                while let Some(Ok(Message::Text(t))) = ws.next().await {
                    let req: Value = serde_json::from_str(t.as_str()).unwrap();
                    let _ = seen.send(req.clone());
                    if forget && req["previous_response_id"].is_string() {
                        forget = false;
                        let _ = ws.send(Message::text(json!({ "type": "error", "status": 400, "error": { "code": "previous_response_not_found", "message": "gone" } }).to_string())).await;
                        continue;
                    }
                    n += 1;
                    let item = json!({ "type": "message", "id": format!("msg_{n}"), "role": "assistant", "content": [{ "type": "output_text", "text": "ok" }] });
                    for ev in [
                        json!({ "type": "response.created", "response": { "id": format!("resp_{n}") } }),
                        json!({ "type": "response.output_item.done", "output_index": 0, "item": item }),
                        json!({ "type": "response.completed", "response": { "id": format!("resp_{n}"), "output": [item], "usage": { "input_tokens": 10 } } }),
                    ] {
                        let _ = ws.send(Message::text(ev.to_string())).await;
                    }
                }
            }
        });
        (format!("http://{addr}"), rx)
    }

    fn user(text: &str) -> Value {
        json!({ "role": "user", "content": [{ "type": "input_text", "text": text }] })
    }

    fn reply() -> Value {
        json!({ "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "ok", "annotations": [] }], "status": "completed", "id": "msg_1" })
    }

    async fn ask(base: &str, session: &str, input: &[Value]) {
        let body = json!({ "model": "m", "store": false, "input": input });
        let to = Wire { base, token: Some("t"), session, headers: &[] };
        stream(to, &body, &mut |_| {}).await.unwrap();
    }

    #[tokio::test]
    async fn a_request_that_extends_the_last_sends_only_whats_new() {
        let (base, mut seen) = backend(false).await;
        let first = [user("a")];
        ask(&base, "ws-a", &first).await;
        replied("ws-a", vec![reply()]);
        let second = [user("a"), reply(), user("b")];
        ask(&base, "ws-a", &second).await;
        // (Not built on: a request whose input doesn't start with the last's.)
        ask(&base, "ws-a", &[user("x")]).await;

        assert_eq!(seen.recv().await.unwrap(), json!({ "connected": BETA }));
        let one = seen.recv().await.unwrap();
        assert_eq!(one["type"], "response.create");
        assert_eq!(one["input"], json!(first));
        assert!(one.get("previous_response_id").is_none());
        let two = seen.recv().await.unwrap();
        assert_eq!(two["previous_response_id"], "resp_1");
        assert_eq!(two["input"], json!([user("b")]));
        assert_eq!(two["model"], "m");
        let three = seen.recv().await.unwrap();
        assert!(three.get("previous_response_id").is_none(), "one connection, but sent whole");
        assert_eq!(three["input"], json!([user("x")]));
    }

    #[tokio::test]
    async fn a_reply_the_backend_forgot_is_sent_again_whole() {
        let (base, mut seen) = backend(true).await;
        ask(&base, "ws-b", &[user("a")]).await;
        replied("ws-b", vec![reply()]);
        let second = [user("a"), reply(), user("b")];
        ask(&base, "ws-b", &second).await;

        assert!(seen.recv().await.unwrap()["connected"].is_string());
        assert!(seen.recv().await.unwrap()["previous_response_id"].is_null());
        assert_eq!(seen.recv().await.unwrap()["previous_response_id"], "resp_1");
        // (Refused: a new connection, the whole input.)
        assert!(seen.recv().await.unwrap()["connected"].is_string());
        let again = seen.recv().await.unwrap();
        assert!(again.get("previous_response_id").is_none());
        assert_eq!(again["input"], json!(second));
    }

    #[tokio::test]
    async fn no_websocket_there_is_unusable() {
        // (A plain HTTP server: the upgrade is refused.)
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
            while let Ok((mut s, _)) = listener.accept().await {
                let mut buf = vec![0u8; 4096];
                let _ = s.read(&mut buf).await;
                let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            }
        });
        let base = format!("http://{addr}");
        let to = Wire { base: &base, token: None, session: "ws-c", headers: &[] };
        let err = stream(to, &json!({ "input": [] }), &mut |_| {}).await.unwrap_err();
        assert!(err.is::<Unusable>(), "{err}");
    }
}
