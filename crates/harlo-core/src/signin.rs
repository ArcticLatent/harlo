//! Signing the agents in to their model providers from harlo: reading what
//! their own sign-in commands print (`claude auth login`) into
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

/// The port on `localhost` a sign-in page ends on, if it does: its own, or
/// its `redirect_uri`'s. (On another machine, that port has to reach it.)
pub fn callback_port(url: &str) -> Option<u16> {
    let local = |u: &str| -> Option<u16> {
        let rest = u.strip_prefix("http://")?;
        let place = rest.split(['/', '?', '#']).next()?;
        let (name, port) = place.rsplit_once(':')?;
        ["localhost", "127.0.0.1", "[::1]"].contains(&name).then(|| port.parse().ok()).flatten()
    };
    local(url).or_else(|| {
        let value = url.split_once("redirect_uri=")?.1.split('&').next()?;
        let bytes = value.as_bytes();
        let (mut out, mut i) = (Vec::new(), 0);
        while i < bytes.len() {
            let hex = (bytes[i] == b'%').then(|| value.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())).flatten();
            match hex {
                Some(b) => { out.push(b); i += 3; }
                None => { out.push(bytes[i]); i += 1; }
            }
        }
        local(&String::from_utf8_lossy(&out))
    })
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
        assert_eq!(callback_port("http://localhost:1455/launch"), Some(1455));
        assert_eq!(callback_port("https://auth.example/authorize?client_id=x&redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback&scope=y"), Some(1455));
        assert_eq!(callback_port("https://auth.example/a?redirect_uri=http://127.0.0.1:53692/cb"), Some(53692));
        assert_eq!(callback_port("https://platform.example/oauth?redirect_uri=https%3A%2F%2Fplatform.example%2Fcb"), None);
        assert_eq!(callback_port("https://example.com/localhost:80"), None);
        assert_eq!(url_in("no address here"), None);

        assert_eq!(prompt("Paste your DeepSeek API key (sk-...): ").unwrap()["kind"], "secret");
        assert_eq!(prompt("Paste the authorization code (or full redirect URL): ").unwrap()["kind"], "code");
        assert_eq!(prompt("Paste code here if prompted > ").unwrap()["message"], "Paste code here if prompted");
        assert_eq!(prompt("Enter number (1-84): ").unwrap()["kind"], "text");
        assert!(prompt("Waiting for browser authentication...").is_none());
    }
}
