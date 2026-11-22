//! How Harlo looks: which of the bundled themes, and its light or dark
//! variant, as chosen in Settings → Appearance.
//!
//! The themes are Harlo's own and OpenCode's built-in desktop themes. The
//! choice is Harlo's alone; OpenCode's own theme setting is left as it is.

// Rust guideline compliant 2026-10-09

use harlo_core::layout::Appearance;
use harlo_core::theme::{Mode, Theme};

use super::App;

impl App {
    /// The theme chosen, or Harlo's own when it isn't bundled any more.
    #[must_use]
    pub fn theme(&self) -> &Theme {
        self.themes
            .iter()
            .find(|t| t.id == self.preferences.theme)
            .unwrap_or(&self.themes[0])
    }

    /// The variant shown: the one chosen, or the desktop's for System.
    #[must_use]
    pub fn mode(&self) -> Mode {
        match self.preferences.appearance {
            Appearance::Light => Mode::Light,
            Appearance::Dark => Mode::Dark,
            Appearance::System => self.system_mode,
        }
    }

    /// Takes the colours from the chosen theme and variant.
    pub(super) fn retheme(&mut self) {
        self.tokens = self.theme().tokens(self.mode());
    }

    /// Notes whether the desktop prefers dark, which System follows.
    pub(super) fn set_system_dark(&mut self, dark: bool) {
        self.system_mode = if dark { Mode::Dark } else { Mode::Light };
        self.retheme();
    }
}
