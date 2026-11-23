//! The sidebar's session lists: ages, the visible limit, the filter and folder names.

// Rust guideline compliant 2026-10-09

use std::time::Duration;

use harlo_core::sessions::{SessionSummary, VISIBLE_SESSIONS, compact_age, folder_name, visible};

fn rows(titles: &[&str]) -> Vec<SessionSummary> {
    titles
        .iter()
        .enumerate()
        .map(|(i, t)| SessionSummary {
            id: format!("ses_{i}"),
            title: (*t).into(),
            maker: None,
            age: Duration::ZERO,
            agent: None,
            busy: false,
            waiting: false,
            asking: false,
            children: Vec::new(),
        })
        .collect()
}

#[test]
fn ages_are_compact() {
    let cases = [
        (0, "now"),
        (59, "now"),
        (60, "1m"),
        (3_599, "59m"),
        (4 * 3_600, "4h"),
        (86_399, "23h"),
        (86_400, "1d"),
        (10 * 86_400, "10d"),
        (29 * 86_400, "29d"),
        (30 * 86_400, "1mo"),
        (364 * 86_400, "12mo"),
        (365 * 86_400, "1y"),
        (800 * 86_400, "2y"),
    ];
    for (secs, want) in cases {
        assert_eq!(compact_age(Duration::from_secs(secs)), want, "{secs}s");
    }
}

#[test]
fn long_lists_show_a_limited_number_until_expanded() {
    let titles: Vec<String> = (0..31).map(|i| format!("session {i}")).collect();
    let titles: Vec<&str> = titles.iter().map(String::as_str).collect();
    let list = rows(&titles);

    let (shown, hidden) = visible(&list, "", false);
    assert_eq!(
        (shown.len(), hidden),
        (VISIBLE_SESSIONS, 31 - VISIBLE_SESSIONS)
    );
    assert_eq!(shown[0].title, "session 0", "newest first, as given");

    let (shown, hidden) = visible(&list, "", true);
    assert_eq!((shown.len(), hidden), (31, 0));

    assert_eq!(visible(&rows(&["a", "b"]), "", false).1, 0);
}

#[test]
fn the_filter_ignores_case_and_shows_every_match() {
    let mut titles = vec!["Fix the Retry loop"; 20];
    titles.push("unrelated");
    let list = rows(&titles);
    let (shown, hidden) = visible(&list, "  retry ", false);
    assert_eq!((shown.len(), hidden), (20, 0));
    assert!(visible(&list, "nothing like it", false).0.is_empty());
}

#[test]
fn folders_are_named_by_their_last_part() {
    assert_eq!(folder_name("/home/u/code/harlo"), "harlo");
    assert_eq!(folder_name("/home/u/code/harlo/"), "harlo");
    assert_eq!(folder_name("/"), "");
    assert_eq!(folder_name("relative"), "relative");
}
