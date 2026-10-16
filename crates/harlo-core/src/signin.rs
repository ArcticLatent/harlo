//! Signing the agents in to their model providers from harlo: reading what
//! their own sign-in commands print (`claude auth login`, `omp login`) into
//! steps a dialog shows: links to open, codes to see, what they ask for.
//!
//! The agents keep their credentials themselves; harlo only shows the
//! links and hands what the user types to the command, as a terminal would.

use serde_json::{Value, json};

/// ANSI colours and cursor moves, and OSC sequences (titles, OSC 8 links:
/// their text stays), out of a line.
pub fn plain(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\x1b' {
            if c != '\r' && (c == '\t' || c == '\n' || !c.is_control()) {
                out.push(c);
            }
            continue;
        }
        match it.peek() {
            Some('[') => {
                it.next();
                // Parameters, then one final letter.
                for c in it.by_ref() {
                    if c.is_ascii_alphabetic() || c == '~' {
                        break;
                    }
                }
            }
            Some(']') => {
                it.next();
                // Up to BEL or ESC \.
                while let Some(c) = it.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' {
                        if it.peek() == Some(&'\\') {
                            it.next();
                        }
                        break;
                    }
                }
            }
            _ => {
                it.next();
            }
        }
    }
    out
}

/// The first web address in a line.
pub fn url_in(line: &str) -> Option<String> {
    let at = line.find("https://").or_else(|| line.find("http://"))?;
    let url: String = line[at..].chars().take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '<' | '>' | ')' | ']')).collect();
    let url = url.trim_end_matches(['.', ',', ';']).to_owned();
    (url.len() > 10).then_some(url)
}

/// What the command waits for, from the text it left without a newline
/// ("Paste your DeepSeek API key (sk-...): ", "Paste code here if
/// prompted > "): `{ kind: "secret" | "code" | "text", message }`.
pub fn prompt(pending: &str) -> Option<Value> {
    let t = pending.trim();
    if t.is_empty() || !(t.ends_with(':') || t.ends_with('>') || t.ends_with('?')) {
        return None;
    }
    let message = t.trim_end_matches([':', '>', ' ']).trim().to_owned();
    let lower = message.to_lowercase();
    let kind = if ["key", "token", "secret", "password"].iter().any(|w| lower.contains(w)) {
        "secret"
    } else if lower.contains("code") || lower.contains("redirect") {
        "code"
    } else {
        "text"
    };
    Some(json!({ "kind": kind, "message": message }))
}

/// `omp login`'s numbered list ("  7. GitHub Copilot"): `(number, name)`.
pub fn omp_providers(text: &str) -> Vec<(u32, String)> {
    plain(text)
        .lines()
        .filter_map(|l| {
            let (n, name) = l.trim().split_once(". ")?;
            Some((n.parse().ok()?, name.trim().to_owned()))
        })
        .filter(|(_, name)| !name.is_empty())
        .collect()
}

/// A provider sign-in Anthropic doesn't allow outside its own apps: a
/// Claude subscription (Pro/Max) in another agent.
pub fn claude_subscription(name: &str) -> bool {
    let n = name.to_lowercase();
    (n.contains("anthropic") || n.contains("claude")) && (n.contains("pro/max") || n.contains("subscription"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_sign_in_commands_print() {
        let osc8 = "If the browser didn't open, visit: \x1b]8;;https://claude.com/cai/oauth/authorize?code=true&state=x\x1b\\https://claude.com/cai/oauth/authorize?code=true&state=x\x1b]8;;\x1b\\";
        let line = plain(osc8);
        assert_eq!(line, "If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?code=true&state=x");
        assert_eq!(url_in(&line).as_deref(), Some("https://claude.com/cai/oauth/authorize?code=true&state=x"));
        assert_eq!(plain("\x1b[1;32mDone\x1b[0m\r"), "Done");
        assert_eq!(url_in("Local shortcut (this machine only): http://localhost:1455/launch.").as_deref(), Some("http://localhost:1455/launch"));
        assert_eq!(url_in("no address here"), None);

        assert_eq!(prompt("Paste your DeepSeek API key (sk-...): ").unwrap()["kind"], "secret");
        assert_eq!(prompt("Paste the authorization code (or full redirect URL): ").unwrap()["kind"], "code");
        assert_eq!(prompt("Paste code here if prompted > ").unwrap()["message"], "Paste code here if prompted");
        assert_eq!(prompt("Enter number (1-84): ").unwrap()["kind"], "text");
        assert!(prompt("Waiting for browser authentication...").is_none());

        let list = omp_providers("Select a provider:\n\n  1. ChatGPT Plus/Pro (Codex Subscription)\n  2. Anthropic (Claude Pro/Max)\n 19. ai&\n\nEnter number (1-84): ");
        assert_eq!(list, vec![(1, "ChatGPT Plus/Pro (Codex Subscription)".into()), (2, "Anthropic (Claude Pro/Max)".into()), (19, "ai&".into())]);
        assert!(claude_subscription("Anthropic (Claude Pro/Max)"));
        assert!(!claude_subscription("Anthropic"));
        assert!(!claude_subscription("ChatGPT Plus/Pro (Codex Subscription)"));
    }
}
