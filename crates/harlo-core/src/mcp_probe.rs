//! Tests an MCP server the way the agents would reach it: starts a local
//! (stdio) one, or calls a remote (streamable HTTP, or HTTP+SSE) one,
//! initializes, and
//! lists its tools. harlo shows the result as the server's status. (The
//! harnesses don't report MCP status over RPC.)

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::mcp::Form;

const PROTOCOL: &str = "2025-06-18";

/// What a test found.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Probe {
    /// "connected", "failed" or "needs_auth".
    pub state: String,
    pub error: String,
    /// Server name and version, when it says.
    pub server: String,
    /// (name, description).
    pub tools: Vec<(String, String)>,
}

impl Probe {
    fn failed(error: impl Into<String>) -> Self {
        Probe { state: "failed".into(), error: error.into(), ..Default::default() }
    }
}

fn initialize(id: u64) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": "initialize", "params": {
        "protocolVersion": PROTOCOL, "capabilities": {}, "clientInfo": { "name": "harlo", "version": env!("CARGO_PKG_VERSION") } } })
}

fn server_info(result: &Value) -> String {
    let info = &result["serverInfo"];
    [info["name"].as_str().unwrap_or_default(), info["version"].as_str().unwrap_or_default()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ")
}

fn tools_of(result: &Value) -> Vec<(String, String)> {
    result["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| (t["name"].as_str().unwrap_or_default().to_owned(), t["description"].as_str().unwrap_or_default().trim().to_owned()))
        .collect()
}

/// `${VAR}`, `${VAR:-default}` from the environment; a whole value of
/// `!command` is that command's output (as Harlo does).
pub fn expand(value: &str) -> String {
    if let Some(cmd) = value.strip_prefix('!') {
        let out = Command::new("sh").arg("-c").arg(cmd).stdin(Stdio::null()).stderr(Stdio::null()).output();
        return out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default();
    }
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = &rest[start + 2..start + end];
        let (name, default) = inner.split_once(":-").unwrap_or((inner, ""));
        out.push_str(&std::env::var(name).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| default.to_owned()));
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

/// Harlo's sign-in to the server at `url` (`harlo-agent mcp login`, kept in
/// its `mcp-auth.json`) as a bearer token, refreshed (and kept) when it's
/// about to expire, as Harlo would. None without one.
pub fn harlo_token(url: &str) -> Option<String> {
    let dir = crate::mcp::harlo_dir();
    let file = dir.join("mcp-auth.json");
    let read = || -> Option<Value> { serde_json::from_str::<Value>(&std::fs::read_to_string(&file).ok()?).ok() };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let fresh = |t: &Value| t["expires_at"].as_u64().is_none_or(|e| e == 0 || e > now + 60);
    let t = read()?.get(url)?.clone();
    if fresh(&t) || t["refresh_token"].as_str().is_none_or(str::is_empty) {
        return t["access_token"].as_str().map(str::to_owned);
    }
    // (Under Harlo's lock: a refresh token can be used once.)
    let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join("mcp-auth.lock")).ok()?;
    lock.lock().ok()?;
    let mut all = read()?;
    let t = all.get(url)?.clone();
    if fresh(&t) {
        return t["access_token"].as_str().map(str::to_owned);
    }
    let s = |k: &str| t[k].as_str().unwrap_or_default().to_owned();
    let mut form = vec![("grant_type", "refresh_token".to_owned()), ("refresh_token", s("refresh_token")), ("client_id", s("client_id"))];
    for k in ["resource", "client_secret"] {
        if !s(k).is_empty() {
            form.push((k, s(k)));
        }
    }
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).build().into();
    let mut resp = agent.post(&s("token_endpoint")).header("Accept", "application/json").send_form(form).ok()?;
    let got: Value = serde_json::from_str(&resp.body_mut().read_to_string().ok()?).ok()?;
    let access = got["access_token"].as_str()?.to_owned();
    let entry = &mut all[url];
    entry["access_token"] = json!(access);
    if let Some(r) = got["refresh_token"].as_str() {
        entry["refresh_token"] = json!(r);
    }
    entry["expires_at"] = json!(got["expires_in"].as_u64().map_or(0, |e| now + e));
    let tmp = file.with_extension("tmp");
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp).ok()?;
        f.write_all(serde_json::to_string_pretty(&all).ok()?.as_bytes()).ok()?;
    }
    std::fs::rename(&tmp, &file).ok()?;
    Some(access)
}

/// Tests `form`. `project` is the folder relative `cwd`s start from (a
/// remote project's server runs on its host, over SSH).
pub fn probe(form: &Form, project: Option<&Path>, timeout: Duration) -> Probe {
    if form.transport == "stdio" {
        probe_stdio(form, project, timeout)
    } else if form.transport == "sse" {
        probe_sse(form, timeout)
    } else {
        probe_http(form, timeout)
    }
}

fn probe_stdio(form: &Form, project: Option<&Path>, timeout: Duration) -> Probe {
    let Some(program) = form.command.first() else { return Probe::failed("No command.") };
    let remote = project.and_then(|p| crate::remote::split(p).and_then(|(h, dir)| Some((crate::remote::host(&h)?, dir))));
    let mut cmd = match &remote {
        // On the host: its shell expands the variables.
        Some((host, dir)) => {
            let q = crate::remote::quote;
            let cwd = if form.cwd.is_empty() { dir.clone() } else if form.cwd.starts_with('/') { form.cwd.clone() } else { format!("{dir}/{}", form.cwd) };
            let env: Vec<String> = form.env.iter().map(|(k, v)| format!("{k}={}", match v.strip_prefix('!') { Some(cmd) => format!("\"$({cmd})\""), None => q(v) })).collect();
            let script = format!("cd {} && exec env {} {}", q(&cwd), env.join(" "), form.command.iter().map(|w| q(w)).collect::<Vec<_>>().join(" "));
            crate::remote::command(host, &script)
        }
        None => {
            let mut c = Command::new(expand(program));
            c.args(form.command[1..].iter().map(|a| expand(a)));
            for (k, v) in &form.env {
                c.env(k, expand(v));
            }
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_else(|| "/".into());
            let base = project.map(Path::to_path_buf).unwrap_or(home.clone());
            let cwd = if form.cwd.is_empty() { base } else if let Some(r) = form.cwd.strip_prefix("~/") { home.join(r) } else { base.join(&form.cwd) };
            if cwd.is_dir() {
                c.current_dir(cwd);
            }
            c
        }
    };
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Probe::failed(format!("Could not start {program}: {e}")),
    };
    let stdin = child.stdin.take().expect("piped");
    let (tx, rx) = mpsc::channel::<String>();
    let out = child.stdout.take().expect("piped");
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let err = child.stderr.take().expect("piped");
    let stderr = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let tail = stderr.clone();
    // Said when the server's error output ends (it exited).
    let (err_done, err_ended) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        for line in BufReader::new(err).lines().map_while(Result::ok) {
            let mut t = tail.lock().unwrap();
            t.push(line);
            if t.len() > 20 {
                t.remove(0);
            }
        }
        let _ = err_done.send(());
    });
    let mut conn = Stdio_ { stdin, rx, deadline: Instant::now() + timeout, timeout };
    let result = (|| {
        let init = conn.request(1, initialize(1))?;
        let mut probe = Probe { state: "connected".into(), server: server_info(&init), ..Default::default() };
        conn.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))?;
        let mut cursor: Option<String> = None;
        for page in 0..20u64 {
            let mut params = json!({});
            if let Some(c) = &cursor {
                params["cursor"] = json!(c);
            }
            let id = 2 + page;
            let result = conn.request(id, json!({ "jsonrpc": "2.0", "id": id, "method": "tools/list", "params": params }))?;
            probe.tools.extend(tools_of(&result));
            cursor = result["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok::<Probe, String>(probe)
    })();
    let _ = child.kill();
    let _ = child.wait();
    // What it said before it went (a crash's reason) may still be on its way
    // in: a moment for it (not longer: something it started may hold it open).
    if result.is_err() {
        let _ = err_ended.recv_timeout(Duration::from_millis(500));
    }
    result.unwrap_or_else(|e| {
        let last = stderr.lock().unwrap().iter().rev().find(|l| !l.trim().is_empty()).cloned().unwrap_or_default();
        Probe::failed(if last.is_empty() { e } else { format!("{e} {last}") })
    })
}

/// A stdio connection: messages are lines of JSON.
struct Stdio_ {
    stdin: std::process::ChildStdin,
    rx: mpsc::Receiver<String>,
    deadline: Instant,
    timeout: Duration,
}

impl Stdio_ {
    fn send(&mut self, msg: &Value) -> Result<(), String> {
        writeln!(self.stdin, "{msg}").and_then(|_| self.stdin.flush()).map_err(|e| format!("The server closed its input: {e}"))
    }

    /// Sends a request and waits for its answer (skipping logs,
    /// notifications and the server's own requests).
    fn request(&mut self, id: u64, msg: Value) -> Result<Value, String> {
        self.send(&msg)?;
        loop {
            match self.rx.recv_timeout(self.deadline.saturating_duration_since(Instant::now())) {
                Ok(line) => {
                    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { continue };
                    if v["id"] != json!(id) || v.get("method").is_some() {
                        continue;
                    }
                    if let Some(e) = v.get("error") {
                        return Err(e["message"].as_str().unwrap_or("error").to_owned());
                    }
                    return Ok(v["result"].clone());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => return Err(format!("No answer in {} s.", self.timeout.as_secs())),
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err("The server exited.".into()),
            }
        }
    }
}

fn probe_http(form: &Form, timeout: Duration) -> Probe {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(timeout)).http_status_as_error(false).build().into();
    let url = expand(form.url.trim());
    let headers: Vec<(String, String)> = form.headers.iter().map(|(k, v)| (k.clone(), expand(v))).collect();
    let mut session: Option<String> = None;
    let mut post = |body: &Value| -> Result<Option<Value>, Probe> {
        let mut req = agent.post(&url).header("Content-Type", "application/json").header("Accept", "application/json, text/event-stream").header("MCP-Protocol-Version", PROTOCOL);
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(s) = &session {
            req = req.header("Mcp-Session-Id", s.as_str());
        }
        let mut resp = req.send(body.to_string()).map_err(|e| Probe::failed(format!("Could not reach {url}: {e}")))?;
        let status = resp.status().as_u16();
        if status == 401 || status == 403 {
            return Err(Probe { state: "needs_auth".into(), error: "The server wants you to sign in.".into(), ..Default::default() });
        }
        if let Some(s) = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            session = Some(s.to_owned());
        }
        if body.get("id").is_none() {
            return Ok(None);
        }
        let event_stream = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).is_some_and(|c| c.starts_with("text/event-stream"));
        let text = resp.body_mut().read_to_string().map_err(|e| Probe::failed(format!("Reading the answer: {e}")))?;
        if !(200..300).contains(&status) {
            let detail: String = text.chars().take(200).collect();
            return Err(Probe::failed(format!("HTTP {status} from {url}{}", if detail.trim().is_empty() { String::new() } else { format!(": {}", detail.trim()) })));
        }
        // JSON, or the first matching `data:` of an event stream.
        let candidates: Vec<String> = if event_stream {
            text.lines().filter_map(|l| l.strip_prefix("data:")).map(|d| d.trim().to_owned()).collect()
        } else {
            vec![text]
        };
        for c in candidates {
            let Ok(v) = serde_json::from_str::<Value>(&c) else { continue };
            if v["id"] == body["id"] {
                if let Some(e) = v.get("error") {
                    return Err(Probe::failed(e["message"].as_str().unwrap_or("error").to_owned()));
                }
                return Ok(Some(v["result"].clone()));
            }
        }
        Err(Probe::failed("The server's answer didn't make sense as MCP."))
    };
    let mut run = || -> Result<Probe, Probe> {
        let init = post(&initialize(1))?.unwrap_or_default();
        let mut probe = Probe { state: "connected".into(), server: server_info(&init), ..Default::default() };
        post(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))?;
        let mut cursor: Option<String> = None;
        for page in 0..20u64 {
            let mut params = json!({});
            if let Some(c) = &cursor {
                params["cursor"] = json!(c);
            }
            let result = post(&json!({ "jsonrpc": "2.0", "id": 2 + page, "method": "tools/list", "params": params }))?.unwrap_or_default();
            probe.tools.extend(tools_of(&result));
            cursor = result["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok(probe)
    };
    run().unwrap_or_else(|p| p)
}

/// `to` (an SSE server's `endpoint`) against the stream's address.
fn join_url(base: &str, to: &str) -> String {
    if to.starts_with("http://") || to.starts_with("https://") {
        return to.to_owned();
    }
    let origin_end = base.find("://").map_or(0, |i| i + 3);
    let origin = &base[..base[origin_end..].find('/').map_or(base.len(), |i| origin_end + i)];
    if to.starts_with('/') {
        return format!("{origin}{to}");
    }
    let path = &base[origin.len()..];
    let dir = &path[..path.rfind('/').map_or(0, |i| i + 1)];
    format!("{origin}{}{to}", if dir.is_empty() { "/" } else { dir })
}

/// The older transport: a stream (GET) whose `endpoint` event says where
/// to POST messages; the answers come on the stream.
fn probe_sse(form: &Form, timeout: Duration) -> Probe {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_connect(Some(timeout)).http_status_as_error(false).build().into();
    let url = expand(form.url.trim());
    let headers: Vec<(String, String)> = form.headers.iter().map(|(k, v)| (k.clone(), expand(v))).collect();
    let deadline = Instant::now() + timeout;
    let mut get = agent.get(&url).header("Accept", "text/event-stream");
    for (k, v) in &headers {
        get = get.header(k.as_str(), v.as_str());
    }
    let resp = match get.call() {
        Ok(r) => r,
        Err(e) => return Probe::failed(format!("Could not reach {url}: {e}")),
    };
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return Probe { state: "needs_auth".into(), error: "The server wants you to sign in.".into(), ..Default::default() };
    }
    if !(200..300).contains(&status) {
        return Probe::failed(format!("HTTP {status} from {url}"));
    }
    // The stream's events, as they come.
    let (tx, rx) = mpsc::channel::<(String, String)>();
    std::thread::spawn(move || {
        let reader = BufReader::new(resp.into_body().into_reader());
        let (mut event, mut data) = ("message".to_owned(), Vec::<String>::new());
        for line in reader.lines().map_while(Result::ok) {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                if !data.is_empty() && tx.send((std::mem::replace(&mut event, "message".into()), data.join("\n"))).is_err() {
                    break;
                }
                data.clear();
                event = "message".into();
            } else if let Some(e) = line.strip_prefix("event:") {
                event = e.trim().to_owned();
            } else if let Some(d) = line.strip_prefix("data:") {
                data.push(d.strip_prefix(' ').unwrap_or(d).to_owned());
            }
        }
    });
    let next = |rx: &mpsc::Receiver<(String, String)>| -> Result<(String, String), Probe> {
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now())).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => Probe::failed(format!("No answer in {} s.", timeout.as_secs())),
            mpsc::RecvTimeoutError::Disconnected => Probe::failed("The server closed its stream."),
        })
    };
    let run = || -> Result<Probe, Probe> {
        let endpoint = loop {
            let (event, data) = next(&rx)?;
            if event == "endpoint" {
                break join_url(&url, data.trim());
            }
        };
        let post = |body: &Value| -> Result<(), Probe> {
            let mut req = agent.post(&endpoint).header("Content-Type", "application/json");
            for (k, v) in &headers {
                req = req.header(k.as_str(), v.as_str());
            }
            let resp = req.send(body.to_string()).map_err(|e| Probe::failed(format!("Could not reach {endpoint}: {e}")))?;
            match resp.status().as_u16() {
                401 | 403 => Err(Probe { state: "needs_auth".into(), error: "The server wants you to sign in.".into(), ..Default::default() }),
                s if (200..300).contains(&s) => Ok(()),
                s => Err(Probe::failed(format!("HTTP {s} from {endpoint}"))),
            }
        };
        let request = |body: Value| -> Result<Value, Probe> {
            post(&body)?;
            loop {
                let (_, data) = next(&rx)?;
                let Ok(v) = serde_json::from_str::<Value>(&data) else { continue };
                // (The server asking something of its own: answered.)
                if v.get("method").is_some() && v.get("id").is_some() {
                    post(&json!({ "jsonrpc": "2.0", "id": v["id"], "result": {} }))?;
                    continue;
                }
                if v["id"] == body["id"] && v.get("method").is_none() {
                    if let Some(e) = v.get("error") {
                        return Err(Probe::failed(e["message"].as_str().unwrap_or("error").to_owned()));
                    }
                    return Ok(v["result"].clone());
                }
            }
        };
        let init = request(initialize(1))?;
        let mut probe = Probe { state: "connected".into(), server: server_info(&init), ..Default::default() };
        post(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))?;
        let mut cursor: Option<String> = None;
        for page in 0..20u64 {
            let mut params = json!({});
            if let Some(c) = &cursor {
                params["cursor"] = json!(c);
            }
            let result = request(json!({ "jsonrpc": "2.0", "id": 2 + page, "method": "tools/list", "params": params }))?;
            probe.tools.extend(tools_of(&result));
            cursor = result["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok(probe)
    };
    run().unwrap_or_else(|p| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_values() {
        // SAFETY: tests set a variable no other test reads.
        unsafe { std::env::set_var("HARLO_PROBE_TEST", "v") };
        assert_eq!(expand("a ${HARLO_PROBE_TEST} ${HARLO_NOPE:-d} ${HARLO_NOPE}"), "a v d ");
        assert_eq!(expand("!printf hi"), "hi");
        assert_eq!(expand("plain"), "plain");
    }

    /// A tiny MCP server in sh: answers initialize and tools/list.
    const FAKE_SERVER: &str = r#"while read -r line; do
case "$line" in
*'"initialize"'*) echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1.0"}}}' ;;
*'"tools/list"'*) echo '{"jsonrpc":"2.0","method":"notifications/message","params":{}}'; echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"look","description":"Looks."},{"name":"cook"}]}}' ;;
esac
done"#;

    #[test]
    fn tests_a_stdio_server() {
        let form = Form { name: "fake".into(), transport: "stdio".into(), command: vec!["sh".into(), "-c".into(), FAKE_SERVER.into()], enabled: true, ..Default::default() };
        let p = probe(&form, None, Duration::from_secs(5));
        assert_eq!(p.state, "connected", "{p:?}");
        assert_eq!(p.server, "fake 1.0");
        assert_eq!(p.tools, [("look".into(), "Looks.".into()), ("cook".into(), String::new())]);
    }

    #[test]
    fn reports_failures() {
        let missing = Form { transport: "stdio".into(), command: vec!["harlo-no-such-server".into()], ..Default::default() };
        assert!(probe(&missing, None, Duration::from_secs(5)).error.contains("Could not start"));
        let crash = Form { transport: "stdio".into(), command: vec!["sh".into(), "-c".into(), "echo boom >&2; exit 3".into()], ..Default::default() };
        let p = probe(&crash, None, Duration::from_secs(5));
        assert!(p.state == "failed" && p.error.contains("boom"), "{p:?}");
        let silent = Form { transport: "stdio".into(), command: vec!["sleep".into(), "30".into()], ..Default::default() };
        assert!(probe(&silent, None, Duration::from_secs(1)).error.contains("No answer"));
        let nowhere = Form { transport: "http".into(), url: "http://127.0.0.1:9/mcp".into(), ..Default::default() };
        assert!(probe(&nowhere, None, Duration::from_secs(3)).error.contains("Could not reach"));
    }

    /// A tiny streamable-HTTP MCP server (or one that wants a sign-in).
    fn http_server(auth: bool) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(6) {
                let Ok(mut stream) = stream else { continue };
                // Headers, then the body (Content-Length).
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    data.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&data).into_owned();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let len = text.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                        if data.len() >= end + 4 + len || n == 0 {
                            break;
                        }
                    } else if n == 0 {
                        break;
                    }
                }
                let req = String::from_utf8_lossy(&data).into_owned();
                let (status, ctype, body) = if auth {
                    ("401 Unauthorized", "application/json", "{}".to_owned())
                } else if req.contains("\"initialize\"") {
                    ("200 OK", "text/event-stream", "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"serverInfo\":{\"name\":\"web\"}}}\n\n".to_owned())
                } else if req.contains("tools/list") {
                    assert!(req.to_ascii_lowercase().contains("mcp-session-id: s1"), "session id sent back");
                    ("200 OK", "application/json", "{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[{\"name\":\"fetch\",\"description\":\"Fetches.\"}]}}".to_owned())
                } else {
                    ("202 Accepted", "application/json", String::new())
                };
                let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nMcp-Session-Id: s1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        format!("http://127.0.0.1:{port}/mcp")
    }

    #[test]
    fn uses_harlos_sign_in() {
        let _one = crate::mcp::tests::HARLO_DIR.lock().unwrap_or_else(|e| e.into_inner());
        let dir = crate::mcp::harlo_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let later = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 3600;
        std::fs::write(dir.join("mcp-auth.json"), json!({ "https://h/mcp": { "access_token": "t1", "expires_at": later, "client_id": "c", "token_endpoint": "https://h/token" }, "https://o/mcp": { "access_token": "t2" } }).to_string()).unwrap();
        assert_eq!(harlo_token("https://h/mcp").as_deref(), Some("t1"));
        assert_eq!(harlo_token("https://o/mcp").as_deref(), Some("t2"));
        assert_eq!(harlo_token("https://none/mcp"), None);
        let _ = std::fs::remove_file(dir.join("mcp-auth.json"));
    }

    #[test]
    fn joins_endpoints() {
        assert_eq!(join_url("http://h:1/a/sse", "/messages?s=1"), "http://h:1/messages?s=1");
        assert_eq!(join_url("http://h:1/a/sse", "msg"), "http://h:1/a/msg");
        assert_eq!(join_url("http://h:1", "msg"), "http://h:1/msg");
        assert_eq!(join_url("http://h/sse", "https://o/x"), "https://o/x");
    }

    /// A tiny HTTP+SSE MCP server: GET /sse streams, POST /m answers there.
    fn sse_server() -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel::<String>();
        let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
        std::thread::spawn(move || {
            for stream in listener.incoming().take(8) {
                let Ok(mut stream) = stream else { continue };
                let (tx, rx) = (tx.clone(), rx.clone());
                std::thread::spawn(move || {
                    let mut data = Vec::new();
                    let mut buf = [0u8; 4096];
                    loop {
                        let n = stream.read(&mut buf).unwrap_or(0);
                        data.extend_from_slice(&buf[..n]);
                        let text = String::from_utf8_lossy(&data).into_owned();
                        if let Some(end) = text.find("\r\n\r\n") {
                            let len = text.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                            if data.len() >= end + 4 + len || n == 0 {
                                break;
                            }
                        } else if n == 0 {
                            break;
                        }
                    }
                    let req = String::from_utf8_lossy(&data).into_owned();
                    if req.starts_with("GET") {
                        let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\nevent: endpoint\ndata: /m?s=1\n\n");
                        let _ = stream.flush();
                        while let Ok(msg) = rx.lock().unwrap().recv() {
                            if write!(stream, "event: message\ndata: {msg}\n\n").and_then(|_| stream.flush()).is_err() {
                                break;
                            }
                        }
                        return;
                    }
                    assert!(req.starts_with("POST /m?s=1"), "{req}");
                    let _ = write!(stream, "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    if req.contains("\"initialize\"") {
                        let _ = tx.send(r#"{"jsonrpc":"2.0","id":"srv","method":"ping"}"#.into());
                        let _ = tx.send(r#"{"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"old","version":"2"}}}"#.into());
                    } else if req.contains("tools/list") {
                        let _ = tx.send(r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"shout","description":"Shouts."}]}}"#.into());
                    }
                });
            }
        });
        format!("http://127.0.0.1:{port}/sse")
    }

    #[test]
    fn tests_an_sse_server() {
        let form = Form { transport: "sse".into(), url: sse_server(), ..Default::default() };
        let p = probe(&form, None, Duration::from_secs(5));
        assert_eq!((p.state.as_str(), p.server.as_str()), ("connected", "old 2"), "{p:?}");
        assert_eq!(p.tools, [("shout".into(), "Shouts.".into())]);
        let nowhere = Form { transport: "sse".into(), url: "http://127.0.0.1:9/sse".into(), ..Default::default() };
        assert!(probe(&nowhere, None, Duration::from_secs(3)).error.contains("Could not reach"));
    }

    #[test]
    fn tests_an_http_server() {
        let form = Form { transport: "http".into(), url: http_server(false), headers: vec![("X-Key".into(), "k".into())], ..Default::default() };
        let p = probe(&form, None, Duration::from_secs(5));
        assert_eq!((p.state.as_str(), p.server.as_str()), ("connected", "web"), "{p:?}");
        assert_eq!(p.tools, [("fetch".into(), "Fetches.".into())]);
        let locked = Form { transport: "http".into(), url: http_server(true), ..Default::default() };
        assert_eq!(probe(&locked, None, Duration::from_secs(5)).state, "needs_auth");
    }
}
