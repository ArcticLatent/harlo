//! One-off answers from a model, outside any session's turn: a session's
//! title, a quick side question.

// Rust guideline compliant 2026-10-09

use tokio_util::sync::CancellationToken;

use crate::Error;
use crate::chat::{self, Block};
use crate::config::{ModelRef, ProviderSpec};
use crate::message::{Created, Message, Tokens};

/// What `model` answers to `prompt` with `system` as its instructions, as
/// plain text.
///
/// # Errors
///
/// When the provider can't be reached or refuses, or `stop` is cancelled.
pub async fn generate(
    provider: &ProviderSpec,
    model: &ModelRef,
    system: &str,
    prompt: &str,
    stop: &CancellationToken,
) -> Result<String, Error> {
    let history = [user(prompt)];
    Ok(answer(provider, model, system, &history, stop).await?.text)
}

/// A prompt from the person, as a message of its own.
#[must_use]
pub fn user(prompt: &str) -> Message {
    Message::User {
        id: String::new(),
        time: Created { created: 0 },
        text: prompt.to_owned(),
        skills: Vec::new(),
        files: Vec::new(),
    }
}

/// A model's answer outside a turn.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// What it wrote.
    pub text: String,
    /// What the request used.
    pub tokens: Tokens,
}

/// What `model` answers after `history`, with `system` as its
/// instructions and no tools.
///
/// # Errors
///
/// When the provider can't be reached or refuses, or `stop` is cancelled.
pub async fn answer(
    provider: &ProviderSpec,
    model: &ModelRef,
    system: &str,
    history: &[Message],
    stop: &CancellationToken,
) -> Result<Answer, Error> {
    let (to, body) = chat::prepare(provider, model, system, history, &[]);
    let reply = chat::stream(&to, &body, stop, &mut |_| {}).await?;
    let text = reply
        .blocks
        .into_iter()
        .filter_map(|b| match b {
            Block::Text(text) => Some(text),
            _ => None,
        })
        .collect();
    Ok(Answer {
        text,
        tokens: reply.tokens,
    })
}

/// The instructions for naming a session.
pub const TITLE_SYSTEM: &str = "Name the conversation so the person can find it later in a list \
    of sessions. Reply with the title alone: one line of three to six words that names the \
    specific subject, such as the feature, file, error or tool. No quotes, no label, and no full \
    stop at the end.";

/// The request to name a session that starts with `first`.
#[must_use]
pub fn title_prompt(first: &str) -> String {
    format!(
        "The conversation's first message:\n\n<message>\n{first}\n</message>\n\nReply with the title only."
    )
}

/// A model's title cleaned up: its first line, without quotes, a label or
/// a final full stop, at most 80 characters; none when nothing is left.
#[must_use]
pub fn clean_title(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = line.strip_prefix("Title:").unwrap_or(line).trim();
    let line = line
        .trim_matches(|c| matches!(c, '"' | '\'' | '“' | '”' | '`' | '*'))
        .trim_end_matches('.')
        .trim();
    if line.is_empty() {
        return None;
    }
    let mut title: String = line.chars().take(80).collect();
    if line.chars().count() > 80 {
        title.push('…');
    }
    Some(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_cleaned() {
        assert_eq!(
            clean_title("\n\"Login bug in auth module.\"\nmore").as_deref(),
            Some("Login bug in auth module")
        );
        assert_eq!(
            clean_title("Title: Cache choice").as_deref(),
            Some("Cache choice")
        );
        assert_eq!(clean_title("  \n ").as_deref(), None);
        assert_eq!(clean_title("\"\"").as_deref(), None);
    }
}
