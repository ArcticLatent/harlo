//! Plugins' own code: a tool written in a plugin's `plugin.js` runs for a
//! script once a person allowed it, reaches only the hosts the plugin
//! names, keeps what it stores, and a command written there becomes a
//! prompt. A tool reads and changes the project only through the
//! session's own tools, under its rules; code reads what a person filled
//! in for it, and runs when a turn ends. Every plugin comes from a git
//! repository in the scratch folder; the starter plugin of the guide is
//! copied there from `docs/plugins/starter`.

// Rust guideline compliant 2026-10-09

mod support;

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use harlo_client::{Delivery, Effect, Message, PermissionReply, Prompt, Rule};
use serde_json::{Value, json};
use support::Sandbox;

/// A plugin whose tools try what code can reach.
const PLUGIN_JS: &str = r#"
harlo.tool("shout", async (args, context) => ({
  loud: args.text.toUpperCase(),
  plugin: context.plugin.name,
  version: harlo.plugin.version,
  where: context.session.directory,
}));
harlo.tool("reach", async ({ url }) => {
  const r = await harlo.fetch(url, { method: "POST", headers: { "x-from": "plugin" }, body: "ping" });
  return { status: r.status, ok: r.ok, said: r.status === 200 ? r.json() : r.text() };
});
harlo.tool("count", async () => {
  const n = ((await harlo.storage.get("n")) || 0) + 1;
  await harlo.storage.set("n", n);
  return n;
});
harlo.tool("around", async () => [typeof require, typeof process, typeof fetch, typeof Deno, typeof setTimeout]);
harlo.tool("broken", async () => { console.log("step one"); throw new Error("boom"); });
harlo.command("greet", async (text, context) => `Say hello to ${text} from ${context.plugin.name}.`);
harlo.command("mute", async () => "");
"#;

/// A plugin whose code reaches the session's tools and its own settings,
/// and runs when a turn ends.
const REACH_JS: &str = r#"
harlo.tool("peek", async ({ path }) => await harlo.project.read({ path }));
harlo.tool("fix", async ({ path }) => await harlo.project.edit({ path, oldString: "one", newString: "two" }));
harlo.tool("run", async () => await harlo.project.shell({ command: "echo hi" }));
harlo.tool("whoami", async () => ({ token: await harlo.settings.get("token"), base: await harlo.settings.get("base") }));
harlo.tool("other", async () => await harlo.settings.get("nope"));
harlo.tool("ended", async ({ id }) => await harlo.storage.get("s:" + id));
harlo.command("look", async () => await harlo.project.read({ path: "notes.txt" }));
harlo.on("turn.ended", async (event, context) => {
  await harlo.storage.set("s:" + event.session.id, { outcome: event.outcome, where: context.session.directory });
});
"#;

/// Runs git in `dir` with no one's own git settings.
fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("git")
        .success();
    assert!(ok, "git {args:?}");
}

/// Writes `manifest` and `code` as the plugin "helper"'s repository in the
/// sandbox's scratch folder, and commits them; its folder.
fn repository(sandbox: &Sandbox, manifest: &Value, code: Option<&str>) -> PathBuf {
    let repo = sandbox.dir.path().join("repos/helper");
    if !repo.is_dir() {
        std::fs::create_dir_all(&repo).expect("the repository");
        git(&repo, &["init", "-q"]);
    }
    std::fs::write(repo.join("plugin.json"), manifest.to_string()).expect("the manifest");
    if let Some(code) = code {
        std::fs::write(repo.join("plugin.js"), code).expect("the code");
    }
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "a change"]);
    repo
}

/// The plugin "helper"'s `plugin.json`, allowed to reach `hosts`.
fn manifest(hosts: &[String]) -> Value {
    let no_arguments = json!({ "type": "object", "properties": {} });
    json!({
        "name": "helper",
        "description": "Tries what a plugin's code can do.",
        "version": "0.2.0",
        "tools": [
            {
                "name": "shout",
                "description": "Says it louder.",
                "parameters": {
                    "type": "object",
                    "properties": { "text": { "type": "string" } },
                    "required": ["text"],
                },
            },
            {
                "name": "reach",
                "description": "Asks an address.",
                "parameters": {
                    "type": "object",
                    "properties": { "url": { "type": "string" } },
                    "required": ["url"],
                },
            },
            { "name": "count", "description": "Counts its calls.", "parameters": no_arguments },
            { "name": "around", "description": "Looks around.", "parameters": no_arguments },
            { "name": "broken", "description": "Fails." },
            { "name": "unwritten", "description": "Has no code." },
        ],
        "commands": [
            { "name": "greet", "description": "Greets someone." },
            { "name": "mute" },
        ],
        "permissions": { "network": hosts },
    })
}

/// The `plugin.json` of the plugin "helper" with [`REACH_JS`].
fn reaching() -> Value {
    let tools = ["peek", "fix", "run", "whoami", "other", "ended"]
        .map(|name| json!({ "name": name, "description": "Tries it." }));
    json!({
        "name": "helper",
        "tools": tools,
        "commands": [{ "name": "look" }],
        "events": ["turn.ended"],
        "settings": [
            { "key": "token", "label": "API token", "secret": true },
            { "key": "base" },
        ],
        "permissions": { "project": ["read", "edit"] },
    })
}

/// Calls the sandbox's server; the status and the JSON answer, if any.
async fn call(
    sandbox: &Sandbox,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let mut request = reqwest::Client::new()
        .request(method, format!("{}{path}", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password));
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.expect("request");
    let status = response.status().as_u16();
    (
        status,
        response.json::<Value>().await.unwrap_or(Value::Null),
    )
}

/// Posts to `path`; the plugin the server answers with.
async fn post(sandbox: &Sandbox, path: &str, body: Option<Value>) -> Value {
    let (status, answer) = call(sandbox, reqwest::Method::POST, path, body).await;
    assert_eq!(status, 200, "{answer}");
    answer["data"].clone()
}

/// Starts `code` as a script in a new session under `rules` (the
/// server's own when there are none), whose scripted model then says
/// "Done."; the session's ID.
async fn begin(sandbox: &Sandbox, code: &str, rules: &[Rule]) -> String {
    let client = sandbox.client();
    let session = client
        .create_session(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("session");
    if !rules.is_empty() {
        client
            .set_permissions(&session.id, rules)
            .await
            .expect("rules");
    }
    let prompt = format!("tool execute {} ;; say Done.", json!({ "code": code }));
    client
        .prompt(&session.id, &Prompt::new(&prompt, Delivery::Queue))
        .await
        .expect("prompt");
    session.id
}

/// Waits for the turn of `session` to end; the state of its script's
/// call.
async fn finished(sandbox: &Sandbox, session: &str) -> Value {
    let client = sandbox.client();
    for _ in 0..600 {
        let messages = client.messages(session).await.expect("messages");
        if messages.iter().any(|m| matches!(m, Message::Idle { .. })) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (_, listed) = call(
        sandbox,
        reqwest::Method::GET,
        &format!("/api/session/{session}/message"),
        None,
    )
    .await;
    listed["data"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
        .find(|part| part["type"] == "tool")
        .map(|call| call["state"].clone())
        .expect("the script's call")
}

/// Runs `code` as a script in a new session under `rules`; the state of
/// the script's call.
async fn script_under(sandbox: &Sandbox, code: &str, rules: &[Rule]) -> Value {
    let session = begin(sandbox, code, rules).await;
    finished(sandbox, &session).await
}

/// Runs `code` as a script in a new session, whose scripted model then
/// says "Done."; the state of the script's call, and the session's ID.
async fn script(sandbox: &Sandbox, code: &str) -> (Value, String) {
    let session = begin(sandbox, code, &[]).await;
    (finished(sandbox, &session).await, session)
}

/// The plugin "helper" as the server lists it.
async fn helper(sandbox: &Sandbox) -> Value {
    let (_, listed) = call(sandbox, reqwest::Method::GET, "/harlo/plugins", None).await;
    listed["data"][0].clone()
}

/// What the script whose call's state is `state` returned, as text.
fn returned(state: &Value) -> String {
    state["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part["text"].as_str())
        .collect()
}

/// A web server on this computer that answers every request once it has
/// read it: `/away/<port>` sends the asker to that port, anything else
/// says what was asked. Its port.
fn stand_in() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("its address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut read = vec![0; 8192];
            let got = stream.read(&mut read).unwrap_or_default();
            let request = String::from_utf8_lossy(&read[..got]).into_owned();
            let line = request.lines().next().unwrap_or_default().to_owned();
            let path = line.split(' ').nth(1).unwrap_or_default();
            let answer = if let Some(to) = path.strip_prefix("/away/") {
                format!(
                    "HTTP/1.1 302 Found\r\nlocation: http://127.0.0.1:{to}/landed\r\n\
                     content-length: 0\r\nconnection: close\r\n\r\n"
                )
            } else {
                let body = json!({
                    "asked": line,
                    "from": request.to_lowercase().contains("x-from: plugin"),
                    "body": request.ends_with("ping"),
                })
                .to_string();
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            let _ = stream.write_all(answer.as_bytes());
        }
    });
    port
}

/// The description of the `execute` tool in the last request a turn sent.
fn execute_description(sandbox: &Sandbox) -> String {
    let requests = support::turn_requests(sandbox.dir.path());
    let last = requests.last().expect("a request");
    last["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["function"]["name"] == "execute")
        .map(|t| t["function"]["description"].to_string())
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_in_a_plugins_code_runs_for_a_script_once_allowed() {
    let sandbox = Sandbox::start().await;
    let repo = repository(&sandbox, &manifest(&[]), Some(PLUGIN_JS));
    let shout = r#"return await tools.helper.shout({text: "hello"});"#;

    // Its code waits for a yes like anything that acts for the person.
    let plugin = post(&sandbox, "/harlo/plugins", Some(json!({ "src": repo }))).await;
    assert_eq!(plugin["state"], "waiting", "{plugin}");
    assert_eq!(
        plugin["asks"],
        json!(["Run its code (plugin.js) in a sandbox, for its tools and commands"])
    );
    assert_eq!(plugin["tools"].as_array().map(Vec::len), Some(6));
    assert_eq!(plugin["commands"], 2);
    let (state, _) = script(&sandbox, shout).await;
    assert_eq!(state["status"], "error", "{state}");
    assert!(!execute_description(&sandbox).contains("tools.helper"));

    post(&sandbox, "/harlo/plugins/helper/approve", None).await;
    let (state, _) = script(&sandbox, shout).await;
    assert_eq!(state["status"], "completed", "{state}");
    let loud = returned(&state);
    assert!(loud.contains("HELLO") && loud.contains("0.2.0"), "{loud}");
    assert!(
        loud.contains(sandbox.project().to_str().expect("utf-8")),
        "the session's folder: {loud}"
    );
    // Scripts are told of its tools, with what each takes.
    let told = execute_description(&sandbox);
    assert!(told.contains("helper"), "{told}");
    assert!(
        told.contains("Tries what a plugin's code can do."),
        "{told}"
    );
    assert!(
        told.contains("shout") && told.contains("text: string"),
        "{told}"
    );

    // The code has nothing of the computer it runs on.
    let (state, _) = script(&sandbox, "return await tools.helper.around({});").await;
    assert_eq!(returned(&state).matches("undefined").count(), 5, "{state}");
    // What it throws is the call's error, with what it logged.
    let failing = r"try { await tools.helper.broken({}); } catch (e) { return String(e); }";
    let (state, _) = script(&sandbox, failing).await;
    let why = returned(&state);
    assert!(
        why.contains("The plugin helper") && why.contains("boom"),
        "{why}"
    );
    assert!(why.contains("step one"), "{why}");
    assert!(!why.contains("eval_script"), "no JavaScript stack: {why}");
    // A tool plugin.json names and the code doesn't register says so.
    let missing = r"try { await tools.helper.unwritten({}); } catch (e) { return String(e); }";
    let (state, _) = script(&sandbox, missing).await;
    assert!(returned(&state).contains("registers no tool"), "{state}");
    let unknown = r"try { await tools.helper.nothing({}); } catch (e) { return String(e); }";
    let (state, _) = script(&sandbox, unknown).await;
    assert!(returned(&state).contains("has no tool nothing"), "{state}");

    // New code that asks for nothing new stays in use…
    repository(
        &sandbox,
        &manifest(&[]),
        Some(&PLUGIN_JS.replace("toUpperCase()", "toUpperCase() + '!'")),
    );
    let updated = post(&sandbox, "/harlo/plugins/helper/update", None).await;
    assert_eq!(updated["state"], "active", "{updated}");
    let (state, _) = script(&sandbox, shout).await;
    assert!(returned(&state).contains("HELLO!"), "{state}");
    // …and code that wants to reach a host waits again.
    repository(&sandbox, &manifest(&["api.example.com".to_owned()]), None);
    let wants = post(&sandbox, "/harlo/plugins/helper/update", None).await;
    assert_eq!(wants["state"], "waiting", "{wants}");
    assert!(
        wants["asks"]
            .to_string()
            .contains("Reach api.example.com over the network"),
        "{wants}"
    );
    let (state, _) = script(&sandbox, shout).await;
    assert_eq!(state["status"], "error", "{state}");
}

#[tokio::test(flavor = "multi_thread")]
async fn code_reaches_only_the_hosts_it_names_and_keeps_what_it_stores() {
    let sandbox = Sandbox::start().await;
    let (allowed, other) = (stand_in(), stand_in());
    let hosts = [format!("127.0.0.1:{allowed}")];
    let repo = repository(&sandbox, &manifest(&hosts), Some(PLUGIN_JS));
    post(&sandbox, "/harlo/plugins", Some(json!({ "src": repo }))).await;
    let plugin = post(&sandbox, "/harlo/plugins/helper/approve", None).await;
    assert_eq!(plugin["state"], "active", "{plugin}");

    let reach = format!(
        r#"const went = [];
for (const url of ["http://127.0.0.1:{allowed}/hello", "http://127.0.0.1:{other}/hello",
                   "http://127.0.0.1:{allowed}/away/{other}", "https://example.com/"]) {{
  try {{ went.push(await tools.helper.reach({{url}})); }} catch (e) {{ went.push(String(e)); }}
}}
return went;"#
    );
    let (state, _) = script(&sandbox, &reach).await;
    assert_eq!(state["status"], "completed", "{state}");
    let went: Value = serde_json::from_str(&returned(&state)).expect("what the script returned");
    let went = went.as_array().cloned().unwrap_or_default();
    assert_eq!(went.len(), 4, "{state}");
    // The host it names answers, with the method, header and body sent.
    assert_eq!(went[0]["status"], 200, "{state}");
    assert_eq!(went[0]["said"]["asked"], "POST /hello HTTP/1.1");
    assert_eq!(went[0]["said"]["from"], true);
    assert_eq!(went[0]["said"]["body"], true);
    // Another port, and another host, are refused before anything is sent.
    let refused = went[1].as_str().unwrap_or_default();
    assert!(refused.contains("isn't allowed to reach"), "{state}");
    assert!(
        went[3]
            .as_str()
            .unwrap_or_default()
            .contains("isn't allowed to reach"),
        "{state}"
    );
    // A redirect off the hosts it names isn't followed.
    assert_eq!(went[2]["status"], 302, "{state}");

    // What it stores is there on its next call, and the next session's.
    let count = "return [await tools.helper.count({}), await tools.helper.count({})];";
    let (state, _) = script(&sandbox, count).await;
    assert!(returned(&state).contains("[1,2]"), "{state}");
    let (state, _) = script(&sandbox, "return await tools.helper.count({});").await;
    assert!(returned(&state).contains('3'), "{state}");
    // Removed and added again, it starts with nothing.
    let (status, _) = call(
        &sandbox,
        reqwest::Method::DELETE,
        "/harlo/plugins/helper",
        None,
    )
    .await;
    assert_eq!(status, 204);
    post(&sandbox, "/harlo/plugins", Some(json!({ "src": repo }))).await;
    post(&sandbox, "/harlo/plugins/helper/approve", None).await;
    let (state, _) = script(&sandbox, "return await tools.helper.count({});").await;
    assert!(returned(&state).contains('1'), "{state}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_in_a_plugins_code_becomes_the_prompt() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    // The person's own command of a name the plugin's code also has.
    let own = sandbox.project().join(".harlo/commands");
    std::fs::create_dir_all(&own).expect("commands folder");
    std::fs::write(own.join("mute.md"), "say Mine wins.\n").expect("command");
    let repo = repository(&sandbox, &manifest(&[]), Some(PLUGIN_JS));
    post(&sandbox, "/harlo/plugins", Some(json!({ "src": repo }))).await;
    let listed = client.commands(&folder).await.expect("commands");
    assert!(
        !listed.iter().any(|c| c.name == "greet"),
        "not before a yes"
    );
    post(&sandbox, "/harlo/plugins/helper/approve", None).await;

    let listed = client.commands(&folder).await.expect("commands");
    let greet = listed.iter().find(|c| c.name == "greet").expect("greet");
    assert_eq!(greet.description.as_deref(), Some("Greets someone."));
    assert_eq!(listed.iter().filter(|c| c.name == "mute").count(), 1);

    let session = client.create_session(&folder).await.expect("session");
    client
        .run_command(&session.id, "greet", "Ada", Delivery::Queue)
        .await
        .expect("the command");
    client
        .run_command(&session.id, "mute", "", Delivery::Queue)
        .await
        .expect("the person's own");
    let mut sent = String::new();
    for _ in 0..400 {
        sent = support::turn_requests(sandbox.dir.path())
            .iter()
            .map(|request| request["messages"].to_string())
            .collect();
        if sent.contains("Mine wins.") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(sent.contains("Say hello to Ada from helper."), "{sent}");
    assert!(sent.contains("Mine wins."), "{sent}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plugin_whose_code_cant_run_isnt_taken() {
    let sandbox = Sandbox::start().await;
    let add = reqwest::Method::POST;
    // It names tools, and has no plugin.js.
    let repo = repository(&sandbox, &manifest(&[]), None);
    let src = json!({ "src": repo });
    let (status, said) = call(&sandbox, add.clone(), "/harlo/plugins", Some(src.clone())).await;
    assert_eq!(status, 400, "{said}");
    assert!(said.to_string().contains("has no plugin.js"), "{said}");
    let (_, listed) = call(&sandbox, reqwest::Method::GET, "/harlo/plugins", None).await;
    assert_eq!(listed["data"], json!([]));

    repository(&sandbox, &manifest(&[]), Some(PLUGIN_JS));
    post(&sandbox, "/harlo/plugins", Some(src)).await;
    post(&sandbox, "/harlo/plugins/helper/approve", None).await;

    // An update that asks to reach what this Harlo can't give, or whose
    // code is outside its folder, is refused: the one in use stays.
    let mut wants = manifest(&[]);
    wants["permissions"]["files"] = json!("write");
    repository(&sandbox, &wants, None);
    let (status, said) = call(&sandbox, add.clone(), "/harlo/plugins/helper/update", None).await;
    assert_eq!(status, 400, "{said}");
    assert!(said.to_string().contains("files"), "{said}");
    let mut outside = manifest(&[]);
    outside["main"] = json!("../../../../etc/passwd");
    repository(&sandbox, &outside, None);
    let (status, said) = call(&sandbox, add, "/harlo/plugins/helper/update", None).await;
    assert_eq!(status, 400, "{said}");
    assert!(said.to_string().contains("its own folder"), "{said}");

    let (state, _) = script(&sandbox, "return await tools.helper.shout({text: 'x'});").await;
    assert_eq!(state["status"], "completed", "{state}");
    assert!(returned(&state).contains('X'), "{state}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_reaches_the_project_only_under_the_sessions_rules() {
    let sandbox = Sandbox::start().await;
    let notes = sandbox.project().join("notes.txt");
    std::fs::write(&notes, "one\n").expect("a file");
    let repo = repository(&sandbox, &reaching(), Some(REACH_JS));
    let plugin = post(&sandbox, "/harlo/plugins", Some(json!({ "src": repo }))).await;
    let asks = plugin["asks"].to_string();
    assert!(
        asks.contains(
            "Use a session's own tools from its code, under the session's rules: edit, read"
        ),
        "{asks}"
    );
    post(&sandbox, "/harlo/plugins/helper/approve", None).await;
    let under = |edit: Effect| {
        [
            Rule::any("execute", Effect::Allow),
            Rule::any("read", Effect::Allow),
            Rule::any("edit", edit),
        ]
    };
    let fix = r#"try { return await tools.helper.fix({path: "notes.txt"}); } catch (e) { return String(e); }"#;

    // It reads through the session's read.
    let peek = r#"return await tools.helper.peek({path: "notes.txt"});"#;
    let state = script_under(&sandbox, peek, &under(Effect::Allow)).await;
    assert_eq!(state["status"], "completed", "{state}");
    assert!(returned(&state).contains("one"), "{state}");

    // What the session's rules refuse, its code is refused.
    let state = script_under(&sandbox, fix, &under(Effect::Deny)).await;
    assert!(returned(&state).contains("The plugin helper"), "{state}");
    assert_eq!(std::fs::read_to_string(&notes).expect("read"), "one\n");

    // Where they say to ask, the person is asked, with the change.
    let session = begin(&sandbox, fix, &under(Effect::Ask)).await;
    let client = sandbox.client();
    let mut pending = Vec::new();
    for _ in 0..400 {
        pending = client.permissions(&session).await.expect("permissions");
        if !pending.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let ask = pending.first().expect("it asks");
    assert_eq!(ask.action, "edit");
    assert_eq!(ask.resources, ["notes.txt"]);
    assert_eq!(std::fs::read_to_string(&notes).expect("read"), "one\n");
    client
        .reply_permission(&session, &ask.id, PermissionReply::Once, None)
        .await
        .expect("allowed");
    let state = finished(&sandbox, &session).await;
    assert_eq!(state["status"], "completed", "{state}");
    assert_eq!(std::fs::read_to_string(&notes).expect("read"), "two\n");

    // A tool its plugin.json doesn't name isn't its to call…
    let run = r"try { return await tools.helper.run({}); } catch (e) { return String(e); }";
    let state = script_under(&sandbox, run, &under(Effect::Allow)).await;
    assert!(
        returned(&state).contains("isn't allowed harlo.project.shell"),
        "{state}"
    );
    // …the script itself has none of them…
    let own = r#"try { return await tools.project.read({path: "notes.txt"}); } catch (e) { return String(e); }"#;
    let state = script_under(&sandbox, own, &under(Effect::Allow)).await;
    assert!(returned(&state).contains("tools.project"), "{state}");
    assert!(!returned(&state).contains("two"), "{state}");
    // …and a command has no session's tools to call.
    let (status, answer) = call(
        &sandbox,
        reqwest::Method::POST,
        &format!("/api/session/{session}/command"),
        Some(json!({ "name": "look", "text": "", "delivery": "queue" })),
    )
    .await;
    assert_eq!(status, 400, "{answer}");
    assert!(
        answer
            .to_string()
            .contains("harlo.project is there for a tool"),
        "{answer}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn code_reads_what_a_person_filled_in_for_it() {
    let sandbox = Sandbox::start().await;
    let repo = repository(&sandbox, &reaching(), Some(REACH_JS));
    let plugin = post(&sandbox, "/harlo/plugins", Some(json!({ "src": repo }))).await;
    assert_eq!(
        plugin["settings"],
        json!([
            { "key": "token", "label": "API token", "secret": true, "set": false },
            { "key": "base", "label": "base", "secret": false, "set": false },
        ])
    );
    post(&sandbox, "/harlo/plugins/helper/approve", None).await;
    let fill = |body: Value| {
        call(
            &sandbox,
            reqwest::Method::PUT,
            "/harlo/plugins/helper/settings",
            Some(body),
        )
    };

    let (status, filled) = fill(json!({ "token": "s3cret", "base": "https://example.com" })).await;
    assert_eq!(status, 200, "{filled}");
    // A secret is said to be there, and never given back.
    assert_eq!(filled["data"]["settings"][0]["set"], true);
    assert_eq!(
        filled["data"]["settings"][1]["value"],
        "https://example.com"
    );
    assert!(!helper(&sandbox).await.to_string().contains("s3cret"));
    let whoami = "return await tools.helper.whoami({});";
    let (state, _) = script(&sandbox, whoami).await;
    let said = returned(&state);
    assert!(
        said.contains("s3cret") && said.contains("https://example.com"),
        "{said}"
    );

    // Only what plugin.json names is kept or read.
    let (status, why) = fill(json!({ "nope": "x" })).await;
    assert_eq!(status, 400, "{why}");
    let (status, why) = fill(json!({ "token": 5 })).await;
    assert_eq!(status, 400, "{why}");
    let other = r"try { return await tools.helper.other({}); } catch (e) { return String(e); }";
    let (state, _) = script(&sandbox, other).await;
    assert!(returned(&state).contains("names no setting"), "{state}");

    // Cleared, it's gone; removed, so is the rest.
    let (_, cleared) = fill(json!({ "token": null })).await;
    assert_eq!(cleared["data"]["settings"][0]["set"], false);
    assert_eq!(cleared["data"]["settings"][1]["set"], true);
    let (status, _) = call(
        &sandbox,
        reqwest::Method::DELETE,
        "/harlo/plugins/helper",
        None,
    )
    .await;
    assert_eq!(status, 204);
    let again = post(&sandbox, "/harlo/plugins", Some(json!({ "src": repo }))).await;
    assert_eq!(again["settings"][1]["set"], false, "{again}");
}

#[tokio::test(flavor = "multi_thread")]
async fn code_runs_when_a_turn_ends() {
    let sandbox = Sandbox::start().await;
    let repo = repository(&sandbox, &reaching(), Some(REACH_JS));
    let plugin = post(&sandbox, "/harlo/plugins", Some(json!({ "src": repo }))).await;
    assert!(
        plugin["asks"]
            .to_string()
            .contains("Run its code each time a session's turn ends"),
        "{plugin}"
    );
    assert_eq!(plugin["events"], json!(["turn.ended"]));
    post(&sandbox, "/harlo/plugins/helper/approve", None).await;

    // A turn ends; its code is run with how, for which session.
    let (_, first) = script(&sandbox, "return 1;").await;
    let ended = format!(
        "return await tools.helper.ended({});",
        json!({ "id": first })
    );
    let mut said = String::new();
    for _ in 0..40 {
        let (state, _) = script(&sandbox, &ended).await;
        said = returned(&state);
        if said.contains("succeeded") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(said.contains("succeeded"), "{said}");
    assert!(
        said.contains(sandbox.project().to_str().expect("utf-8")),
        "{said}"
    );
    assert_eq!(helper(&sandbox).await.get("eventError"), None);

    // Why it failed is kept for the person, until it next runs through.
    let failing = REACH_JS.replace(
        "await harlo.storage.set(\"s:\" + event",
        "throw new Error(\"nope\"); await harlo.storage.set(\"s:\" + event",
    );
    repository(&sandbox, &reaching(), Some(&failing));
    let updated = post(&sandbox, "/harlo/plugins/helper/update", None).await;
    assert_eq!(updated["state"], "active", "{updated}");
    script(&sandbox, "return 2;").await;
    let mut why = Value::Null;
    for _ in 0..100 {
        why = helper(&sandbox).await["eventError"].clone();
        if !why.is_null() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(why.to_string().contains("nope"), "{why}");
    repository(&sandbox, &reaching(), Some(REACH_JS));
    post(&sandbox, "/harlo/plugins/helper/update", None).await;
    script(&sandbox, "return 3;").await;
    for _ in 0..100 {
        why = helper(&sandbox).await["eventError"].clone();
        if why.is_null() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(why.is_null(), "{why}");
}

/// Copies the folder `from` and what's in it to `to`.
fn copy_folder(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a folder");
    for entry in std::fs::read_dir(from).expect("the folder") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("its kind").is_dir() {
            copy_folder(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copied");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_guides_starter_plugin_works_as_the_guide_says() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let folder = sandbox.project().to_str().expect("utf-8").to_owned();
    let starter = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/plugins/starter");
    let repo = sandbox.dir.path().join("repos/starter");
    copy_folder(&starter, &repo);
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "the starter"]);
    script(&sandbox, "return 0;").await;
    let before = execute_description(&sandbox);

    let plugin = post(&sandbox, "/harlo/plugins", Some(json!({ "src": repo }))).await;
    assert_eq!(plugin["name"], "starter");
    assert_eq!(plugin["state"], "waiting", "{plugin}");
    assert_eq!(
        plugin["asks"],
        json!([
            "Run its code (plugin.js) in a sandbox, for its tools and commands",
            "Use a session's own tools from its code, under the session's rules: read",
        ])
    );
    assert_eq!(plugin["skills"], 1);
    post(&sandbox, "/harlo/plugins/starter/approve", None).await;

    // Its tool reads a file of the project through the session.
    std::fs::write(sandbox.project().join("notes.txt"), "one two\nthree\n").expect("a file");
    let rules = [
        Rule::any("execute", Effect::Allow),
        Rule::any("read", Effect::Allow),
    ];
    let count = r#"return await tools.starter.count_words({path: "notes.txt"});"#;
    let state = script_under(&sandbox, count, &rules).await;
    assert_eq!(state["status"], "completed", "{state}");
    let counted: Value = serde_json::from_str(&returned(&state)).expect("JSON");
    assert_eq!(counted["words"], 3, "{counted}");
    assert_eq!(counted["characters"], 14, "{counted}");

    // What its tool adds to every request: its plugin's line and its own.
    let added = execute_description(&sandbox).len() - before.len();
    assert!(
        (150..300).contains(&added),
        "{added} characters, where the guide says some 210"
    );

    // Its command sends a prompt, signed with what a person filled in.
    let (status, filled) = call(
        &sandbox,
        reqwest::Method::PUT,
        "/harlo/plugins/starter/settings",
        Some(json!({ "signature": "Ada" })),
    )
    .await;
    assert_eq!(status, 200, "{filled}");
    let session = client.create_session(&folder).await.expect("session");
    client
        .run_command(&session.id, "standup", "the parser", Delivery::Queue)
        .await
        .expect("the command");
    let mut sent = String::new();
    for _ in 0..400 {
        sent = support::turn_requests(sandbox.dir.path())
            .iter()
            .map(|request| request["messages"].to_string())
            .collect();
        if sent.contains("Write a standup note") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(sent.contains("Focus on the parser."), "{sent}");
    assert!(sent.contains("Sign it \\\"Ada\\\"."), "{sent}");
}
