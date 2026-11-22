use iced::{Point, Rectangle};
use iced_dock::model::{Axis, DockOperation, Layout, NodeId, NodeKind};
use iced_dock::unstable::{DockManager, DragSession, Factory};

fn single_pane(layout: &mut Layout<u32>, id: &str, key: u32) -> (NodeId, NodeId) {
    let factory = Factory;
    let panel = factory.insert_panel(layout, id, id, key);
    let pane = factory.create_pane(layout);
    factory.add_panel_to_pane(layout, pane, panel).unwrap();
    (pane, panel)
}

fn children(layout: &Layout<u32>, node: NodeId) -> (Axis, Vec<NodeId>) {
    match layout.kind(node) {
        Some(NodeKind::Proportional(pg)) => (pg.axis, pg.children.clone()),
        _ => panic!("expected split"),
    }
}

struct TopOverRow {
    layout: Layout<u32>,
    top: (NodeId, NodeId),
    a: (NodeId, NodeId),
    b: (NodeId, NodeId),
    row: NodeId,
    column: NodeId,
}

/// `Top` above a row of `A | B`.
fn top_over_row() -> TopOverRow {
    let factory = Factory;
    let mut layout = Layout::<u32>::new();
    let top = single_pane(&mut layout, "top", 0);
    let a = single_pane(&mut layout, "a", 1);
    let b = single_pane(&mut layout, "b", 2);
    let row = factory.create_proportional(&mut layout, Axis::Horizontal, vec![a.0, b.0]);
    let column = factory.create_proportional(&mut layout, Axis::Vertical, vec![top.0, row]);
    layout.set_root_child(Some(column));
    TopOverRow {
        layout,
        top,
        a,
        b,
        row,
        column,
    }
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
    Rectangle {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn edge_ancestors_only_count_splits_across_the_edge() {
    let t = top_over_row();
    let a = t.a.0;
    assert_eq!(
        DockManager::edge_ancestors(&t.layout, a, DockOperation::Bottom),
        vec![t.row]
    );
    assert_eq!(
        DockManager::edge_ancestors(&t.layout, a, DockOperation::Top),
        vec![t.row]
    );
    assert_eq!(
        DockManager::edge_ancestors(&t.layout, a, DockOperation::Left),
        vec![t.column]
    );
    // `A` is not at the row's right edge, so nothing encloses it there.
    assert!(DockManager::edge_ancestors(&t.layout, a, DockOperation::Right).is_empty());
    assert!(DockManager::edge_ancestors(&t.layout, t.top.0, DockOperation::Bottom).is_empty());
}

#[test]
fn outer_band_lifts_hover_to_enclosing_split() {
    let t = top_over_row();
    let targets = [
        (t.top.0, rect(0.0, 0.0, 200.0, 100.0)),
        (t.a.0, rect(0.0, 100.0, 100.0, 100.0)),
        (t.b.0, rect(100.0, 100.0, 100.0, 100.0)),
    ];
    let hover = |point: Point| {
        let mut session = DragSession::new(t.top.0, t.top.1, 0.2);
        DockManager::update_drag_hover(&mut session, point, &targets);
        DockManager::lift_drag_hover(&mut session, &t.layout, point, &targets, 8.0);
        (session.hover_target, session.operation)
    };

    assert_eq!(
        hover(Point::new(50.0, 198.0)),
        (Some(t.row), Some(DockOperation::Bottom))
    );
    assert_eq!(
        hover(Point::new(50.0, 185.0)),
        (Some(t.a.0), Some(DockOperation::Bottom))
    );
    assert_eq!(
        hover(Point::new(50.0, 150.0)),
        (Some(t.a.0), Some(DockOperation::Fill))
    );
}

#[test]
fn zero_band_keeps_pane_hover() {
    let t = top_over_row();
    let targets = [(t.a.0, rect(0.0, 100.0, 100.0, 100.0))];
    let point = Point::new(50.0, 199.0);
    let mut session = DragSession::new(t.top.0, t.top.1, 0.2);
    DockManager::update_drag_hover(&mut session, point, &targets);
    DockManager::lift_drag_hover(&mut session, &t.layout, point, &targets, 0.0);
    assert_eq!(session.hover_target, Some(t.a.0));
}

#[test]
fn node_bounds_unions_panes_beneath() {
    let t = top_over_row();
    let targets = [
        (t.top.0, rect(0.0, 0.0, 200.0, 100.0)),
        (t.a.0, rect(0.0, 100.0, 100.0, 100.0)),
        (t.b.0, rect(100.0, 100.0, 100.0, 100.0)),
    ];
    assert_eq!(
        DockManager::node_bounds(&t.layout, t.row, &targets),
        Some(rect(0.0, 100.0, 200.0, 100.0))
    );
}

#[test]
fn drop_below_split_spans_the_row() {
    let mut t = top_over_row();
    let session = DragSession {
        hover_target: Some(t.row),
        operation: Some(DockOperation::Bottom),
        ..DragSession::new(t.top.0, t.top.1, 0.2)
    };
    DockManager.execute(&mut t.layout, session).unwrap();

    let root = t.layout.root_child().unwrap();
    assert_eq!(
        children(&t.layout, root),
        (Axis::Vertical, vec![t.row, t.top.0])
    );
    assert_eq!(
        children(&t.layout, t.row),
        (Axis::Horizontal, vec![t.a.0, t.b.0])
    );
    assert!(t.layout.get(t.column).is_none());
}

#[test]
fn drop_beside_own_collapsing_split_lands_on_survivor() {
    let mut t = top_over_row();
    // Drag `A` below the row it sits in: the row collapses into `B`.
    let session = DragSession {
        hover_target: Some(t.row),
        operation: Some(DockOperation::Bottom),
        ..DragSession::new(t.a.0, t.a.1, 0.2)
    };
    DockManager.execute(&mut t.layout, session).unwrap();

    assert_eq!(
        children(&t.layout, t.column),
        (Axis::Vertical, vec![t.top.0, t.b.0, t.a.0])
    );
    assert!(t.layout.get(t.row).is_none());
}

#[test]
fn multi_tab_source_peels_and_spans_the_row() {
    let factory = Factory;
    let mut t = top_over_row();
    let extra = factory.insert_panel(&mut t.layout, "extra", "extra", 9);
    factory
        .add_panel_to_pane(&mut t.layout, t.b.0, extra)
        .unwrap();

    let session = DragSession {
        hover_target: Some(t.row),
        operation: Some(DockOperation::Top),
        ..DragSession::new(t.b.0, extra, 0.2)
    };
    DockManager.execute(&mut t.layout, session).unwrap();

    let (axis, column) = children(&t.layout, t.column);
    assert_eq!(axis, Axis::Vertical);
    assert_eq!(column.len(), 3);
    assert_eq!((column[0], column[2]), (t.top.0, t.row));
    assert_eq!(t.layout.get(extra).and_then(|e| e.owner), Some(column[1]));
}
