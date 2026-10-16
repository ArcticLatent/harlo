//! The running harlo's side of orchestration (see `harlo_core::orchestration`):
//! a local socket where `harlo --mcp-bridge` hands over agents' tool calls.
//! Each call is carried out on the UI thread, where the sessions are.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::time::Duration;

use cxx_qt::CxxQtThread;
use serde_json::{Value, json};

use crate::workspace::qobject::Workspace;

/// Listens for the bridges (in the background) for as long as harlo runs.
pub fn serve(thread: CxxQtThread<Workspace>) {
    let path = harlo_core::orchestration::socket_path();
    // A socket left by an earlier harlo.
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("orchestration: can't listen on {}: {e}", path.display());
            return;
        }
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    std::thread::Builder::new()
        .name("orchestration".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let thread = thread.clone();
                std::thread::spawn(move || {
                    let mut line = String::new();
                    if BufReader::new(&stream).read_line(&mut line).is_err() {
                        return;
                    }
                    let answer = match serde_json::from_str::<Value>(&line) {
                        Ok(call) => match handle(&thread, &call) {
                            Ok(result) => json!({ "ok": true, "result": result }),
                            Err(error) => json!({ "ok": false, "error": error }),
                        },
                        Err(_) => json!({ "ok": false, "error": "not a tool call" }),
                    };
                    let mut stream = stream;
                    let _ = writeln!(stream, "{answer}");
                });
            }
        })
        .ok();
}

/// One tool call: done on the UI thread (and, for read_session, the
/// session's messages asked for here).
fn handle(thread: &CxxQtThread<Workspace>, call: &Value) -> Result<Value, String> {
    let (session, opencode) = (call["session"].as_str().unwrap_or_default().to_owned(), call["opencode"] == true);
    let (tool, args) = (call["tool"].as_str().unwrap_or_default().to_owned(), call["args"].clone());
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let (tool, args) = (tool.clone(), args.clone());
        thread
            .queue(move |this| {
                let _ = tx.send(this.orchestrate(&session, opencode, &tool, &args));
            })
            .map_err(|_| "harlo is closing".to_owned())?;
    }
    let result = rx.recv_timeout(Duration::from_secs(60)).map_err(|_| "harlo didn't answer in time".to_owned())??;
    if tool != "read_session" {
        return Ok(result);
    }
    // The messages: asked of the session itself.
    let key = result["session"].as_str().unwrap_or_default().to_owned();
    let Some(live) = crate::registry::get(&key) else { return Ok(result) };
    let wanted = args["messages"].as_u64().unwrap_or(1).clamp(1, 50) as usize;
    let messages = crate::runtime().block_on(async { tokio::time::timeout(Duration::from_secs(30), live.handle.request(json!({ "type": "get_messages" }))).await });
    let list: Vec<Value> = match messages {
        Ok(Ok(data)) => data["messages"].as_array().cloned().unwrap_or_default(),
        _ => Vec::new(),
    };
    let readable: Vec<Value> = list
        .iter()
        .filter_map(|m| {
            let role = m["role"].as_str()?;
            if role != "user" && role != "assistant" {
                return None;
            }
            let text: String = match &m["content"] {
                Value::String(s) => s.clone(),
                Value::Array(blocks) => blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
                _ => String::new(),
            };
            (!text.trim().is_empty()).then(|| json!({ "role": role, "text": text }))
        })
        .collect();
    let mut result = result;
    if wanted == 1 {
        result["lastReply"] = readable.iter().rev().find(|m| m["role"] == "assistant").map(|m| m["text"].clone()).unwrap_or(Value::Null);
    } else {
        result["messages"] = json!(readable[readable.len().saturating_sub(wanted)..]);
    }
    Ok(result)
}
