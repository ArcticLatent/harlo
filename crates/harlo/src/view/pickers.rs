//! Chips that choose a session's agent, model and reasoning level, and a new session's folder.
//!
//! Each chip opens a menu under it (or over it, near the bottom of the
//! window): a card of rows with the current choice ticked. The model menu
//! has a filter at the top and names each model's provider, and the folder
//! menu ends with "Add project…". Up and Down move through a menu, Enter
//! picks, and Escape or a click elsewhere closes it.

// Rust guideline compliant 2026-10-09

use std::fmt;

use harlo_core::theme::Tokens;
use iced::widget::{Id, button, column, container, row, rule, scrollable, space, text, text_input};
use iced::{Center, Element, Fill, Length, Padding};
use lucide_icons::Icon;

use crate::app::{App, Message};
use crate::style::{self, size};
use crate::view::icon;
use crate::widget::{ellipsis, popover};

/// Rows a menu shows before it scrolls.
pub const MENU_ROWS: usize = 8;

/// The height of one menu row.
pub const MENU_ROW: f32 = 32.0;

/// The space between menu rows.
pub const ROW_GAP: f32 = 2.0;

/// The space inside a menu's card, around its rows.
const CARD_PADDING: f32 = 6.0;

/// The space between a scrolling menu's rows and its scrollbar.
const SCROLLBAR_GAP: f32 = 4.0;

/// The ID of the open menu's scrolling list.
pub const MENU_LIST: &str = "picker-menu-list";

/// The ID of the model menu's filter field.
pub const MENU_FILTER: &str = "picker-menu-filter";

/// Which picker's menu is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Picker {
    /// The folder a new session starts in.
    Folder,
    /// The agent.
    Agent,
    /// The reasoning level.
    Variant,
    /// The model.
    Model,
}

impl Picker {
    /// The width of this picker's menu.
    fn width(self) -> f32 {
        match self {
            Self::Agent | Self::Variant => 200.0,
            Self::Folder => 260.0,
            Self::Model => 320.0,
        }
    }

    /// The ID of this picker's chip.
    fn id(self) -> &'static str {
        match self {
            Self::Folder => crate::ids::FOLDER,
            Self::Agent => crate::ids::AGENT,
            Self::Variant => crate::ids::VARIANT,
            Self::Model => crate::ids::MODEL,
        }
    }
}

/// One row of a picker's menu.
#[derive(Debug, Clone)]
pub struct Item {
    /// The choice's name.
    pub label: String,
    /// A note on the right, such as a model's provider.
    pub detail: Option<String>,
    /// Whether it's the current choice.
    pub selected: bool,
    /// What choosing it does.
    pub message: Message,
}

/// A model the picker offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChoice {
    /// The provider's ID.
    pub provider_id: String,
    /// The model's ID.
    pub id: String,
    /// The name shown.
    pub name: String,
}

impl fmt::Display for ModelChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

/// An agent the picker offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentChoice {
    /// The agent's ID.
    pub id: String,
    /// The name shown.
    pub name: String,
}

impl fmt::Display for AgentChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

/// A reasoning level; `None` is the model's default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant(pub Option<String>);

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_deref().unwrap_or("default"))
    }
}

/// A folder a new session can start in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    /// The folder, or `None` for the home folder.
    pub directory: Option<String>,
    /// The name shown.
    pub label: String,
}

impl fmt::Display for Folder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

/// Returns whether `text` matches a filter: every word of `filter` appears
/// in it, ignoring case.
fn matches(text: &str, filter: &str) -> bool {
    let text = text.to_lowercase();
    filter
        .to_lowercase()
        .split_whitespace()
        .all(|word| text.contains(word))
}

/// The rows of `picker`'s menu, as it shows them.
///
/// The model menu lists only the models matching the filter.
#[must_use]
pub fn items(app: &App, picker: Picker) -> Vec<Item> {
    match picker {
        Picker::Folder => app
            .folder_choices()
            .into_iter()
            .map(|folder| Item {
                selected: folder.directory == app.new_folder,
                label: folder.label,
                detail: None,
                message: Message::PickFolder(folder.directory),
            })
            .collect(),
        Picker::Agent => {
            let current = app.current_agent();
            app.agent_choices()
                .into_iter()
                .map(|agent| Item {
                    selected: Some(&agent.id) == current.as_ref(),
                    label: agent.name,
                    detail: None,
                    message: Message::PickAgent(agent.id),
                })
                .collect()
        }
        Picker::Variant => {
            let current = Variant(app.current_model().and_then(|m| m.variant));
            app.variant_choices()
                .into_iter()
                .map(|variant| Item {
                    selected: variant == current,
                    label: variant.to_string(),
                    detail: None,
                    message: Message::PickVariant(variant),
                })
                .collect()
        }
        Picker::Model => {
            let current = app.current_model_choice();
            app.models
                .iter()
                .filter(|model| {
                    matches(
                        &format!("{} {}", model.name, model.provider_id),
                        &app.picker_filter,
                    )
                })
                .map(|model| Item {
                    selected: Some(model) == current.as_ref(),
                    label: model.name.clone(),
                    detail: Some(model.provider_id.clone()),
                    message: Message::PickModel(model.clone()),
                })
                .collect()
        }
    }
}

/// The agent, reasoning level and model chips for the current session or new one.
pub fn controls(app: &App) -> Element<'_, Message> {
    let mut chips = row![].spacing(6).align_y(Center);
    let agents = app.agent_choices();
    if !agents.is_empty() {
        let current = app.current_agent();
        let label = agents
            .iter()
            .find(|a| Some(&a.id) == current.as_ref())
            .map_or_else(|| "Agent".to_owned(), |a| a.name.clone());
        chips = chips.push(chip(app, Picker::Agent, Icon::Bot, label));
    }
    if app.variant_choices().len() > 1 {
        let label = Variant(app.current_model().and_then(|m| m.variant)).to_string();
        chips = chips.push(chip(app, Picker::Variant, Icon::Brain, label));
    }
    let label = app
        .current_model_choice()
        .map_or_else(|| "Model".to_owned(), |m| m.name);
    chips = chips.push(chip(app, Picker::Model, Icon::Sparkles, label));
    // Where they don't fit on one line, the pickers wrap.
    chips.wrap().vertical_spacing(6).into()
}

/// The folder a new session starts in.
pub fn folder(app: &App) -> Element<'_, Message> {
    let label = app
        .folder_choices()
        .into_iter()
        .find(|f| f.directory == app.new_folder)
        .map(|f| f.label)
        .unwrap_or_default();
    let glyph = if app.new_folder.is_some() {
        Icon::Folder
    } else {
        Icon::House
    };
    chip(app, Picker::Folder, glyph, label)
}

/// A chip showing the current choice, which opens `picker`'s menu.
fn chip(app: &App, picker: Picker, glyph: Icon, label: String) -> Element<'_, Message> {
    let t = &app.tokens;
    let open = app.picker == Some(picker);
    let face = row![
        icon(glyph, 14.0, t.text_interactive_base),
        text(label)
            .size(size::LABEL)
            .color(style::color(t.text_strong)),
        icon(Icon::ChevronDown, 13.0, t.icon_base),
    ]
    .spacing(8)
    .align_y(Center);
    let face = button(face)
        .padding(Padding {
            top: 6.0,
            bottom: 6.0,
            left: 12.0,
            right: 10.0,
        })
        .on_press(Message::TogglePicker(picker))
        .style(style::picker_chip(t, open));
    let anchor = container(face).id(picker.id());
    let menu = open.then(|| menu(app, picker));
    popover(anchor, menu, Message::ClosePicker).into()
}

/// The open menu of `picker`: an optional filter, its rows, and any actions.
fn menu(app: &App, picker: Picker) -> Element<'_, Message> {
    let t = &app.tokens;
    let items = items(app, picker);
    let mut list = column![].spacing(ROW_GAP);
    for (index, item) in items.iter().enumerate() {
        list = list.push(entry(t, item, index == app.picker_index));
    }
    let rows = items.len().min(MENU_ROWS);
    let shown = u16::try_from(rows).unwrap_or(u16::MAX);
    let height = f32::from(shown) * (MENU_ROW + ROW_GAP);
    let mut card = column![].spacing(6);
    if picker == Picker::Model {
        card = card.push(
            text_input("Filter models", &app.picker_filter)
                .id(Id::new(MENU_FILTER))
                .on_input(Message::PickerFilter)
                .on_submit(Message::PickerSubmit)
                .size(size::LABEL)
                .padding([7, 10])
                .style(style::menu_filter(t)),
        );
    }
    if items.is_empty() {
        card = card.push(
            container(
                text("Nothing matches")
                    .size(size::LABEL)
                    .color(style::color(t.text_weak)),
            )
            .padding([8, 10]),
        );
    } else {
        let mut list = scrollable(list)
            .id(Id::new(MENU_LIST))
            .on_scroll(|viewport| Message::PickerScrolled(viewport.absolute_offset().y))
            .height(Length::Fixed(height))
            .style(style::thin_scroll(t));
        // A list that scrolls keeps its scrollbar clear of the rows.
        if items.len() > MENU_ROWS {
            list = list.spacing(SCROLLBAR_GAP);
        }
        card = card.push(list);
    }
    if picker == Picker::Folder {
        card = card
            .push(
                rule::horizontal(1).style(style::divider(style::edge(t, t.surface_raised_strong))),
            )
            .push(action(t, "Add project…", Message::AddProject));
    }
    container(card)
        .width(picker.width())
        .padding(CARD_PADDING)
        .style(style::menu_card(t))
        .into()
}

/// A menu row: the name, a note on the right, and a tick on the current choice.
fn entry<'a>(t: &Tokens, item: &Item, highlighted: bool) -> Element<'a, Message> {
    let mut content = row![
        ellipsis(item.label.clone())
            .size(size::LABEL)
            .color(style::color(t.text_strong)),
    ]
    .spacing(8)
    .align_y(Center)
    .height(Fill);
    if let Some(detail) = &item.detail {
        content = content.push(
            text(detail.clone())
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        );
    }
    let tick: Element<'a, Message> = if item.selected {
        icon(Icon::Check, 14.0, t.text_strong).into()
    } else {
        space().width(14).into()
    };
    button(content.push(tick))
        .width(Fill)
        .height(MENU_ROW)
        .padding([0, 10])
        .on_press(item.message.clone())
        .style(style::menu_row(t, highlighted))
        .into()
}

/// A row at the foot of a menu that does something rather than choose.
fn action<'a>(t: &Tokens, label: &'a str, message: Message) -> Element<'a, Message> {
    button(
        text(label)
            .size(size::LABEL)
            .color(style::color(t.text_base)),
    )
    .width(Fill)
    .height(MENU_ROW)
    .padding([7, 10])
    .on_press(message)
    .style(style::menu_row(t, false))
    .into()
}
