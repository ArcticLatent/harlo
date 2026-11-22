# iced_dock

A docking layout widget for [iced](https://github.com/iced-rs/iced) 0.14. Resizable splits, tabbed panes, drag-and-drop
docking, focus tracking, keyboard navigation, tab groups, tabs overflow handling, tab-less elements with a drag handle.

![Dock_Demo](assets/demo.gif)

## Example

```toml
[dependencies]
iced_dock = { git = "https://github.com/Fee0/iced_dock.git" }
iced = { version = "0.14", features = ["wgpu", "svg"] }
```

```rust
use iced::widget::{container, text};
use iced::{application, Element, Length, Task};
use iced_dock::{dock, horizontal, panel as tab, tabs, DockEvent, DockSession, LayoutTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Panel {
    Explorer,
    Editor,
    Terminal,
}

fn layout() -> LayoutTree<Panel> {
    horizontal([
        tabs([tab("explorer", "Explorer", Panel::Explorer)]),
        tabs([tab("editor", "main.rs", Panel::Editor)]),
        tabs([tab("terminal", "Terminal", Panel::Terminal)]),
    ])
    .weights([0.2, 0.6, 0.2])
}

fn main() -> iced::Result {
    application(App::new, update, view).run()
}

struct App {
    dock: DockSession<Panel>,
}

impl App {
    fn new() -> Self {
        Self {
            dock: DockSession::from_tree(layout()).unwrap(),
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Dock(DockEvent<Panel>),
}

fn update(_app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Dock(_event) => {
            // react to events
        }
    }
    Task::none()
}

fn view(app: &App) -> Element<'_, Message> {
    container(
        dock()
            .state(app.dock.state())
            .on_event(Message::Dock)
            .content(panel)
            .build(),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn panel(key: Panel) -> Element<'static, Message> {
    let label = match key {
        Panel::Explorer => "Explorer",
        Panel::Editor => "Editor",
        Panel::Terminal => "Terminal",
    };
    container(text(label))
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .into()
}
```

## Elements

Besides tabbed panes, a layout can contain *elements*: a single panel without a tab strip that is
moved by dragging a small handle. The handle side is configurable.

```rust
use iced_dock::{element, horizontal, panel, tabs, HandleSide};

horizontal([
    tabs([panel("editor", "main.rs", Panel::Editor)]),
    element(panel("minimap", "Minimap", Panel::Minimap)).handle(HandleSide::Left),
])
```

- `HandleSide` is `Left`, `Right`, `Top` (default) or `Bottom`; set the strip thickness with
  `dock().element_handle_size(..)`.
- An element docks against the edges of any pane and keeps its handle when moved.
- It is not a tab group: tabs cannot be dropped into it, and it cannot be dropped into a tab group.
- `.named(..)` works as for tab groups, so a closed element reopens in place via
  `DockSession::open_panel(PaneTarget::Named(..), ..)`.

See `examples/ide.rs` for a full layout with two elements.
