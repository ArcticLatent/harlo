# iced_dock, vendored

Copied from https://github.com/Fee0/iced_dock at commit
`495cbea7af91aea668decb27c52cd53561cf78e3` (2026-10-07): `src/`, `tests/`,
`README.md` and the two icons in `assets/`. It isn't published on crates.io.
Its manifest declares the MIT License; the repository has no licence file, so
`LICENSE` holds the standard MIT text.

## Changes

In the code each carries a `Harlo:` comment, except the added where clauses.

- `Cargo.toml`: rewritten as a workspace member on the workspace's `iced`,
  with the upstream lint list kept. Two restriction lints that clippy added
  after the vendored commit (`inline_trait_bounds`, `inline_modules`) are
  allowed.
- `tests/focus.rs`: one `assert!(a != b)` became `assert_ne!`, for a newer
  clippy lint, and its hand-built widget state sets `content_fill`.
- `tests/anchored.rs` is new: it covers the anchored pane, the drop rules and
  the half split below.
- Tabs: `DockBuilder::tab_title` gives tab titles at view time, and
  `DockBuilder::tab_icon` draws a glyph (`TabIcon`) before each title. The
  tab strip's where clauses gain `Renderer::Font: From<iced::Font>` for it.
- A middle click on a closable tab closes it.
- `PaneTarget::Beside(id)` opens a panel right after the panel `id`, in its pane.
- `DockSession::set_tab_bar` and `DockSession::tab_bar` hide or show the tab
  bar of a pane holding one panel, turning it into an element pane with a
  drag handle and back.
- A click in a pane's content publishes `PaneFocused` only when it moves the
  focus.
- Anchored panes: `TabsNode::anchored` and `LayoutTree::anchored` (and
  `Pane::anchored`, kept in saved layouts) mark a pane whose tabs only reorder
  within its own strip. Nothing joins it, splits beside it or leaves it, and
  `DockSession::split_active_panel` refuses it.
- Drop rules: `DockManager::keep_valid_hover` clears a drag's hover wherever
  the drop isn't allowed, so no hint shows and a release there does nothing.
  `DockBuilder::content_fill(false)` stops the middle of a pane from taking a
  tab: tabs join through the strip, and the edges split. The edge under the
  pointer is the nearest one within the edge band.
- Split sizes and hints: an edge drop gives the new pane half of the target's
  share, and the drop hint previews exactly that half, or the whole pane when
  joining it through its strip. The hint follows the drag's validated hover in
  both tab and element panes.
- `DockBuilder::pane_actions` puts an element at the right end of each pane's
  tab bar, and `DockBuilder::tab_closable` decides per tab whether it shows a
  close button.
- `DockSession::open_panel_split` opens a panel in a new named pane beside
  another, with given shares; `pane_tab_count`, `pane_name` and `panels_in`
  describe where panels are.
- Shares stay fractions: `open_panel_split` normalises its group's
  proportions, and a closed named pane remembers its share of the whole, so
  reopening it gives back the size it had rather than double that.
