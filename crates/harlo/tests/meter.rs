//! The rail's usage meter: the provider it shows, the few characters
//! under its ring, and its menu, with Claude Code's limits, the server's
//! readings and the server's events fed in as messages. Nothing real is
//! reached.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{ClaudeEvent, Message, MeterEvent, Section, Sheet, SheetEvent};
use harlo_core::claude::{Account, ClaudeCode, Limit, Login, Severity};
use harlo_core::server::{ProviderUsage, Update, UsageAmount, UsageLimit};
use harlo_core::workspace::{Event, EventKind};
use support::{Ui, demo, ui};

/// The demo's newest session, on an `OpenAI` model, and one on Anthropic's.
const ON_OPENAI: &str = "ses_demo_00";
const ON_ANTHROPIC: &str = "ses_demo_01";

fn event(kind: EventKind) -> Message {
    Message::Server(Update::Event(Event {
        id: "evt".into(),
        seq: None,
        time: 1.0,
        kind,
    }))
}

/// What the server read of provider `id`: nothing yet.
fn read(id: &str) -> ProviderUsage {
    ProviderUsage {
        provider: id.to_owned(),
        plan: None,
        limits: Vec::new(),
        amount: None,
        manage: None,
        reached: false,
        error: None,
    }
}

/// A `ChatGPT` plan, which shows its limits on its own page only.
fn chatgpt() -> ProviderUsage {
    ProviderUsage {
        plan: Some("ChatGPT plan".to_owned()),
        manage: Some("https://chatgpt.com/settings/usage".to_owned()),
        ..read("openai")
    }
}

/// An `OpenRouter` key with `percent` of its limit spent.
fn openrouter(percent: f64) -> ProviderUsage {
    ProviderUsage {
        limits: vec![UsageLimit {
            label: "Key limit".to_owned(),
            percent,
            resets_at: None,
            period: Some("monthly".to_owned()),
        }],
        amount: Some(UsageAmount {
            label: "Spent this month".to_owned(),
            value: 23.0,
            currency: "USD".to_owned(),
        }),
        ..read("openrouter")
    }
}

/// Signs Claude Code in to a Max plan with a session and a week limit.
fn claude_plan(ui: &Ui) {
    let now = demo::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_secs();
    let now = i64::try_from(now).expect("fits");
    let limit = |kind: &str, group: &str, percent, resets: i64| Limit {
        kind: kind.into(),
        group: group.into(),
        model: None,
        percent,
        resets_at: Some(now + resets),
        severity: Severity::Normal,
        active: kind == "session",
    };
    ui.send(Message::Claude(ClaudeEvent::Checked(
        ClaudeCode::Installed {
            version: "2.1.330".into(),
            login: Login::Subscription(Account {
                plan: Some("max".to_owned()),
                ..Account::default()
            }),
        },
    )));
    ui.send(Message::Claude(ClaudeEvent::Usage(Ok(Some(vec![
        limit("session", "session", 23.0, 2 * 3600 + 14 * 60 + 5),
        limit("weekly_all", "weekly", 61.0, 3 * 86_400 + 4 * 3600),
    ])))));
}

fn usage(ui: &Ui, read: Vec<ProviderUsage>) {
    ui.send(Message::Meter(MeterEvent::Usage(Ok(read))));
}

#[test]
fn a_claude_plan_shows_its_fullest_limit_and_each_in_the_menu() {
    let ui = ui(demo::app());
    assert!(ui.find("Usage").is_empty(), "nothing to show yet");

    claude_plan(&ui);
    // The week is fuller than the session: it's the one under the ring.
    assert!(ui.shows("61%"));
    assert_eq!(
        ui.button("Usage").accessible_description().as_deref(),
        Some("Claude: 61% of this week used, resets in 3 days 4 h")
    );

    ui.press("Usage");
    assert!(ui.shows("Claude · Max"));
    assert!(ui.shows("5-hour session"));
    assert!(ui.shows("23% · resets in 2 h 14 min"));
    assert!(ui.shows("This week"));
    assert!(ui.shows("61% · resets in 3 days 4 h"));
    assert!(ui.shows("Last used"));

    ui.press("All limits…");
    let app = ui.app();
    assert_eq!(app.sheets.open, Some(Sheet::Settings));
    assert_eq!(app.sheets.section, Section::Providers);
}

#[test]
fn a_session_at_work_makes_its_provider_the_one_shown() {
    let ui = ui(demo::app());
    claude_plan(&ui);
    usage(&ui, vec![chatgpt(), openrouter(23.0)]);
    // Nothing used yet: the fullest of those with a limit.
    assert!(ui.shows("61%"));

    ui.send(event(EventKind::Busy {
        session_id: ON_OPENAI.into(),
    }));
    assert_eq!(ui.app().preferences.meter_last, "openai");
    // A plan whose limits only its own page shows has no ring to fill.
    assert!(ui.shows("plan") && !ui.shows("61%"));
    ui.press("Usage");
    assert!(ui.shows("OpenAI · ChatGPT plan"));
    assert!(ui.shows("Its limits show on its own page only."));
    ui.press("Manage usage…");
    assert!(
        ui.messages().iter().any(
            |m| matches!(m, Message::OpenLink(url) if url == "https://chatgpt.com/settings/usage")
        ),
        "the provider's own page opens"
    );

    // Refused over a limit, it says so in place of a percentage.
    usage(
        &ui,
        vec![
            ProviderUsage {
                reached: true,
                ..chatgpt()
            },
            openrouter(23.0),
        ],
    );
    assert!(ui.shows("limit"));

    // A provider nothing is known of shows as itself, with a dash.
    ui.send(event(EventKind::Busy {
        session_id: ON_ANTHROPIC.into(),
    }));
    assert_eq!(ui.app().preferences.meter_last, "anthropic");
    assert_eq!(
        ui.button("Usage").accessible_description().as_deref(),
        Some("Anthropic tells apps nothing of its limits (used last)")
    );
}

#[test]
fn a_pinned_provider_stays_whatever_is_used_and_follows_its_readings() {
    let ui = ui(demo::app());
    usage(&ui, vec![chatgpt(), openrouter(23.0)]);
    ui.send(event(EventKind::Busy {
        session_id: ON_OPENAI.into(),
    }));
    assert!(ui.shows("plan"));

    ui.press("Usage");
    ui.press("OpenRouter");
    assert_eq!(ui.app().preferences.meter_pin, "openrouter");
    assert!(ui.shows("23%") && !ui.shows("plan"));

    // Work elsewhere doesn't move it, and a new reading fills its ring.
    ui.send(event(EventKind::Busy {
        session_id: ON_ANTHROPIC.into(),
    }));
    usage(&ui, vec![chatgpt(), openrouter(97.4)]);
    assert!(ui.shows("97%"));
    ui.press("Usage");
    assert!(ui.shows("Spent this month: $23.00"));

    // Settings → Providers lists every provider's, not only the one shown.
    ui.press("All limits…");
    assert!(ui.shows("OpenRouter · Key limit"));
    assert!(ui.shows("97% used"));
    assert!(ui.shows("OpenRouter · Spent this month"));
    assert!(ui.shows("OpenAI · ChatGPT plan"));
    assert!(ui.shows("Its limits show on its own page only"));
    ui.send(Message::Sheet(SheetEvent::Close));
    ui.press("Usage");

    // "Last used" lets go of the pin.
    ui.press("Last used");
    assert_eq!(ui.app().preferences.meter_pin, "");
    assert_eq!(
        ui.button("Usage").accessible_description().as_deref(),
        Some("Anthropic tells apps nothing of its limits (used last)")
    );

    // A reading that fails leaves the last one standing.
    ui.send(Message::Meter(MeterEvent::Usage(Err(
        "unreachable".to_owned()
    ))));
    assert_eq!(ui.app().meter.usage.len(), 2);
}
