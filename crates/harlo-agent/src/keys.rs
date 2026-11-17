//! API keys for the built-in providers, kept as the ChatGPT sign-in is:
//! `keys.json` in harlo-agent's config folder, readable only by the user
//! (`{ "openrouter": "sk-or-…" }`). harlo's Settings writes it through
//! `harlo-agent key set`, the key on stdin, so it never shows in a process
//! list.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::auth;

fn path() -> PathBuf {
    auth::config_dir().join("keys.json")
}

/// Every key saved in `file`, by provider (none when it's missing or
/// unreadable).
fn all(file: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// The key saved for `provider`, if any.
pub fn get(provider: &str) -> Option<String> {
    get_in(&path(), provider)
}

fn get_in(file: &Path, provider: &str) -> Option<String> {
    all(file).remove(provider).filter(|k| !k.trim().is_empty())
}

/// Saves `key` for `provider`, replacing any before it.
///
/// # Errors
///
/// When the key is empty or has spaces in it, or the file can't be written.
pub fn set(provider: &str, key: &str) -> Result<()> {
    auth::ensure_dir()?;
    set_in(&path(), provider, key)
}

fn set_in(file: &Path, provider: &str, key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() {
        bail!("The key is empty");
    }
    if key.chars().any(char::is_whitespace) {
        bail!("A key has no spaces in it: this looks like more than a key");
    }
    let mut keys = all(file);
    keys.insert(provider.to_owned(), key.to_owned());
    write(file, &keys)
}

/// Forgets `provider`'s key; whether there was one.
///
/// # Errors
///
/// When the file can't be written.
pub fn remove(provider: &str) -> Result<bool> {
    remove_in(&path(), provider)
}

fn remove_in(file: &Path, provider: &str) -> Result<bool> {
    let mut keys = all(file);
    if keys.remove(provider).is_none() {
        return Ok(false);
    }
    write(file, &keys)?;
    Ok(true)
}

fn write(file: &Path, keys: &BTreeMap<String, String>) -> Result<()> {
    auth::write_private(file, &serde_json::to_string_pretty(keys)?).with_context(|| format!("Couldn't save {}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_kept_privately() {
        let dir = std::env::temp_dir().join(format!("harlo-agent-keys-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("keys.json");
        assert_eq!(get_in(&file, "openrouter"), None);
        set_in(&file, "openrouter", " sk-or-1 \n").unwrap();
        set_in(&file, "groq", "gsk_2").unwrap();
        assert_eq!((get_in(&file, "openrouter").as_deref(), get_in(&file, "groq").as_deref()), (Some("sk-or-1"), Some("gsk_2")));
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&file).unwrap().permissions()) & 0o777, 0o600);
        set_in(&file, "x", "two words").unwrap_err();
        set_in(&file, "x", "  ").unwrap_err();
        assert!(remove_in(&file, "groq").unwrap());
        assert!(!remove_in(&file, "groq").unwrap());
        assert_eq!(get_in(&file, "groq"), None);
        assert_eq!(get_in(&file, "openrouter").as_deref(), Some("sk-or-1"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
