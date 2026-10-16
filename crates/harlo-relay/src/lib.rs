//! harlo-relay: lets phones reach harlo from anywhere.
//!
//! It serves harlo's phone web app and passes WebSocket frames between a
//! harlo (the host) and its paired phones. The frames are end-to-end
//! encrypted between harlo and each phone (see harlo-core's `phone`); the
//! relay can't read or change them, and stores nothing. It sees IP
//! addresses, timing and sizes.
//!
//! - harlo connects to `/host` and says `{"type":"host","secret":…}`. Its id
//!   on the relay is derived from the secret (base64url of the first 16
//!   bytes of SHA-256), so only the holder of the secret can take that id.
//! - A phone connects to `/ws?id=<id>`. The relay tells harlo
//!   `{"type":"open","c":n}`, then wraps frames both ways as
//!   `{"type":"data","c":n,"d":…}`, and `{"type":"close","c":n}` when either
//!   side goes.
//!
//! With the app's APNs key (`--apns-key`), it also sends the iOS app's
//! notifications for every harlo: harlo posts each to `/push` (the phone's
//! token and the notification, whose text is sealed for the phone, so the
//! relay and Apple see a placeholder), and `GET /push` says whether this
//! relay does. Apple only takes notifications signed with a key of the
//! app's team, which can't be in everyone's harlo.
//!
//! It's meant to be shared (one relay for everyone's harlo), so it limits
//! what anyone can take: connections per IP address, harlos in all, phones
//! per harlo, bytes per second per connection, quiet connections, and
//! notifications a minute (per phone and per address).
//!
//! Run it behind something that adds HTTPS (Caddy, a Cloudflare Tunnel,
//! Tailscale Funnel):
//!
//! ```sh
//! harlo-relay --listen 0.0.0.0:8080      # or HARLO_RELAY_LISTEN
//! ```

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

/// The relay id for a host secret.
pub fn relay_id(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&digest[..16])
}

/// What anyone may take of the relay.
#[derive(Debug, Clone)]
pub struct Limits {
    /// harlos connected at once, in all.
    pub hosts: usize,
    /// Phones one harlo may have connected at once.
    pub phones_per_host: usize,
    /// Connections (harlos and phones) from one IP address at once.
    pub per_ip: usize,
    /// The largest frame passed on.
    pub frame: usize,
    /// Bytes per second a connection may send (a burst of `burst` first);
    /// faster senders are slowed down, not cut off.
    pub rate: usize,
    pub burst: usize,
    /// A connection that sends nothing (not even a ping) for this long is
    /// closed. harlo pings every 25 s, the phone app every 30 s.
    pub idle: Duration,
    /// Notifications a minute to one phone, and from one address.
    pub pushes_per_phone: u32,
    pub pushes_per_ip: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            hosts: 10_000, phones_per_host: 16, per_ip: 64, frame: 4 << 20, rate: 1 << 20, burst: 16 << 20, idle: Duration::from_secs(120),
            pushes_per_phone: 30, pushes_per_ip: 300,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub limits: Limits,
    /// Behind a proxy that says who's connecting (`CF-Connecting-IP`,
    /// `X-Forwarded-For`): count connections by that address. Only turn
    /// it on when the relay can't be reached except through the proxy.
    pub trust_forwarded: bool,
    /// Sends the iOS app's notifications (the app's APNs key).
    pub push: Option<Push>,
}

/// The app's APNs key, as a client, and the app's bundle id.
#[derive(Clone)]
pub struct Push {
    pub client: Arc<harlo_apns::Client>,
    pub topic: String,
}

impl std::fmt::Debug for Push {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Push {{ topic: {} }}", self.topic)
    }
}

/// A connected harlo: where to send it frames, and its phones.
struct Host {
    /// Which connection of this id it is (a newer one replaces it).
    generation: u64,
    to_host: mpsc::Sender<String>,
    phones: HashMap<u64, mpsc::Sender<String>>,
    next_phone: u64,
}

struct Relay {
    config: Config,
    hosts: Mutex<HashMap<String, Host>>,
    generations: AtomicU64,
    per_ip: Mutex<HashMap<IpAddr, usize>>,
    /// Notifications this minute: per phone token, per address.
    pushes: Mutex<(Instant, HashMap<String, u32>)>,
}

impl Default for Relay {
    fn default() -> Self {
        Self {
            config: Config::default(), hosts: Default::default(), generations: Default::default(), per_ip: Default::default(),
            pushes: Mutex::new((Instant::now(), HashMap::new())),
        }
    }
}

/// One connection counted against its IP address while it lasts.
struct IpSlot {
    relay: Arc<Relay>,
    ip: Option<IpAddr>,
}

impl Relay {
    fn take_slot(self: &Arc<Self>, ip: Option<IpAddr>) -> Option<IpSlot> {
        if let Some(ip) = ip {
            let mut counts = self.per_ip.lock().unwrap();
            let n = counts.entry(ip).or_insert(0);
            if *n >= self.config.limits.per_ip {
                return None;
            }
            *n += 1;
        }
        Some(IpSlot { relay: self.clone(), ip })
    }
}

impl Drop for IpSlot {
    fn drop(&mut self) {
        if let Some(ip) = self.ip {
            let mut counts = self.relay.per_ip.lock().unwrap();
            if let Some(n) = counts.get_mut(&ip) {
                *n -= 1;
                if *n == 0 {
                    counts.remove(&ip);
                }
            }
        }
    }
}

/// Who's connecting: the peer's address, or (trusted) the proxy's word.
struct ClientIp(Option<IpAddr>);

impl FromRequestParts<Arc<Relay>> for ClientIp {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, relay: &Arc<Relay>) -> Result<Self, Self::Rejection> {
        let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip());
        let forwarded = relay.config.trust_forwarded.then(|| forwarded_ip(&parts.headers)).flatten();
        Ok(ClientIp(forwarded.or(peer)))
    }
}

fn forwarded_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
    header("cf-connecting-ip")
        .or_else(|| header("x-forwarded-for").and_then(|v| v.split(',').next().map(str::to_owned)))
        .and_then(|v| v.trim().parse().ok())
}

/// Slows a connection that sends faster than its rate.
struct Budget {
    rate: f64,
    burst: f64,
    tokens: f64,
    at: Instant,
}

impl Budget {
    fn new(limits: &Limits) -> Self {
        Self { rate: limits.rate as f64, burst: limits.burst as f64, tokens: limits.burst as f64, at: Instant::now() }
    }

    async fn spend(&mut self, bytes: usize) {
        let now = Instant::now();
        self.tokens = (self.tokens + now.duration_since(self.at).as_secs_f64() * self.rate).min(self.burst);
        self.at = now;
        self.tokens -= bytes as f64;
        if self.tokens < 0.0 {
            tokio::time::sleep(Duration::from_secs_f64(-self.tokens / self.rate)).await;
        }
    }
}

/// The next text frame from a connection: None when it closes, errs, or
/// goes quiet for too long. Pings count as life.
async fn next_text(stream: &mut futures_util::stream::SplitStream<WebSocket>, idle: Duration) -> Option<String> {
    loop {
        match tokio::time::timeout(idle, stream.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => return Some(t.to_string()),
            Ok(Some(Ok(Message::Close(_)))) | Ok(Some(Err(_))) | Ok(None) | Err(_) => return None,
            Ok(Some(Ok(_))) => continue,
        }
    }
}

fn asset(content_type: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}

impl Relay {
    /// One more notification to `token` from `ip`, if neither has had its
    /// minute's worth.
    fn push_allowed(&self, token: &str, ip: Option<IpAddr>) -> bool {
        let limits = &self.config.limits;
        let mut pushes = self.pushes.lock().unwrap();
        if pushes.0.elapsed() > Duration::from_secs(60) {
            *pushes = (Instant::now(), HashMap::new());
        }
        let counts = &mut pushes.1;
        let ip_key = ip.map(|ip| format!("ip:{ip}"));
        let over = counts.get(&format!("t:{token}")).is_some_and(|n| *n >= limits.pushes_per_phone)
            || ip_key.as_ref().is_some_and(|k| counts.get(k).is_some_and(|n| *n >= limits.pushes_per_ip));
        if over {
            return false;
        }
        *counts.entry(format!("t:{token}")).or_default() += 1;
        if let Some(k) = ip_key {
            *counts.entry(k).or_default() += 1;
        }
        true
    }
}

/// A notification from a harlo, sent on to Apple with the app's key.
async fn push(relay: Arc<Relay>, ip: Option<IpAddr>, body: String) -> Response {
    let reply = |status: StatusCode, v: Value| (status, [(header::CONTENT_TYPE, "application/json")], v.to_string()).into_response();
    let Some(push) = &relay.config.push else {
        return reply(StatusCode::NOT_FOUND, json!({ "error": "this relay doesn't send notifications" }));
    };
    let v: Value = serde_json::from_str(&body).unwrap_or_default();
    let f = match harlo_apns::read_forward(&v) {
        Ok(p) => p,
        Err(e) => return reply(StatusCode::BAD_REQUEST, json!({ "error": e })),
    };
    if !relay.push_allowed(&f.token, ip) {
        return reply(StatusCode::TOO_MANY_REQUESTS, json!({ "error": "too many notifications this minute" }));
    }
    let sent = push.client.send(&harlo_apns::Push { device_token: &f.token, sandbox: f.sandbox, topic: &push.topic, payload: &f.payload, collapse_id: f.collapse.as_deref(), kind: f.kind }).await;
    match sent {
        harlo_apns::Sent::Ok => reply(StatusCode::OK, json!({ "ok": true })),
        harlo_apns::Sent::Gone(reason) => reply(StatusCode::GONE, json!({ "reason": reason })),
        harlo_apns::Sent::Failed(e) => reply(StatusCode::BAD_GATEWAY, json!({ "error": e })),
    }
}

fn busy() -> Response {
    (StatusCode::TOO_MANY_REQUESTS, "too many connections").into_response()
}

pub fn router() -> Router {
    router_with(Config::default())
}

pub fn router_with(config: Config) -> Router {
    let relay = Arc::new(Relay { config, ..Default::default() });
    Router::new()
        .route("/", get(|| async { asset("text/html; charset=utf-8", include_str!("../../harlo-app/phone/index.html")) }))
        .route("/app.js", get(|| async { asset("text/javascript; charset=utf-8", include_str!("../../harlo-app/phone/app.js")) }))
        .route("/style.css", get(|| async { asset("text/css; charset=utf-8", include_str!("../../harlo-app/phone/style.css")) }))
        .route("/nacl.js", get(|| async { asset("text/javascript; charset=utf-8", include_str!("../../harlo-app/phone/nacl-fast.min.js")) }))
        .route("/manifest.webmanifest", get(|| async { asset("application/manifest+json", include_str!("../../harlo-app/phone/manifest.webmanifest")) }))
        .route("/icon.svg", get(|| async { asset("image/svg+xml", include_str!("../../harlo-app/icons/app/harlo.svg")) }))
        // The privacy policy and help with the app: on harlo's website now
        // (the iPhone app's first builds and the App Store linked here).
        .route("/privacy", get(|| async { Redirect::permanent("https://harlo.run/privacy") }))
        .route("/support", get(|| async { Redirect::permanent("https://harlo.run/support") }))
        .route("/health", get(|| async { "ok" }))
        .route("/push", get(|State(relay): State<Arc<Relay>>| async move {
            ([(header::CONTENT_TYPE, "application/json")], json!({ "push": relay.config.push.is_some() }).to_string())
        }).post(|ClientIp(ip): ClientIp, State(relay): State<Arc<Relay>>, body: String| async move {
            push(relay, ip, body).await
        }))
        .route("/host", get(|ws: WebSocketUpgrade, ClientIp(ip): ClientIp, State(relay): State<Arc<Relay>>| async move {
            let Some(slot) = relay.take_slot(ip) else { return busy() };
            ws.max_message_size(relay.config.limits.frame).on_upgrade(move |socket| host(socket, relay, slot))
        }))
        .route("/ws", get(|ws: WebSocketUpgrade, ClientIp(ip): ClientIp, Query(q): Query<HashMap<String, String>>, State(relay): State<Arc<Relay>>| async move {
            let Some(slot) = relay.take_slot(ip) else { return busy() };
            let id = q.get("id").cloned().unwrap_or_default();
            ws.max_message_size(relay.config.limits.frame).on_upgrade(move |socket| phone(socket, relay, id, slot))
        }))
        .with_state(relay)
}

/// Serves the relay on `listener` until `shutdown` (with client addresses,
/// for the per-address limit).
pub async fn serve(listener: tokio::net::TcpListener, config: Config, shutdown: impl Future<Output = ()> + Send + 'static) -> std::io::Result<()> {
    axum::serve(listener, router_with(config).into_make_service_with_connect_info::<SocketAddr>()).with_graceful_shutdown(shutdown).await
}

async fn send_json(sink: &mut futures_util::stream::SplitSink<WebSocket, Message>, v: Value) -> bool {
    sink.send(Message::Text(v.to_string().into())).await.is_ok()
}

async fn host(socket: WebSocket, relay: Arc<Relay>, _slot: IpSlot) {
    let limits = relay.config.limits.clone();
    let (mut sink, mut stream) = socket.split();
    // The secret first.
    let Some(text) = next_text(&mut stream, Duration::from_secs(15)).await else { return };
    let v: Value = serde_json::from_str(&text).unwrap_or_default();
    let secret = v["secret"].as_str().unwrap_or_default();
    if v["type"] != "host" || secret.len() < 32 {
        send_json(&mut sink, json!({ "type": "error", "error": "bad host hello" })).await;
        return;
    }
    let id = relay_id(secret);
    let generation = relay.generations.fetch_add(1, Ordering::Relaxed);
    let (to_host, mut from_relay) = mpsc::channel::<String>(256);
    let admitted = {
        let mut hosts = relay.hosts.lock().unwrap();
        let full = hosts.len() >= limits.hosts && !hosts.contains_key(&id);
        if !full {
            // A newer connection of the same harlo replaces the old one.
            hosts.insert(id.clone(), Host { generation, to_host, phones: HashMap::new(), next_phone: 1 });
        }
        !full
    };
    if !admitted {
        send_json(&mut sink, json!({ "type": "error", "error": "the relay is full; try again later" })).await;
        return;
    }
    if send_json(&mut sink, json!({ "type": "hosting", "id": id })).await {
        let writer = async {
            while let Some(t) = from_relay.recv().await {
                if sink.send(Message::Text(t.into())).await.is_err() {
                    break;
                }
            }
        };
        let reader = async {
            let mut budget = Budget::new(&limits);
            while let Some(text) = next_text(&mut stream, limits.idle).await {
                budget.spend(text.len()).await;
                let v: Value = serde_json::from_str(&text).unwrap_or_default();
                let c = v["c"].as_u64().unwrap_or(0);
                // Who to give it to, without holding the lock while waiting.
                let target = {
                    let mut hosts = relay.hosts.lock().unwrap();
                    let Some(h) = hosts.get_mut(&id).filter(|h| h.generation == generation) else { break };
                    match v["type"].as_str() {
                        Some("data") => h.phones.get(&c).cloned(),
                        Some("close") => {
                            h.phones.remove(&c);
                            None
                        }
                        _ => None,
                    }
                };
                if let (Some(tx), Some(d)) = (target, v["d"].as_str()) {
                    let _ = tx.send(d.to_owned()).await;
                }
            }
        };
        tokio::select! {
            _ = writer => {}
            _ = reader => {}
        }
    }
    let mut hosts = relay.hosts.lock().unwrap();
    if hosts.get(&id).is_some_and(|h| h.generation == generation) {
        // Its phones' senders go with it: their sockets close.
        hosts.remove(&id);
    }
}

async fn phone(socket: WebSocket, relay: Arc<Relay>, id: String, _slot: IpSlot) {
    let limits = relay.config.limits.clone();
    let (mut sink, mut stream) = socket.split();
    let (to_phone, mut for_phone) = mpsc::channel::<String>(256);
    let joined = {
        let mut hosts = relay.hosts.lock().unwrap();
        match hosts.get_mut(&id) {
            Some(h) if h.phones.len() < limits.phones_per_host => {
                let c = h.next_phone;
                h.next_phone += 1;
                h.phones.insert(c, to_phone);
                let _ = h.to_host.try_send(json!({ "type": "open", "c": c }).to_string());
                Ok((c, h.to_host.clone(), h.generation))
            }
            Some(_) => Err((4429, "too many phones connected to this harlo")),
            None => Err((4404, "harlo isn't connected to this relay right now")),
        }
    };
    let (c, to_host, generation) = match joined {
        Ok(j) => j,
        Err((code, reason)) => {
            let _ = sink.send(Message::Close(Some(CloseFrame { code, reason: reason.into() }))).await;
            return;
        }
    };
    let writer = async {
        while let Some(t) = for_phone.recv().await {
            if sink.send(Message::Text(t.into())).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    };
    let reader = async {
        let mut budget = Budget::new(&limits);
        while let Some(text) = next_text(&mut stream, limits.idle).await {
            budget.spend(text.len()).await;
            if to_host.send(json!({ "type": "data", "c": c, "d": text }).to_string()).await.is_err() {
                break;
            }
        }
    };
    tokio::select! {
        _ = writer => {}
        _ = reader => {}
    }
    let mut hosts = relay.hosts.lock().unwrap();
    if let Some(h) = hosts.get_mut(&id).filter(|h| h.generation == generation)
        && h.phones.remove(&c).is_some()
    {
        let _ = h.to_host.try_send(json!({ "type": "close", "c": c }).to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as M;

    async fn start_relay() -> std::net::SocketAddr {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(l, router()).await.unwrap() });
        addr
    }

    async fn text(ws: &mut (impl StreamExt<Item = Result<M, tokio_tungstenite::tungstenite::Error>> + Unpin)) -> Value {
        loop {
            match tokio::time::timeout(Duration::from_secs(5), ws.next()).await.unwrap() {
                Some(Ok(M::Text(t))) => return serde_json::from_str(&t).unwrap_or(json!({ "raw": t.to_string() })),
                Some(Ok(M::Close(f))) => return json!({ "closed": f.map(|f| u16::from(f.code)).unwrap_or(0) }),
                Some(Ok(_)) => continue,
                _ => return Value::Null,
            }
        }
    }

    /// The pages the App Store linked to, support and privacy: sent on to
    /// harlo's website.
    #[tokio::test]
    async fn sends_its_pages_to_the_website() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let addr = start_relay().await;
        for path in ["/support", "/privacy"] {
            let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
            c.write_all(format!("GET {path} HTTP/1.1\r\nHost: relay\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
            let mut page = String::new();
            c.read_to_string(&mut page).await.unwrap();
            assert!(page.starts_with("HTTP/1.1 308") && page.to_lowercase().contains(&format!("location: https://harlo.run{path}")), "{path}: {}", &page[..page.len().min(200)]);
        }
    }

    /// Notifications: sent on to Apple (a stand-in speaking HTTP/2) with the
    /// app's key; Apple's "gone" comes back; what isn't a notification,
    /// and too many to one phone, are refused; a relay without the key says so.
    #[tokio::test]
    async fn sends_notifications_on() {
        use axum::extract::Path;
        let seen: Arc<Mutex<Vec<(String, String)>>> = Default::default();
        let kinds: Arc<Mutex<Vec<(String, String)>>> = Default::default();
        let apple = {
            let (seen, kinds) = (seen.clone(), kinds.clone());
            axum::Router::new().route("/3/device/{token}", axum::routing::post(move |Path(token): Path<String>, headers: HeaderMap| {
                let (seen, kinds) = (seen.clone(), kinds.clone());
                async move {
                    let header = |h: &str| headers.get(h).and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
                    let topic = header("apns-topic");
                    kinds.lock().unwrap().push((header("apns-push-type"), header("apns-priority")));
                    if topic.ends_with(".liveactivity") {
                        return (StatusCode::OK, "");
                    }
                    seen.lock().unwrap().push((token.clone(), topic));
                    if token == "dead" { (StatusCode::GONE, r#"{"reason":"Unregistered"}"#) } else { (StatusCode::OK, "") }
                }
            }))
        };
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let apple_addr = l.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(l, apple).await.unwrap() });

        let credentials = harlo_apns::Credentials::new("ABCDE12345", "KEY1234567", &harlo_apns::test_p8()).unwrap();
        let client = harlo_apns::Client::new(credentials, Some(format!("http://{apple_addr}"))).unwrap();
        let config = Config {
            limits: Limits { pushes_per_phone: 2, ..Default::default() },
            push: Some(Push { client: Arc::new(client), topic: "com.arcticlatent.harlo".into() }),
            ..Default::default()
        };
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(serve(l, config, std::future::pending()));
        let relay = harlo_apns::Forwarder::new(&format!("http://{addr}")).unwrap();
        assert_eq!(relay.available().await, Ok(true));

        let payload = json!({ "aps": { "badge": 1 } });
        let push = |token| harlo_apns::Push { device_token: token, sandbox: true, topic: "ignored", payload: &payload, collapse_id: None, kind: harlo_apns::Kind::Alert };
        assert_eq!(relay.send(&push("abcd01")).await, harlo_apns::Sent::Ok);
        assert_eq!(relay.send(&push("dead")).await, harlo_apns::Sent::Gone("Unregistered".into()));
        assert!(matches!(relay.send(&push("not a token")).await, harlo_apns::Sent::Failed(e) if e.contains("device token")));
        let not_a_notification = json!({ "hello": 1 });
        let bad = harlo_apns::Push { device_token: "abcd01", sandbox: true, topic: "", payload: &not_a_notification, collapse_id: None, kind: harlo_apns::Kind::Alert };
        assert!(matches!(relay.send(&bad).await, harlo_apns::Sent::Failed(e) if e.contains("not a notification")));
        // A Live Activity's update goes to the app's Live Activity topic, as one.
        let update = json!({ "aps": { "event": "update", "timestamp": 1_700_000_000, "content-state": { "phase": "working" } } });
        let activity = harlo_apns::Push { device_token: "ac01", sandbox: true, topic: "", payload: &update, collapse_id: None, kind: harlo_apns::Kind::Activity { urgent: false } };
        assert_eq!(relay.send(&activity).await, harlo_apns::Sent::Ok);
        assert_eq!(kinds.lock().unwrap().last().cloned(), Some(("liveactivity".into(), "5".into())));
        let no_state = harlo_apns::Push { payload: &payload, ..activity };
        assert!(matches!(relay.send(&no_state).await, harlo_apns::Sent::Failed(e) if e.contains("Live Activity")));
        // Two a minute to a phone here: the third waits.
        assert_eq!(relay.send(&push("abcd01")).await, harlo_apns::Sent::Ok);
        assert!(matches!(relay.send(&push("abcd01")).await, harlo_apns::Sent::Failed(e) if e.contains("too many")));
        // Apple got them for the app (the relay's topic, not the sender's).
        assert_eq!(*seen.lock().unwrap(), vec![("abcd01".into(), "com.arcticlatent.harlo".into()), ("dead".into(), "com.arcticlatent.harlo".into()),
                                                ("abcd01".into(), "com.arcticlatent.harlo".into())]);

        // Without the key.
        let plain = start_relay().await;
        let relay = harlo_apns::Forwarder::new(&format!("http://{plain}")).unwrap();
        assert_eq!(relay.available().await, Ok(false));
        assert!(matches!(relay.send(&push("abcd01")).await, harlo_apns::Sent::Failed(e) if e.contains("doesn't send notifications")));
    }

    #[tokio::test]
    async fn limits_connections_per_address() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        let config = Config { limits: Limits { per_ip: 2, ..Default::default() }, ..Default::default() };
        tokio::spawn(serve(l, config, std::future::pending()));
        // Two harlos waiting to say hello hold their slots.
        let url = format!("ws://{addr}/host");
        let (a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        let (b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        // A third from the same address: refused before the upgrade.
        assert!(tokio_tungstenite::connect_async(format!("ws://{addr}/ws?id=x")).await.is_err());
        drop((a, b));
        tokio::time::sleep(Duration::from_millis(200)).await;
        let (_, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/host")).await.unwrap();
    }

    #[test]
    fn forwarded_addresses() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.7, 10.0.0.1".parse().unwrap());
        assert_eq!(forwarded_ip(&h), Some("203.0.113.7".parse().unwrap()));
        h.insert("cf-connecting-ip", "2001:db8::1".parse().unwrap());
        assert_eq!(forwarded_ip(&h), Some("2001:db8::1".parse().unwrap()));
    }

    #[tokio::test]
    async fn closes_quiet_connections() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        let config = Config { limits: Limits { idle: Duration::from_millis(300), ..Default::default() }, ..Default::default() };
        tokio::spawn(serve(l, config, std::future::pending()));
        let (mut host, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/host")).await.unwrap();
        host.send(M::Text(json!({ "type": "host", "secret": "q".repeat(43) }).to_string().into())).await.unwrap();
        assert_eq!(text(&mut host).await["type"], "hosting");
        // Pings keep it.
        for _ in 0..3 {
            tokio::time::sleep(Duration::from_millis(150)).await;
            host.send(M::Ping(Vec::new().into())).await.unwrap();
        }
        // Silence ends it.
        let start = Instant::now();
        assert!(text(&mut host).await.get("closed").is_some() || start.elapsed() < Duration::from_secs(5));
        assert!(start.elapsed() >= Duration::from_millis(250));
    }

    #[tokio::test]
    async fn forwards_between_harlo_and_phones() {
        let addr = start_relay().await;
        let secret = "s".repeat(43);
        // No harlo yet: a phone is turned away.
        let (mut early, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws?id={}", relay_id(&secret))).await.unwrap();
        assert_eq!(text(&mut early).await["closed"], 4404);

        let (mut host, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/host")).await.unwrap();
        host.send(M::Text(json!({ "type": "host", "secret": secret }).to_string().into())).await.unwrap();
        let hosting = text(&mut host).await;
        assert_eq!(hosting["id"], relay_id(&secret));

        let (mut ph, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws?id={}", relay_id(&secret))).await.unwrap();
        let open = text(&mut host).await;
        assert_eq!(open["type"], "open");
        let c = open["c"].as_u64().unwrap();
        ph.send(M::Text("hello".into())).await.unwrap();
        assert_eq!(text(&mut host).await, json!({ "type": "data", "c": c, "d": "hello" }));
        host.send(M::Text(json!({ "type": "data", "c": c, "d": "{\"ok\":1}" }).to_string().into())).await.unwrap();
        assert_eq!(text(&mut ph).await, json!({ "ok": 1 }));
        // The phone goes: harlo hears.
        ph.close(None).await.unwrap();
        assert_eq!(text(&mut host).await, json!({ "type": "close", "c": c }));

        // Another secret gets another id: it can't take this one.
        let (mut other, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/host")).await.unwrap();
        other.send(M::Text(json!({ "type": "host", "secret": "x".repeat(43) }).to_string().into())).await.unwrap();
        assert_ne!(text(&mut other).await["id"], hosting["id"]);
        // A short secret is refused.
        let (mut weak, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/host")).await.unwrap();
        weak.send(M::Text(json!({ "type": "host", "secret": "short" }).to_string().into())).await.unwrap();
        assert_eq!(text(&mut weak).await["type"], "error");
        // harlo goes: its phones are closed.
        let (mut ph2, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws?id={}", relay_id(&secret))).await.unwrap();
        let _ = text(&mut host).await;
        host.close(None).await.unwrap();
        assert!(text(&mut ph2).await.get("closed").is_some());
        // The app is served.
        let (mut s, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws?id=nope")).await.unwrap();
        assert_eq!(text(&mut s).await["closed"], 4404);
    }
}
