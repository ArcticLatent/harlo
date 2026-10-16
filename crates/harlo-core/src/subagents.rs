//! omp subagents as the UI shows them, built from both sources omp offers:
//! `subagent_lifecycle` / `subagent_progress` events (which also cover
//! detached background jobs) and the `get_subagents` list (which does not).

use serde_json::{Value, json};

/// Folds one `subagent_lifecycle` / `subagent_progress` frame into `list`
/// (one entry per subagent id, in first-seen order).
pub fn apply_event(list: &mut Vec<Value>, frame: &Value) {
    let p = &frame["payload"];
    let Some(id) = p["id"].as_str().or_else(|| p["progress"]["id"].as_str()) else { return };
    let pos = match list.iter().position(|e| e["id"] == id) {
        Some(pos) => pos,
        None => {
            list.push(json!({ "id": id }));
            list.len() - 1
        }
    };
    let entry = &mut list[pos];
    for key in ["agent", "agentSource", "parentToolCallId", "sessionFile", "index", "detached", "description", "task", "assignment", "name", "canCancel", "canSteer", "childSession", "result"] {
        if !p[key].is_null() {
            entry[key] = p[key].clone();
        }
    }
    if p["progress"].is_object() {
        entry["progress"] = p["progress"].clone();
        if let Some(status) = p["progress"]["status"].as_str() {
            entry["status"] = json!(status);
        }
    }
    if let Some(status) = p["status"].as_str() {
        entry["status"] = json!(status);
    }
    // "started" reads as running everywhere else.
    if entry["status"] == "started" {
        entry["status"] = json!("running");
    }
    // A one-line description: omp's, else the assignment's first line.
    if entry["description"].as_str().is_none_or(str::is_empty)
        && let Some(first) = entry["assignment"].as_str().and_then(|a| a.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')))
    {
        let short: String = first.chars().take(120).collect();
        entry["description"] = json!(short);
    }
    // One subagent, listed twice (an OpenCode child read from the database
    // before its call named it): the call's entry stays.
    if let Some(child) = entry["childSession"].as_str().map(str::to_owned) {
        let id = id.to_owned();
        list.retain(|e| e["id"] == id.as_str() || e["childSession"] != child.as_str());
    }
}

/// Subagents from both sources, in a stable order (first seen in events,
/// then any only `get_subagents` knows); listed fields win, events fill in
/// the rest.
pub fn merged(from_events: &[Value], listed: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(from_events.len() + listed.len());
    for event in from_events {
        let mut entry = listed.iter().find(|l| l["id"] == event["id"]).cloned().unwrap_or_else(|| event.clone());
        if let (Some(dst), Some(src)) = (entry.as_object_mut(), event.as_object()) {
            for (k, v) in src {
                if dst.get(k).is_none_or(Value::is_null) {
                    dst.insert(k.clone(), v.clone());
                }
            }
        }
        out.push(entry);
    }
    for l in listed {
        if !out.iter().any(|e| e["id"] == l["id"]) {
            out.push(l.clone());
        }
    }
    out
}

/// The subagents a saved omp session started, from its file: each `task`
/// result lists its subagents (`details.progress`); their transcripts sit
/// in a folder named after the session file. Nothing is running in a saved
/// session, so the outcome comes from omp's job notice (`<task-result
/// status="…">`), else a `yield` in the transcript (delivered), else
/// "ended".
pub fn from_session_file(path: &std::path::Path) -> Vec<Value> {
    // On a remote host: the file, which transcripts exist and which yielded,
    // each in one SSH command.
    let remote = crate::remote::is_remote(path);
    let text = if remote { crate::remote::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).ok() } else { std::fs::read_to_string(path).ok() };
    let Some(text) = text else { return Vec::new() };
    let dir = path.with_extension("");
    let remote_files: std::collections::HashSet<String> = if remote {
        crate::remote::list_dir(&dir).map(|l| l.into_iter().map(|e| e.name).collect()).unwrap_or_default()
    } else {
        Default::default()
    };
    let remote_yielded: std::collections::HashSet<String> = match (remote, crate::remote::split(&dir).and_then(|(h, d)| Some((crate::remote::host(&h)?, d)))) {
        (true, Some((host, d))) => crate::remote::run(&host, &format!("cd {} 2>/dev/null && grep -l -F '\"name\":\"yield\"' -- *.jsonl 2>/dev/null", crate::remote::quote(&d)), std::time::Duration::from_secs(30))
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(str::to_owned).collect())
            .unwrap_or_default(),
        _ => Default::default(),
    };
    let exists = |file: &std::path::Path| if remote { file.file_name().is_some_and(|n| remote_files.contains(&*n.to_string_lossy())) } else { file.exists() };
    let mut list = Vec::new();
    let mut outcome: std::collections::HashMap<String, String> = Default::default();
    for line in text.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else { continue };
        let m = &entry["message"];
        if entry["type"] == "message" && m["role"] == "toolResult" && m["toolName"] == "task" {
            for p in m["details"]["progress"].as_array().into_iter().flatten() {
                let Some(id) = p["id"].as_str() else { continue };
                let mut payload = json!({ "id": id, "parentToolCallId": m["toolCallId"] });
                for key in ["agent", "agentSource", "index", "assignment", "task", "description"] {
                    if !p[key].is_null() {
                        payload[key] = p[key].clone();
                    }
                }
                let file = dir.join(format!("{id}.jsonl"));
                if exists(&file) {
                    payload["sessionFile"] = json!(file.to_string_lossy());
                }
                apply_event(&mut list, &json!({ "payload": payload }));
            }
        }
        if entry["type"] == "custom_message"
            && let Some(content) = entry["content"].as_str()
        {
            for tag in content.split("<task-result ").skip(1) {
                let attr = |name: &str| tag.split(&format!("{name}=\"")).nth(1).and_then(|r| r.split('"').next()).map(str::to_owned);
                if let (Some(id), Some(status)) = (attr("id"), attr("status")) {
                    outcome.insert(id, status.split_whitespace().next().unwrap_or("ended").to_owned());
                }
            }
        }
    }
    for entry in &mut list {
        let id = entry["id"].as_str().unwrap_or_default().to_owned();
        let delivered = entry["sessionFile"].as_str().is_some_and(|f| {
            if remote {
                remote_yielded.contains(f.rsplit('/').next().unwrap_or_default())
            } else {
                std::fs::read_to_string(f).is_ok_and(|t| t.contains("\"name\":\"yield\""))
            }
        });
        let status = outcome.get(&id).cloned().unwrap_or_else(|| if delivered { "completed".into() } else { "ended".into() });
        entry["status"] = json!(status);
    }
    list
}

/// A saved remote Claude session's subagents: its `subagents/` folder on
/// the host, read in one SSH call (and the transcript for the calls).
pub fn claude_on_host(path: &std::path::Path) -> Vec<Value> {
    let Some((host, file)) = crate::remote::split(path).and_then(|(h, f)| Some((crate::remote::host(&h)?, f))) else { return Vec::new() };
    let dir = std::path::Path::new(&file).with_extension("").join("subagents");
    let script = format!(
        r#"d={}; [ -d "$d" ] || exit 0
find "$d" -name 'agent-*.meta.json' -type f | while IFS= read -r m; do
  t="${{m%.meta.json}}.jsonl"; w=0; e=0
  if [ -f "$t" ]; then w=1; tail -n 40 "$t" | grep -q '"stop_reason":"end_turn"' && e=1; fi
  printf '%s\t%s\t%s\t' "$m" "$w" "$e"; tr -d '\n' < "$m"; printf '\n'
done"#,
        crate::remote::quote(&dir.to_string_lossy())
    );
    let Ok(listing) = crate::remote::run_ok(&host, &script, std::time::Duration::from_secs(30)) else { return Vec::new() };
    let metas: Vec<harlo_harness::claude::SubagentMeta> = listing
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, '\t');
            let (meta_path, has, ended, json) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
            let name = meta_path.rsplit('/').next()?;
            let agent_id = name.strip_prefix("agent-")?.strip_suffix(".meta.json")?.to_owned();
            let transcript = (has == "1").then(|| crate::remote::join(&host.name, &format!("{}.jsonl", meta_path.trim_end_matches(".meta.json"))));
            Some(harlo_harness::claude::SubagentMeta { agent_id, meta: serde_json::from_str(json).ok()?, transcript, ended_well: ended == "1" })
        })
        .collect();
    if metas.is_empty() {
        return Vec::new();
    }
    let text = crate::remote::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    harlo_harness::claude::subagents_from(&text, metas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_subagents_of_a_saved_session() {
        let dir = std::env::temp_dir().join(format!("harlo-subagents-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("s1")).unwrap();
        let progress = json!([
            { "index": 0, "id": "ListFiles", "agent": "scout", "status": "pending", "assignment": "# Target\nList every file." },
            { "index": 1, "id": "Summarize", "agent": "scout", "status": "pending", "assignment": "Summarize the docs." },
            { "index": 2, "id": "Broken", "agent": "task", "status": "pending", "assignment": "Do it." }
        ]);
        let lines = [
            json!({ "type": "session", "id": "s1" }),
            json!({ "type": "message", "message": { "role": "toolResult", "toolName": "task", "toolCallId": "call_1|fc_2",
                "content": [{ "type": "text", "text": "Spawned 3" }], "details": { "progress": progress } } }),
            json!({ "type": "custom_message", "customType": "async-result",
                "content": "<system-notice>\n<task-result id=\"Broken\" agent=\"task\" status=\"failed (exit 1)\"> x </task-result></system-notice>" }),
        ];
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        let file = dir.join("s1.jsonl");
        std::fs::write(&file, text).unwrap();
        std::fs::write(dir.join("s1/ListFiles.jsonl"), "{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"toolCall\",\"name\":\"yield\"}]}}\n").unwrap();
        std::fs::write(dir.join("s1/Summarize.jsonl"), "{\"type\":\"session\"}\n").unwrap();

        let list = from_session_file(&file);
        let view: Vec<(String, String)> = list.iter().map(|e| (e["id"].as_str().unwrap().to_owned(), e["status"].as_str().unwrap().to_owned())).collect();
        assert_eq!(view, [("ListFiles".into(), "completed".into()), ("Summarize".into(), "ended".into()), ("Broken".into(), "failed".into())]);
        assert_eq!(list[0]["parentToolCallId"], "call_1|fc_2");
        assert_eq!(list[0]["description"], "List every file.");
        assert!(list[0]["sessionFile"].as_str().unwrap().ends_with("s1/ListFiles.jsonl"));
        assert!(list[2]["sessionFile"].is_null());
        assert!(from_session_file(&dir.join("missing.jsonl")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn lifecycle(id: &str, status: &str) -> Value {
        json!({ "type": "subagent_lifecycle", "payload": { "agent": "scout", "detached": true, "id": id, "index": 0,
            "parentToolCallId": "call_1|fc_2", "sessionFile": "/s/x.jsonl", "status": status } })
    }

    #[test]
    fn builds_detached_subagents_from_events() {
        let mut list = Vec::new();
        apply_event(&mut list, &lifecycle("Inventory", "started"));
        assert_eq!(list[0]["status"], "running");
        assert_eq!(list[0]["parentToolCallId"], "call_1|fc_2");
        apply_event(&mut list, &json!({ "type": "subagent_progress", "payload": {
            "id": "Inventory", "assignment": "# Target\nList every file with its size.\nMore detail.",
            "progress": { "id": "Inventory", "status": "running", "currentTool": "bash", "currentToolArgs": { "command": "ls" } } } }));
        assert_eq!(list[0]["description"], "List every file with its size.");
        assert_eq!(list[0]["progress"]["currentTool"], "bash");
        apply_event(&mut list, &lifecycle("Inventory", "failed"));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["status"], "failed");
        // Frames without an id are ignored.
        apply_event(&mut list, &json!({ "type": "subagent_progress", "payload": {} }));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn merges_listed_and_event_subagents() {
        let mut events = Vec::new();
        apply_event(&mut events, &lifecycle("A", "started"));
        apply_event(&mut events, &lifecycle("B", "started"));
        let listed = vec![json!({ "id": "C" }), json!({ "id": "B", "status": "completed", "description": "from the list" })];
        let all = merged(&events, &listed);
        let ids: Vec<&str> = all.iter().map(|e| e["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["A", "B", "C"]);
        assert_eq!(all[1]["status"], "completed");
        assert_eq!(all[1]["parentToolCallId"], "call_1|fc_2");
        assert_eq!(merged(&events, &[]).len(), 2);
    }
}
