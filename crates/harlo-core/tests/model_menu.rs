//! Which models the model menus offer.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use harlo_core::model_menu::{key, shown, shown_by_default};
use harlo_core::server::Model;

/// 2026-11-23, as the menus' clock.
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_795_400_000)
}

/// Milliseconds `days` before [`now`].
fn days_ago(days: u64) -> i64 {
    let at = now() - Duration::from_hours(24 * days);
    i64::try_from(
        at.duration_since(UNIX_EPOCH)
            .expect("after 1970")
            .as_millis(),
    )
    .expect("a date")
}

fn model(
    provider: &str,
    id: &str,
    family: Option<&str>,
    released: Option<i64>,
    free: bool,
) -> Model {
    Model {
        provider_id: provider.into(),
        id: id.into(),
        name: id.into(),
        variants: Vec::new(),
        images: false,
        context: None,
        family: family.map(str::to_owned),
        released,
        free,
    }
}

#[test]
fn the_newest_of_each_recent_family_shows_unless_chosen_otherwise() {
    let models = [
        model("openai", "gpt-5", Some("gpt"), Some(days_ago(90)), false),
        model("openai", "gpt-5.1", Some("gpt"), Some(days_ago(10)), false),
        model("openai", "o3", Some("o"), Some(days_ago(400)), false),
        model("openai", "dated-never", None, None, false),
        model(
            "opencode",
            "big-pickle",
            Some("pickle"),
            Some(days_ago(700)),
            true,
        ),
        model(
            "opencode",
            "paid",
            Some("pickle"),
            Some(days_ago(700)),
            false,
        ),
        model(
            "anthropic",
            "gpt-5.1",
            Some("gpt"),
            Some(days_ago(5)),
            false,
        ),
    ];
    let defaults = shown_by_default(&models, now());
    let no_choices = BTreeMap::new();
    let offered = |id: &str, provider: &str| shown(&no_choices, &defaults, &key(provider, id));
    assert!(offered("gpt-5.1", "openai"), "the newest of its family");
    assert!(!offered("gpt-5", "openai"), "superseded");
    assert!(!offered("o3", "openai"), "older than six months");
    assert!(offered("dated-never", "openai"), "undated");
    assert!(
        offered("big-pickle", "opencode"),
        "OpenCode's free, however old"
    );
    assert!(!offered("paid", "opencode"), "old and paid");
    assert!(offered("gpt-5.1", "anthropic"), "families are per provider");

    let choices = BTreeMap::from([
        (key("openai", "gpt-5"), true),
        (key("openai", "gpt-5.1"), false),
    ]);
    assert!(
        shown(&choices, &defaults, &key("openai", "gpt-5")),
        "chosen"
    );
    assert!(
        !shown(&choices, &defaults, &key("openai", "gpt-5.1")),
        "hidden"
    );
}
