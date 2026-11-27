//! Whose logo stands for a session's model: the server's provider IDs
//! mapped to the logos Harlo's window and its phone app both carry.

// Rust guideline compliant 2026-10-09

/// The logos, by name: the window's files in `ui/images/providers` and the
/// phone app's `Maker-` images are called this. The window numbers them
/// in this order.
pub const LOGOS: [&str; 16] = [
    "anthropic",
    "openai",
    "zai-coding-plan",
    "claude-code",
    "google",
    "xai",
    "deepseek",
    "mistral",
    "minimax-coding-plan",
    "kimi-for-coding",
    "groq",
    "cerebras",
    "together",
    "fireworks",
    "openrouter",
    "opencode",
];

/// Provider IDs, or the word they start with before a `-`, and the logo
/// each one's models show. A provider that serves one maker's models
/// under several IDs, such as `zai` and `zai-coding-plan`, is listed once.
const PROVIDERS: [(&str, &str); 18] = [
    ("anthropic", "anthropic"),
    ("openai", "openai"),
    ("zai", "zai-coding-plan"),
    // Claude subscriptions, through the server's integration.
    ("claude", "claude-code"),
    ("google", "google"),
    ("xai", "xai"),
    ("deepseek", "deepseek"),
    ("mistral", "mistral"),
    ("minimax", "minimax-coding-plan"),
    ("moonshotai", "kimi-for-coding"),
    ("kimi", "kimi-for-coding"),
    ("groq", "groq"),
    ("cerebras", "cerebras"),
    ("togetherai", "together"),
    ("together", "together"),
    ("fireworks", "fireworks"),
    ("openrouter", "openrouter"),
    ("opencode", "opencode"),
];

/// Returns the name of the logo for the server's provider `id`, or `None`
/// when Harlo carries none for it (a local model, a provider of its own).
#[must_use]
pub fn logo(id: &str) -> Option<&'static str> {
    PROVIDERS
        .iter()
        .find(|(key, _)| {
            id.strip_prefix(key)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('-'))
        })
        .map(|(_, logo)| *logo)
}

/// Returns the logo's number in the window, from 1, or 0 when provider
/// `id` has none.
#[must_use]
pub fn logo_number(id: &str) -> usize {
    logo(id)
        .and_then(|name| LOGOS.iter().position(|logo| *logo == name))
        .map_or(0, |at| at + 1)
}

/// What people call the provider with the server's ID `id`, or `None`
/// when Harlo carries no logo for it and so knows no name either.
#[must_use]
pub fn name(id: &str) -> Option<&'static str> {
    Some(match logo(id)? {
        "anthropic" => "Anthropic",
        "openai" => "OpenAI",
        "zai-coding-plan" => "Z.ai",
        "claude-code" => "Claude",
        "google" => "Google",
        "xai" => "xAI",
        "deepseek" => "DeepSeek",
        "mistral" => "Mistral",
        "minimax-coding-plan" => "MiniMax",
        "kimi-for-coding" => "Kimi",
        "groq" => "Groq",
        "cerebras" => "Cerebras",
        "together" => "Together",
        "fireworks" => "Fireworks",
        "openrouter" => "OpenRouter",
        "opencode" => "OpenCode",
        _ => return None,
    })
}
