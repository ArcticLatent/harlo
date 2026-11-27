//! The usage meter: what it shows of the providers' readings, how full
//! and how urgent, and in which words.

// Rust guideline compliant 2026-10-09

use harlo_core::claude::{Limit, Severity};
use harlo_core::meter::{CLAUDE, Level, Reading, Why, shown};
use harlo_core::server::{ProviderUsage, UsageAmount, UsageLimit};

/// A Claude limit of `kind` used to `percent`, resetting at `resets_at`.
fn limit(
    kind: &str,
    model: Option<&str>,
    percent: f64,
    severity: Severity,
    resets_at: i64,
) -> Limit {
    Limit {
        kind: kind.to_owned(),
        group: if kind == "session" {
            "session"
        } else {
            "weekly"
        }
        .to_owned(),
        model: model.map(str::to_owned),
        percent,
        resets_at: Some(resets_at),
        severity,
        active: kind == "session",
    }
}

/// What the server read of provider `id`.
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

#[test]
fn a_claude_plan_shows_its_fullest_window_and_names_each() {
    let now = 1_000_000;
    let claude = Reading::claude(
        &[
            limit("session", None, 23.4, Severity::Normal, now + 8040),
            limit("weekly_all", None, 61.0, Severity::Normal, now + 3 * 86_400),
            limit(
                "weekly_scoped",
                Some("Opus"),
                12.0,
                Severity::Normal,
                now + 3 * 86_400,
            ),
        ],
        Some("max"),
    );
    assert_eq!(claude.provider, CLAUDE);
    assert_eq!(claude.title(), "Claude · Max");
    let labels: Vec<&str> = claude.windows.iter().map(|w| w.label.as_str()).collect();
    assert_eq!(labels, ["5-hour session", "This week", "This week, Opus"]);
    // The week is fuller than the session, so it's the one that counts.
    assert_eq!(claude.short(), "61%");
    assert_eq!(claude.fraction(), Some(0.61));
    assert_eq!(claude.level(), Level::Calm);
    assert_eq!(claude.windows[0].status(now), "23% · resets in 2 h 14 min");
    assert_eq!(
        claude.sentence(now),
        "Claude: 61% of this week used, resets in 3 days"
    );
}

#[test]
fn the_ring_warns_as_a_limit_fills_and_by_the_providers_own_grade() {
    let by_share = |percent: f64| {
        let mut usage = read("openrouter");
        usage.limits.push(UsageLimit {
            label: "Key limit".to_owned(),
            percent,
            resets_at: None,
            period: Some("monthly".to_owned()),
        });
        Reading::from_server(&usage, "OpenRouter").level()
    };
    assert_eq!(by_share(74.9), Level::Calm);
    assert_eq!(by_share(75.0), Level::Warning);
    assert_eq!(by_share(95.0), Level::Critical);

    // Claude grades its own limits: its grade counts, not the share.
    let graded = Reading::claude(&[limit("session", None, 60.0, Severity::Warning, 0)], None);
    assert_eq!(graded.level(), Level::Warning);
    assert_eq!(graded.title(), "Claude");
}

#[test]
fn a_provider_with_no_limit_to_read_still_says_what_it_can() {
    // An amount left to spend.
    let mut usage = read("deepseek");
    usage.amount = Some(UsageAmount {
        label: "Balance".to_owned(),
        value: 4.2,
        currency: "USD".to_owned(),
    });
    let deep = Reading::from_server(&usage, "DeepSeek");
    assert_eq!((deep.short().as_str(), deep.fraction()), ("$4.20", None));
    assert_eq!(deep.sentence(0), "DeepSeek: balance $4.20");

    // A plan whose limits only its own page shows.
    let mut usage = read("openai");
    usage.plan = Some("ChatGPT plan".to_owned());
    usage.manage = Some("https://chatgpt.com/settings/usage".to_owned());
    let plan = Reading::from_server(&usage, "OpenAI");
    assert_eq!((plan.short().as_str(), plan.fraction()), ("plan", None));
    assert_eq!(plan.title(), "OpenAI · ChatGPT plan");
    assert_eq!(plan.level(), Level::Calm);

    // The same plan once a request was refused over a limit: a full,
    // urgent ring.
    usage.reached = true;
    let spent = Reading::from_server(&usage, "OpenAI");
    assert_eq!(
        (spent.short().as_str(), spent.fraction()),
        ("limit", Some(1.0))
    );
    assert_eq!(spent.level(), Level::Critical);
    assert_eq!(
        spent.sentence(0),
        "OpenAI refused its last request over a limit"
    );

    // Nothing at all.
    let quiet = Reading::unknown("xai", "xAI");
    assert_eq!((quiet.short().as_str(), quiet.fraction()), ("—", None));
    assert_eq!(quiet.sentence(0), "xAI tells apps nothing of its limits");
}

#[test]
fn the_meter_shows_the_pinned_provider_then_the_last_used_then_the_fullest() {
    let claude = Reading::claude(&[limit("session", None, 10.0, Severity::Normal, 0)], None);
    let mut usage = read("openrouter");
    usage.limits.push(UsageLimit {
        label: "Key limit".to_owned(),
        percent: 80.0,
        resets_at: None,
        period: None,
    });
    let router = Reading::from_server(&usage, "OpenRouter");
    let quiet = Reading::unknown("xai", "xAI");
    let all = [claude, router, quiet];
    let pick =
        |pin: &str, last: &str| shown(&all, pin, last).map(|s| (s.reading.provider.clone(), s.why));

    assert_eq!(pick("xai", CLAUDE), Some(("xai".to_owned(), Why::Pinned)));
    assert_eq!(pick("", CLAUDE), Some((CLAUDE.to_owned(), Why::Last)));
    // A pin on a provider that is gone falls back to the last used.
    assert_eq!(pick("gone", "xai"), Some(("xai".to_owned(), Why::Last)));
    // Nothing used yet: the fullest of those with a limit.
    assert_eq!(pick("", ""), Some(("openrouter".to_owned(), Why::Fullest)));
    assert_eq!(shown(&all[2..], "", ""), None, "nothing to show a ring for");
}
