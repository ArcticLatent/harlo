//! Motion: how things appear, slide and pulse, built on iced's `Animation`.
//!
//! Motion is short and quiet. It confirms that something happened (a prompt
//! went out, a chip was added, a panel opened) or that work is going on
//! (a session working, a prompt on its way), and never delays anything.
//!
//! Two clocks drive it. One-off motion (fades, slides) asks the app for
//! every frame, but only until it settles. The pulse of work in progress
//! runs as long as the work does, which can be hours, so it never involves
//! the app: an indicator's colour is worked out when it's drawn (see
//! [`Moment::breathing`]), and `widget::breathe` asks for the next redraw.

// Rust guideline compliant 2026-10-09

use harlo_core::theme::Rgba;
use iced::Theme;
use iced::animation::{Animation, Easing};
use iced::time::{Duration, Instant};
use iced::widget::text;

use crate::style;

/// How long something new takes to fade in.
pub const APPEAR: Duration = Duration::from_millis(220);

/// How long a side panel takes to slide open or closed.
pub const SLIDE: Duration = Duration::from_millis(200);

/// Half a pulse: from dim to bright, or back.
///
/// Slow enough to read as breathing rather than blinking.
pub const PULSE: Duration = Duration::from_millis(900);

/// How often a breathing indicator redraws.
///
/// Every redraw draws the whole window, so the pulse redraws 15 times a
/// second rather than at the display's rate. A fade this slow still looks
/// smooth at that speed.
pub const BREATH_FRAME: Duration = Duration::from_millis(66);

/// How far, in pixels, a new prompt rises as it fades in.
pub const RISE: f32 = 8.0;

/// The dimmest a pulsing indicator gets, as a share of its colour.
const PULSE_LOW: f32 = 0.35;

/// A fade-in starting `at`: false is hidden, true is shown.
#[must_use]
pub fn appear(at: Instant) -> Animation<bool> {
    Animation::new(false)
        .duration(APPEAR)
        .easing(Easing::EaseOutCubic)
        .go(true, at)
}

/// An open-or-closed state that slides between the two.
#[must_use]
pub fn slide(open: bool) -> Animation<bool> {
    Animation::new(open)
        .duration(SLIDE)
        .easing(Easing::EaseInOutCubic)
}

/// A pulse that started `at` and never stops.
#[must_use]
pub fn pulse(at: Instant) -> Animation<bool> {
    Animation::new(false)
        .duration(PULSE)
        .easing(Easing::EaseInOut)
        .repeat_forever()
        .auto_reverse()
        .go(true, at)
}

/// How far a fade-in has come at `now`, from 0 (hidden) to 1 (shown).
#[must_use]
pub fn shown(animation: &Animation<bool>, now: Instant) -> f32 {
    animation.interpolate(0.0, 1.0, now)
}

/// A pulsing indicator's strength at `now`, from [`PULSE_LOW`] to 1.
#[must_use]
pub fn beat(pulse: &Animation<bool>, now: Instant) -> f32 {
    pulse.interpolate(PULSE_LOW, 1.0, now)
}

/// `color` with its opacity scaled by `amount` (0 to 1).
#[must_use]
pub fn fade(color: Rgba, amount: f32) -> Rgba {
    color.alpha(color.a * amount.clamp(0.0, 1.0))
}

/// The moment a view is drawn for: the time, and when the pulse started.
#[derive(Debug, Clone, Copy)]
pub struct Moment {
    /// The time one-off motion is drawn at.
    pub now: Instant,
    /// When the pulse started; every breathing indicator keeps time with it.
    pub since: Instant,
}

impl Moment {
    /// A text style whose `color` breathes with the pulse.
    ///
    /// The colour is worked out each time the text is drawn, so the text
    /// must sit in a `widget::breathe`, which keeps redrawing it.
    pub fn breathing(self, color: Rgba) -> impl Fn(&Theme) -> text::Style + 'static {
        let since = self.since;
        move |_| text::Style {
            color: Some(style::color(fade(
                color,
                beat(&pulse(since), Instant::now()),
            ))),
        }
    }
}
