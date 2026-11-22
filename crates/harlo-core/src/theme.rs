//! Colour themes in OpenCode's desktop theme format, resolved to Harlo's tokens.
//!
//! A theme file has a `light` and a `dark` variant. Each variant gives a small
//! `palette` (`neutral` background, `ink` text, `primary`, status colours) and
//! optional `overrides` keyed by token name, such as `"text-weak": "#8b8c8e"`
//! or `"icon-base": "var(--text-base)"`. Harlo derives every token it uses
//! from the palette the way OpenCode's desktop app does (its OKLCH scales,
//! in [`color`]), then applies the overrides it knows, so a theme looks as it
//! does in OpenCode. Token names are OpenCode's, so its theme files load as
//! they are; overrides for tokens Harlo doesn't use, and `v2Overrides`, are
//! ignored.

// Rust guideline compliant 2026-10-09

use std::backtrace::Backtrace;
use std::collections::BTreeMap;
use std::fmt::{self, Display};

use serde::Deserialize;

use color::Hex;

mod color;

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

    /// The colour as 8-bit channels, without its opacity.
    #[must_use]
    fn hex(self) -> color::Hex {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to 0..=1 first, so the scaled value fits a u8"
        )]
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [byte(self.r), byte(self.g), byte(self.b)]
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

impl From<color::Hex> for Rgba {
    fn from([r, g, b]: color::Hex) -> Self {
        Self {
            r: f32::from(r) / 255.0,
            g: f32::from(g) / 255.0,
            b: f32::from(b) / 255.0,
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

            /// The token OpenCode names `name`, such as `text-weak`.
            #[must_use]
            pub fn get(&self, name: &str) -> Option<Rgba> {
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
    /// Comments in code.
    syntax_comment = "syntax-comment",
    /// Keywords, such as `fn` and `if`.
    syntax_keyword = "syntax-keyword",
    /// Strings.
    syntax_string = "syntax-string",
    /// Built-in types and values, such as `bool` and `true`.
    syntax_primitive = "syntax-primitive",
    /// Properties and fields.
    syntax_property = "syntax-property",
    /// Type names.
    syntax_type = "syntax-type",
    /// Constants and numbers.
    syntax_constant = "syntax-constant",
    /// Operators.
    syntax_operator = "syntax-operator",
    /// Punctuation, such as brackets.
    syntax_punctuation = "syntax-punctuation",
    /// Variables and parameters.
    syntax_variable = "syntax-variable",
}

/// OpenCode's built-in desktop themes, bundled from the pinned release.
const OPENCODE_THEMES: &[&str] = include!(concat!(env!("OUT_DIR"), "/opencode_themes.rs"));

/// Every bundled theme: Harlo's own first, then OpenCode's by name.
///
/// A file that doesn't parse is left out; the tests check they all do.
#[must_use]
pub fn bundled() -> Vec<Theme> {
    let mut opencode: Vec<Theme> = OPENCODE_THEMES
        .iter()
        .filter_map(|json| parse(json).ok())
        .collect();
    opencode.sort_by_cached_key(|theme| theme.name.to_lowercase());
    let mut themes: Vec<Theme> = parse(crate::DEFAULT_THEME).into_iter().collect();
    themes.extend(opencode);
    themes
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

/// A variant's colours as its file gives them.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Palette {
    /// Whether the file gives a `palette`, rather than OpenCode's older
    /// `seeds`; the two derive text and code colours differently.
    compact: bool,
    neutral: Hex,
    ink: Option<Hex>,
    primary: Hex,
    accent: Hex,
    success: Hex,
    warning: Hex,
    error: Hex,
    info: Hex,
    interactive: Hex,
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
    palette: Option<FilePalette>,
    seeds: Option<FilePalette>,
    #[serde(default)]
    overrides: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct FilePalette {
    neutral: String,
    ink: Option<String>,
    primary: String,
    accent: Option<String>,
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
    let (p, compact) = match (file.palette, file.seeds) {
        (Some(palette), None) => (palette, true),
        (None, Some(seeds)) => (seeds, false),
        _ => {
            return Err(ThemeError::bad_color(
                mode,
                "palette",
                "needs one of palette or seeds",
            ));
        }
    };
    let color = |name: &'static str, value: &str| {
        Rgba::parse(value)
            .map(Rgba::hex)
            .ok_or_else(|| ThemeError::bad_color(mode, name, value))
    };
    let info = color("info", &p.info)?;
    let primary = color("primary", &p.primary)?;
    let palette = Palette {
        compact,
        neutral: color("neutral", &p.neutral)?,
        // OpenCode's older seeds have no ink.
        ink: match (&p.ink, compact) {
            (Some(ink), true) => Some(color("ink", ink)?),
            _ => None,
        },
        primary,
        // As in OpenCode, a theme without an accent uses its info colour,
        // and one without an interactive colour its primary.
        accent: match &p.accent {
            Some(value) if compact => color("accent", value)?,
            _ => info,
        },
        success: color("success", &p.success)?,
        warning: color("warning", &p.warning)?,
        error: color("error", &p.error)?,
        info,
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
        let (mut tokens, references) = derive(&variant.palette, &variant.overrides, mode);
        // Some colours follow other tokens, as in OpenCode, so an override of
        // those reaches them too, unless the theme sets them itself.
        let mut overrides = variant.overrides.clone();
        for (name, target) in references {
            overrides
                .entry(name.to_owned())
                .or_insert_with(|| format!("var(--{target})"));
        }
        apply_overrides(&mut tokens, &overrides);
        // A compact theme's own weak text sets its weaker text too.
        let weak = variant.overrides.get("text-weak").map(|v| v.trim());
        if variant.palette.compact && !variant.overrides.contains_key("text-weaker") {
            match weak.and_then(Rgba::parse) {
                Some(weak) => {
                    let (l, c) = match mode {
                        Mode::Dark => (-0.12, 0.75),
                        Mode::Light => (0.12, 0.75),
                    };
                    tokens.text_weaker = Rgba::from(color::shift(weak.hex(), l, c, 0.0));
                }
                None if weak.is_some() => tokens.text_weaker = tokens.text_weak,
                None => {}
            }
        }
        tokens
    }
}

/// Derives every token from the palette as OpenCode's desktop app does,
/// and names the tokens that follow others (`syntax-comment` follows
/// `text-weak`, say), which overrides are then applied through.
///
/// Ported from OpenCode's `packages/ui/src/theme/resolve.ts`, Copyright (c)
/// 2025 opencode, under the MIT licence in `themes/opencode/LICENSE`.
fn derive(
    p: &Palette,
    overrides: &BTreeMap<String, String>,
    mode: Mode,
) -> (Tokens, Vec<(&'static str, &'static str)>) {
    use color::{blend, neutral_scale};

    let dark = mode == Mode::Dark;
    // The first value for the dark variant, the second for the light.
    macro_rules! pick {
        ($dark:expr, $light:expr $(,)?) => {
            if dark { $dark } else { $light }
        };
    }
    let neutral = neutral_scale(p.neutral, dark, p.ink);
    let scales = Scales::new(p, dark);
    let ink = p.ink.unwrap_or(p.neutral);
    // Translucent tones are laid over the background, or over a theme's own
    // background colour when it sets one.
    let background = overrides
        .get("background-base")
        .and_then(|v| Rgba::parse(v.trim()))
        .map_or(neutral[0], Rgba::hex);
    let border = |light: f64, dark_amount: f64| {
        let extra = if p.compact { 0.08 } else { 0.0 };
        let alpha = if dark {
            (dark_amount + 0.024 + extra).min(1.0)
        } else {
            (light + 0.024).min(1.0)
        };
        blend(ink, background, alpha)
    };
    let alphas = pick!(
        [
            0.038, 0.066, 0.1, 0.142, 0.19, 0.252, 0.334, 0.446, 0.58, 0.718, 0.854, 0.985,
        ],
        [
            0.03, 0.06, 0.1, 0.145, 0.2, 0.265, 0.35, 0.47, 0.61, 0.74, 0.86, 0.97,
        ],
    );
    let neutral_alpha = alphas.map(|alpha| blend(neutral[11], neutral[0], alpha));
    let [text_base, text_weak, text_weaker, text_strong] = text_colors(p, &neutral, dark);
    let (icon_base, icon_selected) = if p.compact && !dark {
        (text_weak, text_strong)
    } else {
        (neutral[pick!(9, 8)], neutral[11])
    };
    let (border_base, border_weak, border_weaker) = if p.compact {
        (border(0.22, 0.16), border(0.1, 0.08), border(0.06, 0.04))
    } else {
        (
            neutral_alpha[6],
            neutral_alpha[pick!(5, 4)],
            neutral_alpha[2],
        )
    };
    let [string, keyword, primitive, property, kind, constant] =
        code_colors(p, &scales, text_weak, dark);
    let quiet = pick!("text-weak", "text-base");
    let mut references = vec![
        ("syntax-comment", "text-weak"),
        ("syntax-operator", quiet),
        ("syntax-punctuation", quiet),
        ("syntax-variable", "text-strong"),
    ];
    if !p.compact {
        references.push(("syntax-keyword", "text-weak"));
    }
    let tokens = Tokens {
        background_base: neutral[0].into(),
        background_stronger: pick!(neutral[1], [0xfc, 0xfc, 0xfc]).into(),
        surface_inset_base: neutral_alpha[1].into(),
        surface_raised_base: neutral_alpha[0].into(),
        surface_raised_strong: pick!(neutral_alpha[3], neutral[0]).into(),
        surface_base_hover: neutral_alpha[2].into(),
        surface_base_active: neutral_alpha[2].into(),
        surface_brand_base: scales.primary[8].into(),
        border_weaker_base: border_weaker.into(),
        border_weak_base: border_weak.into(),
        border_base: border_base.into(),
        text_strong: text_strong.into(),
        text_base: text_base.into(),
        text_weak: text_weak.into(),
        text_weaker: text_weaker.into(),
        text_interactive_base: scales.interactive[pick!(10, 9)].into(),
        icon_base: icon_base.into(),
        icon_weak_base: neutral[pick!(5, 6)].into(),
        icon_selected: icon_selected.into(),
        icon_success_base: scales.success[pick!(8, 6)].into(),
        icon_warning_base: scales.amber[pick!(8, 6)].into(),
        icon_critical_base: scales.error[pick!(8, 9)].into(),
        icon_info_base: scales.info[pick!(8, 6)].into(),
        syntax_comment: text_weak.into(),
        syntax_keyword: keyword.into(),
        syntax_string: string.into(),
        syntax_primitive: primitive.into(),
        syntax_property: property.into(),
        syntax_type: kind.into(),
        syntax_constant: constant.into(),
        syntax_operator: pick!(text_weak, text_base).into(),
        syntax_punctuation: pick!(text_weak, text_base).into(),
        syntax_variable: text_strong.into(),
    };
    (tokens, references)
}

/// The palette's colour scales for one variant.
struct Scales {
    primary: [Hex; 12],
    accent: [Hex; 12],
    success: [Hex; 12],
    warning: [Hex; 12],
    /// The warning colour turned towards amber, for warning icons.
    amber: [Hex; 12],
    error: [Hex; 12],
    info: [Hex; 12],
    interactive: [Hex; 12],
}

impl Scales {
    fn new(p: &Palette, dark: bool) -> Self {
        use color::{scale, shift};
        let amber = if dark {
            shift(p.warning, -0.058, 1.14, -16.0)
        } else {
            shift(p.warning, -0.082, 0.94, -22.0)
        };
        Self {
            primary: scale(p.primary, dark),
            accent: scale(p.accent, dark),
            success: scale(p.success, dark),
            warning: scale(p.warning, dark),
            amber: scale(amber, dark),
            error: scale(p.error, dark),
            info: scale(p.info, dark),
            interactive: scale(p.interactive, dark),
        }
    }
}

/// Body, weak, weaker and strong text: a compact theme's from its ink,
/// the older seeds' from the greys.
fn text_colors(p: &Palette, neutral: &[Hex; 12], dark: bool) -> [Hex; 4] {
    use color::{blend, oklch, shift};
    if !p.compact {
        return [neutral[10], neutral[8], neutral[7], neutral[11]];
    }
    let ink = p.ink.unwrap_or(p.neutral);
    let tint = oklch(ink);
    if dark {
        let body = shift(ink, (0.88 - tint.l).max(0.0) * 0.4, 1.04, 0.0);
        [
            body,
            shift(body, -0.11, 0.9, 0.0),
            shift(body, -0.2, 0.78, 0.0),
            blend([255, 255, 255], body, 0.9),
        ]
    } else {
        let body = shift(ink, -(tint.l - 0.18).max(0.0) * 0.24, 1.02, 0.0);
        [
            body,
            shift(body, 0.11, 0.9, 0.0),
            shift(body, 0.21, 0.72, 0.0),
            shift(body, -0.07, 1.04, 0.0),
        ]
    }
}

/// String, keyword, primitive, property, type and constant colours: a
/// compact theme's from its palette, the older seeds' fixed.
fn code_colors(p: &Palette, s: &Scales, text_weak: Hex, dark: bool) -> [Hex; 6] {
    use color::{oklch, shift};
    if !p.compact {
        let fixed = |dark_value: &str, light_value: &str| {
            Rgba::parse(if dark { dark_value } else { light_value }).map_or([0, 0, 0], Rgba::hex)
        };
        return [
            fixed("#00ceb9", "#006656"),
            text_weak,
            fixed("#ffba92", "#fb4804"),
            fixed("#ff9ae2", "#ed6dc8"),
            fixed("#ecf58c", "#596600"),
            fixed("#93e9f6", "#007b80"),
        ];
    }
    // A content colour: the seed's scale, made a little livelier.
    let content = |seed: Hex, steps: &[Hex; 12]| {
        let value = if dark && oklch(seed).l > 0.84 {
            shift(seed, 0.0, 1.18, 0.0)
        } else {
            steps[10]
        };
        if dark {
            shift(value, 0.034, 1.3, 0.0)
        } else {
            shift(value, -0.024, 1.18, 0.0)
        }
    };
    [
        content(p.success, &s.success),
        content(p.accent, &s.accent),
        content(p.primary, &s.primary),
        content(p.info, &s.info),
        content(p.warning, &s.warning),
        content(p.accent, &s.accent),
    ]
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
