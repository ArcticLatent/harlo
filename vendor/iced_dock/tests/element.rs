//! Element panes hold a single panel without a tab strip. They dock against the edges of other
//! panes but never take part in tab groups.

use iced_dock::model::{DockOperation, HandleSide, NodeId, NodeKind};
use iced_dock::unstable::{DockManager, DragSession};
use iced_dock::{element, horizontal, panel, tabs, DockSession, Error, LayoutTree, PaneTarget};

fn layout() -> LayoutTree<u32> {
    horizontal([
        tabs([panel("a", "A", 0u32), panel("b", "B", 1u32)]),
        element(panel("minimap", "Minimap", 2u32))
            .handle(HandleSide::Left)
            .named("minimap"),
        tabs([panel("c", "C", 3u32)]),
    ])
}

fn session() -> DockSession<u32> {
    DockSession::from_tree(layout()).unwrap()
}

fn handle_of(session: &DockSession<u32>, pane: NodeId) -> Option<HandleSide> {
    match session.state().borrow().layout.kind(pane) {
        Some(NodeKind::Pane(p)) => p.handle,
        _ => panic!("not a pane"),
    }
}

fn tab_count(session: &DockSession<u32>, pane: NodeId) -> usize {
    match session.state().borrow().layout.kind(pane) {
        Some(NodeKind::Pane(p)) => p.tabs.len(),
        _ => panic!("not a pane"),
    }
}

fn drop_on(
    session: &DockSession<u32>,
    panel_id: &str,
    target: NodeId,
    op: DockOperation,
) -> iced_dock::Result {
    let panel = session.panel_node(panel_id).unwrap();
    let source = session.pane_for_panel(panel_id).unwrap();
    let mut drag = DragSession::new(source, panel, 0.2);
    drag.hover_target = Some(target);
    drag.operation = Some(op);
    let state = session.state();
    let mut state = state.borrow_mut();
    DockManager.execute(&mut state.layout, drag)
}

#[test]
fn element_compiles_to_handle_pane_with_one_panel() {
    let session = session();
    let pane = session.pane_for_panel("minimap").unwrap();
    assert_eq!(handle_of(&session, pane), Some(HandleSide::Left));
    assert_eq!(tab_count(&session, pane), 1);
}

#[test]
fn tab_cannot_join_element() {
    let session = session();
    let element = session.pane_for_panel("minimap").unwrap();
    assert_eq!(
        drop_on(&session, "a", element, DockOperation::Fill),
        Err(Error::ValidationFailed)
    );
    assert_eq!(tab_count(&session, element), 1);
    assert!(session.pane_for_panel("a").is_some());
}

#[test]
fn element_cannot_join_tab_group() {
    let session = session();
    let tabs_pane = session.pane_for_panel("c").unwrap();
    assert_eq!(
        drop_on(&session, "minimap", tabs_pane, DockOperation::Fill),
        Err(Error::ValidationFailed)
    );
    assert_eq!(tab_count(&session, tabs_pane), 1);
}

#[test]
fn element_cannot_be_tab_inserted() {
    let session = session();
    let tabs_pane = session.pane_for_panel("a").unwrap();
    let element = session.pane_for_panel("minimap").unwrap();
    let panel = session.panel_node("minimap").unwrap();
    let drag = DragSession::new(element, panel, 0.2);
    let state = session.state();
    let mut state = state.borrow_mut();
    assert!(DockManager
        .execute_tab_insert(&mut state.layout, drag, tabs_pane, 0)
        .is_err());
}

#[test]
fn element_edge_dock_keeps_handle() {
    let session = session();
    let target = session.pane_for_panel("a").unwrap();
    drop_on(&session, "minimap", target, DockOperation::Bottom).unwrap();
    session.state().borrow_mut().sync_index();

    let pane = session.pane_for_panel("minimap").unwrap();
    assert_eq!(handle_of(&session, pane), Some(HandleSide::Left));
    assert_eq!(tab_count(&session, pane), 1);
}

#[test]
fn tab_edge_dock_beside_element_gets_plain_pane() {
    let session = session();
    let element = session.pane_for_panel("minimap").unwrap();
    drop_on(&session, "b", element, DockOperation::Right).unwrap();
    session.state().borrow_mut().sync_index();

    let pane = session.pane_for_panel("b").unwrap();
    assert_ne!(pane, element);
    assert_eq!(handle_of(&session, pane), None);
    assert_eq!(tab_count(&session, element), 1);
}

#[test]
fn element_edge_drop_on_itself_is_rejected() {
    let session = session();
    let element = session.pane_for_panel("minimap").unwrap();
    assert_eq!(
        drop_on(&session, "minimap", element, DockOperation::Left),
        Err(Error::ValidationFailed)
    );
}

#[test]
fn closed_element_revives_as_element_and_stays_single() {
    let session = session();
    session.close_panel("minimap").unwrap();
    assert!(session.pane_for_panel("minimap").is_none());

    session
        .open_panel(
            PaneTarget::Named("minimap".into()),
            panel("minimap", "Minimap", 2u32),
        )
        .unwrap();
    let pane = session.pane_for_panel("minimap").unwrap();
    assert_eq!(handle_of(&session, pane), Some(HandleSide::Left));

    assert_eq!(
        session.open_panel(
            PaneTarget::Named("minimap".into()),
            panel("other", "Other", 9u32),
        ),
        Err(Error::ElementOccupied { pane })
    );
    assert!(session.panel_node("other").is_none());
}
