//! Colour themes in OpenCode's desktop theme format, resolved to Harlo's tokens.
//!
//! A theme file has a `light` and a `dark` variant. Each variant gives a small
//! `palette` (`neutral` background, `ink` text, `primary`, status colours) and
//! optional `overrides` keyed by token name, such as `"text-weak": "#8b8c8e"`
//! or `"icon-base": "var(--text-base)"`. Harlo derives every token it uses
//! from the palette, then applies the overrides it knows. Token names are
//! OpenCode's, so its theme files load as they are; overrides for tokens Harlo
//! doesn't use, and `v2Overrides`, are ignored.

// Rust guideline compliant 2026-10-09

use std::backtrace::Backtrace;
use std::collections::BTreeMap;
use std::fmt::{self, Display};

use serde::Deserialize;

/// How many rounds of `var(--…)` references are applied.
///
/// One round resolves a direct reference; real themes chain at most two or
/// three, so eight leaves plenty of room.
const MAX_REFERENCE_DEPTH: usize = 8;

/// A colour with straight (not premultiplied) alpha, each channel in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    /// Red.
    pub r: f32,
    /// Green.
    pub g: f32,
    /// Blue.
    pub b: f32,
    /// Opacity.
    pub a: f32,
}

impl Rgba {
    /// Fully opaque black.
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    /// Parses `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`.
    #[must_use]
    pub fn parse(hex: &str) -> Option<Self> {
        let digits = hex.strip_prefix('#')?;
        if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |i: usize, width: usize| -> Option<f32> {
            let text = digits.get(i * width..(i + 1) * width)?;
            let value = u8::from_str_radix(text, 16).ok()?;
            // A one-digit channel `f` means `ff`.
            let value = if width == 1 { value * 17 } else { value };
            Some(f32::from(value) / 255.0)
        };
        let (width, count) = match digits.len() {
            3 => (1, 3),
            4 => (1, 4),
            6 => (2, 3),
            8 => (2, 4),
            _ => return None,
        };
        Some(Self {
            r: channel(0, width)?,
            g: channel(1, width)?,
            b: channel(2, width)?,
            a: if count == 4 { channel(3, width)? } else { 1.0 },
        })
    }

    /// Returns this colour with the given opacity.
    #[must_use]
    pub fn alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Mixes `amount` (0 to 1) of this colour into `base`, giving an opaque result.
    #[must_use]
    pub fn over(self, base: Self, amount: f32) -> Self {
        let mix = |top: f32, bottom: f32| bottom + (top - bottom) * amount;
        Self {
            r: mix(self.r, base.r),
            g: mix(self.g, base.g),
            b: mix(self.b, base.b),
            a: 1.0,
        }
    }
}

impl Display for Rgba {
    /// Writes `#rrggbb`, or `#rrggbbaa` when the colour is translucent.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to 0..=255 first"
        )]
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        write!(
            f,
            "#{:02x}{:02x}{:02x}",
            byte(self.r),
            byte(self.g),
            byte(self.b)
        )?;
        if self.a < 1.0 {
            write!(f, "{:02x}", byte(self.a))?;
        }
        Ok(())
    }
}

/// Which variant of a theme to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Dark text on a light background.
    Light,
    /// Light text on a dark background.
    Dark,
}

macro_rules! tokens {
    ($( $(#[$doc:meta])* $field:ident = $name:literal ),* $(,)?) => {
        /// Every colour Harlo's interface uses, resolved for one theme variant.
        ///
        /// Field names follow OpenCode's token names (`text_weak` is `text-weak`).
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct Tokens {
            $( $(#[$doc])* pub $field: Rgba, )*
        }

        impl Tokens {
            fn slot(&mut self, name: &str) -> Option<&mut Rgba> {
                match name {
                    $( $name => Some(&mut self.$field), )*
                    _ => None,
                }
            }

            fn get(&self, name: &str) -> Option<Rgba> {
                match name {
                    $( $name => Some(self.$field), )*
                    _ => None,
                }
            }
        }
    };
}

tokens! {
    /// The main background: the conversation and side panels.
    background_base = "background-base",
    /// The darker background of the rails and the sidebar.
    background_stronger = "background-stronger",
    /// Sunken fields, such as the session filter.
    surface_inset_base = "surface-inset-base",
    /// Raised areas, such as the composer.
    surface_raised_base = "surface-raised-base",
    /// Buttons, chips and pickers.
    surface_raised_strong = "surface-raised-strong",
    /// The wash under a hovered item.
    surface_base_hover = "surface-base-hover",
    /// The wash under a pressed or selected item.
    surface_base_active = "surface-base-active",
    /// The primary brand surface.
    surface_brand_base = "surface-brand-base",
    /// The faintest separators.
    border_weaker_base = "border-weaker-base",
    /// Outlines of raised areas and panel dividers.
    border_weak_base = "border-weak-base",
    /// Stronger outlines, such as a focused field.
    border_base = "border-base",
    /// Headings and the most important text.
    text_strong = "text-strong",
    /// Body text.
    text_base = "text-base",
    /// Secondary text, such as ages and hints.
    text_weak = "text-weak",
    /// Placeholders and disabled text.
    text_weaker = "text-weaker",
    /// Links and accented text.
    text_interactive_base = "text-interactive-base",
    /// Icons.
    icon_base = "icon-base",
    /// Secondary icons.
    icon_weak_base = "icon-weak-base",
    /// The icon of the selected rail item.
    icon_selected = "icon-selected",
    /// Success, such as a connected server.
    icon_success_base = "icon-success-base",
    /// Warnings.
    icon_warning_base = "icon-warning-base",
    /// Errors.
    icon_critical_base = "icon-critical-base",
    /// Information.
    icon_info_base = "icon-info-base",
}

/// A theme read from a file, with both variants.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// The slug, such as `harlo`.
    pub id: String,
    /// The name people see, such as `Harlo`.
    pub name: String,
    light: Variant,
    dark: Variant,
}

#[derive(Debug, Clone, PartialEq)]
struct Variant {
    palette: Palette,
    overrides: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Palette {
    neutral: Rgba,
    ink: Rgba,
    primary: Rgba,
    success: Rgba,
    warning: Rgba,
    error: Rgba,
    info: Rgba,
    interactive: Rgba,
}

#[derive(Deserialize)]
struct FileTheme {
    id: String,
    name: String,
    light: FileVariant,
    dark: FileVariant,
}

#[derive(Deserialize)]
struct FileVariant {
    // OpenCode's older files call the palette `seeds`.
    #[serde(alias = "seeds")]
    palette: FilePalette,
    #[serde(default)]
    overrides: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct FilePalette {
    neutral: String,
    ink: Option<String>,
    primary: String,
    success: String,
    warning: String,
    error: String,
    info: String,
    interactive: Option<String>,
}

/// Reads a theme in OpenCode's desktop theme format.
///
/// # Errors
///
/// Returns an error when the text isn't a theme file, or a palette colour
/// isn't a hex colour. Overrides are checked when tokens are resolved, and a
/// bad one is skipped rather than rejected.
pub fn parse(json: &str) -> Result<Theme, ThemeError> {
    let file: FileTheme = serde_json::from_str(json)?;
    Ok(Theme {
        light: variant(file.light, Mode::Light)?,
        dark: variant(file.dark, Mode::Dark)?,
        id: file.id,
        name: file.name,
    })
}

fn variant(file: FileVariant, mode: Mode) -> Result<Variant, ThemeError> {
    let color = |name: &'static str, value: &str| {
        Rgba::parse(value).ok_or_else(|| ThemeError::bad_color(mode, name, value))
    };
    let p = &file.palette;
    let neutral = color("neutral", &p.neutral)?;
    let primary = color("primary", &p.primary)?;
    let ink = match &p.ink {
        Some(ink) => color("ink", ink)?,
        // Without ink, text contrasts with the background.
        None if mode == Mode::Dark => Rgba::parse("#ececec").unwrap_or(Rgba::BLACK),
        None => Rgba::parse("#1c1c1c").unwrap_or(Rgba::BLACK),
    };
    let palette = Palette {
        neutral,
        ink,
        primary,
        success: color("success", &p.success)?,
        warning: color("warning", &p.warning)?,
        error: color("error", &p.error)?,
        info: color("info", &p.info)?,
        interactive: match &p.interactive {
            Some(value) => color("interactive", value)?,
            None => primary,
        },
    };
    Ok(Variant {
        palette,
        overrides: file.overrides,
    })
}

impl Theme {
    /// Resolves every token for the given variant.
    #[must_use]
    pub fn tokens(&self, mode: Mode) -> Tokens {
        let variant = match mode {
            Mode::Light => &self.light,
            Mode::Dark => &self.dark,
        };
        let mut tokens = derive(&variant.palette, mode);
        apply_overrides(&mut tokens, &variant.overrides);
        tokens
    }
}

/// Derives every token from the palette by mixing ink into the background.
///
/// The amounts make a dark grey palette of `#292929` and `#d5d5d5` give
/// Harlo's own greys within one step of 255.
fn derive(p: &Palette, mode: Mode) -> Tokens {
    let tint = |amount: f32| p.ink.over(p.neutral, amount);
    let text_base = tint(0.72);
    Tokens {
        background_base: p.neutral,
        background_stronger: match mode {
            Mode::Dark => Rgba::BLACK.over(p.neutral, 0.1),
            Mode::Light => tint(0.04),
        },
        surface_inset_base: tint(0.025),
        surface_raised_base: tint(0.035),
        surface_raised_strong: tint(0.07),
        surface_base_hover: p.ink.alpha(0.05),
        surface_base_active: p.ink.alpha(0.07),
        surface_brand_base: p.primary,
        border_weaker_base: tint(0.05),
        border_weak_base: tint(0.08),
        border_base: tint(0.14),
        text_strong: p.ink,
        text_base,
        text_weak: tint(0.58),
        text_weaker: tint(0.37),
        text_interactive_base: p.interactive,
        icon_base: text_base,
        icon_weak_base: tint(0.5),
        icon_selected: p.primary,
        icon_success_base: p.success,
        icon_warning_base: p.warning,
        icon_critical_base: p.error,
        icon_info_base: p.info,
    }
}

/// Applies overrides for tokens Harlo uses, following `var(--token)` references.
///
/// Plain colours go first. References are then applied for a fixed number of
/// rounds, so a chain of `k` references is right after `k` rounds; a cycle
/// just settles on some value of its members. A bad value is skipped.
fn apply_overrides(tokens: &mut Tokens, overrides: &BTreeMap<String, String>) {
    let mut references = Vec::new();
    for (name, value) in overrides {
        let value = value.trim();
        match value
            .strip_prefix("var(--")
            .and_then(|v| v.strip_suffix(')'))
        {
            Some(target) => references.push((name.as_str(), target)),
            None => {
                if let (Some(color), Some(slot)) = (Rgba::parse(value), tokens.slot(name)) {
                    *slot = color;
                }
            }
        }
    }
    for _ in 0..MAX_REFERENCE_DEPTH {
        for &(name, target) in &references {
            if let (Some(color), Some(slot)) = (tokens.get(target), tokens.slot(name)) {
                *slot = color;
            }
        }
    }
}

/// Reading a theme failed.
#[derive(Debug)]
pub struct ThemeError {
    kind: ErrorKind,
    backtrace: Backtrace,
}

#[derive(Debug)]
enum ErrorKind {
    Json(serde_json::Error),
    BadColor {
        mode: Mode,
        name: &'static str,
        value: String,
    },
}

impl ThemeError {
    fn bad_color(mode: Mode, name: &'static str, value: &str) -> Self {
        Self {
            kind: ErrorKind::BadColor {
                mode,
                name,
                value: value.to_owned(),
            },
            backtrace: Backtrace::capture(),
        }
    }

    /// Whether the text wasn't a theme file at all.
    #[must_use]
    pub fn is_format(&self) -> bool {
        matches!(self.kind, ErrorKind::Json(_))
    }

    /// Whether a palette colour wasn't a hex colour.
    #[must_use]
    pub fn is_bad_color(&self) -> bool {
        matches!(self.kind, ErrorKind::BadColor { .. })
    }

    /// Where the error was found, when backtraces are enabled.
    pub fn backtrace(&self) -> &Backtrace {
        &self.backtrace
    }
}

impl Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::Json(e) => write!(f, "not a theme file: {e}"),
            ErrorKind::BadColor { mode, name, value } => {
                let mode = match mode {
                    Mode::Light => "light",
                    Mode::Dark => "dark",
                };
                write!(
                    f,
                    "the {mode} palette's {name} colour isn't a hex colour: {value:?}"
                )
            }
        }
    }
}

impl std::error::Error for ThemeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ErrorKind::Json(e) => Some(e),
            ErrorKind::BadColor { .. } => None,
        }
    }
}

impl From<serde_json::Error> for ThemeError {
    fn from(e: serde_json::Error) -> Self {
        Self {
            kind: ErrorKind::Json(e),
            backtrace: Backtrace::capture(),
        }
    }
}
