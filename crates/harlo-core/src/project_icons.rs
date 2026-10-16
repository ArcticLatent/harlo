//! Project icons (Settings → Projects): the preset symbols.

/// The preset icons (names in the app's icon set), in the picker's order.
pub const PRESETS: &[&str] = &[
    "code-box", "terminal-box", "rocket", "flask", "gamepad", "briefcase", "home", "global", "leaf", "shield", "palette",
    "server", "smartphone", "database-2", "lightbulb", "music", "camera", "book-open", "heart",
];

/// A stored icon as shown: a preset's name, or "" (the folder) for anything
/// else.
pub fn shown(icon: &str) -> &str {
    if PRESETS.contains(&icon) { icon } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_values() {
        assert_eq!(shown("rocket"), "rocket");
        assert_eq!(shown("file:3-1.png"), "");
        assert_eq!(shown(""), "");
    }
}
