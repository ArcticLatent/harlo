//! `harlo-agent serve-tools`: harlo-agent's tools as an MCP server on
//! stdin/stdout, for another agent to work with (Claude Code, started by
//! harlo with its own tools off).
//!
//! The tools are the ones harlo-agent's own sessions use, run the same way:
//! a file must be read before it's edited, an edit or overwrite of a file
//! that changed since is refused, commands run in harlo-agent's sandbox
//! (when it's on), long output is saved to a file to read on.
//!
//! Not served: what needs harlo-agent's own loop (background commands and
//! `jobs`, which tell the model when they end; `recall` of outputs it set
//! aside; `ask`, which harlo answers in its own sessions), and `bash`'s
//! `outside_sandbox` (there's no one here to ask).
//!
//! MCP over stdio: JSON-RPC 2.0, a message a line. Calls run at once,
//! answered as each ends.

use std::path::PathBuf;

use futures_util::stream::{FuturesUnordered, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::tools::{self, Tools};

/// Our tools that are served.
const SERVED: &[&str] = &["read", "write", "edit", "bash", "grep", "find", "ls", "fetch"];

/// The protocol version when the client doesn't name one.
const PROTOCOL: &str = "2025-06-18";

/// Serves the tools in `cwd` until stdin closes.
pub async fn run(cwd: PathBuf) -> anyhow::Result<()> {
    let tools = tools_for(cwd);
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    let mut pending = FuturesUnordered::new();
    let mut open = true;
    while open || !pending.is_empty() {
        tokio::select! {
            line = lines.next_line(), if open => match line? {
                Some(line) => {
                    let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                        send(&mut out, &json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "not JSON" } })).await?;
                        continue;
                    };
                    pending.push(answer(&tools, msg));
                }
                None => open = false,
            },
            Some(reply) = pending.next() => {
                if let Some(reply) = reply {
                    send(&mut out, &reply).await?;
                }
            }
        }
    }
    Ok(())
}

async fn send(out: &mut tokio::io::Stdout, msg: &Value) -> anyhow::Result<()> {
    let mut line = msg.to_string();
    line.push('\n');
    out.write_all(line.as_bytes()).await?;
    out.flush().await?;
    Ok(())
}

/// The tools as harlo-agent's sessions have them in `cwd` (its sandbox
/// too, when it's on).
fn tools_for(cwd: PathBuf) -> Tools {
    let mut t = Tools::new(cwd.clone(), std::env::temp_dir().join("harlo-agent"));
    t.sandbox = crate::sandbox::for_dir(&cwd);
    t
}

/// The reply to a message (None for a notification).
async fn answer(tools: &Tools, msg: Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let method = msg["method"].as_str().unwrap_or_default();
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or(PROTOCOL),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "harlo-agent", "version": env!("CARGO_PKG_VERSION") },
            "instructions": instructions(tools),
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": specs(&tools.cwd) })),
        "tools/call" => Ok(call(tools, &msg["params"]).await),
        _ => Err(json!({ "code": -32601, "message": format!("no method {method}") })),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
    })
}

/// How to work with the tools (MCP's server instructions: the client adds
/// them to its prompt).
fn instructions(tools: &Tools) -> String {
    let mut s = format!(
        "These are harlo-agent's tools for the project in {}: use them for all work on its files and for its commands.\n\
         - Look before you change: read a file before editing it. When the task names a file, read it; search (grep, find) only when you don't know where something is.\n\
         - Change files with edit (small, exact replacements) or write (new files). Never edit files through bash.\n\
         - Run commands with bash to build, test and check your work. Commands can't take input.\n\
         - When a tool fails, read its message: it says what to do next.",
        tools.cwd.display()
    );
    if let Some(sb) = &tools.sandbox {
        s.push_str(&format!("\n{}", sb.describe_for(false)));
    }
    s
}

/// The served tools, as MCP lists them.
fn specs(cwd: &std::path::Path) -> Vec<Value> {
    let mut list: Vec<Value> = tools::specs().as_array().cloned().unwrap_or_default().into_iter().filter(|s| SERVED.iter().any(|n| s["name"] == *n)).collect();
    if crate::lsp::offered(cwd) {
        list.push(crate::lsp::spec());
    }
    list.into_iter()
        .map(|mut s| {
            let name = s["name"].as_str().unwrap_or_default().to_owned();
            if name == "bash" {
                // (Foreground only, inside the sandbox.)
                let d = s["description"].as_str().unwrap_or_default();
                let d = d.split("With background:").next().unwrap_or(d).trim().to_owned();
                s["description"] = d.into();
                if let Some(p) = s["parameters"]["properties"].as_object_mut() {
                    p.remove("background");
                    p.remove("outside_sandbox");
                }
            }
            let mut t = json!({ "name": name, "description": s["description"], "inputSchema": s["parameters"] });
            if tools::read_only(&name) || name == "fetch" {
                t["annotations"] = json!({ "readOnlyHint": true });
            }
            t
        })
        .collect()
}

/// A `tools/call`: the tool's text and pictures, and whether it failed.
async fn call(tools: &Tools, params: &Value) -> Value {
    let name = params["name"].as_str().unwrap_or_default();
    let served = SERVED.contains(&name) || (name == "lsp" && crate::lsp::offered(&tools.cwd));
    let mut args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let out = if !served {
        tools::Outcome::new(format!("There's no tool called {name} here."), true)
    } else {
        if name == "bash"
            && let Some(a) = args.as_object_mut()
        {
            a.remove("background");
            a.remove("outside_sandbox");
        }
        tools.call(name, &args).await
    };
    let mut content = vec![json!({ "type": "text", "text": out.text })];
    content.extend(out.images.into_iter().map(|i| json!({ "type": "image", "data": i["data"], "mimeType": i["mimeType"] })));
    json!({ "content": content, "isError": out.failed })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rpc(id: i64, method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    }

    #[tokio::test]
    async fn serves_the_tools() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-serve-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        let tools = Tools::new(dir.clone(), dir.join("spill"));

        let init = answer(&tools, rpc(1, "initialize", json!({ "protocolVersion": "2025-03-26" }))).await.unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert!(init["result"]["instructions"].as_str().unwrap().contains("read a file before editing it"));
        assert!(answer(&tools, json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await.is_none());

        let list = answer(&tools, rpc(2, "tools/list", json!({}))).await.unwrap();
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        for n in ["read", "write", "edit", "bash", "grep", "find", "ls", "fetch"] {
            assert!(names.contains(&n), "{n} in {names:?}");
        }
        for n in ["jobs", "ask", "recall", "current_time"] {
            assert!(!names.contains(&n), "{n} not served");
        }
        let tools_list = list["result"]["tools"].as_array().unwrap();
        let bash = tools_list.iter().find(|t| t["name"] == "bash").unwrap();
        assert!(bash["inputSchema"]["properties"].get("background").is_none() && bash["inputSchema"]["properties"].get("outside_sandbox").is_none());
        assert!(!bash["description"].as_str().unwrap().contains("background"));
        assert_eq!(tools_list.iter().find(|t| t["name"] == "read").unwrap()["annotations"]["readOnlyHint"], true);
        assert!(tools_list.iter().find(|t| t["name"] == "edit").unwrap().get("annotations").is_none());

        // The same rules as harlo-agent's sessions: a file changed by
        // someone else since is read again before it's edited.
        let edit = |old: &str, new: &str| json!({ "name": "edit", "arguments": { "path": "a.txt", "edits": [{ "old_text": old, "new_text": new }] } });
        let edited = answer(&tools, rpc(3, "tools/call", edit("two", "three"))).await.unwrap();
        assert_eq!(edited["result"]["isError"], false, "{edited}");
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "one\nthree\n");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("a.txt"), "one\nthree\nfour\n").unwrap();
        let refused = answer(&tools, rpc(4, "tools/call", edit("three", "3"))).await.unwrap();
        assert_eq!(refused["result"]["isError"], true, "{refused}");
        assert!(refused["result"]["content"][0]["text"].as_str().unwrap().contains("changed since you last read it"), "{refused}");
        let read = answer(&tools, rpc(5, "tools/call", json!({ "name": "read", "arguments": { "path": "a.txt" } }))).await.unwrap();
        assert_eq!(read["result"]["isError"], false);
        assert!(read["result"]["content"][0]["text"].as_str().unwrap().contains("four"));
        let again = answer(&tools, rpc(6, "tools/call", edit("three", "3"))).await.unwrap();
        assert_eq!(again["result"]["isError"], false, "{again}");

        let ran = answer(&tools, rpc(6, "tools/call", json!({ "name": "bash", "arguments": { "command": "echo hi", "background": true } }))).await.unwrap();
        assert_eq!(ran["result"]["isError"], false);
        assert!(ran["result"]["content"][0]["text"].as_str().unwrap().contains("hi"), "{ran}");

        let nope = answer(&tools, rpc(7, "tools/call", json!({ "name": "jobs", "arguments": { "action": "list" } }))).await.unwrap();
        assert_eq!(nope["result"]["isError"], true);
        let bad = answer(&tools, rpc(8, "resources/list", json!({}))).await.unwrap();
        assert_eq!(bad["error"]["code"], -32601);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
