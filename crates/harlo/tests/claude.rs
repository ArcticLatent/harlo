//! The Claude subscription in Settings → Models, with Claude Code's checks,
//! the plan's usage
//! and OpenCode's answers fed in as messages. The real flow, against a
//! private server and a stand-in Claude Code, is in `server_claude.rs`.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{App, ClaudeEvent, Message};
use harlo_core::claude::{Account, ClaudeCode, Limit, Login, Severity};
use harlo_core::server::{Connection, Integration, Location};
use support::{Ui, demo, ui};

/// Claude Agent with these connections.
fn agent(connections: Vec<Connection>) -> Integration {
    Integration {
        id: "claude-agent".into(),
        name: "Claude Agent".into(),
        methods: Vec::new(),
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
            method: Some("external".into()),
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
            method: Some("external".into()),
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

#[test]
fn a_plans_usage_shows_each_limit_and_when_it_resets() {
    let now = demo::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_secs();
    let now = i64::try_from(now).expect("fits");
    let limit =
        |kind: &str, group: &str, model: Option<&str>, percent, resets: Option<i64>| Limit {
            kind: kind.into(),
            group: group.into(),
            model: model.map(str::to_owned),
            percent,
            resets_at: resets.map(|s| now + s),
            severity: if percent > 90.0 {
                Severity::Critical
            } else if percent > 70.0 {
                Severity::Warning
            } else {
                Severity::Normal
            },
            active: false,
        };
    let ui = models(
        demo::app(),
        Some(ClaudeCode::Installed {
            version: "2.1.330".into(),
            login: Login::Subscription(Account::default()),
        }),
        agent(Vec::new()),
    );
    assert!(ui.shows("Plan usage"), "asked once signed in to a plan");
    ui.send(Message::Claude(ClaudeEvent::Usage(Ok(Some(vec![
        limit(
            "session",
            "session",
            None,
            37.0,
            Some(2 * 3600 + 14 * 60 + 5),
        ),
        limit(
            "weekly_all",
            "weekly",
            None,
            74.4,
            Some(3 * 86_400 + 4 * 3600),
        ),
        limit("weekly_scoped", "weekly", Some("Fable"), 91.5, None),
    ])))));
    assert!(ui.shows("Current session"));
    assert!(ui.shows("37% used · resets in 2 h 14 min"));
    assert!(ui.shows("This week, all models"));
    assert!(ui.shows("74% used · resets in 3 days 4 h"));
    assert!(ui.shows("This week, Fable"));
    assert!(ui.shows("92% used"));
    assert!(!ui.shows("Plan usage"), "the checking row goes");

    // Without a plan there's nothing to show.
    ui.send(Message::Claude(ClaudeEvent::Usage(Ok(None))));
    assert!(!ui.shows("Current session"));
}
