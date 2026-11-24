//! The best-known model providers: a line on what each offers, where its
//! keys are made, where its prices are, and anything to know about its
//! key.
//!
//! Settings → Providers lists the popular ones to connect in one press,
//! and a provider's sign-in page links to its key and price pages. Prices
//! themselves are never shown, only linked. The pages were checked against
//! each provider's docs on 2026-11-16.

// Rust guideline compliant 2026-10-09

/// What Harlo knows of one provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Known {
    /// The server's ID for it (the model catalog's).
    pub id: &'static str,
    /// What it offers, in a line.
    pub about: &'static str,
    /// Where its keys are made.
    pub key_page: &'static str,
    /// Where its prices are.
    pub prices: &'static str,
    /// Anything to know about its key; empty when nothing.
    pub note: &'static str,
    /// Listed under Popular until it's connected.
    pub popular: bool,
    /// Tagged "Recommended" there.
    pub recommended: bool,
}

/// A provider with no note, not popular.
const fn plain(
    id: &'static str,
    about: &'static str,
    key_page: &'static str,
    prices: &'static str,
) -> Known {
    Known {
        id,
        about,
        key_page,
        prices,
        note: "",
        popular: false,
        recommended: false,
    }
}

/// A popular provider with no note.
const fn popular(
    id: &'static str,
    about: &'static str,
    key_page: &'static str,
    prices: &'static str,
) -> Known {
    Known {
        popular: true,
        ..plain(id, about, key_page, prices)
    }
}

/// The providers, popular ones first in the order Popular lists them.
pub const KNOWN: [Known; 16] = [
    popular(
        "openai",
        "GPT models, with your ChatGPT plan or an API key.",
        "https://platform.openai.com/api-keys",
        "https://openai.com/api/pricing/",
    ),
    popular(
        "anthropic",
        "Claude models, with an API key. A Claude plan connects through Claude Code.",
        "https://platform.claude.com/settings/keys",
        "https://platform.claude.com/docs/en/about-claude/pricing",
    ),
    Known {
        note: "A key from an OpenCode Go subscription, made in the OpenCode Console. Each \
               model has its own limits, by the 5 hours, the week and the month.",
        recommended: true,
        ..popular(
            "opencode-go",
            "A low-cost subscription to open coding models.",
            "https://opencode.ai/console",
            "https://opencode.ai/docs/go/",
        )
    },
    popular(
        "openrouter",
        "Most providers' models through one account: sign in, or paste a key.",
        "https://openrouter.ai/settings/keys",
        "https://openrouter.ai/models",
    ),
    popular(
        "google",
        "Gemini models, with a Google AI Studio key.",
        "https://aistudio.google.com/apikey",
        "https://ai.google.dev/gemini-api/docs/pricing",
    ),
    popular(
        "xai",
        "Grok models.",
        "https://console.x.ai/team/default/api-keys",
        "https://docs.x.ai/docs/models",
    ),
    popular(
        "deepseek",
        "DeepSeek's models.",
        "https://platform.deepseek.com/api_keys",
        "https://api-docs.deepseek.com/quick_start/pricing",
    ),
    popular(
        "mistral",
        "Mistral's models.",
        "https://console.mistral.ai/api-keys",
        "https://docs.mistral.ai/inference/pricing",
    ),
    Known {
        note: "A MiniMax M Plan key (sk-cp-…, from your plan's page) works here too.",
        ..plain(
            "minimax",
            "MiniMax's models.",
            "https://platform.minimax.io/user-center/basic-information/interface-key",
            "https://platform.minimax.io/docs/guides/pricing",
        )
    },
    plain(
        "zai",
        "GLM models, from Z.ai.",
        "https://z.ai/manage-apikey/apikey-list",
        "https://docs.z.ai/guides/overview/pricing",
    ),
    plain(
        "moonshotai",
        "Kimi models, from Moonshot AI.",
        "https://platform.kimi.ai/console/api-keys",
        "https://platform.kimi.ai/docs/pricing/chat",
    ),
    plain(
        "groq",
        "Open models, answered fast.",
        "https://console.groq.com/keys",
        "https://console.groq.com/docs/models",
    ),
    plain(
        "cerebras",
        "Open models, answered fast.",
        "https://cloud.cerebras.ai",
        "https://inference-docs.cerebras.ai/models/overview",
    ),
    plain(
        "togetherai",
        "Many open models through one account.",
        "https://api.together.ai/settings/api-keys",
        "https://docs.together.ai/docs/serverless-models",
    ),
    plain(
        "fireworks-ai",
        "Many open models through one account.",
        "https://app.fireworks.ai/settings/users/api-keys",
        "https://fireworks.ai/pricing",
    ),
    plain(
        "opencode",
        "Hand-picked models, Claude, GPT and Gemini among them.",
        "https://opencode.ai/console",
        "https://opencode.ai/docs/zen/",
    ),
];

/// What Harlo knows of provider `id`, if it's one of the best-known.
#[must_use]
pub fn known(id: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|k| k.id == id)
}

/// Provider `id`'s link numbered `which`: 0 its key page, 1 its prices.
#[must_use]
pub fn link(id: &str, which: i32) -> Option<&'static str> {
    let known = known(id)?;
    match which {
        0 => Some(known.key_page),
        1 => Some(known.prices),
        _ => None,
    }
}

/// The host of `url`, as a link's row shows it.
#[must_use]
pub fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split('/').next().unwrap_or(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_provider_is_in_the_catalog() {
        // The catalog the server bundles, by provider ID.
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../../harlo-engine/catalog/models.json"))
                .expect("the bundled catalog");
        for k in KNOWN {
            assert!(catalog.get(k.id).is_some(), "{}", k.id);
            assert!(k.key_page.starts_with("https://"), "{}", k.id);
            assert!(k.prices.starts_with("https://"), "{}", k.id);
        }
    }

    #[test]
    fn links_are_the_key_page_then_the_prices() {
        assert_eq!(
            link("openai", 0),
            Some("https://platform.openai.com/api-keys")
        );
        assert_eq!(link("openai", 1), Some("https://openai.com/api/pricing/"));
        assert_eq!(link("openai", 2), None);
        assert_eq!(link("nobody", 0), None);
        assert_eq!(
            host("https://platform.openai.com/api-keys"),
            "platform.openai.com"
        );
        assert_eq!(host("https://cloud.cerebras.ai"), "cloud.cerebras.ai");
    }
}
