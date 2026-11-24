//! A Claude subscription against a private `harlo-server`: Settings →
//! Models checks Claude Code, signs it in through a terminal, and connects
//! and disconnects the server's Claude Agent.
//!
//! Claude Code is a stand-in script in the sandbox's scratch folder, at the
//! path the sandbox server runs for `claude`; the real Claude Code and its
//! sign-in are never touched.

// Rust guideline compliant 2026-10-09

#[path = "../../harlo-client/tests/support/mod.rs"]
mod server;
mod support;

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::sync::Arc;

use harlo::app::{Message, TerminalEvent};
use harlo_core::server::{Connected, Update};
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// A stand-in Claude Code: signed out until `auth login` runs, then signed
/// in to a Max plan; asked for its models the way the server's Claude Agent
/// asks, it offers Opus and Sonnet, and asked for the plan's usage, the
/// session's limit is 42% used.
const STAND_IN: &str = r#"#!/usr/bin/env python3
import json, os, sys
status = os.path.join(os.path.dirname(os.path.abspath(__file__)), "status.json")
args = sys.argv[1:]
if args[:1] == ["--version"]:
    print("2.1.330 (Claude Code)")
elif args[:2] == ["auth", "status"]:
    print(open(status).read())
elif args[:2] == ["auth", "login"]:
    print("Signed in.")
    json.dump({"loggedIn": True, "authMethod": "claude.ai",
               "email": "a@example.com", "subscriptionType": "max"}, open(status, "w"))
else:
    efforts = ["low", "medium", "high", "xhigh", "max"]
    models = [{"value": v, "displayName": n, "description": n, "supportsEffort": True,
               "supportedEffortLevels": efforts} for v, n in (("opus", "Opus"), ("sonnet", "Sonnet"))]
    for line in sys.stdin:
        request = json.loads(line)
        if request.get("type") != "control_request":
            continue
        if request["request"]["subtype"] == "get_usage":
            print(json.dumps({"type": "control_response", "response": {
                "subtype": "success", "request_id": request["request_id"],
                "response": {"rate_limits_available": True, "rate_limits": {"limits": [
                    {"kind": "session", "group": "session", "percent": 42,
                     "severity": "normal", "resets_at": None, "is_active": True,
                     "scope": None}]}}}}), flush=True)
        else:
            print(json.dumps({"type": "control_response", "response": {
                "subtype": "success", "request_id": request["request_id"],
                "response": {"commands": [], "agents": [], "models": models,
                             "account": {"tokenSource": "none", "apiProvider": "firstParty"}}}}),
                  flush=True)
"#;

/// Writes the stand-in Claude Code at `path`, signed out.
fn stand_in(path: &Path) {
    let folder = path.parent().expect("a folder");
    std::fs::create_dir_all(folder).expect("the folder");
    std::fs::write(
        folder.join("status.json"),
        r#"{"loggedIn": false, "authMethod": "none"}"#,
    )
    .expect("the status");
    std::fs::write(path, STAND_IN).expect("the stand-in");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("executable");
}

/// The window connected to `sandbox`, with Claude Code at `claude`.
fn window(sandbox: &Sandbox, claude: &Path) -> Ui {
    let mut app = demo::new_app().with_claude_program(claude);
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        server: "Harlo".into(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        snapshot: demo::snapshot(),
    }))));
    app.new_folder = Some(sandbox.project_str());
    ui(app)
}

#[test]
fn a_claude_subscription_signs_in_connects_and_disconnects() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let claude = sandbox.root().join("bin/claude");
    stand_in(&claude);
    let ui = window(&sandbox, &claude);

    // The server loads its integrations in the background after it starts.
    let client = sandbox.client();
    tasks.block_on(async {
        tokio::time::timeout(WAIT, async {
            while client
                .integration("claude-agent")
                .await
                .ok()
                .flatten()
                .is_none()
            {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        })
        .await
        .expect("Claude Agent loads");
    });

    ui.press("Settings");
    ui.press("Providers");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Not signed in")
            && ui.shows("Not connected")),
        "signed out and not connected at first"
    );
    assert!(ui.shows("2.1.330"));

    // Signing in runs in a terminal, and is checked again once it ends.
    ui.press("Sign in");
    assert_eq!(ui.app().sheets.open, None, "Settings steps aside");
    assert_eq!(ui.app().terminals.shells().len(), 1);
    ui.send(Message::Terminal(TerminalEvent::Typed(b"\r".to_vec())));
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().terminals.shells().is_empty()),
        "the sign-in ends when Enter is pressed"
    );
    ui.press("Settings");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("a@example.com · Max plan")),
        "the account and its plan"
    );
    assert!(ui.button("Switch account").accessible_label().is_some());
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("42% used")),
        "the plan's usage"
    );

    ui.press("Connect");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Disconnect")),
        "the server is connected"
    );
    let project = sandbox.project_str();
    // Claude Agent's models, with Claude Code's effort levels, are offered.
    let deadline = std::time::Instant::now() + WAIT;
    let opus = loop {
        let models = tasks.block_on(client.models(&project)).expect("the models");
        if let Some(opus) = models
            .into_iter()
            .find(|m| m.provider_id == "claude-agent" && m.id == "opus")
        {
            break opus;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no Claude Agent models"
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
    };
    assert_eq!(opus.variants, ["low", "medium", "high", "xhigh", "max"]);

    ui.press("Disconnect");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Not connected")),
        "the server is disconnected"
    );
}
