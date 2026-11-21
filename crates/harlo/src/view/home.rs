//! The main area before a session is open: "What are we working on?" and the composer.

// Rust guideline compliant 2026-10-09

use iced::widget::{column, container, row, space, text};
use iced::{Center, Element, Fill, Padding};

use crate::app::{App, Message};
use crate::style::{self, size};
use crate::view::composer::{self, Composer};
use crate::view::pickers;

/// The composer's widest size; it narrows with the window.
const COMPOSER_WIDTH: f32 = 760.0;

/// Space either side of the composer.
const MARGIN: f32 = 24.0;

/// The empty state, centred in the main area.
pub fn view(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    // The key hint shares the tool row; a narrow composer leaves it out.
    let width = (app.main_width() - 2.0 * MARGIN).min(COMPOSER_WIDTH);
    let hint = if app.starting() {
        Some("Starting the session…")
    } else {
        (width >= composer::INLINE_CONTROLS)
            .then_some("Enter to start · Shift+Enter for a new line")
    };
    let question = text("What are we working on?")
        .size(size::HEADLINE)
        .color(style::color(t.text_strong));
    // In a narrow window the pickers wrap onto a second line.
    let choices = row![pickers::folder(app), pickers::controls(app)]
        .spacing(6)
        .align_y(Center)
        .wrap();
    let body = column![
        question,
        space().height(22),
        choices,
        space().height(14),
        composer::view(
            t,
            Composer {
                content: &app.composer,
                placeholder: "Ask, or describe what to build.",
                hint,
                error: app.send_error.as_deref(),
                attached: &app.draft.attached,
                menu: app.menu.as_ref(),
                busy: false,
                wide: true,
                feedback: app.feedback(),
            },
            None,
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
            left: MARGIN,
            right: MARGIN,
        })
        .center(Fill)
        .style(style::fill(t.background_base))
        .into()
}
