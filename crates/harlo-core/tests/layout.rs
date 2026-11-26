//! Saving and loading the window layout and Harlo's own settings.
//!
//! Each test uses a scratch folder outside the repository, never the user's state folder.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;

use harlo_core::layout::{
    Appearance, ChatWidth, Layout, Preferences, Speed, Store, Tabs, TextSize,
};
use harlo_core::theme::{Contrast, DiffColors};

fn sample() -> Layout {
    Layout {
        order: vec!["/home/u/code/harlo".into(), "/home/u".into()],
        hidden: vec!["/home/u/old".into()],
        sidebar: false,
        expanded: vec!["/home/u".into()],
        panel: Some("Notes".into()),
        sidebar_width: Some(320),
        panel_width: Some(410),
        terminal_height: Some(240),
        tabs: BTreeMap::from([(
            "Local".to_owned(),
            Tabs {
                open: vec!["ses_a".into(), "ses_b".into()],
                active: Some("ses_b".into()),
                unread: vec!["ses_c".into()],
                followed: 1_800_000_000_000,
            },
        )]),
    }
}

#[test]
fn a_missing_file_gives_the_default_layout() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let store = Store::new(scratch.path().join("nested/harlo.db"));
    let layout = store.load().expect("no file is fine");
    assert_eq!(layout, Layout::default());
    assert!(layout.sidebar);
    assert!(!store.path().exists(), "loading creates nothing");
}

#[test]
fn a_saved_layout_loads_back_and_overwrites() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let store = Store::new(scratch.path().join("nested/harlo.db"));
    store.save(&sample()).expect("the first save");
    assert_eq!(store.load().expect("the saved layout"), sample());

    let mut changed = sample();
    changed.sidebar = true;
    changed.expanded.clear();
    store.save(&changed).expect("the second save");
    assert_eq!(store.load().expect("the saved layout"), changed);
}

#[test]
fn an_older_layout_still_loads() {
    // Before the sizes were saved, the open right panel was already named.
    let json = r#"{"panel":"Context","tabs":{"Local":{"open":["ses_a"],"active":"ses_a"}}}"#;
    let layout: Layout = serde_json::from_str(json).expect("the older layout");
    assert_eq!(layout.panel.as_deref(), Some("Context"));
    assert_eq!(layout.sidebar_width, None);
    assert_eq!(layout.tabs.get("Local").expect("its tabs").open, ["ses_a"]);
}

#[test]
fn a_layout_saved_with_panes_keeps_its_tabs() {
    // While the main area could be split into panes, the arrangement was saved too.
    let json = r#"{"tabs":{"Local":{"open":["ses_a"],"active":"ses_a",
        "dock":{"tabs":{"name":"center","group":"documents","tabs":["session:ses_a"],
        "shown":"session:ses_a"}}}}}"#;
    let layout: Layout = serde_json::from_str(json).expect("the layout with panes");
    let tabs = layout.tabs.get("Local").expect("its tabs");
    assert_eq!(tabs.open, ["ses_a"]);
    assert_eq!(tabs.active.as_deref(), Some("ses_a"));
}

#[tokio::test]
async fn saving_later_runs_off_the_calling_thread() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let store = Store::new(scratch.path().join("harlo.db"));
    store.clone().save_later(sample()).await.expect("the save");
    assert_eq!(store.load().expect("the saved layout"), sample());
}

#[test]
fn an_unreadable_layout_is_an_error() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let path = scratch.path().join("harlo.db");
    std::fs::write(&path, b"not a database").expect("a junk file");
    let error = Store::new(&path).load().expect_err("junk can't be read");
    assert!(
        error.to_string().starts_with("Harlo's database failed"),
        "{error}"
    );
}

#[test]
fn preferences_load_back_beside_the_layout() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let store = Store::new(scratch.path().join("harlo.db"));
    assert_eq!(
        store.load_preferences().expect("no file is fine"),
        Preferences::default()
    );
    assert!(
        Preferences::default().reopen_tabs,
        "tabs reopen unless turned off"
    );

    store.save(&sample()).expect("the layout");
    let chosen = Preferences {
        reopen_tabs: false,
        keep_in_tray: false,
        setup_done: true,
        notifications: harlo_core::notifications::Notifications {
            while_focused: true,
            quiet: harlo_core::notifications::Quiet {
                on: true,
                ..harlo_core::notifications::Quiet::default()
            },
            ..harlo_core::notifications::Notifications::default()
        },
        dark_theme: "dracula".to_owned(),
        light_theme: "github".to_owned(),
        contrast: Contrast::More,
        diff_colors: DiffColors::BlueOrange,
        motion: Speed::Off,
        chat_width: ChatWidth::Full,
        text_size: TextSize::Larger,
        code_size: TextSize::Small,
        glass: true,
        interface_font: "Noto Sans".to_owned(),
        code_font: "Fira Code".to_owned(),
        own_tools: false,
        models: std::collections::BTreeMap::from([("openai/gpt-5".to_owned(), false)]),
        voice_model: "small".to_owned(),
        voice_language: "de".to_owned(),
        voice_microphone: "pulseaudio:usb-mic".to_owned(),
        check_updates: false,
        update_announced: "2.1.0".to_owned(),
        local_engine: "llamacpp".to_owned(),
        local_servers: vec!["llamacpp".to_owned()],
        llamacpp_model: "/models/tiny.gguf".to_owned(),
        computers: vec!["-p 2222 me@buildbox".to_owned()],
        phone: true,
        phone_relay: true,
        phone_paused: true,
        avatar: "scarf".to_owned(),
        appearance: Appearance::Light,
        project_models: [(
            "/home/u/code/harlo".to_owned(),
            "fake/fake-coder".to_owned(),
        )]
        .into_iter()
        .collect(),
        commit_model: Some("fake/fake-small".to_owned()),
        gitlab_host: "gitlab.example.org".to_owned(),
        forgejo_host: "http://forgejo:3000".to_owned(),
        forgejo_account: Some("forgejo:3000 me".to_owned()),
    };
    store.save_preferences(&chosen).expect("the preferences");
    assert_eq!(store.load_preferences().expect("saved"), chosen);
    assert_eq!(
        store.load().expect("the layout"),
        sample(),
        "each keeps its own row"
    );
}

/// Preferences saved before a setting existed load with its default, and a
/// setting since dropped is ignored.
#[test]
fn older_preferences_load_with_defaults() {
    let old = r#"{"reopen_tabs": false, "theme": "dracula", "appearance": "light"}"#;
    let loaded: Preferences = serde_json::from_str(old).expect("an older save");
    assert_eq!(
        loaded,
        Preferences {
            reopen_tabs: false,
            appearance: Appearance::Light,
            ..Preferences::default()
        }
    );
}
