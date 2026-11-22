//! A named pane collapses when its last tab closes; opening a panel targeted at that name must
//! bring the pane back where it was instead of stacking it into an unrelated pane.

use iced_dock::model::{DockOperation, Layout, NodeId, NodeKind};
use iced_dock::unstable::DockManager;
use iced_dock::{horizontal, panel, tabs, vertical, DockSession, LayoutTree, PaneTarget};

fn editor_layout() -> LayoutTree<u32> {
    horizontal([
        vertical([
            tabs([panel("hierarchy_bar", "Hierarchy", 0u32)])
                .named("hierarchy")
                .group("tools"),
            tabs([
                panel("history_bar", "History", 1u32),
                panel("bookmarks_bar", "Bookmarks", 2u32),
            ])
            .named("history")
            .group("tools"),
        ])
        .weights([0.55, 0.45]),
        tabs([panel("welcome", "Welcome", 3u32)])
            .named("documents")
            .group("files")
            .persistent(true),
        tabs([panel("search_bar", "Search", 4u32)])
            .named("right_tools")
            .group("tools"),
    ])
    .weights([0.22, 0.56, 0.22])
}

/// Named panes left to right / top to bottom.
fn pane_order(session: &DockSession<u32>) -> Vec<String> {
    fn walk(layout: &Layout<u32>, node: NodeId, out: &mut Vec<String>) {
        match layout.kind(node) {
            Some(NodeKind::Pane(pane)) => out.extend(pane.name.clone()),
            Some(NodeKind::Proportional(pg)) => {
                for &child in &pg.children {
                    walk(layout, child, out);
                }
            }
            _ => {}
        }
    }

    let state = session.state();
    let state = state.borrow();
    let mut out = Vec::new();
    let root = state.layout.root_child().expect("root child");
    walk(&state.layout, root, &mut out);
    out
}

#[test]
fn collapsed_pane_revives_beside_its_old_neighbor() {
    let session = DockSession::from_tree(editor_layout()).expect("valid layout");
    let documents = session.pane_for_panel("welcome").expect("documents pane");

    session.close_panel("search_bar").expect("close search");
    session.close_panel("history_bar").expect("close history");
    session
        .close_panel("bookmarks_bar")
        .expect("close bookmarks");

    session
        .open_panel(
            PaneTarget::Named("history".into()),
            panel("history_bar", "History", 1u32),
        )
        .expect("history reopens");
    let history = session.pane_for_panel("history_bar").expect("history pane");
    assert_ne!(history, documents, "must not land in the documents pane");

    // Left of the documents pane, where it was, and still named so its siblings rejoin it.
    assert_eq!(pane_order(&session), ["hierarchy", "history", "documents"]);
    session
        .open_panel(
            PaneTarget::Named("history".into()),
            panel("bookmarks_bar", "Bookmarks", 2u32),
        )
        .expect("bookmarks reopens");
    assert_eq!(session.pane_for_panel("bookmarks_bar"), Some(history));

    session
        .open_panel(
            PaneTarget::Named("right_tools".into()),
            panel("search_bar", "Search", 4u32),
        )
        .expect("search reopens");
    assert_eq!(
        pane_order(&session),
        ["hierarchy", "history", "documents", "right_tools"]
    );
}

#[test]
fn revival_survives_losing_the_recorded_neighbor() {
    let session = DockSession::from_tree(editor_layout()).expect("valid layout");
    let documents = session.pane_for_panel("welcome").expect("documents pane");

    // Hierarchy's neighbor is the history pane, which is closed next: the grave must follow.
    session
        .close_panel("hierarchy_bar")
        .expect("close hierarchy");
    session.close_panel("history_bar").expect("close history");
    session
        .close_panel("bookmarks_bar")
        .expect("close bookmarks");

    session
        .open_panel(
            PaneTarget::Named("hierarchy".into()),
            panel("hierarchy_bar", "Hierarchy", 0u32),
        )
        .expect("hierarchy reopens");
    assert_ne!(
        session.pane_for_panel("hierarchy_bar"),
        Some(documents),
        "must not land in the documents pane"
    );
    assert_eq!(
        pane_order(&session),
        ["hierarchy", "documents", "right_tools"]
    );
}

#[test]
fn unknown_pane_name_still_fails() {
    let session = DockSession::from_tree(editor_layout()).expect("valid layout");
    assert!(session
        .open_panel(
            PaneTarget::Named("nowhere".into()),
            panel("stray", "Stray", 9u32),
        )
        .is_err());
}

/// A reopened panel must be as undraggable into a foreign group as it was at startup.
fn droppable_into_documents(session: &DockSession<u32>, panel_id: &str) -> bool {
    let documents = session.pane_for_panel("welcome").expect("documents pane");
    let source_pane = session.pane_for_panel(panel_id).expect("source pane");
    let source_panel = session.panel_node(panel_id).expect("source panel");
    let state = session.state();
    let state = state.borrow();
    DockManager.validate(
        &state.layout,
        source_pane,
        source_panel,
        documents,
        DockOperation::Fill,
    )
}

#[test]
fn reopened_panel_inherits_its_panes_group() {
    let session = DockSession::from_tree(editor_layout()).expect("valid layout");
    assert!(!droppable_into_documents(&session, "history_bar"));

    // Into a pane that is still alive.
    session.close_panel("history_bar").expect("close history");
    session
        .open_panel(
            PaneTarget::Named("history".into()),
            panel("history_bar", "History", 1u32),
        )
        .expect("history reopens");
    assert!(!droppable_into_documents(&session, "history_bar"));

    // Into a revived pane.
    session.close_panel("history_bar").expect("close history");
    session
        .close_panel("bookmarks_bar")
        .expect("close bookmarks");
    session
        .open_panel(
            PaneTarget::Named("history".into()),
            panel("history_bar", "History", 1u32),
        )
        .expect("history reopens");
    assert!(!droppable_into_documents(&session, "history_bar"));
}

#[test]
fn revival_survives_losing_the_recorded_group() {
    let layout = horizontal([
        tabs([panel("left_bar", "Left", 0u32)])
            .named("left")
            .group("tools"),
        vertical([
            tabs([panel("top_bar", "Top", 1u32)]).named("top"),
            tabs([panel("bottom_bar", "Bottom", 2u32)]).named("bottom"),
        ]),
    ]);
    let session = DockSession::from_tree(layout).expect("valid layout");

    // The left pane is anchored to the right column, which then collapses into one pane.
    session.close_panel("left_bar").expect("close left");
    session.close_panel("top_bar").expect("close top");

    session
        .open_panel(
            PaneTarget::Named("left".into()),
            panel("left_bar", "Left", 0u32),
        )
        .expect("left reopens");
    assert_eq!(pane_order(&session), ["left", "bottom"]);
}
