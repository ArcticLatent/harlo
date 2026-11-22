//! Key presses as the window reports them: the key's text, which Slint gives
//! as the character typed or a private-use character for a named key, and
//! the modifiers held.

// Rust guideline compliant 2026-10-09

/// A key, as far as Harlo's shortcuts tell keys apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// A key that types this character, lowercased.
    Character(char),
    /// Tab.
    Tab,
    /// Enter.
    Enter,
    /// Escape.
    Escape,
    /// The arrow keys.
    Left,
    /// Right arrow.
    Right,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Any other key.
    Other,
}

/// The text Slint reports for its named keys (see `slint::platform::Key`).
mod named {
    pub const TAB: char = '\t';
    pub const BACKTAB: char = '\u{19}';
    pub const ENTER: char = '\n';
    pub const RETURN: char = '\r';
    pub const ESCAPE: char = '\u{1b}';
    pub const LEFT: char = '\u{F702}';
    pub const RIGHT: char = '\u{F703}';
    pub const UP: char = '\u{F700}';
    pub const DOWN: char = '\u{F701}';
    pub const PAGE_UP: char = '\u{F72C}';
    pub const PAGE_DOWN: char = '\u{F72D}';
}

/// A key press, with the modifiers held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPress {
    /// The key.
    pub key: Key,
    /// Control (Command on macOS).
    pub command: bool,
    /// Shift.
    pub shift: bool,
    /// Alt.
    pub alt: bool,
}

impl KeyPress {
    /// Reads a key press from the text Slint reports and the modifiers.
    #[must_use]
    pub fn new(text: &str, command: bool, shift: bool, alt: bool) -> Self {
        let mut chars = text.chars();
        let key = match (chars.next(), chars.next()) {
            (Some(c), None) => match c {
                named::TAB | named::BACKTAB => Key::Tab,
                named::ENTER | named::RETURN => Key::Enter,
                named::ESCAPE => Key::Escape,
                named::LEFT => Key::Left,
                named::RIGHT => Key::Right,
                named::UP => Key::Up,
                named::DOWN => Key::Down,
                named::PAGE_UP => Key::PageUp,
                named::PAGE_DOWN => Key::PageDown,
                c if !c.is_control() => Key::Character(c.to_lowercase().next().unwrap_or(c)),
                _ => Key::Other,
            },
            _ => Key::Other,
        };
        Self {
            key,
            command,
            shift,
            alt,
        }
    }
}
