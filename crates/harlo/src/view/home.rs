//! The main area before a session is open: "What are we working on?" and the composer.

// Rust guideline compliant 2026-10-09

use std::fmt;

use harlo_core::theme::Tokens;
use iced::widget::{button, column, container, overlay::menu, pick_list, row, space, text};
use iced::{Background, Border, Center, Element, Fill, Padding, Shadow, Theme};
use lucide_icons::Icon;

use crate::app::{App, Message};
use crate::style::{self, size};
use crate::view::{composer, icon};

/// The composer's widest size; it narrows with the window.
const COMPOSER_WIDTH: f32 = 760.0;

/// A folder a new session can start in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    /// The folder, or `None` for the home folder.
    pub directory: Option<String>,
    label: String,
}

impl fmt::Display for Folder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

/// The folders the picker offers: home, then each project and its worktrees.
fn folders(app: &App) -> Vec<Folder> {
    let mut list = vec![Folder {
        directory: None,
        label: "Home folder".to_owned(),
    }];
    for project in &app.projects {
        list.push(Folder {
            directory: Some(project.directory.clone()),
            label: project.name.clone(),
        });
        for worktree in &project.worktrees {
            list.push(Folder {
                directory: Some(worktree.directory.clone()),
                label: format!("{} / {}", project.name, worktree.name),
            });
        }
    }
    list
}

/// The empty state, centred in the main area.
pub fn view(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    let question = text("What are we working on?")
        .size(size::HEADLINE)
        .color(style::color(t.text_strong));
    let pickers = row![
        folder_picker(app),
        picker(t, Icon::Sparkles, "Default model")
    ]
    .spacing(5);
    let body = column![
        question,
        space().height(22),
        pickers,
        space().height(14),
        composer::view(
            t,
            &app.composer,
            "Ask, or describe what to build.",
            "Enter to start · Shift+Enter for a new line",
            app.send_error.as_deref(),
        )
    ]
    .align_x(Center)
    .max_width(COMPOSER_WIDTH);
    container(body)
        .width(Fill)
        .height(Fill)
        // Optically centred: a little above the middle, as in the design.
        .padding(Padding {
            top: 0.0,
            bottom: 82.0,
            left: 24.0,
            right: 24.0,
        })
        .center(Fill)
        .style(style::fill(t.background_base))
        .into()
}

/// The folder a new session starts in.
fn folder_picker(app: &App) -> Element<'_, Message> {
    let t = app.tokens;
    let options = folders(app);
    let selected = options
        .iter()
        .find(|f| f.directory == app.new_folder)
        .cloned();
    let list = pick_list(options, selected, |folder: Folder| {
        Message::PickFolder(folder.directory)
    })
    .text_size(size::LABEL)
    .padding(Padding {
        top: 6.0,
        bottom: 6.0,
        left: 0.0,
        right: 4.0,
    })
    .handle(pick_list::Handle::Static(pick_list::Icon {
        font: iced::Font::with_name("lucide"),
        code_point: Icon::ChevronDown.unicode(),
        size: Some(13.0.into()),
        line_height: iced::widget::text::LineHeight::Relative(1.0),
        shaping: iced::widget::text::Shaping::Basic,
    }))
    .style(move |_: &Theme, _| pick_list::Style {
        text_color: style::color(t.text_strong),
        placeholder_color: style::color(t.text_weak),
        handle_color: style::color(t.icon_base),
        background: Background::Color(iced::Color::TRANSPARENT),
        border: Border::default(),
    })
    .menu_style(move |_: &Theme| menu::Style {
        background: style::color(t.surface_raised_strong).into(),
        border: Border {
            radius: 8.0.into(),
            width: 1.0,
            color: style::color(t.border_weak_base),
        },
        text_color: style::color(t.text_strong),
        selected_text_color: style::color(t.text_strong),
        selected_background: style::color(t.surface_base_active.over(t.surface_raised_strong, 1.0))
            .into(),
        shadow: Shadow::default(),
    });
    let glyph = if app.new_folder.is_some() {
        Icon::Folder
    } else {
        Icon::House
    };
    container(
        row![icon(glyph, 14.0, t.text_interactive_base), list]
            .spacing(8)
            .align_y(Center),
    )
    .padding(Padding {
        top: 0.0,
        bottom: 0.0,
        left: 12.0,
        right: 6.0,
    })
    .id(crate::ids::FOLDER)
    .style(style::boxed(t.surface_raised_strong, None, size::RADIUS))
    .into()
}

fn picker<'a>(t: &Tokens, glyph: Icon, label: &'a str) -> Element<'a, Message> {
    let content = row![
        icon(glyph, 14.0, t.text_interactive_base),
        text(label)
            .size(size::LABEL)
            .color(style::color(t.text_strong)),
        icon(Icon::ChevronDown, 13.0, t.icon_base),
    ]
    .spacing(8)
    .align_y(Center);
    button(content)
        .padding([6, 12])
        .style(style::chip(t))
        .into()
}
