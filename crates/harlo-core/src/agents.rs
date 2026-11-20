//! How harlo starts the agents: their arguments, and what each supports.

// Rust guideline compliant 2026-10-09

use crypto_box::aead::OsRng;
use crypto_box::aead::rand_core::RngCore;
use harlo_harness::HarnessKind;

/// Whether an agent's sessions have a shield (harlo's approval modes).
pub fn has_shield(harness: &str) -> bool {
    matches!(harness, "demo" | "pi")
}

/// An agent's name as people read it ("demo" → "Demo").
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
    fn names_shields_and_tokens() {
        assert_eq!(label("demo"), "Demo");
        assert_eq!(label("someone"), "someone");
        assert!(has_shield("pi") && !has_shield("harlo") && !has_shield("claude"));
        let (a, b) = (session_token(), session_token());
        assert!(a.len() == 32 && a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
