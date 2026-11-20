//! pi's cloud providers in Settings → Models: which ones pi has a key for,
//! and the commands that change that.
//!
//! harlo never reads or writes pi's credentials. Which providers have a key
//! is what `pi --list-models` lists (its first column: a provider pi has no
//! key for has no models there). A key is typed into pi itself, in harlo's
//! terminal (`pi auth login <id> --api-key`, which reads it without
//! echoing), removed with `pi auth logout <id>`, and checked with
//! `pi auth check --provider <id> --json` (which prints no key).

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;

use serde_json::{Value, json};

/// One of pi's providers that takes an API key: its id in pi, its name,
/// the logo harlo draws for it, where to get a key and see its prices.
#[derive(Debug, Clone, Copy)]
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub logo: &'static str,
    pub key_page: &'static str,
    pub price_page: &'static str,
}

const fn p(id: &'static str, name: &'static str, logo: &'static str, key_page: &'static str, price_page: &'static str) -> Provider {
    Provider { id, name, logo, key_page, price_page }
}

/// The providers offered (pi's own ids; the rest of pi's are set up in
/// `models.json` or sign in with an account).
pub const PROVIDERS: &[Provider] = &[
    p("anthropic", "Anthropic", "anthropic", "https://platform.claude.com/settings/keys", "https://platform.claude.com/docs/en/about-claude/pricing"),
    p("openai", "OpenAI", "openai", "https://platform.openai.com/api-keys", "https://platform.openai.com/docs/pricing"),
    p("openrouter", "OpenRouter", "openrouter", "https://openrouter.ai/settings/keys", "https://openrouter.ai/models"),
    p("deepseek", "DeepSeek", "deepseek", "https://platform.deepseek.com/api_keys", "https://api-docs.deepseek.com/quick_start/pricing"),
    p("xai", "xAI", "xai", "https://console.x.ai/team/default/api-keys", "https://docs.x.ai/docs/models"),
    p("google", "Google Gemini", "gemini", "https://aistudio.google.com/apikey", "https://ai.google.dev/gemini-api/docs/pricing"),
    p("mistral", "Mistral", "mistral", "https://console.mistral.ai/api-keys", "https://docs.mistral.ai/inference/pricing"),
    p("groq", "Groq", "groq", "https://console.groq.com/keys", "https://console.groq.com/docs/models"),
    p("cerebras", "Cerebras", "cerebras", "https://cloud.cerebras.ai", "https://inference-docs.cerebras.ai/models/overview"),
    p("together", "Together AI", "together", "https://api.together.ai/settings/api-keys", "https://docs.together.ai/docs/serverless-models"),
    p("fireworks", "Fireworks", "fireworks", "https://app.fireworks.ai/settings/users/api-keys", "https://fireworks.ai/pricing"),
    p("zai", "Z.ai Coding Plan", "zai", "https://z.ai/manage-apikey/apikey-list", "https://docs.z.ai/guides/overview/pricing"),
    p("moonshotai", "Moonshot AI (Kimi)", "moonshot", "https://platform.kimi.ai/console/api-keys", "https://platform.kimi.ai/docs/pricing/chat"),
    p("kimi-coding", "Kimi For Coding", "moonshot", "", ""),
    p("minimax", "MiniMax", "minimax", "https://platform.minimax.io/user-center/basic-information/interface-key", "https://platform.minimax.io/docs/guides/pricing"),
    p("opencode-go", "OpenCode Go", "opencode-go", "https://opencode.ai/console", "https://opencode.ai/docs/go/"),
    p("opencode", "OpenCode Zen", "opencode-go", "https://opencode.ai/console", "https://opencode.ai/docs/zen/"),
    p("huggingface", "Hugging Face", "huggingface", "https://huggingface.co/settings/tokens", ""),
    p("nvidia", "NVIDIA NIM", "nvidia", "https://build.nvidia.com/settings/api-keys", ""),
    p("vercel-ai-gateway", "Vercel AI Gateway", "vercel", "", ""),
    p("baseten", "Baseten", "baseten", "https://app.baseten.co/settings/api_keys", ""),
];

/// Lists pi's models (`$PI` is pi's program, else `pi` on PATH), offline:
/// what has a key is all it's asked.
pub const LIST_SCRIPT: &str = r#"PI_OFFLINE=1 "${PI:-pi}" --list-models 2>&1"#;

/// The models `pi --list-models` printed, by provider (its table: a header
/// row `provider model …`, then one row a model).
pub fn models_by_provider(listing: &str) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut rows = listing.lines().map(str::split_whitespace);
    // (Rows start after the header; a listing without one has no models.)
    if !rows.by_ref().any(|mut r| r.next() == Some("provider") && r.next() == Some("model")) {
        return out;
    }
    for mut r in rows {
        if let (Some(provider), Some(model)) = (r.next(), r.next()) {
            out.entry(provider.to_owned()).or_default().push(model.to_owned());
        }
    }
    out
}

/// The providers as the Models page shows them, from a `pi --list-models`
/// listing: `[{ id, name, maker, key: "saved" | "none", models: [{ id }],
/// keyPage, pricePage }]`.
pub fn list_json(listing: &str) -> Value {
    let models = models_by_provider(listing);
    Value::Array(
        PROVIDERS
            .iter()
            .map(|p| {
                let mine = models.get(p.id).map(Vec::as_slice).unwrap_or_default();
                json!({
                    "id": p.id, "name": p.name, "maker": p.logo,
                    "key": if mine.is_empty() { "none" } else { "saved" },
                    "models": mine.iter().map(|m| json!({ "id": m })).collect::<Vec<_>>(),
                    "keyPage": p.key_page, "pricePage": p.price_page,
                })
            })
            .collect(),
    )
}

/// Whether `id` is one of [`PROVIDERS`] (only those reach pi's commands).
pub fn known(id: &str) -> bool {
    PROVIDERS.iter().any(|p| p.id == id)
}

/// pi's arguments that save a key for `id`, typed in the terminal.
pub fn login_args(id: &str) -> [String; 4] {
    ["auth".into(), "login".into(), id.to_owned(), "--api-key".into()]
}

/// pi's arguments that forget the key it saved for `id`.
pub fn logout_args(id: &str) -> [String; 3] {
    ["auth".into(), "logout".into(), id.to_owned()]
}

/// pi's arguments that check `id`'s key (no key printed).
pub fn check_args(id: &str) -> [String; 5] {
    ["auth".into(), "check".into(), "--provider".into(), id.to_owned(), "--json".into()]
}

/// What `pi auth check --json` said, in words: Ok when the key works, the
/// reason otherwise.
///
/// # Errors
///
/// When pi has no key for it, or the key is invalid.
pub fn check_said(out: &str) -> anyhow::Result<String> {
    let v: Value = out.lines().rev().find_map(|l| serde_json::from_str(l.trim()).ok()).ok_or_else(|| anyhow::anyhow!("pi didn't say: {}", out.trim()))?;
    match v["status"].as_str() {
        Some("ready") => Ok(String::new()),
        Some("invalid") => anyhow::bail!("pi says the key isn't valid ({}).", v["reason"].as_str().unwrap_or("no reason given")),
        _ => anyhow::bail!("pi has no key for it ({}).", v["reason"].as_str().unwrap_or("not ready").replace('_', " ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_which_providers_have_models() {
        let listing = "provider    model           context  max-out  thinking  images\nanthropic   claude-opus-5-5 200K     64K      yes       yes\nanthropic   claude-haiku    200K     64K      yes       yes\nfake        fake-coder      32K      16.4K    no        no\n";
        let by = models_by_provider(listing);
        assert_eq!(by["anthropic"], ["claude-opus-5-5", "claude-haiku"]);
        assert_eq!(by["fake"], ["fake-coder"]);
        assert!(models_by_provider("No models available. Use /login to log into a provider").is_empty());

        let list = list_json(listing);
        let anthropic = list.as_array().unwrap().iter().find(|p| p["id"] == "anthropic").unwrap();
        assert_eq!((anthropic["key"].as_str(), anthropic["models"].as_array().unwrap().len()), (Some("saved"), 2));
        assert!(list.as_array().unwrap().iter().filter(|p| p["id"] != "anthropic").all(|p| p["key"] == "none"));
        // (A local server in models.json isn't one of these.)
        assert!(list.as_array().unwrap().iter().all(|p| p["id"] != "fake"));
    }

    #[test]
    fn reads_pis_check() {
        assert_eq!(check_said(r#"{"status":"ready","provider":"openai","authType":"api_key"}"#).unwrap(), "");
        assert!(check_said(r#"{"status":"not_ready","provider":"anthropic","reason":"credentials_not_configured"}"#).unwrap_err().to_string().contains("credentials not configured"));
        check_said("boom").unwrap_err();
        assert!(known("openrouter") && !known("rm -rf"));
        assert_eq!(login_args("xai").join(" "), "auth login xai --api-key");
    }
}
