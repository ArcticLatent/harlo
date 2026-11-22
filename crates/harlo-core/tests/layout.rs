//! Saving and loading the window layout.
//!
//! Each test uses a scratch folder outside the repository, never the user's state folder.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;

use harlo_core::layout::{DockNode, Layout, Store, Tabs};

fn sample() -> Layout {
    Layout {
        order: vec!["/home/u/code/harlo".into(), "/home/u".into()],
        sidebar: false,
        expanded: vec!["/home/u".into()],
        tabs: BTreeMap::from([(
            "Local".to_owned(),
            Tabs {
                open: vec!["ses_a".into(), "ses_b".into()],
                active: Some("ses_b".into()),
                dock: Some(DockNode::Split {
                    across: true,
                    shares: vec![700, 300],
                    parts: vec![
                        DockNode::Tabs {
                            name: Some("center".into()),
                            group: Some("documents".into()),
                            tabs: vec!["session:ses_a".into(), "session:ses_b".into()],
                            shown: Some("session:ses_b".into()),
                        },
                        DockNode::Tabs {
                            name: Some("right".into()),
                            group: Some("tools".into()),
                            tabs: vec!["panel:notes".into()],
                            shown: Some("panel:notes".into()),
                        },
                    ],
                }),
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
fn a_layout_saved_before_the_dock_still_loads() {
    // Older layouts also named the one open right panel.
    let json = r#"{"panel":"Context","tabs":{"Local":{"open":["ses_a"],"active":"ses_a"}}}"#;
    let layout: Layout = serde_json::from_str(json).expect("the older layout");
    let tabs = layout.tabs.get("Local").expect("its tabs");
    assert_eq!(tabs.open, ["ses_a"]);
    assert_eq!(tabs.dock, None);
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
