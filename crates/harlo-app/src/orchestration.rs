//! The running harlo's side of orchestration (see `harlo_core::orchestration`):
//! a local socket where `harlo --mcp-bridge` hands over agents' tool calls,
//! and harlo-agent's calls on its own pipe ([`answer_pipe`]). Each call is
//! carried out on the UI thread, where the sessions are.

// Rust guideline compliant 2026-10-09

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
            tracing::event!(name: "orchestration.listen.failed", tracing::Level::WARN, file.path = %path.display(), error.message = %e, "orchestration: can't listen on {{file.path}}: {{error.message}}");
            return;
        }
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    let _ = std::thread::Builder::new()
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
                            Err(error) => json!({ "ok": false, "error": format!("{error:#}") }),
                        },
                        Err(_) => json!({ "ok": false, "error": "not a tool call" }),
                    };
                    let mut stream = stream;
                    let _ = writeln!(stream, "{answer}");
                });
            }
        });
}

/// One tool call: done on the UI thread (and, for read_session, the
/// session's messages asked for here).
fn handle(thread: &CxxQtThread<Workspace>, call: &Value) -> anyhow::Result<Value> {
    let session = call["session"].as_str().unwrap_or_default().to_owned();
    let (tool, args) = (call["tool"].as_str().unwrap_or_default().to_owned(), call["args"].clone());
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let (tool, args) = (tool.clone(), args.clone());
        thread
            .queue(move |this| {
                let _ = tx.send(this.orchestrate(&session, &tool, &args));
            })
            .ok().ok_or_else(|| anyhow::Error::msg("harlo is closing"))?;
    };
    let result = rx.recv_timeout(Duration::from_secs(60)).ok().ok_or_else(|| anyhow::Error::msg("harlo didn't answer in time"))??;
    finish(thread, &tool, &args, result)
}

/// `f` done on the UI thread, waited for here (not on the UI thread).
fn on_ui<T: Send + 'static>(thread: &CxxQtThread<Workspace>, f: impl FnOnce(std::pin::Pin<&mut Workspace>) -> T + Send + 'static) -> anyhow::Result<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    thread
        .queue(move |this| {
            let _ = tx.send(f(this));
        })
        .ok().ok_or_else(|| anyhow::Error::msg("harlo is closing"))?;
    rx.recv_timeout(Duration::from_secs(60)).ok().ok_or_else(|| anyhow::Error::msg("harlo didn't answer in time"))
}

/// A harlo-agent session's call on its pipe, done on the UI thread
/// (`result`): finished here in the background, then the answer to the
/// session.
pub fn answer_pipe(thread: CxxQtThread<Workspace>, key: String, call: String, tool: String, args: Value, result: anyhow::Result<Value>) {
    std::thread::spawn(move || {
        let answer = match result.and_then(|r| finish(&thread, &tool, &args, r)) {
            Ok(result) => json!({ "type": "harlo_result", "call": call, "ok": true, "result": result }),
            Err(error) => json!({ "type": "harlo_result", "call": call, "ok": false, "error": format!("{error:#}") }),
        };
        if let Some(live) = crate::registry::get(&key) {
            live.handle.send_raw(answer);
        }
    });
}

/// What's left of a call after the UI thread's part: the wiki and the plan
/// worked on, a session's messages read.
fn finish(thread: &CxxQtThread<Workspace>, tool: &str, args: &Value, result: Value) -> anyhow::Result<Value> {
    // The wiki and the plan: their work here, with what the UI thread gave.
    if result["_defer"] == true {
        if tool == "bring_over" {
            let path = |k: &str| std::path::PathBuf::from(result[k].as_str().unwrap_or_default());
            let text = |k: &str| result[k].as_str().unwrap_or_default().to_owned();
            let helper = path("helper");
            let brought = harlo_core::git::bring_over(&helper, &path("here"), &text("start"), &text("base")).map_err(|e| anyhow::anyhow!("Not brought over: {e:#}."))?;
            let shown = harlo_core::remote::display(&helper);
            return Ok(json!({ "applied": brought.applied, "clashed": brought.clashed, "text": harlo_core::orchestration::brought_text(&brought.applied, &brought.clashed, &shown) }));
        }
        // Past sessions' messages: searched here, then shown with what the
        // UI thread knows of them.
        if tool == "list_sessions" {
            let reader = on_ui(thread, |this| this.session_search())?.ok_or_else(|| anyhow::Error::msg("Searching sessions isn't available here."))?;
            let query = result["query"].as_str().unwrap_or_default().to_owned();
            let hits = reader.lock().ok().ok_or_else(|| anyhow::Error::msg("The session search broke."))?.search(&query, 300).unwrap_or_default();
            let path = |k: &str| result[k].as_str().map(std::path::PathBuf::from);
            let (everywhere, cwd, own) = (result["everywhere"] == true, path("cwd"), path("own"));
            return on_ui(thread, move |this| this.found_sessions(&hits, everywhere, cwd.as_deref(), own.as_deref()));
        }
        if tool == harlo_core::orchestration::INSTRUCTIONS {
            // (Checked here, off the UI thread: another machine's is asked over SSH.)
            let text = result["text"].as_str().unwrap_or_default();
            let map = result["map"] != false;
            return Ok(match harlo_core::wiki::reading_note(std::path::Path::new(result["cwd"].as_str().unwrap_or_default()), map) {
                Some(note) => json!(format!("{text}\n\n{note}")),
                None => json!(text),
            });
        }
        if tool == harlo_core::tool_calls::LOOKUP {
            let cwd = std::path::PathBuf::from(result["cwd"].as_str().unwrap_or_default());
            let query = harlo_core::tool_calls::lookup_query(args);
            if query.name.is_empty() {
                anyhow::bail!("Give the name to look up.");
            }
            return Ok(json!(harlo_core::agent_host::lookup(&cwd, &query)?));
        }
        if tool == harlo_core::wiki::CHECK {
            let cwd = std::path::PathBuf::from(result["cwd"].as_str().unwrap_or_default());
            let writer = result["writer"].as_str().unwrap_or_default().to_owned();
            let answer = harlo_core::wiki::call_check(&cwd, &writer, args)?;
            crate::workspace::forget_wiki_reads();
            let _ = thread.queue(|mut this| this.as_mut().wiki_ready());
            return Ok(answer);
        }
        let outcome = crate::plans::tool(tool, args, &result);
        let _ = thread.queue(super::workspace::qobject::Workspace::plans_changed);
        return outcome;
    }
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
        result["lastReply"] = readable.iter().rev().find(|m| m["role"] == "assistant").map_or(Value::Null, |m| m["text"].clone());
    } else {
        result["messages"] = json!(readable[readable.len().saturating_sub(wanted)..]);
    }
    Ok(result)
}
