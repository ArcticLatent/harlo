//! Chips that choose a session's agent, model and reasoning level, and a new session's folder.

// Rust guideline compliant 2026-10-09

use std::fmt;

use harlo_core::theme::Tokens;
use iced::widget::{combo_box, container, pick_list, row, stack};
use iced::{Center, Element, Fill, Padding, padding};
use lucide_icons::Icon;

use crate::app::{App, Message};
use crate::style::{self, size};
use crate::view::icon;

/// Width of the model field.
const MODEL_WIDTH: f32 = 190.0;

/// A chip's left padding: room for its icon, which is drawn over it.
const CHIP_LEFT: f32 = 34.0;

/// The gap between a chip's label and its arrow.
const CHIP_GAP: f32 = 6.0;

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

/// The agent, reasoning level and model chips for the current session or new one.
pub fn controls(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    let mut chips = row![].spacing(6).align_y(Center);
    let agents = app.agent_choices();
    if !agents.is_empty() {
        let current = app.current_agent();
        let selected = agents
            .iter()
            .find(|a| Some(&a.id) == current.as_ref())
            .cloned();
        chips = chips.push(chip(
            t,
            Icon::Bot,
            agents,
            selected,
            |agent: AgentChoice| Message::PickAgent(agent.id),
            crate::ids::AGENT,
        ));
    }
    let variants = app.variant_choices();
    if variants.len() > 1 {
        let selected = Variant(app.current_model().and_then(|m| m.variant));
        chips = chips.push(chip(
            t,
            Icon::Brain,
            variants,
            Some(selected),
            Message::PickVariant,
            crate::ids::VARIANT,
        ));
    }
    let selected = app.current_model_choice();
    let models = combo_box(&app.models, "Model", selected.as_ref(), Message::PickModel)
        .width(MODEL_WIDTH)
        .size(size::LABEL)
        .padding([6, 10])
        .icon(iced::widget::text_input::Icon {
            font: iced::Font::with_name("lucide"),
            code_point: Icon::Sparkles.unicode(),
            size: Some(13.0.into()),
            spacing: 8.0,
            side: iced::widget::text_input::Side::Left,
        })
        .input_style(style::picker_field(t))
        .menu_style(style::menu(t));
    let chips = chips.push(container(models).id(crate::ids::MODEL));
    // Where they don't fit on one line, the pickers wrap.
    chips.wrap().vertical_spacing(6).into()
}

/// The folder a new session starts in.
pub fn folder(app: &App) -> Element<'_, Message> {
    let options = app.folder_choices();
    let selected = options
        .iter()
        .find(|f| f.directory == app.new_folder)
        .cloned();
    let glyph = if app.new_folder.is_some() {
        Icon::Folder
    } else {
        Icon::House
    };
    chip(
        &app.tokens,
        glyph,
        options,
        selected,
        |folder: Folder| Message::PickFolder(folder.directory),
        crate::ids::FOLDER,
    )
}

/// A chip that opens a list of `options`.
fn chip<'a, T>(
    t: &Tokens,
    glyph: Icon,
    options: Vec<T>,
    selected: Option<T>,
    on_select: impl Fn(T) -> Message + 'a,
    id: &'static str,
) -> Element<'a, Message>
where
    T: fmt::Display + PartialEq + Clone + 'a,
{
    // The whole chip is the pick list, so its menu opens under the chip at
    // its width; the icon is drawn over the list's left padding.
    let list = pick_list(options, selected, on_select)
        .text_size(size::LABEL)
        .padding(Padding {
            top: 6.0,
            bottom: 6.0,
            left: CHIP_LEFT,
            right: 10.0,
        })
        .handle(pick_list::Handle::Static(pick_list::Icon {
            font: iced::Font::with_name("lucide"),
            code_point: Icon::ChevronDown.unicode(),
            size: Some(13.0.into()),
            line_height: iced::widget::text::LineHeight::Relative(1.0),
            shaping: iced::widget::text::Shaping::Basic,
        }))
        .style(style::picker(t))
        .menu_style(style::menu(t));
    let glyph = container(icon(glyph, 14.0, t.text_interactive_base))
        .padding(padding::left(12))
        .height(Fill)
        .center_y(Fill);
    // The list also counts its left padding as the gap before its arrow.
    let list = crate::widget::snug(list, CHIP_LEFT - CHIP_GAP);
    container(stack![list, glyph]).id(id).into()
}
