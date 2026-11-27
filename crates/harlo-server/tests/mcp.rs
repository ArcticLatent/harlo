//! MCP servers named in a project's config: listed with their status, and
//! their tools called by an agent, through Code Mode or on their own.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Delivery, Effect, McpStatus, Message, PermissionReply, Prompt, Rule};
use support::Sandbox;

/// A stdio MCP server with one tool, `echo`.
const ECHO: &str = r#"import json, sys
for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue
    method = msg.get("method")
    if method == "initialize":
        result = {"protocolVersion": msg["params"]["protocolVersion"],
                  "capabilities": {"tools": {}},
                  "serverInfo": {"name": "echo", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": [{"name": "echo", "description": "Says the text back.",
                             "inputSchema": {"type": "object", "required": ["text"],
                                             "properties": {"text": {"type": "string"}}}}]}
    elif method == "tools/call":
        said = msg["params"]["arguments"]["text"]
        result = {"content": [{"type": "text", "text": "echo: " + said}]}
    else:
        print(json.dumps({"jsonrpc": "2.0", "id": msg["id"],
                          "error": {"code": -32601, "message": "no"}}), flush=True)
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}), flush=True)
"#;

/// The text of the tool results the agent's requests carried.
fn results(sandbox: &Sandbox) -> String {
    support::turn_requests(sandbox.dir.path())
        .iter()
        .flat_map(|r| r["messages"].as_array().cloned().unwrap_or_default())
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Sends `script` and waits until a tool result holds `wanted`.
async fn run_until(sandbox: &Sandbox, session: &str, script: &str, wanted: &str) {
    let client = sandbox.client();
    client
        .prompt(session, &Prompt::new(script, Delivery::Queue))
        .await
        .expect("prompt");
    for _ in 0..200 {
        if results(sandbox).contains(wanted) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no result with {wanted:?}: {}", results(sandbox));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_projects_servers_connect_and_their_tools_answer() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let folder = project.to_str().expect("utf-8").to_owned();
    let echo = sandbox.dir.path().join("echo.py");
    std::fs::write(&echo, ECHO).expect("echo.py");
    let command = serde_json::json!(["python3", "-I", echo]);
    let config = serde_json::json!({ "mcp": { "servers": {
        "probe": { "type": "local", "command": command },
        "plain": { "type": "local", "command": command, "codemode": false },
        "off": { "type": "local", "command": command, "disabled": true },
    } } });
    std::fs::write(project.join("harlo.json"), config.to_string()).expect("config");
    // A project's servers start once the person allowed them.
    assert!(
        client
            .mcp_servers(&folder)
            .await
            .expect("listed")
            .is_empty()
    );
    support::allow_project_settings(&client, &folder).await;

    let mut listed = Vec::new();
    for _ in 0..200 {
        listed = client.mcp_servers(&folder).await.expect("listed");
        if listed
            .iter()
            .filter(|s| s.status == McpStatus::Connected)
            .count()
            == 2
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let status = |name: &str| {
        listed
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.status.clone())
    };
    assert_eq!(status("probe"), Some(McpStatus::Connected), "{listed:?}");
    assert_eq!(status("plain"), Some(McpStatus::Connected));
    assert_eq!(status("off"), Some(McpStatus::Disabled));

    let session = client.create_session(&folder).await.expect("session");
    let code = serde_json::json!({ "code": "return await tools.probe.echo({text: 'hi'})" });
    run_until(
        &sandbox,
        &session.id,
        &format!("tool execute {code} ;; say done"),
        "echo: hi",
    )
    .await;
    // Not through Code Mode: offered on its own.
    let offered = support::turn_requests(sandbox.dir.path())
        .last()
        .map(|r| r["tools"].to_string())
        .unwrap_or_default();
    assert!(offered.contains("plain_echo"), "{offered}");
    assert!(!offered.contains("probe_echo"));
    run_until(
        &sandbox,
        &session.id,
        r#"tool plain_echo {"text":"yo"} ;; say ok"#,
        "echo: yo",
    )
    .await;

    // Turned off until connected again.
    client.disconnect_mcp(&folder, "probe").await.expect("off");
    let listed = client.mcp_servers(&folder).await.expect("listed");
    assert!(
        listed
            .iter()
            .any(|s| s.name == "probe" && s.status == McpStatus::Disabled)
    );
    client.connect_mcp(&folder, "probe").await.expect("on");
    client
        .connect_mcp(&folder, "missing")
        .await
        .expect_err("no such server");
}

/// A stdio MCP server with one tool, `count`, and two resources.
const NOTES: &str = r#"import json, sys
for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue
    method, params = msg.get("method"), msg.get("params") or {}
    if method == "initialize":
        result = {"protocolVersion": params["protocolVersion"],
                  "capabilities": {"tools": {}, "resources": {}},
                  "serverInfo": {"name": "notes", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": [{"name": "count", "description": "Counts the notes.",
                             "inputSchema": {"type": "object", "properties": {}}}]}
    elif method == "tools/call":
        result = {"content": [{"type": "text", "text": "2 notes"}]}
    elif method == "resources/list":
        result = {"resources": [{"uri": "notes://one", "name": "First note", "mimeType": "text/plain"},
                                {"uri": "notes://two", "name": "notes://two"},
                                {"uri": "notes://pic", "name": "Picture", "mimeType": "image/png"},
                                {"uri": "notes://zip", "name": "Archive", "mimeType": "application/zip"},
                                {"uri": "notes://long", "name": "Long"}]}
    elif method == "resources/templates/list":
        result = {"resourceTemplates": [{"uriTemplate": "notes://day/{n}", "name": "Day"}]}
    elif method == "resources/read" and params.get("uri") == "notes://one":
        result = {"contents": [{"uri": "notes://one", "mimeType": "text/plain",
                                "text": "The harbour opens at six."}]}
    elif method == "resources/read" and params.get("uri") == "notes://pic":
        result = {"contents": [{"uri": "notes://pic", "mimeType": "image/png", "blob": "aGVsbG8="}]}
    elif method == "resources/read" and params.get("uri") == "notes://zip":
        result = {"contents": [{"uri": "notes://zip", "mimeType": "application/zip", "blob": "UEsDBA=="}]}
    elif method == "resources/read" and params.get("uri") == "notes://long":
        text = "".join("line %d of the long note\n" % n for n in range(12000))
        result = {"contents": [{"uri": "notes://long", "text": text}]}
    elif method == "resources/read" and params.get("uri", "").startswith("notes://day/"):
        result = {"contents": [{"uri": params["uri"], "text": "Day %s: fog." % params["uri"][12:]}]}
    else:
        print(json.dumps({"jsonrpc": "2.0", "id": msg["id"],
                          "error": {"code": -32002, "message": "No such resource"}}), flush=True)
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}), flush=True)
"#;

/// Waits until `folder`'s server `name` stands as `wanted` says.
async fn stands(
    sandbox: &Sandbox,
    folder: &str,
    name: &str,
    wanted: impl Fn(&McpStatus) -> bool,
) -> McpStatus {
    let client = sandbox.client();
    let mut last = None;
    for _ in 0..200 {
        let listed = client.mcp_servers(folder).await.expect("listed");
        last = listed
            .into_iter()
            .find(|s| s.name == name)
            .map(|s| s.status);
        if last.as_ref().is_some_and(&wanted) {
            return last.expect("a status");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{name} never stood as wanted: {last:?}");
}

/// An agent reads what a server offers as resources, through a tool Harlo
/// adds to the server's own; a server with no resources gets none.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_reads_a_servers_resources() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let folder = project.to_str().expect("utf-8").to_owned();
    let (notes, echo) = (
        sandbox.dir.path().join("notes.py"),
        sandbox.dir.path().join("echo.py"),
    );
    std::fs::write(&notes, NOTES).expect("notes.py");
    std::fs::write(&echo, ECHO).expect("echo.py");
    let config = serde_json::json!({ "mcp": { "servers": {
        "notes": { "type": "local", "command": ["python3", "-I", notes] },
        "plain": { "type": "local", "command": ["python3", "-I", notes], "codemode": false },
        "probe": { "type": "local", "command": ["python3", "-I", echo] },
    } } });
    std::fs::write(project.join("harlo.json"), config.to_string()).expect("config");
    support::allow_project_settings(&client, &folder).await;
    for name in ["notes", "plain", "probe"] {
        stands(&sandbox, &folder, name, |s| *s == McpStatus::Connected).await;
    }

    let session = client.create_session(&folder).await.expect("session");
    let code = serde_json::json!({
        "code": "return await tools.notes.read_resource({uri: 'notes://one'})" });
    run_until(
        &sandbox,
        &session.id,
        &format!("tool execute {code} ;; say done"),
        "The harbour opens at six.",
    )
    .await;
    // What the model is told of the tool: the resources by uri, and by
    // name where the name says more.
    let offered = support::turn_requests(sandbox.dir.path())
        .last()
        .map(|r| r["tools"].to_string())
        .unwrap_or_default();
    assert!(
        offered.contains(
            "Offered: notes://one (First note), notes://two, notes://pic (Picture), \
             notes://zip (Archive), notes://long (Long), notes://day/{n}."
        ),
        "{offered}"
    );
    assert!(offered.contains("plain_read_resource"), "{offered}");
    assert!(
        !offered.contains("probe_read_resource") && offered.matches("read_resource").count() == 2,
        "a server with no resources has no such tool: {offered}"
    );

    // With no uri it lists them; on its own, for a server outside scripts.
    let code = serde_json::json!({
        "code": "const all = await tools.notes.read_resource({}); \
                 return all.resources.map(r => r.uri).join('+')" });
    run_until(
        &sandbox,
        &session.id,
        &format!("tool execute {code} ;; say done"),
        "notes://one+notes://two",
    )
    .await;
    run_until(
        &sandbox,
        &session.id,
        r#"tool plain_read_resource {"uri":"notes://one"} ;; say ok"#,
        "The harbour opens at six.\"",
    )
    .await;
    // One the server doesn't have fails with what the server said.
    run_until(
        &sandbox,
        &session.id,
        r#"tool plain_read_resource {"uri":"notes://nine"} ;; say ok"#,
        "couldn't read notes://nine",
    )
    .await;
}

/// A resource mentioned in a prompt is read from the server that offers
/// it and sent with the prompt; one no server offers refuses the prompt.
#[tokio::test(flavor = "multi_thread")]
async fn a_mentioned_resource_is_sent_with_the_prompt() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let folder = project.to_str().expect("utf-8").to_owned();
    let notes = sandbox.dir.path().join("notes.py");
    std::fs::write(&notes, NOTES).expect("notes.py");
    let config = serde_json::json!({ "mcp": { "servers": {
        "notes": { "type": "local", "command": ["python3", "-I", notes] },
    } } });
    std::fs::write(project.join("harlo.json"), config.to_string()).expect("config");
    support::allow_project_settings(&client, &folder).await;
    stands(&sandbox, &folder, "notes", |s| *s == McpStatus::Connected).await;
    let session = client.create_session(&folder).await.expect("session");

    let mut prompt = Prompt::new("say read", Delivery::Queue);
    prompt.files.push(harlo_client::Attachment {
        uri: "notes://one".to_owned(),
        name: Some("First note".to_owned()),
        mention: None,
    });
    client.prompt(&session.id, &prompt).await.expect("prompt");
    let mut sent = String::new();
    for _ in 0..200 {
        sent = support::turn_requests(sandbox.dir.path())
            .iter()
            .flat_map(|r| r["messages"].as_array().cloned().unwrap_or_default())
            .filter(|m| m["role"] == "user")
            .map(|m| m["content"].to_string())
            .collect();
        if sent.contains("The harbour opens at six.") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(sent.contains("The harbour opens at six."), "{sent}");
    let mut missing = Prompt::new("say never", Delivery::Queue);
    missing.files.push(harlo_client::Attachment {
        uri: "notes://nine".to_owned(),
        name: None,
        mention: None,
    });
    let refused = client
        .prompt(&session.id, &missing)
        .await
        .expect_err("no server offers it");
    assert_eq!(
        refused.server_message(),
        Some("No connected MCP server offers notes://nine.")
    );
}

/// Waits until session `id` has no run under way.
async fn idle(client: &harlo_client::Client, id: &str) {
    for _ in 0..600 {
        let active = client.active().await.expect("the active sessions");
        if !active.iter().any(|a| a == id) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the session never went idle");
}

/// The request the model was last sent, as text.
fn last_request(sandbox: &Sandbox) -> String {
    support::turn_requests(sandbox.dir.path())
        .last()
        .map(ToString::to_string)
        .unwrap_or_default()
}

/// A resource that is a picture reaches the model as one, another kind of
/// file is named for what it is, a template's address is read, and a long
/// one is cut as any tool's result is.
#[tokio::test(flavor = "multi_thread")]
async fn resources_that_are_pictures_templates_or_long() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let folder = project.to_str().expect("utf-8").to_owned();
    let notes = sandbox.dir.path().join("notes.py");
    std::fs::write(&notes, NOTES).expect("notes.py");
    let config = serde_json::json!({ "mcp": { "servers": {
        "notes": { "type": "local", "command": ["python3", "-I", notes] },
        "plain": { "type": "local", "command": ["python3", "-I", notes], "codemode": false },
    } } });
    std::fs::write(project.join("harlo.json"), config.to_string()).expect("config");
    support::allow_project_settings(&client, &folder).await;
    for name in ["notes", "plain"] {
        stands(&sandbox, &folder, name, |s| *s == McpStatus::Connected).await;
    }
    let session = client.create_session(&folder).await.expect("session");
    let read = |uri: &str| format!(r#"tool plain_read_resource {{"uri":"{uri}"}} ;; say ok"#);

    run_until(
        &sandbox,
        &session.id,
        &read("notes://pic"),
        "(image image/png)",
    )
    .await;
    let after = last_request(&sandbox);
    assert!(
        after.contains("aGVsbG8=") && after.contains("image/png"),
        "{after}"
    );
    run_until(
        &sandbox,
        &session.id,
        &read("notes://zip"),
        "(application/zip: not text)",
    )
    .await;
    assert!(
        !last_request(&sandbox).contains("UEsDBA=="),
        "bytes the model can't read aren't sent"
    );
    run_until(&sandbox, &session.id, &read("notes://day/7"), "Day 7: fog.").await;

    // Long: on its own it is saved whole and shown in part; a script's
    // result is cut where every script's is.
    run_until(&sandbox, &session.id, &read("notes://long"), "bytes shown").await;
    let shown = results(&sandbox);
    assert!(
        shown.contains("line 0 of the long note"),
        "its start is shown"
    );
    assert!(
        !shown.contains("line 11999 of the long note"),
        "not all of it"
    );
    let code = serde_json::json!({
        "code": "return await tools.notes.read_resource({uri: 'notes://long'})" });
    run_until(
        &sandbox,
        &session.id,
        &format!("tool execute {code} ;; say done"),
        "(cut at 32 KB)",
    )
    .await;

    // A picture mentioned in a prompt goes with it as a picture.
    let mut prompt = Prompt::new("say seen", Delivery::Queue);
    prompt.files.push(harlo_client::Attachment {
        uri: "notes://pic".to_owned(),
        name: Some("Picture".to_owned()),
        mention: None,
    });
    client.prompt(&session.id, &prompt).await.expect("prompt");
    for _ in 0..200 {
        if last_request(&sandbox).contains("data:image/png;base64,aGVsbG8=") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the picture wasn't sent: {}", last_request(&sandbox));
}

/// A picture a script's call answered with reaches the model beside the
/// script's result; of many, the last few do, and the result says so.
#[tokio::test(flavor = "multi_thread")]
async fn a_scripts_pictures_reach_the_model() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let folder = project.to_str().expect("utf-8").to_owned();
    let notes = sandbox.dir.path().join("notes.py");
    std::fs::write(&notes, NOTES).expect("notes.py");
    let config = serde_json::json!({ "mcp": { "servers": {
        "notes": { "type": "local", "command": ["python3", "-I", notes] },
    } } });
    std::fs::write(project.join("harlo.json"), config.to_string()).expect("config");
    support::allow_project_settings(&client, &folder).await;
    stands(&sandbox, &folder, "notes", |s| *s == McpStatus::Connected).await;
    let pictures = |sandbox: &Sandbox| last_request(sandbox).matches("aGVsbG8=").count();

    let session = client.create_session(&folder).await.expect("session");
    let code = serde_json::json!({
        "code": "return await tools.notes.read_resource({uri: 'notes://pic'})" });
    run_until(
        &sandbox,
        &session.id,
        &format!("tool execute {code} ;; say done"),
        "(image image/png)",
    )
    .await;
    assert_eq!(pictures(&sandbox), 1, "{}", last_request(&sandbox));

    let session = client.create_session(&folder).await.expect("session");
    let code = serde_json::json!({
        "code": "for (let n = 0; n < 6; n++) await tools.notes.read_resource({uri: 'notes://pic'}); \
                 return 'read'" });
    run_until(
        &sandbox,
        &session.id,
        &format!("tool execute {code} ;; say done"),
        "(The script's calls answered with 6 pictures; the last 4 are shown.)",
    )
    .await;
    assert_eq!(pictures(&sandbox), 4, "{}", last_request(&sandbox));
}

/// A stdio MCP server with one tool, `echo`, that stops when told `die`.
const FRAIL: &str = r#"import json, sys
for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue
    method = msg.get("method")
    if method == "initialize":
        result = {"protocolVersion": msg["params"]["protocolVersion"],
                  "capabilities": {"tools": {}},
                  "serverInfo": {"name": "frail", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": [{"name": "echo", "description": "Says the text back.",
                             "inputSchema": {"type": "object", "required": ["text"],
                                             "properties": {"text": {"type": "string"}}}}]}
    elif method == "tools/call":
        said = msg["params"]["arguments"]["text"]
        if said == "die":
            sys.exit(1)
        result = {"content": [{"type": "text", "text": "echo: " + said}]}
    else:
        result = {}
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}), flush=True)
"#;

/// A server that stops in the middle of a session is started again by a
/// later turn.
#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_stopped_is_started_again_by_a_later_turn() {
    let sandbox = Sandbox::start_with_env(&[("HARLO_MCP_RETRY_MS", "200")]).await;
    let client = sandbox.client();
    let project = sandbox.project();
    let folder = project.to_str().expect("utf-8").to_owned();
    let frail = sandbox.dir.path().join("frail.py");
    std::fs::write(&frail, FRAIL).expect("frail.py");
    let config = serde_json::json!({ "mcp": { "servers": {
        "probe": { "type": "local", "command": ["python3", "-I", frail], "codemode": false },
    } } });
    std::fs::write(project.join("harlo.json"), config.to_string()).expect("config");
    support::allow_project_settings(&client, &folder).await;
    stands(&sandbox, &folder, "probe", |s| *s == McpStatus::Connected).await;

    let session = client.create_session(&folder).await.expect("session");
    run_until(
        &sandbox,
        &session.id,
        r#"tool probe_echo {"text":"one"} ;; say ok"#,
        "echo: one",
    )
    .await;
    client
        .prompt(
            &session.id,
            &Prompt::new(
                r#"tool probe_echo {"text":"die"} ;; say ok"#,
                Delivery::Queue,
            ),
        )
        .await
        .expect("prompt");
    let stopped = stands(&sandbox, &folder, "probe", |s| {
        matches!(s, McpStatus::Failed(_))
    })
    .await;
    assert_eq!(stopped, McpStatus::Failed("The server stopped.".to_owned()));
    idle(&client, &session.id).await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    client
        .prompt(&session.id, &Prompt::new("say hello", Delivery::Queue))
        .await
        .expect("prompt");
    stands(&sandbox, &folder, "probe", |s| *s == McpStatus::Connected).await;
    idle(&client, &session.id).await;
    run_until(
        &sandbox,
        &session.id,
        r#"tool probe_echo {"text":"two"} ;; say ok"#,
        "echo: two",
    )
    .await;
}

/// A server that takes the connection and never answers holds a turn up
/// for its whole startup time once; tried again, it holds a turn up only
/// for a moment.
#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_hangs_holds_a_later_turn_only_a_moment() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let at = listener.local_addr().expect("address");
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });
    let sandbox = Sandbox::start_with_env(&[("HARLO_MCP_RETRY_MS", "200")]).await;
    let client = sandbox.client();
    let path = sandbox.dir.path().join("config/harlo/harlo.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("config")).expect("JSON");
    config["mcp"] = serde_json::json!({ "servers": { "probe": {
        "type": "remote", "url": format!("http://{at}/mcp"), "timeout": { "startup": 6000 },
    } } });
    std::fs::write(&path, config.to_string()).expect("config");
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    let session = client.create_session(&folder).await.expect("session");
    let timed = |text: &'static str| {
        let (client, id) = (client.clone(), session.id.clone());
        async move {
            let began = std::time::Instant::now();
            client
                .prompt(&id, &Prompt::new(text, Delivery::Queue))
                .await
                .expect("prompt");
            tokio::time::sleep(Duration::from_millis(100)).await;
            idle(&client, &id).await;
            began.elapsed()
        }
    };

    let first = timed("say one").await;
    assert!(
        first >= Duration::from_secs(5),
        "its first start is waited for: {first:?}"
    );
    let failed = stands(&sandbox, &folder, "probe", |s| {
        matches!(s, McpStatus::Failed(_))
    })
    .await;
    assert_eq!(
        failed,
        McpStatus::Failed("It took too long to start.".to_owned())
    );

    tokio::time::sleep(Duration::from_millis(300)).await;
    let second = timed("say two").await;
    assert!(
        second >= Duration::from_millis(1500) && second < Duration::from_secs(4),
        "tried again, it is waited for a moment only: {second:?}"
    );
    // While that try is still under way, the next turn doesn't wait at all.
    let third = timed("say three").await;
    assert!(third < Duration::from_millis(1500), "{third:?}");
}

/// A server that failed is tried again when a turn starts, not before, and
/// its tools are there once it answers.
#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_failed_is_tried_again_when_a_turn_starts() {
    let sandbox = Sandbox::start_with_env(&[("HARLO_MCP_RETRY_MS", "200")]).await;
    let client = sandbox.client();
    let project = sandbox.project();
    let folder = project.to_str().expect("utf-8").to_owned();
    // Its program isn't there yet.
    let late = sandbox.dir.path().join("late.py");
    let config = serde_json::json!({ "mcp": { "servers": {
        "probe": { "type": "local", "command": ["python3", "-I", late] },
    } } });
    std::fs::write(project.join("harlo.json"), config.to_string()).expect("config");
    support::allow_project_settings(&client, &folder).await;
    let failed = |s: &McpStatus| matches!(s, McpStatus::Failed(_));
    stands(&sandbox, &folder, "probe", failed).await;

    // Now it is, and nothing tries the server while no turn starts.
    std::fs::write(&late, ECHO).expect("late.py");
    tokio::time::sleep(Duration::from_millis(600)).await;
    stands(&sandbox, &folder, "probe", failed).await;

    let session = client.create_session(&folder).await.expect("session");
    client
        .prompt(&session.id, &Prompt::new("say hello", Delivery::Queue))
        .await
        .expect("prompt");
    stands(&sandbox, &folder, "probe", |s| *s == McpStatus::Connected).await;
    let code = serde_json::json!({ "code": "return await tools.probe.echo({text: 'back'})" });
    run_until(
        &sandbox,
        &session.id,
        &format!("tool execute {code} ;; say done"),
        "echo: back",
    )
    .await;
}

/// A stdio MCP server with one tool, `where`, saying the folder it runs in.
const WHERE: &str = r#"import json, os, sys
for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue
    method = msg.get("method")
    if method == "initialize":
        result = {"protocolVersion": msg["params"]["protocolVersion"],
                  "capabilities": {"tools": {}},
                  "serverInfo": {"name": "where", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": [{"name": "where", "description": "Says where it runs.",
                             "inputSchema": {"type": "object", "properties": {}}}]}
    else:
        result = {"content": [{"type": "text", "text": "runs in " + os.path.basename(os.getcwd())}]}
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}), flush=True)
"#;

/// Waits until `folder`'s server "here" is connected.
async fn connected(sandbox: &Sandbox, folder: &str) {
    let client = sandbox.client();
    for _ in 0..200 {
        let listed = client.mcp_servers(folder).await.expect("listed");
        if listed
            .iter()
            .any(|s| s.name == "here" && s.status == McpStatus::Connected)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the server never connected in {folder}");
}

#[tokio::test(flavor = "multi_thread")]
async fn each_folder_has_its_own_server_started_in_it() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let script = sandbox.dir.path().join("where.py");
    std::fs::write(&script, WHERE).expect("where.py");
    // One entry in the person's own config, for every folder: a program
    // started in the folder that uses it.
    let path = sandbox.dir.path().join("config/harlo/harlo.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("config")).expect("JSON");
    config["mcp"] = serde_json::json!({ "servers": { "here": {
        "type": "local", "command": ["python3", "-I", script], "codemode": false,
    } } });
    std::fs::write(&path, config.to_string()).expect("config");

    let first = sandbox.project().to_str().expect("utf-8").to_owned();
    let other = sandbox.dir.path().join("other");
    std::fs::create_dir_all(&other).expect("other");
    let other = other.to_str().expect("utf-8").to_owned();
    connected(&sandbox, &first).await;
    connected(&sandbox, &other).await;

    // Each folder's sessions reach the one started there.
    let session = client.create_session(&other).await.expect("session");
    run_until(
        &sandbox,
        &session.id,
        "tool here_where {} ;; say ok",
        "runs in other",
    )
    .await;
    let session = client.create_session(&first).await.expect("session");
    run_until(
        &sandbox,
        &session.id,
        "tool here_where {} ;; say ok",
        "runs in project",
    )
    .await;
    // Neither took the other's place: both are still connected.
    connected(&sandbox, &other).await;
    connected(&sandbox, &first).await;
}

/// A stdio MCP server with one tool, `snap`, answering with a picture.
const SNAP: &str = r#"import json, sys
for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue
    method = msg.get("method")
    if method == "initialize":
        result = {"protocolVersion": msg["params"]["protocolVersion"],
                  "capabilities": {"tools": {}},
                  "serverInfo": {"name": "snap", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": [{"name": "snap", "description": "Takes a picture.",
                             "inputSchema": {"type": "object", "properties": {}}}]}
    else:
        result = {"content": [{"type": "text", "text": "The window:"},
                              {"type": "image", "data": "aGVsbG8=", "mimeType": "image/png"}]}
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}), flush=True)
"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_tools_picture_reaches_the_model() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let script = sandbox.dir.path().join("snap.py");
    std::fs::write(&script, SNAP).expect("snap.py");
    let path = sandbox.dir.path().join("config/harlo/harlo.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("config")).expect("JSON");
    config["mcp"] = serde_json::json!({ "servers": { "here": {
        "type": "local", "command": ["python3", "-I", script], "codemode": false,
    } } });
    std::fs::write(&path, config.to_string()).expect("config");
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    connected(&sandbox, &folder).await;

    let session = client.create_session(&folder).await.expect("session");
    run_until(
        &sandbox,
        &session.id,
        "tool here_snap {} ;; say ok",
        "The window:",
    )
    .await;
    // The request after the call shows the model the picture itself.
    let after = support::turn_requests(sandbox.dir.path())
        .last()
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(after.contains("aGVsbG8="), "{after}");
    assert!(after.contains("image/png"), "{after}");
}

/// Sends `script` and answers what it asks leave for with `reply` until
/// the session is idle again; the actions it asked about.
async fn run_answering(
    sandbox: &Sandbox,
    session: &str,
    script: &str,
    reply: PermissionReply,
    why: Option<&str>,
) -> Vec<String> {
    let client = sandbox.client();
    let idles = |all: &[Message]| {
        all.iter()
            .filter(|m| matches!(m, Message::Idle { .. }))
            .count()
    };
    let before = idles(&client.messages(session).await.expect("messages"));
    client
        .prompt(session, &Prompt::new(script, Delivery::Queue))
        .await
        .expect("prompt");
    let mut asked = Vec::new();
    for _ in 0..400 {
        for request in client.permissions(session).await.expect("permissions") {
            asked.push(request.action.clone());
            client
                .reply_permission(session, &request.id, reply, why)
                .await
                .expect("reply");
        }
        if idles(&client.messages(session).await.expect("messages")) > before {
            return asked;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the turn never ended");
}

/// A rule on a server's tool holds whichever way the tool is called: a
/// script's call of it is refused, or asked about, as the model's own
/// call of the tool would be. Leave to run scripts isn't leave for
/// every tool a script can reach.
#[tokio::test(flavor = "multi_thread")]
async fn a_rule_on_a_servers_tool_holds_when_a_script_calls_it() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let folder = project.to_str().expect("utf-8").to_owned();
    let echo = sandbox.dir.path().join("echo.py");
    std::fs::write(&echo, ECHO).expect("echo.py");
    let config = serde_json::json!({ "mcp": { "servers": {
        "probe": { "type": "local", "command": ["python3", "-I", echo] },
    } } });
    std::fs::write(project.join("harlo.json"), config.to_string()).expect("config");
    support::allow_project_settings(&client, &folder).await;
    for _ in 0..200 {
        let listed = client.mcp_servers(&folder).await.expect("listed");
        if listed.iter().any(|s| s.status == McpStatus::Connected) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let session = client.create_session(&folder).await.expect("session");
    let call = |text: &str| {
        let code = format!(
            "try {{ return await tools.probe.echo({{text: '{text}'}}) }} \
             catch (e) {{ return 'refused: ' + e }}"
        );
        format!(
            "tool execute {} ;; say done",
            serde_json::json!({ "code": code })
        )
    };
    let scripts = Rule::any("execute", Effect::Allow);

    // Refused by a rule: the script may run, the tool may not.
    let rules = [scripts.clone(), Rule::any("probe_echo", Effect::Deny)];
    client
        .set_permissions(&session.id, &rules)
        .await
        .expect("rules");
    let asked = run_answering(
        &sandbox,
        &session.id,
        &call("one"),
        PermissionReply::Once,
        None,
    )
    .await;
    assert!(asked.is_empty(), "a refusal asks nobody: {asked:?}");
    let seen = results(&sandbox);
    assert!(!seen.contains("echo: one"), "{seen}");
    assert!(seen.contains("refused: "), "{seen}");

    // Asked about: the person decides each call, under the tool's name.
    let rules = [scripts.clone(), Rule::any("probe_echo", Effect::Ask)];
    client
        .set_permissions(&session.id, &rules)
        .await
        .expect("rules");
    let asked = run_answering(
        &sandbox,
        &session.id,
        &call("two"),
        PermissionReply::Once,
        None,
    )
    .await;
    assert_eq!(asked, ["probe_echo"]);
    assert!(results(&sandbox).contains("echo: two"));
    let asked = run_answering(
        &sandbox,
        &session.id,
        &call("three"),
        PermissionReply::Reject,
        Some("Not this one."),
    )
    .await;
    assert_eq!(asked, ["probe_echo"]);
    let seen = results(&sandbox);
    assert!(!seen.contains("echo: three"), "{seen}");
    assert!(seen.contains("Not this one."), "{seen}");

    // No rule on the tool: it runs unasked, as before.
    client
        .set_permissions(&session.id, &[scripts])
        .await
        .expect("rules");
    let asked = run_answering(
        &sandbox,
        &session.id,
        &call("four"),
        PermissionReply::Once,
        None,
    )
    .await;
    assert!(asked.is_empty(), "{asked:?}");
    assert!(results(&sandbox).contains("echo: four"));
}
