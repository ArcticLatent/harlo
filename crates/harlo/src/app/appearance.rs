//! How Harlo looks: a dark and a light theme, and which of them shows, as
//! chosen in Settings → Appearance.
//!
//! The choice is Harlo's alone; OpenCode's own theme setting is left as it is.

// Rust guideline compliant 2026-10-09

use harlo_core::layout::Appearance;
use harlo_core::theme::{Mode, Scheme};

use super::App;

impl App {
    /// The theme showing: the one chosen for the window's mode, or Harlo's
    /// own for that mode when the chosen one isn't bundled any more.
    #[must_use]
    pub fn theme(&self) -> &Scheme {
        self.theme_for(self.mode())
    }

    /// The theme chosen for `mode`, or Harlo's own for it.
    ///
    /// # Panics
    ///
    /// Panics if the bundled themes lack Harlo's own, which `harlo-core`'s
    /// tests check they hold.
    #[must_use]
    pub fn theme_for(&self, mode: Mode) -> &Scheme {
        let (chosen, fallback) = match mode {
            Mode::Dark => (&self.preferences.dark_theme, harlo_core::theme::HARLO_DARK),
            Mode::Light => (
                &self.preferences.light_theme,
                harlo_core::theme::HARLO_LIGHT,
            ),
        };
        let pick = |id: &str| self.themes.iter().find(|t| t.id == id && t.mode == mode);
        pick(chosen)
            .or_else(|| pick(fallback))
            .expect("Harlo's own themes are bundled")
    }

    /// Whether the window is dark or light: as chosen, or the desktop's for
    /// System.
    #[must_use]
    pub fn mode(&self) -> Mode {
        match self.preferences.appearance {
            Appearance::Light => Mode::Light,
            Appearance::Dark => Mode::Dark,
            Appearance::System => self.system_mode,
        }
    }

    /// Takes the colours from the theme showing.
    pub(super) fn retheme(&mut self) {
        self.tokens = self.theme().tokens();
    }

    /// Notes whether the desktop prefers dark, which System follows.
    pub(super) fn set_system_dark(&mut self, dark: bool) {
        self.system_mode = if dark { Mode::Dark } else { Mode::Light };
        self.retheme();
    }
}
