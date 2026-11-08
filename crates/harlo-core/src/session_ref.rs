//! Another session mentioned in a message (`@session:<id>`, from the
//! composer's `@` list): its latest messages go with the message, as they
//! were when it was sent, in a file block (shown as an attachment). The
//! agent reads them like a pasted excerpt; nothing is read again later.

use std::path::Path;

use harlo_harness::{HarnessKind, SessionInfo};
use serde_json::Value;

pub const PREFIX: &str = "@session:";
/// Of an excerpt, at most (about 4k tokens).
const BUDGET: usize = 16_000;
/// Of one message in it.
const ONE: usize = 3_000;

/// The id as a mention shows it (its first 8 letters and digits).
pub fn short_id(id: &str) -> String {
    id.chars().filter(char::is_ascii_alphanumeric).take(8).collect()
}

pub fn token(id: &str) -> String {
    format!("{PREFIX}{}", short_id(id))
}

/// The mentions in `text`, each once.
pub fn tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let Some(rest) = word.strip_prefix(PREFIX) else { continue };
        let id: String = rest.chars().take_while(char::is_ascii_alphanumeric).collect();
        let t = format!("{PREFIX}{id}");
        if !id.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// Sessions to offer for `@` in `cwd`'s composer: that folder's, newest
/// first, matching `query` (title words or the id's start), without the
/// one asking (`own`).
pub fn candidates<'a>(sessions: impl IntoIterator<Item = &'a SessionInfo>, cwd: &Path, own: Option<&Path>, query: &str, limit: usize) -> Vec<&'a SessionInfo> {
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let here = real(cwd);
    let q = query.trim().to_lowercase();
    let mut found: Vec<&SessionInfo> = sessions
        .into_iter()
        .filter(|s| own != Some(s.path.as_path()) && s.harness != HarnessKind::Demo)
        .filter(|s| s.cwd == cwd || real(&s.cwd) == here)
        .filter(|s| q.is_empty() || s.display_title().to_lowercase().contains(&q) || short_id(&s.id).to_lowercase().starts_with(&q))
        .collect();
    found.sort_by(|a, b| b.modified.cmp(&a.modified));
    found.truncate(limit);
    found
}

/// A session's messages, oldest first.
pub fn messages_of(kind: HarnessKind, path: &Path) -> Vec<Value> {
    match kind {
        HarnessKind::Claude => harlo_harness::claude::saved_file(path).map(|s| s.messages).unwrap_or_default(),
        _ => harlo_harness::store::read_messages(path).unwrap_or_default(),
    }
}

/// What a message said, for the excerpt (None: nothing to show).
fn said(m: &Value) -> Option<String> {
    let text_of = |content: &Value| -> String {
        match content {
            Value::String(s) => s.clone(),
            Value::Array(blocks) => blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
            _ => String::new(),
        }
    };
    let (who, text) = match m["role"].as_str()? {
        "user" => {
            let raw = text_of(&m["content"]);
            let (files, rest) = harlo_harness::file_blocks::split_blocks(&raw);
            let rest = harlo_harness::file_blocks::strip_plan(&rest).0.trim().to_owned();
            let with = if files.is_empty() { String::new() } else { format!(" [with {}]", files.join(", ")) };
            ("User", format!("{rest}{with}"))
        }
        "assistant" => {
            let mut text = text_of(&m["content"]);
            let tools: Vec<&str> = m["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "toolCall").filter_map(|b| b["name"].as_str()).collect();
            if !tools.is_empty() {
                let mut counted: Vec<(String, usize)> = Vec::new();
                for t in tools {
                    match counted.iter_mut().find(|(n, _)| n == t) {
                        Some((_, c)) => *c += 1,
                        None => counted.push((t.to_owned(), 1)),
                    }
                }
                let list: Vec<String> = counted.into_iter().map(|(n, c)| if c > 1 { format!("{n} ×{c}") } else { n }).collect();
                text.push_str(&format!("{}[used {}]", if text.trim().is_empty() { "" } else { "\n" }, list.join(", ")));
            }
            ("Assistant", text)
        }
        "compactionSummary" | "branchSummary" => ("Summary of what came before", m["summary"].as_str().unwrap_or_default().to_owned()),
        _ => return None,
    };
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let n = text.chars().count();
    let text = if n > ONE {
        let head: String = text.chars().take(ONE * 2 / 3).collect();
        let tail: String = text.chars().skip(n - ONE / 3).collect();
        format!("{head}\n[… {} characters left out …]\n{tail}", n - ONE)
    } else {
        text.to_owned()
    };
    Some(format!("{who}: {text}"))
}

/// The block that goes with a message mentioning a session: its latest
/// messages that fit, oldest of them first.
pub fn block(info: &SessionInfo, messages: &[Value]) -> String {
    let lines: Vec<String> = messages.iter().filter_map(said).collect();
    let mut kept: Vec<&String> = Vec::new();
    let mut size = 0;
    for l in lines.iter().rev() {
        if size + l.len() > BUDGET && !kept.is_empty() {
            break;
        }
        size += l.len() + 2;
        kept.push(l);
    }
    kept.reverse();
    let when = chrono::DateTime::<chrono::Local>::from(info.modified).format("%Y-%m-%d %H:%M");
    let which = if kept.len() == lines.len() { format!("all {} messages", lines.len()) } else { format!("its last {} of {} messages", kept.len(), lines.len()) };
    let body = kept.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n\n");
    let title = info.display_title().replace('"', "'");
    let title: String = title.chars().take(80).collect();
    format!(
        "<file name=\"session: {title} ({})\">\nSession \"{title}\" ({}, last active {when}): {which}, as they were when this message was sent.\n\n{body}\n</file>\n",
        token(&info.id),
        info.harness.label()
    )
}

/// The session a mention (or an id, or its start) names.
pub fn find<'a>(sessions: impl IntoIterator<Item = &'a SessionInfo>, mention: &str) -> Option<&'a SessionInfo> {
    let want = short_id(mention.trim().strip_prefix(PREFIX).unwrap_or(mention.trim())).to_lowercase();
    if want.len() < 4 {
        return None;
    }
    sessions.into_iter().filter(|s| short_id(&s.id).to_lowercase().starts_with(&want)).max_by_key(|s| s.modified)
}

/// A session's latest `n` messages as an agent reads them (what each said,
/// tools named), oldest of them first.
pub fn latest(messages: &[Value], n: usize) -> Vec<String> {
    let all: Vec<String> = messages.iter().filter_map(said).collect();
    all[all.len().saturating_sub(n)..].to_vec()
}

/// A search hit's passage as plain text, the matches between `**`.
pub fn plain_snippet(html: &str) -> String {
    html.replace("<b>", "**").replace("</b>", "**").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

/// `text` with a block for each session it mentions that `find` knows
/// (by mention), before it.
pub fn expand(text: &str, find: impl Fn(&str) -> Option<SessionInfo>) -> String {
    let mut blocks = String::new();
    for t in tokens(text) {
        if let Some(info) = find(&t) {
            blocks.push_str(&block(&info, &messages_of(info.harness, &info.path)));
        }
    }
    format!("{blocks}{text}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;
    use std::time::SystemTime;

    fn info(id: &str, cwd: &str, name: &str, secs: u64) -> SessionInfo {
        SessionInfo {
            harness: HarnessKind::Harlo,
            path: PathBuf::from(format!("/s/{id}.jsonl")),
            id: id.into(),
            cwd: PathBuf::from(cwd),
            name: name.into(),
            first_message: String::new(),
            created: String::new(),
            modified: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs),
            size: 1,
            parent_session: None,
        }
    }

    #[test]
    fn finds_mentions_and_this_folders_sessions() {
        assert_eq!(tokens("see @session:ab12cd34, and @session:ab12cd34 again; @sessions @session:"), vec!["@session:ab12cd34"]);
        let all = [info("aa11-x", "/p", "Fix the sandbox", 10), info("bb22", "/p", "Add fetch", 30), info("cc33", "/other", "Fix fetch", 40), info("dd44", "/p", "Own", 50)];
        let names = |q: &str| candidates(&all, Path::new("/p"), Some(Path::new("/s/dd44.jsonl")), q, 5).iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(""), ["Add fetch", "Fix the sandbox"], "newest first, this folder's, not its own");
        assert_eq!(names("fix"), ["Fix the sandbox"]);
        assert_eq!(names("aa11"), ["Fix the sandbox"], "by the id's start");
    }

    #[test]
    fn finds_a_session_by_its_mention() {
        let all = [info("aa11bb22-1", "/p", "One", 10), info("cc33dd44-2", "/p", "Two", 20)];
        assert_eq!(find(&all, "@session:aa11bb22").map(|s| s.name.as_str()), Some("One"));
        assert_eq!(find(&all, "cc33").map(|s| s.name.as_str()), Some("Two"));
        assert!(find(&all, "aa").is_none(), "too short to tell");
        assert_eq!(plain_snippet("a <b>bwrap</b> &amp; &lt;x&gt;"), "a **bwrap** & <x>");
        let m = [json!({ "role": "user", "content": "hi" }), json!({ "role": "assistant", "content": [{ "type": "text", "text": "hello" }] }), json!({ "role": "user", "content": "bye" })];
        assert_eq!(latest(&m, 2), ["Assistant: hello", "User: bye"]);
    }

    #[test]
    fn the_block_keeps_the_latest_that_fit() {
        let mut messages = vec![json!({ "role": "compactionSummary", "summary": "Earlier: chose bwrap." })];
        for i in 0..40 {
            messages.push(json!({ "role": "user", "content": format!("question {i} {}", "x".repeat(400)) }));
            messages.push(json!({ "role": "assistant", "content": [{ "type": "text", "text": format!("answer {i}") }, { "type": "toolCall", "name": "read" }, { "type": "toolCall", "name": "read" }, { "type": "toolCall", "name": "edit" }] }));
            messages.push(json!({ "role": "toolResult", "content": [{ "type": "text", "text": "not shown" }] }));
        }
        messages.push(json!({ "role": "user", "content": "<file name=\"a.txt\">\nsecret file\n</file>\nlast one" }));
        let b = block(&info("ab12cd34-ffff", "/p", "Sandbox \"work\"", 0), &messages);
        assert!(b.starts_with("<file name=\"session: Sandbox 'work' (@session:ab12cd34)\">\n"), "{}", &b[..120]);
        assert!(b.contains("its last ") && b.contains(" of 82 messages"), "{}", &b[..300]);
        assert!(b.contains("User: last one [with a.txt]") && !b.contains("secret file") && !b.contains("not shown"));
        assert!(b.contains("Assistant: answer 39\n[used read ×2, edit]"));
        assert!(!b.contains("question 0 ") && !b.contains("chose bwrap"), "the oldest don't fit");
        assert!(b.len() < BUDGET + 1000, "{}", b.len());
        let (names, rest) = harlo_harness::file_blocks::split_blocks(&format!("{b}see @session:ab12cd34"));
        assert_eq!((names.len(), rest.as_str()), (1, "see @session:ab12cd34"), "shown as an attachment");
    }
}
