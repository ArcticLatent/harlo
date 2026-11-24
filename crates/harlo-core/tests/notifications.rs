//! Which notifications go out, when quiet hours hold them back, and what
//! their templates say.

// Rust guideline compliant 2026-10-09

use harlo_core::notifications::{
    Kind, Notifications, Quiet, SOUNDS, Values, clock, parse_clock, render, sound_name, summary,
};

fn values() -> Values {
    Values {
        project_name: "harlo".to_owned(),
        worktree: String::new(),
        branch: "main".to_owned(),
        session_name: "Fix the build".to_owned(),
        agent_name: "Build".to_owned(),
        model_name: "Big Pickle".to_owned(),
        last_message: "All tests pass.".to_owned(),
    }
}

#[test]
fn templates_fill_in_their_variables() {
    let values = values();
    assert_eq!(
        render("{agent_name} on {branch} in {project_name}", &values),
        "Build on main in harlo"
    );
    assert_eq!(
        render("{nope} and {", &values),
        "{nope} and {",
        "unknown ones stay"
    );
    // An empty value leaves no dangling separator.
    let quiet = Values {
        last_message: String::new(),
        ..values
    };
    assert_eq!(
        render("{session_name}: {last_message}", &quiet),
        "Fix the build"
    );
    assert_eq!(
        render("{worktree} · {session_name}", &quiet),
        "Fix the build"
    );
}

#[test]
fn empty_templates_use_the_defaults() {
    let mut notifications = Notifications::default();
    assert_eq!(
        notifications.render(Kind::Finished, &values()),
        (
            "Build finished".to_owned(),
            "Fix the build: All tests pass.".to_owned()
        )
    );
    notifications.finished.title = "{model_name} is done".to_owned();
    notifications.finished.message = "  ".to_owned();
    assert_eq!(
        notifications.render(Kind::Finished, &values()).0,
        "Big Pickle is done"
    );
    assert_eq!(
        notifications.render(Kind::Finished, &values()).1,
        "Fix the build: All tests pass.",
        "blank means the default"
    );
}

#[test]
fn quiet_hours_span_midnight_and_can_let_questions_through() {
    let night = Quiet {
        on: true,
        from: 22 * 60,
        to: 8 * 60,
        questions: false,
    };
    assert!(night.covers(23 * 60));
    assert!(night.covers(3 * 60));
    assert!(!night.covers(8 * 60), "the end is outside");
    assert!(!night.covers(12 * 60));
    let lunch = Quiet {
        from: 12 * 60,
        to: 13 * 60,
        ..night.clone()
    };
    assert!(lunch.covers(12 * 60 + 30));
    assert!(!lunch.covers(23 * 60));
    assert!(
        !Quiet {
            on: false,
            ..night.clone()
        }
        .covers(23 * 60),
        "off"
    );

    let mut notifications = Notifications {
        quiet: night,
        ..Notifications::default()
    };
    let late = Some(23 * 60);
    assert!(!notifications.allows(Kind::Finished, late));
    assert!(!notifications.allows(Kind::Asks, late));
    assert!(notifications.allows(Kind::Finished, Some(12 * 60)));
    assert!(notifications.allows(Kind::Finished, None), "time unknown");
    notifications.quiet.questions = true;
    assert!(
        notifications.allows(Kind::Asks, late),
        "questions get through"
    );
    assert!(!notifications.allows(Kind::Failed, late));

    notifications.failed.on = false;
    assert!(
        !notifications.allows(Kind::Failed, Some(12 * 60)),
        "its switch"
    );
    notifications.enabled = false;
    assert!(!notifications.allows(Kind::Asks, Some(12 * 60)), "all off");
}

#[test]
fn each_event_plays_its_sound_unless_quiet_hours_hush_it() {
    let mut notifications = Notifications::default();
    assert_eq!(
        notifications.sound(Kind::Finished, Some(12 * 60)),
        Some("staplebops-01")
    );
    assert_eq!(notifications.sound(Kind::Failed, None), Some("nope-03"));
    // Sounds don't follow the notification switches.
    notifications.enabled = false;
    notifications.finished.on = false;
    assert!(notifications.sound(Kind::Finished, None).is_some());

    notifications.sounds.set(Kind::Approval, None);
    assert_eq!(
        notifications.sound(Kind::Approval, None),
        None,
        "none chosen"
    );
    notifications.sounds.set(Kind::Approval, Some("yup-02"));
    assert_eq!(notifications.sound(Kind::Approval, None), Some("yup-02"));
    notifications.sounds.set(Kind::Asks, Some("no-such-sound"));
    assert_eq!(notifications.sound(Kind::Asks, None), None, "unknown id");

    notifications.quiet = Quiet {
        on: true,
        ..Quiet::default()
    };
    assert_eq!(notifications.sound(Kind::Approval, Some(23 * 60)), None);
    notifications.quiet.questions = true;
    assert!(
        notifications.sound(Kind::Approval, Some(23 * 60)).is_some(),
        "approvals get through with questions"
    );
}

#[test]
fn sounds_are_named_as_settings_lists_them() {
    assert_eq!(sound_name("staplebops-01"), "Staplebops 01");
    assert_eq!(sound_name("bip-bop-10"), "Bip-bop 10");
    assert_eq!(sound_name(""), "None");
    assert_eq!(SOUNDS.len(), 45);
}

#[test]
fn clock_times_read_and_write() {
    assert_eq!(parse_clock("22:00"), Some(22 * 60));
    assert_eq!(parse_clock(" 8:05 "), Some(8 * 60 + 5));
    for bad in ["24:00", "8:5", "8", "ab:cd", "12:60", ""] {
        assert_eq!(parse_clock(bad), None, "{bad:?}");
    }
    assert_eq!(clock(8 * 60 + 5), "08:05");
    assert_eq!(clock(22 * 60), "22:00");
}

#[test]
fn long_messages_are_cut_to_one_line() {
    assert_eq!(summary("one\n  two", 20), "one two");
    assert_eq!(summary("abcdefghij", 5), "abcd…");
}

#[test]
fn the_local_time_is_read() {
    let minute = harlo_core::notifications::local_minute(std::time::SystemTime::now());
    assert!(minute.is_some_and(|m| m < 24 * 60), "{minute:?}");
}
