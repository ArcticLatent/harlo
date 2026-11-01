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

/// Whether omp is signed in to Anthropic with a Claude subscription, from
/// its `agent.db` (`~/.omp/agent/agent.db` by default). Only the kind of
/// each sign-in is read, never its token.
pub fn omp_claude_subscription(db: Option<&std::path::Path>) -> bool {
    let home = std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".omp/agent/agent.db"));
    let Some(path) = db.map(std::path::Path::to_path_buf).or(home).filter(|p| p.is_file()) else { return false };
    let Ok(c) = rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { return false };
    let _ = c.busy_timeout(std::time::Duration::from_secs(2));
    c.query_row(
        "SELECT EXISTS(SELECT 1 FROM auth_credentials WHERE provider = 'anthropic' AND credential_type = 'oauth' AND disabled_cause IS NULL)",
        [],
        |r| r.get::<_, bool>(0),
    )
    .unwrap_or(false)
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

    #[test]
    fn reads_omps_kind_of_sign_in() {
        let dir = std::env::temp_dir().join(format!("harlo-omp-auth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("agent.db");
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute_batch("CREATE TABLE auth_credentials (id INTEGER PRIMARY KEY, provider TEXT NOT NULL, credential_type TEXT NOT NULL, data TEXT NOT NULL, disabled_cause TEXT DEFAULT NULL);
                         INSERT INTO auth_credentials (provider, credential_type, data) VALUES ('openai-codex', 'oauth', '{}'), ('anthropic', 'api_key', '{}');").unwrap();
        assert!(!omp_claude_subscription(Some(&db)));
        c.execute("INSERT INTO auth_credentials (provider, credential_type, data, disabled_cause) VALUES ('anthropic', 'oauth', '{}', 'expired')", []).unwrap();
        assert!(!omp_claude_subscription(Some(&db)));
        c.execute("INSERT INTO auth_credentials (provider, credential_type, data) VALUES ('anthropic', 'oauth', '{}')", []).unwrap();
        assert!(omp_claude_subscription(Some(&db)));
        assert!(!omp_claude_subscription(Some(&dir.join("none.db"))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
