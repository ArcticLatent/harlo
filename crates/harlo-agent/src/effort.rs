//! A turn's thinking level: set by a word in the prompt, or judged.
//!
//! `ultrathink` in a prompt thinks harder for that turn, `chillax` briefly,
//! and `orchestrate` splits the work across subagents; each also leaves the
//! model a note saying so. With `HARLO_AGENT_AUTO_EFFORT=on`, a prompt with
//! none of them is judged by one small request (how much reasoning it needs)
//! and its turn runs at that level. The session's own level comes back
//! after the turn.

// Rust guideline compliant 2026-10-09

/// A word in a prompt that changes how its turn goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Word {
    /// Think harder: the highest level for the turn.
    Ultrathink,
    /// Be brief: the lowest level for the turn.
    Chillax,
    /// Split the work across subagents and check each part.
    Orchestrate,
}

impl Word {
    /// The level its turn runs at (None: the session's).
    pub fn effort(self) -> Option<&'static str> {
        match self {
            Self::Ultrathink => Some("high"),
            Self::Chillax => Some("low"),
            Self::Orchestrate => None,
        }
    }

    /// The note the model gets with the prompt.
    pub fn note(self) -> &'static str {
        match self {
            Self::Ultrathink => "[harlo] ultrathink: this turn runs at a higher thinking level. Think the problem through before acting, consider other explanations, and check your conclusions against the code.",
            Self::Chillax => "[harlo] chillax: keep this turn light. Do the smallest thing that answers, with few tool calls, and reply briefly.",
            Self::Orchestrate => "[harlo] orchestrate: split the work into independent parts and run them together with the task tool (each part's prompt says everything it needs); check each part's result before going on, and finish with the whole picture.",
        }
    }
}

/// The word in `prompt`, if it has one (whole words, any case; the first).
pub fn word(prompt: &str) -> Option<Word> {
    prompt.split(|c: char| !c.is_alphanumeric()).find_map(|w| match w.to_lowercase().as_str() {
        "ultrathink" => Some(Word::Ultrathink),
        "chillax" => Some(Word::Chillax),
        "orchestrate" => Some(Word::Orchestrate),
        _ => None,
    })
}

/// Whether prompts are judged for their level (opt-in).
pub fn auto() -> bool {
    std::env::var("HARLO_AGENT_AUTO_EFFORT").is_ok_and(|v| v.trim() == "on")
}

/// How much of a prompt the judge reads, at most.
pub const JUDGED_CHARS: usize = 4_000;

/// The judging request's instructions.
pub const INSTRUCTIONS: &str = "You rate coding requests for how much reasoning they need. Reply with one word:\n\
    low: a lookup, a direct question, a small obvious change (rename, one-line fix, add a field).\n\
    medium: a typical change or investigation across a few files.\n\
    high: a subtle bug, a change whose design must be worked out, work across many parts.";

/// The level a judge's reply names (None: it named none).
pub fn judged(reply: &str) -> Option<&'static str> {
    let first = reply.split_whitespace().next()?.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    match first.as_str() {
        "low" => Some("low"),
        "medium" => Some("medium"),
        "high" => Some("high"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_found_whole_and_in_any_case() {
        assert_eq!(word("Fix the parser. ULTRATHINK"), Some(Word::Ultrathink));
        assert_eq!(word("chillax, just rename it"), Some(Word::Chillax));
        assert_eq!(word("orchestrate: add tests for each module"), Some(Word::Orchestrate));
        assert_eq!(word("an orchestrated rollout"), None);
        assert_eq!(word("fix it"), None);
        assert_eq!((Word::Ultrathink.effort(), Word::Chillax.effort(), Word::Orchestrate.effort()), (Some("high"), Some("low"), None));
    }

    #[test]
    fn a_judges_reply_names_a_level() {
        assert_eq!(judged("Low"), Some("low"));
        assert_eq!(judged("high."), Some("high"));
        assert_eq!(judged("**medium** because"), Some("medium"));
        assert_eq!(judged("it depends"), None);
        assert_eq!(judged(""), None);
    }
}
