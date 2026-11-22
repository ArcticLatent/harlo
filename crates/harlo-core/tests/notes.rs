//! Todos in notes, and notes kept in the store.

// Rust guideline compliant 2026-10-09

use harlo_core::layout::Store;
use harlo_core::notes::{add_todo, todo_counts, todo_state, toggle_todo};

#[test]
fn todo_lines_are_recognised_and_ticked() {
    assert_eq!(todo_state("- [ ] write tests"), Some(false));
    assert_eq!(todo_state("  - [x] done"), Some(true));
    assert_eq!(todo_state("- [X] done"), Some(true));
    assert_eq!(todo_state("- [ ]"), Some(false), "an empty todo");
    assert_eq!(todo_state("- plain item"), None);

    assert_eq!(toggle_todo("- [ ] ship it"), "- [x] ship it");
    assert_eq!(toggle_todo("  - [x] ship it"), "  - [ ] ship it");
    assert_eq!(toggle_todo("- plain item"), "- [ ] plain item");
    assert_eq!(toggle_todo("an idea"), "- [ ] an idea");
    assert_eq!(toggle_todo(""), "- [ ] ");

    let text = "Plan\n- [ ] one\n- [x] two\n  - [ ] three\n";
    assert_eq!(todo_counts(text), (2, 3));
    assert_eq!(add_todo(""), "- [ ] ");
    assert_eq!(add_todo("Plan"), "Plan\n- [ ] ");
    assert_eq!(add_todo("Plan\n"), "Plan\n- [ ] ");
}

#[test]
fn notes_are_kept_per_server_and_folder() {
    let scratch = tempfile::tempdir().expect("scratch folder");
    let store = Store::new(scratch.path().join("harlo.db"));
    assert_eq!(
        store.load_note("Local", "/a").expect("load"),
        "",
        "nothing yet"
    );

    store.save_note("Local", "/a", "alpha").expect("save");
    store
        .save_note("example:4096", "/a", "remote")
        .expect("save");
    store
        .save_note("Local", "/a", "alpha 2")
        .expect("save again");
    assert_eq!(store.load_note("Local", "/a").expect("load"), "alpha 2");
    assert_eq!(
        store.load_note("example:4096", "/a").expect("load"),
        "remote"
    );
    assert_eq!(store.load_note("Local", "/b").expect("load"), "");

    // An emptied note is removed, and the layout is untouched by notes.
    store.save_note("Local", "/a", "").expect("save empty");
    assert_eq!(store.load_note("Local", "/a").expect("load"), "");
    assert_eq!(
        store.load().expect("layout"),
        harlo_core::layout::Layout::default()
    );
}
