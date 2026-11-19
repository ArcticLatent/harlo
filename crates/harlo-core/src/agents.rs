//! How harlo starts the agents: their arguments, and what each supports.

// Rust guideline compliant 2026-10-09

use crypto_box::aead::OsRng;
use crypto_box::aead::rand_core::RngCore;
use harlo_harness::HarnessKind;

/// Harlo's arguments: harlo's tools, and its sandbox when it isn't the
/// default (`sandbox`, `network`: Settings → Agents → Harlo).
///
/// When orchestration is off, harlo says so and Harlo has no tools.
/// Other agents get none.
pub fn harlo_args(kind: HarnessKind, sandbox: bool, network: bool) -> Vec<String> {
    if kind != HarnessKind::Harlo {
        return Vec::new();
    }
    let mut args = vec!["--harlo-tools".to_owned()];
    match (sandbox, network) {
        (false, _) => args.extend(["--sandbox".into(), "off".into()]),
        (true, false) => args.extend(["--sandbox".into(), "no-network".into()]),
        (true, true) => {}
    }
    args
}

/// Harlo's subagents' model, the one it goes on with at the plan's limit,
/// and the one reviewing its turns, as its arguments.
///
/// An empty setting, or one that isn't a single model name, gives none.
pub fn model_args(task_model: &str, fallback: &str, advisor: &str) -> Vec<String> {
    let mut args = Vec::new();
    for (flag, value) in [("--task-model", task_model.trim()), ("--fallback", fallback.trim()), ("--advisor", advisor.trim())] {
        if !value.is_empty() && !value.contains(char::is_whitespace) {
            args.extend([flag.to_owned(), value.to_owned()]);
        }
    }
    args
}

/// Whether an agent's sessions have a shield (harlo's approval modes).
pub fn has_shield(harness: &str) -> bool {
    matches!(harness, "demo" | "claude" | "harlo")
}

/// An agent's name as people read it ("claude" → "Claude").
pub fn label(agent: &str) -> &str {
    HarnessKind::parse(agent).map_or(agent, |k| k.label())
}

/// A new session's `HARLO_SESSION` token: 128 random bits, in hex.
///
/// Its agent's harlo tools say it, so harlo knows who calls.
pub fn session_token() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harlos_sandbox_goes_on_its_command_line() {
        assert_eq!(harlo_args(HarnessKind::Harlo, true, true), ["--harlo-tools"]);
        assert_eq!(harlo_args(HarnessKind::Harlo, true, false), ["--harlo-tools", "--sandbox", "no-network"]);
        assert_eq!(harlo_args(HarnessKind::Harlo, false, true), ["--harlo-tools", "--sandbox", "off"]);
        assert!(harlo_args(HarnessKind::Claude, false, false).is_empty());
        assert_eq!(model_args(" codex/gpt-6.1-mini ", "", ""), ["--task-model", "codex/gpt-6.1-mini"]);
        assert_eq!(model_args("", "local/qwen", ""), ["--fallback", "local/qwen"]);
        assert_eq!(model_args("", "", "codex/gpt-6.2-sol"), ["--advisor", "codex/gpt-6.2-sol"]);
        assert!(model_args("", "not a model", "").is_empty());
    }

    #[test]
    fn names_shields_and_tokens() {
        assert_eq!(label("claude"), "Claude");
        assert_eq!(label("someone"), "someone");
        assert!(has_shield("harlo") && has_shield("claude") && !has_shield("pi"));
        let (a, b) = (session_token(), session_token());
        assert!(a.len() == 32 && a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
