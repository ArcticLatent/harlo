//! How Harlo looks: a dark and a light theme, and which of them shows, as
//! chosen in Settings → Appearance.
//!
//! The choice is Harlo's alone; OpenCode's own theme setting is left as it is.

// Rust guideline compliant 2026-10-09

use harlo_core::layout::{Appearance, ChatWidth, Speed, TextSize};
use harlo_core::theme::{Contrast, DiffColors, Look, Mode, Scheme};

use super::{App, SettingChoice};

/// Each choice's values, in the order of its options.
const APPEARANCES: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];
const CONTRASTS: [Contrast; 2] = [Contrast::Standard, Contrast::More];
const DIFF_COLORS: [DiffColors; 2] = [DiffColors::GreenRed, DiffColors::BlueOrange];
const SPEEDS: [Speed; 4] = [Speed::Off, Speed::Fast, Speed::Normal, Speed::Slow];
const CHAT_WIDTHS: [ChatWidth; 4] = [
    ChatWidth::Narrow,
    ChatWidth::Normal,
    ChatWidth::Wide,
    ChatWidth::Full,
];
const TEXT_SIZES: [TextSize; 4] = [
    TextSize::Small,
    TextSize::Normal,
    TextSize::Large,
    TextSize::Larger,
];
const CODE_SIZES: [TextSize; 3] = [TextSize::Small, TextSize::Normal, TextSize::Large];

/// Where `value` is among `values`, or 0.
fn place<T: PartialEq>(values: &[T], value: &T) -> usize {
    values.iter().position(|v| v == value).unwrap_or_default()
}

/// Sets `slot` to option `option` of `values`; false if there's no such
/// option.
fn pick<T: Copy>(values: &[T], option: usize, slot: &mut T) -> bool {
    values.get(option).map(|&value| *slot = value).is_some()
}

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

    /// Which option of `choice` is chosen, counted from 0.
    #[must_use]
    pub fn chosen(&self, choice: SettingChoice) -> usize {
        let p = &self.preferences;
        match choice {
            SettingChoice::Appearance => place(&APPEARANCES, &p.appearance),
            SettingChoice::Contrast => place(&CONTRASTS, &p.contrast),
            SettingChoice::DiffColors => place(&DIFF_COLORS, &p.diff_colors),
            SettingChoice::Motion => place(&SPEEDS, &p.motion),
            SettingChoice::ChatWidth => place(&CHAT_WIDTHS, &p.chat_width),
            SettingChoice::TextSize => place(&TEXT_SIZES, &p.text_size),
            SettingChoice::CodeSize => place(&CODE_SIZES, &p.code_size),
            // After "None".
            SettingChoice::ProjectColor => self
                .sheets
                .projects
                .shown
                .as_deref()
                .and_then(|folder| self.workspace.project(folder)?.color.clone())
                .and_then(|color| {
                    harlo_core::server::PROJECT_COLORS
                        .iter()
                        .position(|c| *c == color)
                })
                .map_or(0, |i| i + 1),
            SettingChoice::ProjectSessions => usize::from(self.sheets.projects.archive),
        }
    }

    /// Takes option `option` of `choice`; false if it has no such option.
    pub(super) fn choose(&mut self, choice: SettingChoice, option: usize) -> bool {
        let p = &mut self.preferences;
        match choice {
            SettingChoice::Appearance => pick(&APPEARANCES, option, &mut p.appearance),
            SettingChoice::Contrast => pick(&CONTRASTS, option, &mut p.contrast),
            SettingChoice::DiffColors => pick(&DIFF_COLORS, option, &mut p.diff_colors),
            SettingChoice::Motion => pick(&SPEEDS, option, &mut p.motion),
            SettingChoice::ChatWidth => pick(&CHAT_WIDTHS, option, &mut p.chat_width),
            SettingChoice::TextSize => pick(&TEXT_SIZES, option, &mut p.text_size),
            SettingChoice::CodeSize => pick(&CODE_SIZES, option, &mut p.code_size),
            // Not Harlo's look: Settings → Projects takes them.
            SettingChoice::ProjectColor | SettingChoice::ProjectSessions => false,
        }
    }

    /// Takes the colours from the theme showing.
    pub(super) fn retheme(&mut self) {
        let look = Look {
            contrast: self.preferences.contrast,
            diff: self.preferences.diff_colors,
        };
        self.tokens = self.theme().tokens_with(look);
    }

    /// Notes whether the desktop prefers dark, which System follows.
    pub(super) fn set_system_dark(&mut self, dark: bool) {
        self.system_mode = if dark { Mode::Dark } else { Mode::Light };
        self.retheme();
    }
}
