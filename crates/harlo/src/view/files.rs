//! The Files panel's body: the current folder as a tree. Folders open in
//! place; a file opens in a tab.

// Rust guideline compliant 2026-10-09

use harlo_core::files::{Listing, Row};
use harlo_core::theme::Tokens;
use iced::widget::{Column, button, column, container, row, scrollable, text};
use iced::{Center, Element, Fill, padding};
use lucide_icons::Icon;

use crate::app::{App, FilesEvent, Message};
use crate::style::{self, size};
use crate::view::{file_icon, icon};
use crate::widget::ellipsis;

/// How far each level of folders indents its entries.
const INDENT: f32 = 14.0;

/// A row's height.
const ROW: f32 = 26.0;

/// The space between the list and its scrollbar.
const SCROLLBAR_GAP: f32 = 4.0;

/// The panel's body below its header.
pub fn body(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    let (folder, tree) = app.file_tree();
    let name = folder
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or("/")
        .to_owned();
    let heading = row![
        icon(Icon::Folder, 14.0, t.icon_base),
        ellipsis(name)
            .font(style::SANS_MEDIUM)
            .size(size::LABEL)
            .color(style::color(t.text_strong)),
    ]
    .spacing(8)
    .align_y(Center)
    .padding([12, 18]);
    let list: Element<'_, Message> = match tree.and_then(|tree| tree.root()) {
        None | Some(Listing::Loading) => note(t, "Listing the folder…".to_owned()),
        Some(Listing::Failed(error)) => note(t, format!("Couldn't list it: {error}")),
        Some(Listing::Loaded(nodes)) if nodes.is_empty() => {
            note(t, "This folder is empty.".to_owned())
        }
        Some(Listing::Loaded(_)) => {
            let shown = app.shown_file().map(|f| f.key.clone());
            let mut rows = Column::new().padding(padding::horizontal(8).bottom(12));
            for line in tree.map(harlo_core::files::Tree::rows).unwrap_or_default() {
                let key = std::path::Path::new(&folder)
                    .join(&line.node.path)
                    .to_string_lossy()
                    .into_owned();
                rows = rows.push(entry(t, line, shown.as_deref() == Some(key.as_str())));
            }
            scrollable(rows)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new().spacing(SCROLLBAR_GAP),
                ))
                .style(style::thin_scroll(t))
                .height(Fill)
                .into()
        }
    };
    column![heading, list].into()
}

/// One file or folder: indented by depth, folders with a chevron.
fn entry<'a>(t: &Tokens, line: Row, selected: bool) -> Element<'a, Message> {
    let node = line.node;
    let colour = if node.ignored {
        t.text_weaker
    } else {
        t.text_base
    };
    let chevron = node.directory.then_some(if line.open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    });
    #[expect(
        clippy::cast_precision_loss,
        reason = "folders don't nest anywhere near 2^24 deep"
    )]
    let indent = line.depth as f32 * INDENT;
    let mut content = row![
        container(match chevron {
            Some(chevron) => Element::from(icon(chevron, 12.0, t.icon_weak_base)),
            None => Element::from(iced::widget::space()),
        })
        .width(12)
    ]
    .spacing(6)
    .align_y(Center)
    .padding(padding::left(indent));
    content = content.push(if node.directory {
        icon(
            if line.open {
                Icon::FolderOpen
            } else {
                Icon::Folder
            },
            14.0,
            if node.ignored {
                t.text_weaker
            } else {
                t.icon_base
            },
        )
    } else {
        file_icon(&node.path, 14.0, node.ignored)
    });
    content = content.push(
        ellipsis(node.name.clone())
            .size(size::LABEL)
            .color(style::color(if selected { t.text_strong } else { colour })),
    );
    if line.loading {
        content = content.push(text("…").size(size::SMALL).color(style::color(t.text_weak)));
    }
    let event = if node.directory {
        FilesEvent::Toggle(node.path)
    } else {
        FilesEvent::Open(node.path)
    };
    button(content.height(Fill))
        .width(Fill)
        .height(ROW)
        .padding([0, 6])
        .on_press(Message::Files(event))
        .style(style::menu_row(t, selected))
        .into()
}

/// Plain text in the panel, for an empty state.
fn note<'a>(t: &Tokens, words: String) -> Element<'a, Message> {
    container(
        text(words)
            .size(size::LABEL)
            .color(style::color(t.text_base)),
    )
    .padding([8, 18])
    .into()
}
