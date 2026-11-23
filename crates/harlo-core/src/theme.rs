//! Colour schemes in Tinted Theming's Base16 format, resolved to Harlo's tokens.
//!
//! A scheme is sixteen colours, `base00` to `base0F`, chosen by its designer,
//! and is either dark or light. `base00` to `base07` run from the background
//! to the brightest text; `base08` to `base0F` are its accents (red, orange,
//! yellow, green, cyan, blue, purple, brown). Harlo gives each the role Tinted
//! Theming's styling guide gives it, so a theme keeps its own background,
//! text and accents: <https://github.com/tinted-theming/home/blob/main/styling.md>.
//!
//! Where a scheme's own colour would be too faint to read in Harlo's window
//! (a hint against its background, say), it's moved towards the text colour
//! until it reads, and is otherwise used as it is.
//!
//! The bundled schemes are Harlo's own dark and light ones and a choice of
//! widely used families from <https://github.com/tinted-theming/schemes>, under
//! the MIT licence in `themes/TINTED-LICENSE`; `tools/themes-update.sh`
//! updates them.

// Rust guideline compliant 2026-10-09

use std::backtrace::Backtrace;
use std::collections::BTreeMap;
use std::fmt::{self, Display};

use serde::{Deserialize, Serialize};

/// Harlo's own schemes, a dark and a light one.
const HARLO_SCHEMES: &str = include_str!("../themes/harlo.json");

/// The bundled Tinted Theming schemes.
const TINTED_SCHEMES: &str = include_str!("../themes/tinted.json");

/// The licence of the bundled Tinted Theming schemes, which goes with them.
pub const TINTED_LICENSE: &str = include_str!("../themes/TINTED-LICENSE");

/// The id of Harlo's own dark scheme, the default dark theme.
pub const HARLO_DARK: &str = "harlo-dark";

/// The id of Harlo's own light scheme, the default light theme.
pub const HARLO_LIGHT: &str = "harlo-light";

/// How much black darkens a dark scheme's background for the sidebar and
/// rails.
const SIDEBAR_SHADE: f32 = 0.1;

/// How much white lightens a light scheme's background for the sidebar and
/// rails.
const SIDEBAR_LIGHT: f32 = 0.8;

/// How opaque the text colour's wash under a hovered item is.
const HOVER_WASH: f32 = 0.08;

/// How opaque the text colour's wash under a pressed item is.
const ACTIVE_WASH: f32 = 0.14;

/// How much of the text colour a blue fill takes under the pointer.
const FILL_HOVER: f32 = 0.15;

/// How much of the blue a faint fill, such as behind the selected item,
/// takes over the background.
const WEAK_FILL: f32 = 0.2;

/// How much of the blue an outline in it takes over the background, at rest
/// and under the pointer.
const OUTLINE_REST: f32 = 0.6;

/// See [`OUTLINE_REST`].
const OUTLINE_HOVER: f32 = 0.8;

/// The contrast text on a filled button needs: WCAG's minimum for text.
const BODY_CONTRAST: f64 = 4.5;

/// How much contrast each kind of text needs against the surfaces it sits on.
#[derive(Debug, Clone, Copy)]
struct Needs {
    /// The strongest text.
    body: f64,
    /// Ordinary text: a step above secondary text, so the two stay apart.
    text: f64,
    /// Secondary text: notes, hints, times. It's small, so it needs as much
    /// as body text does.
    secondary: f64,
    /// Placeholders and code comments, against the fields and wells they
    /// sit in.
    faint: f64,
    /// Icons and other graphics.
    icons: f64,
}

/// Standard contrast: WCAG AA's minimums for text (4.5), every level of it,
/// and for graphics (3).
const STANDARD: Needs = Needs {
    body: 4.5,
    text: 4.5,
    secondary: 4.5,
    faint: 4.5,
    icons: 3.0,
};

/// More contrast: WCAG AAA's minimum for text (7), and more for every
/// fainter level.
const MORE: Needs = Needs {
    body: 7.0,
    text: 7.0,
    secondary: 6.0,
    faint: 4.5,
    icons: 4.5,
};

/// How many steps a colour takes towards the text colour when it's too
/// faint; each moves it a tenth of the way.
const READABLE_STEPS: u8 = 10;

/// How far a button's or chip's fill may stand out from the background.
///
/// Most schemes' selection grey sits 1.1 to 1.7 from their background; a
/// few pick a mid grey, which would make every button heavy.
const FILL_CONTRAST: f64 = 1.7;

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

    /// Fully opaque white.
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
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

    /// How well this colour and `other` read against each other.
    ///
    /// WCAG's contrast ratio of their relative luminances, from 1 (the same)
    /// to 21 (black and white). Opacity is ignored.
    #[must_use]
    pub fn contrast(self, other: Self) -> f64 {
        let (x, y) = (self.luminance(), other.luminance());
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
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

    /// WCAG's relative luminance, from 0 (black) to 1 (white).
    fn luminance(self) -> f64 {
        let lift = |v: f32| {
            let v = f64::from(v.clamp(0.0, 1.0));
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.0722f64.mul_add(
            lift(self.b),
            0.2126f64.mul_add(lift(self.r), 0.7152 * lift(self.g)),
        )
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

/// How strongly text stands out from the background.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Contrast {
    /// The scheme's own colours, lifted only where they'd be hard to read.
    #[default]
    Standard,
    /// Text, secondary text and hints lifted further towards black or white.
    More,
}

impl Contrast {
    /// How much contrast each kind of text needs at this level.
    fn needs(self) -> Needs {
        match self {
            Self::Standard => STANDARD,
            Self::More => MORE,
        }
    }
}

/// The colours added and removed lines take in diffs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiffColors {
    /// The scheme's green and red.
    #[default]
    GreenRed,
    /// The scheme's blue and orange, which people who can't tell green from
    /// red can.
    BlueOrange,
}

/// Choices that change how a scheme's colours are resolved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Look {
    /// How strongly text stands out.
    pub contrast: Contrast,
    /// The diff colours.
    pub diff: DiffColors,
}

/// Whether a scheme, or the window, is light or dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Dark text on a light background.
    Light,
    /// Light text on a dark background.
    Dark,
}

macro_rules! tokens {
    ($( $(#[$doc:meta])* $field:ident = $name:literal ),* $(,)?) => {
        /// Every colour Harlo's interface uses, resolved from one scheme.
        ///
        /// Each also has a name, such as `text-weak` for `text_weak`, which
        /// [`Tokens::get`] looks up.
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct Tokens {
            $( $(#[$doc])* pub $field: Rgba, )*
        }

        impl Tokens {
            /// The colour named `name`, such as `text-weak`.
            #[must_use]
            pub fn get(&self, name: &str) -> Option<Rgba> {
                match name {
                    $( $name => Some(self.$field), )*
                    _ => None,
                }
            }

            /// Every colour's name, in the order they're declared.
            #[must_use]
            pub fn names() -> &'static [&'static str] {
                &[$( $name, )*]
            }
        }
    };
}

tokens! {
    /// The main background: the conversation and side panels.
    background_base = "background-base",
    /// The background of the rails and the sidebar.
    background_stronger = "background-stronger",
    /// Sunken fields, such as the session filter, and code blocks.
    surface_inset_base = "surface-inset-base",
    /// Raised areas, such as the composer.
    surface_raised_base = "surface-raised-base",
    /// Buttons, chips and pickers.
    surface_raised_strong = "surface-raised-strong",
    /// The wash under a hovered item.
    surface_base_hover = "surface-base-hover",
    /// The wash under a pressed or selected item.
    surface_base_active = "surface-base-active",
    /// The main action's fill, such as the send button.
    surface_brand_base = "surface-brand-base",
    /// The main action's fill under the pointer.
    surface_brand_hover = "surface-brand-hover",
    /// An option turned on, such as a switch.
    surface_interactive_base = "surface-interactive-base",
    /// An option turned on, under the pointer.
    surface_interactive_hover = "surface-interactive-hover",
    /// A faint fill in the interactive colour.
    surface_interactive_weak = "surface-interactive-weak",
    /// The scheme's selection colour, such as behind the selected session.
    surface_selected = "surface-selected",
    /// The faintest separators.
    border_weaker_base = "border-weaker-base",
    /// Outlines of raised areas and panel dividers.
    border_weak_base = "border-weak-base",
    /// Stronger outlines.
    border_base = "border-base",
    /// Headings and the most important text.
    text_strong = "text-strong",
    /// Body text.
    text_base = "text-base",
    /// Secondary text, such as ages and hints.
    text_weak = "text-weak",
    /// Placeholders and disabled text.
    text_weaker = "text-weaker",
    /// Links and other text that acts.
    text_interactive_base = "text-interactive-base",
    /// Text on the main action's fill.
    text_on_brand_base = "text-on-brand-base",
    /// Text on an option turned on.
    text_on_interactive_base = "text-on-interactive-base",
    /// Outlines in the interactive colour.
    border_interactive_base = "border-interactive-base",
    /// Interactive outlines under the pointer.
    border_interactive_hover = "border-interactive-hover",
    /// The outline of a focused field.
    border_interactive_focus = "border-interactive-focus",
    /// The scheme's second accent: the selected item's marker, the open tab,
    /// the working spinner.
    accent_base = "accent-base",
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
    /// Added lines and counts in diffs.
    diff_added = "diff-added",
    /// Removed lines and counts in diffs.
    diff_removed = "diff-removed",
    /// Information.
    icon_info_base = "icon-info-base",
    /// Headings in replies.
    markdown_heading = "markdown-heading",
    /// Bold text in replies.
    markdown_strong = "markdown-strong",
    /// Italic text in replies.
    markdown_emph = "markdown-emph",
    /// Inline code in replies.
    markdown_code = "markdown-code",
    /// Quotes in replies.
    markdown_quote = "markdown-quote",
    /// Links in replies.
    markdown_link = "markdown-link",
    /// Comments in code.
    syntax_comment = "syntax-comment",
    /// Keywords, such as `fn` and `if`.
    syntax_keyword = "syntax-keyword",
    /// Strings.
    syntax_string = "syntax-string",
    /// Functions and methods.
    syntax_function = "syntax-function",
    /// Properties, attributes and built-ins.
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

/// A Base16 colour scheme.
#[derive(Debug, Clone, PartialEq)]
pub struct Scheme {
    /// The slug, such as `gruvbox-dark-medium`.
    pub id: String,
    /// The name people see, such as `Gruvbox Dark`.
    pub name: String,
    /// Who made it, as the scheme credits them.
    pub author: String,
    /// Whether it's a dark or a light scheme.
    pub mode: Mode,
    /// `base00` to `base0F`.
    palette: [Rgba; 16],
}

impl Scheme {
    /// Returns colour `base0N` for `n` from 0 to 15.
    ///
    /// # Panics
    ///
    /// Panics if `n` is 16 or more: a scheme has sixteen colours.
    #[must_use]
    pub fn base(&self, n: usize) -> Rgba {
        self.palette[n]
    }

    /// Whether this is one of Harlo's own schemes.
    #[must_use]
    pub fn is_harlo(&self) -> bool {
        self.id == HARLO_DARK || self.id == HARLO_LIGHT
    }

    /// Resolves every colour Harlo uses from the scheme's sixteen, with
    /// the standard look.
    #[must_use]
    pub fn tokens(&self) -> Tokens {
        self.tokens_with(Look::default())
    }

    /// Resolves every colour Harlo uses from the scheme's sixteen, with
    /// `look`'s contrast and diff colours.
    #[must_use]
    pub fn tokens_with(&self, look: Look) -> Tokens {
        derive(&self.palette, self.mode, look)
    }
}

/// Every bundled scheme: Harlo's dark and light ones first, then the rest
/// by name.
///
/// # Panics
///
/// Panics if a bundled file doesn't parse. The tests check they all do, so
/// this would be a build mistake.
#[must_use]
pub fn bundled() -> Vec<Scheme> {
    let mut schemes = parse(HARLO_SCHEMES).expect("Harlo's schemes are valid");
    let mut tinted = parse(TINTED_SCHEMES).expect("the bundled schemes are valid");
    tinted.sort_by_cached_key(|scheme| scheme.name.to_lowercase());
    schemes.extend(tinted);
    schemes
}

/// Harlo's own scheme for `mode`.
///
/// # Panics
///
/// Panics if Harlo's bundled schemes don't parse. The tests check they do.
#[must_use]
pub fn harlo(mode: Mode) -> Scheme {
    let id = match mode {
        Mode::Dark => HARLO_DARK,
        Mode::Light => HARLO_LIGHT,
    };
    parse(HARLO_SCHEMES)
        .expect("Harlo's schemes are valid")
        .into_iter()
        .find(|scheme| scheme.id == id)
        .expect("Harlo's schemes hold a dark and a light one")
}

/// A scheme as its file gives it.
#[derive(Deserialize)]
struct FileScheme {
    id: String,
    name: String,
    #[serde(default)]
    author: String,
    variant: FileVariant,
    palette: BTreeMap<String, String>,
}

/// A scheme's `variant` field.
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum FileVariant {
    Dark,
    Light,
}

/// Reads a JSON list of schemes, each with its `id`, `name`, `author`,
/// `variant` (`dark` or `light`) and a `palette` of `base00` to `base0F`.
///
/// # Errors
///
/// Returns an error if the text isn't such a list, or a palette lacks a
/// colour or has one that isn't a hex colour.
pub fn parse(json: &str) -> Result<Vec<Scheme>, ThemeError> {
    let files: Vec<FileScheme> = serde_json::from_str(json)?;
    files.into_iter().map(scheme).collect()
}

/// Checks a scheme's palette and turns it into a [`Scheme`].
fn scheme(file: FileScheme) -> Result<Scheme, ThemeError> {
    let mut palette = [Rgba::BLACK; 16];
    for (n, slot) in palette.iter_mut().enumerate() {
        let name = format!("base{n:02X}");
        let value = file.palette.get(&name).map_or("", String::as_str);
        *slot = Rgba::parse(value).ok_or_else(|| ThemeError::bad_color(&file.id, &name, value))?;
    }
    Ok(Scheme {
        id: file.id,
        name: file.name,
        author: file.author,
        mode: match file.variant {
            FileVariant::Dark => Mode::Dark,
            FileVariant::Light => Mode::Light,
        },
        palette,
    })
}

/// Gives each of Harlo's colours its Base16 role.
fn derive(p: &[Rgba; 16], mode: Mode, look: Look) -> Tokens {
    let needs = look.contrast.needs();
    let [
        b0,
        b1,
        b2,
        b3,
        b4,
        b5,
        b6,
        b7,
        red,
        orange,
        yellow,
        green,
        cyan,
        blue,
        purple,
        _brown,
    ] = *p;
    let stronger = match mode {
        Mode::Dark => Rgba::BLACK.over(b0, SIDEBAR_SHADE),
        Mode::Light => Rgba::WHITE.over(b0, SIDEBAR_LIGHT),
    };
    let backgrounds = [b0, stronger, b1];
    // Text also sits on raised fills: menus, sheets and buttons.
    let fill = subtle(b2, b1, b0);
    let surfaces = [b0, stronger, b1, fill];
    // The strongest text is the scheme's light foreground, unless it reads
    // worse than the default one.
    let strong = if b6.contrast(b0) >= b5.contrast(b0) {
        b6
    } else {
        b5
    };
    // Each fainter colour falls back on the one above it, and the strongest
    // on black or white, so every one can be made to read.
    let extreme = match mode {
        Mode::Dark => Rgba::WHITE,
        Mode::Light => Rgba::BLACK,
    };
    let strong = readable(strong, extreme, needs.body.max(needs.text), &surfaces);
    let text = readable(b5, strong, needs.text, &surfaces);
    let weak = readable(b4, text, needs.secondary, &surfaces);
    let weaker = readable(b3, weak, needs.faint, &backgrounds);
    let comment = readable(b3, weak, needs.faint, &[b1]);
    let (added, removed) = match look.diff {
        DiffColors::GreenRed => (green, red),
        DiffColors::BlueOrange => (blue, orange),
    };
    let hover = b5.over(blue, FILL_HOVER);
    let on_blue = on(blue, b0, b7);
    Tokens {
        background_base: b0,
        background_stronger: stronger,
        surface_inset_base: b1,
        surface_raised_base: b1,
        surface_raised_strong: fill,
        surface_base_hover: b5.alpha(HOVER_WASH),
        surface_base_active: b5.alpha(ACTIVE_WASH),
        surface_brand_base: blue,
        surface_brand_hover: hover,
        surface_interactive_base: blue,
        surface_interactive_hover: hover,
        surface_interactive_weak: blue.over(b0, WEAK_FILL),
        surface_selected: fill,
        border_weaker_base: b1,
        border_weak_base: b2,
        border_base: b3,
        text_strong: strong,
        text_base: text,
        text_weak: weak,
        text_weaker: weaker,
        text_interactive_base: blue,
        text_on_brand_base: on_blue,
        text_on_interactive_base: on_blue,
        border_interactive_base: blue.over(b0, OUTLINE_REST),
        border_interactive_hover: blue.over(b0, OUTLINE_HOVER),
        border_interactive_focus: blue,
        accent_base: purple,
        icon_base: readable(b4, text, needs.icons, &surfaces),
        icon_weak_base: weaker,
        icon_selected: strong,
        icon_success_base: green,
        icon_warning_base: yellow,
        icon_critical_base: red,
        diff_added: added,
        diff_removed: removed,
        icon_info_base: cyan,
        markdown_heading: blue,
        markdown_strong: yellow,
        markdown_emph: purple,
        markdown_code: green,
        markdown_quote: cyan,
        markdown_link: blue,
        syntax_comment: comment,
        syntax_keyword: purple,
        syntax_string: green,
        syntax_function: blue,
        syntax_property: cyan,
        syntax_type: yellow,
        syntax_constant: orange,
        syntax_operator: b5,
        syntax_punctuation: b5,
        syntax_variable: red,
    }
}

/// `fill` where it stands out from `background` by at most
/// [`FILL_CONTRAST`], else moved towards `toward` until it does.
fn subtle(fill: Rgba, toward: Rgba, background: Rgba) -> Rgba {
    std::iter::once(fill)
        .chain(
            (1..=READABLE_STEPS)
                .map(|step| toward.over(fill, f32::from(step) / f32::from(READABLE_STEPS))),
        )
        .find(|c| c.contrast(background) <= FILL_CONTRAST)
        .unwrap_or(toward)
}

/// `color` where it reads against every background in `against` by at least
/// `min`, else moved towards `toward` until it does.
fn readable(color: Rgba, toward: Rgba, min: f64, against: &[Rgba]) -> Rgba {
    let reads = |c: Rgba| against.iter().all(|&back| c.contrast(back) >= min);
    std::iter::once(color)
        .chain(
            (1..=READABLE_STEPS)
                .map(|step| toward.over(color, f32::from(step) / f32::from(READABLE_STEPS))),
        )
        .find(|&c| reads(c))
        .unwrap_or(toward)
}

/// The text colour for a `fill`: the scheme's background or brightest
/// colour, whichever reads better, or black or white when neither reads.
fn on(fill: Rgba, background: Rgba, brightest: Rgba) -> Rgba {
    let best = |a: Rgba, b: Rgba| {
        if a.contrast(fill) >= b.contrast(fill) {
            a
        } else {
            b
        }
    };
    let own = best(background, brightest);
    if own.contrast(fill) >= BODY_CONTRAST {
        own
    } else {
        best(Rgba::BLACK, Rgba::WHITE)
    }
}

/// Reading a scheme failed.
#[derive(Debug)]
pub struct ThemeError {
    kind: ErrorKind,
    backtrace: Backtrace,
}

#[derive(Debug)]
enum ErrorKind {
    Json(serde_json::Error),
    BadColor {
        scheme: String,
        name: String,
        value: String,
    },
}

impl ThemeError {
    fn bad_color(scheme: &str, name: &str, value: &str) -> Self {
        Self {
            kind: ErrorKind::BadColor {
                scheme: scheme.to_owned(),
                name: name.to_owned(),
                value: value.to_owned(),
            },
            backtrace: Backtrace::capture(),
        }
    }

    /// Whether the text wasn't a list of schemes at all.
    #[must_use]
    pub fn is_format(&self) -> bool {
        matches!(self.kind, ErrorKind::Json(_))
    }

    /// Whether a palette lacked a colour or had one that wasn't a hex colour.
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
            ErrorKind::Json(e) => write!(f, "not a list of colour schemes: {e}"),
            ErrorKind::BadColor {
                scheme,
                name,
                value,
            } => write!(f, "scheme {scheme:?}: {name} isn't a hex colour: {value:?}"),
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
