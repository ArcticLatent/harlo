//! Conversation starters, as older sessions saved them.
//!
//! The new-session page once offered starters, which sent their
//! instructions inside the user message as a trailing
//! `<harlo-starter id="…">…</harlo-starter>` block. Sessions begun that
//! way still have the block; it's left out when they are shown or searched.

// Rust guideline compliant 2026-10-09

/// The message without a trailing starter block.
///
/// Text with no such block comes back unchanged.
///
/// # Examples
///
/// ```
/// use harlo_core::starters::strip;
///
/// let sent = "Fix it\n\n<harlo-starter id=\"debug\">\nFind the cause.\n</harlo-starter>";
/// assert_eq!(strip(sent), "Fix it");
/// assert_eq!(strip("plain"), "plain");
/// ```
#[must_use]
pub fn strip(text: &str) -> &str {
    const OPEN: &str = "<harlo-starter id=\"";
    let trimmed = text.trim_end();
    if !trimmed.ends_with("</harlo-starter>") {
        return text;
    }
    trimmed.rfind(OPEN).map_or(text, |start| trimmed[..start].trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_an_old_starter_block() {
        let sent = "Give me a tour.\n\n<harlo-starter id=\"explore\">\nLook around first.\n</harlo-starter>\n";
        assert_eq!(strip(sent), "Give me a tour.");
        assert_eq!(strip("ends with </harlo-starter>"), "ends with </harlo-starter>");
        assert_eq!(strip("plain"), "plain");
    }
}
