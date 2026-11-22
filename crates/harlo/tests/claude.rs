//! The Claude subscription in Settings → Models, with Claude Code's checks
//! and OpenCode's answers fed in as messages. The real flow, against a
//! private server and a stand-in Claude Code, is in `server_claude.rs`.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{App, ClaudeEvent, Message};
use harlo_core::claude::{Account, ClaudeCode, Login};
use harlo_core::server::{Connection, Integration, Location};
use support::{Ui, demo, ui};

/// Claude Agent with these connections.
fn agent(connections: Vec<Connection>) -> Integration {
    Integration {
        id: "claude-agent".into(),
        name: "Claude Agent".into(),
        connections,
    }
}

/// Settings open at Models, with Claude Code and the integration as given.
fn models(app: App, code: Option<ClaudeCode>, integration: Integration) -> Ui {
    let ui = ui(app);
    ui.press("Settings");
    ui.press("Models");
    if let Some(code) = code {
        ui.send(Message::Claude(ClaudeEvent::Checked(code)));
    }
    ui.send(Message::Claude(ClaudeEvent::Agent(Ok(Some(integration)))));
    ui
}

#[test]
fn a_missing_claude_code_offers_to_install_it() {
    let ui = models(demo::app(), Some(ClaudeCode::Missing), agent(Vec::new()));
    assert!(ui.shows("Claude subscription"));
    assert!(ui.shows("Not installed"));
    ui.button("Install");
    assert!(ui.shows("Not connected"));
    ui.button("Connect");
}

#[test]
fn a_subscription_shows_the_account_and_a_console_sign_in_doesnt_count() {
    let ui = models(
        demo::app(),
        Some(ClaudeCode::Installed {
            version: "2.1.330".into(),
            login: Login::Subscription(Account {
                email: Some("a@example.com".into()),
                organization: None,
                plan: Some("pro".into()),
            }),
        }),
        agent(vec![Connection {
            credential: Some("crd_1".into()),
            label: "Claude Agent".into(),
            needs_sign_in: None,
        }]),
    );
    assert!(ui.shows("a@example.com · Pro plan"));
    ui.button("Switch account");
    assert!(ui.shows("Connected"));
    ui.button("Disconnect");

    ui.send(Message::Claude(ClaudeEvent::Checked(
        ClaudeCode::Installed {
            version: "2.1.330".into(),
            login: Login::Other {
                method: "console".into(),
            },
        },
    )));
    assert!(ui.shows("Signed in with console, not a subscription"));
    ui.button("Sign in");
}

#[test]
fn a_connection_that_needs_signing_in_says_why() {
    let ui = models(
        demo::app(),
        Some(ClaudeCode::Installed {
            version: "2.1.330".into(),
            login: Login::SignedOut,
        }),
        agent(vec![Connection {
            credential: Some("crd_1".into()),
            label: "Claude Agent".into(),
            needs_sign_in: Some("Claude Code is signed out".into()),
        }]),
    );
    assert!(ui.shows("Needs sign-in: Claude Code is signed out"));
    assert_eq!(
        ui.find("Sign in").len(),
        2,
        "on the account and the connection"
    );
}

#[test]
fn a_server_elsewhere_only_shows_its_connection() {
    let mut app = App::new(
        Location::Server {
            url: "http://10.0.0.5:4096".into(),
            password: None,
        },
        Vec::new(),
        demo::layout(),
    );
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(demo::connected(demo::snapshot()));
    let ui = models(app, None, agent(Vec::new()));
    assert!(
        !ui.shows("Claude Code"),
        "this computer's Claude Code isn't that server's"
    );
    assert!(ui.shows(
        "This server runs on another computer, which uses its own Claude Code. Sign in there \
         with `claude auth login`."
    ));
    assert!(ui.shows("Not connected"));
    assert!(ui.find("Install").is_empty());
}
