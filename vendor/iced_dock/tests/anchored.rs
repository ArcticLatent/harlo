//! Harlo: an anchored pane keeps its tabs, drops are refused where nothing may
//! land, and an edge split gives the new pane half of the target's share.

// Rust guideline compliant 2026-10-09

use iced_dock::model::{Axis, DockOperation, Layout, NodeId, NodeKind};
use iced_dock::unstable::{
    build_tree, owning_pane, BuiltLayout, DockManager, DragSession, Factory,
};
use iced_dock::{horizontal, panel, tabs, DockSession, PaneTarget};

/// An anchored center with two documents, and a tool pane beside it.
fn center_and_tools() -> BuiltLayout<u32> {
    build_tree(
        &horizontal([
            tabs([panel("a", "A", 0u32), panel("b", "B", 1u32)]).anchored(),
            tabs([panel("t1", "T1", 2u32), panel("t2", "T2", 3u32)]),
        ])
        .weights([0.7, 0.3]),
    )
    .expect("a valid layout")
}

/// The panel named `id` and the pane holding it.
fn find(built: &BuiltLayout<u32>, id: &str) -> (NodeId, NodeId) {
    let panel = built.index.panel_node(id).expect("the panel");
    let pane = owning_pane(&built.layout, panel).expect("its pane");
    (panel, pane)
}

/// A drag of `id` hovering `insert` in a tab strip and `edge` over a pane.
fn hover(
    built: &BuiltLayout<u32>,
    id: &str,
    insert: Option<(NodeId, usize)>,
    edge: Option<(NodeId, DockOperation)>,
    content_fill: bool,
) -> DragSession {
    let (panel, pane) = find(built, id);
    let mut session = DragSession::new(pane, panel, 0.34);
    session.tab_insert = insert;
    session.hover_target = edge.map(|(target, _)| target);
    session.operation = edge.map(|(_, op)| op);
    DockManager.keep_valid_hover(&mut session, &built.layout, content_fill);
    session
}

#[test]
fn an_anchored_tab_only_reorders_in_its_own_strip() {
    let built = center_and_tools();
    let (_, center) = find(&built, "a");
    let (_, tools) = find(&built, "t1");

    let reorder = hover(&built, "a", Some((center, 1)), None, false);
    assert_eq!(reorder.tab_insert, Some((center, 1)));

    let join = hover(&built, "a", Some((tools, 0)), None, false);
    assert_eq!(join.tab_insert, None, "no joining the tools");

    let split = hover(
        &built,
        "a",
        None,
        Some((tools, DockOperation::Bottom)),
        false,
    );
    assert_eq!(split.operation, None, "no splitting off");
}

#[test]
fn nothing_joins_or_splits_beside_an_anchored_pane() {
    let built = center_and_tools();
    let (_, center) = find(&built, "a");

    let join = hover(&built, "t1", Some((center, 0)), None, false);
    assert_eq!(join.tab_insert, None);

    let split = hover(
        &built,
        "t1",
        None,
        Some((center, DockOperation::Right)),
        true,
    );
    assert_eq!(split.hover_target, None);
    assert_eq!(split.operation, None);
}

#[test]
fn a_tool_splits_its_own_pane_and_joins_only_through_the_strip() {
    let built = center_and_tools();
    let (_, tools) = find(&built, "t1");

    let split = hover(
        &built,
        "t2",
        None,
        Some((tools, DockOperation::Bottom)),
        false,
    );
    assert_eq!(split.operation, Some(DockOperation::Bottom));

    let middle = hover(
        &built,
        "t2",
        None,
        Some((tools, DockOperation::Fill)),
        false,
    );
    assert_eq!(middle.operation, None, "the middle takes nothing");
}

#[test]
fn an_edge_split_takes_half_of_the_targets_share() {
    let factory = Factory;
    let mut layout = Layout::<u32>::new();
    let panes: Vec<NodeId> = (0..2).map(|_| factory.create_pane(&mut layout)).collect();
    let group = factory.create_proportional(&mut layout, Axis::Horizontal, panes.clone());
    factory
        .set_proportions(&mut layout, group, vec![0.7, 0.3])
        .expect("two shares");
    layout.set_root_child(Some(group));

    let source = factory.insert_panel(&mut layout, "new", "New", 9u32);
    let pane = factory.create_pane(&mut layout);
    factory
        .add_panel_to_pane(&mut layout, pane, source)
        .expect("a pane for it");
    factory
        .split(&mut layout, pane, panes[1], DockOperation::Right)
        .expect("the split");

    let Some(NodeKind::Proportional(pg)) = layout.kind(group) else {
        panic!("the group stays proportional");
    };
    let shares: Vec<f32> = pg.proportions.iter().map(|p| (p * 100.0).round()).collect();
    assert_eq!(shares, [70.0, 15.0, 15.0]);
}

#[test]
fn a_named_pane_opened_beside_another_reopens_at_the_same_share() {
    let tree = horizontal([
        tabs([panel("a", "A", 0u32)]).named("center").anchored(),
        tabs([panel("t1", "T1", 2u32)]).named("right"),
    ])
    .weights([0.74, 0.26]);
    let session = DockSession::from_tree(tree).expect("a valid layout");
    let bottom_share = |s: &DockSession<u32>| {
        let state = s.state();
        let state = state.borrow();
        let pane = state
            .index
            .panes
            .get("bottom")
            .copied()
            .expect("the bottom pane");
        let group = state
            .layout
            .get(pane)
            .and_then(|e| e.owner)
            .expect("its group");
        let Some(NodeKind::Proportional(pg)) = state.layout.kind(group) else {
            panic!("a split");
        };
        let total: f32 = pg.proportions.iter().sum();
        let at = pg.children.iter().position(|&c| c == pane).expect("in it");
        (pg.proportions[at] / total * 100.0).round()
    };
    session
        .open_panel_split(
            "bottom",
            "center",
            DockOperation::Bottom,
            0.32,
            panel("x", "X", 5u32),
        )
        .expect("the split");
    assert_eq!(bottom_share(&session), 32.0);

    // Its last tab closes, and a tab opened into it by name brings it back.
    session.close_panel("x").expect("closing it");
    session
        .open_panel(PaneTarget::Named("bottom".into()), panel("x", "X", 5u32))
        .expect("reopening it");
    assert_eq!(bottom_share(&session), 32.0);
}
