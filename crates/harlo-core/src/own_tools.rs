//! Harlo's own tools for the server's agents: finding past sessions on the
//! server and reading one, which the server's own tools don't do.
//!
//! Harlo offers them over the server's client MCP connection while it's open
//! ([`Client::host_tools`]); agents call them in Code Mode as
//! `tools.harlo.past_sessions` and `tools.harlo.read_session`.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::fmt::Write as _;

use jiff::Timestamp;
use opencode_client::{Client, Message, Part, Session, ToolHost, ToolOutcome, ToolRun, ToolSpec};
use serde_json::{Value, json};

/// The name the tools are offered under.
pub const NAME: &str = "harlo";

/// How many sessions `past_sessions` lists unless asked for another number.
const DEFAULT_LIMIT: usize = 10;

/// The most sessions it lists.
const MAX_LIMIT: usize = 30;

/// The most characters `read_session` returns: the start of the session and
/// its latest part, with what's between left out.
const MAX_TEXT: usize = 24_000;

/// Words shorter than this aren't searched on their own.
const MIN_WORD: usize = 3;

/// Harlo's tools, run against the server they're offered to.
#[derive(Debug, Clone)]
pub struct OwnTools {
    /// The server.
    client: Client,
}

impl OwnTools {
    /// The tools, for `client`'s server.
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

impl ToolHost for OwnTools {
    fn tools(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "past_sessions".to_owned(),
                description: "Find earlier sessions on this OpenCode server by words in their \
                              titles, newest first. Use it to pick up earlier work or decisions; \
                              then read one with read_session."
                    .to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Words to look for in session titles.",
                        },
                        "limit": {
                            "type": "integer",
                            "description": "How many sessions to list (default 10, at most 30).",
                        },
                    },
                    "required": ["query"],
                }),
            },
            ToolSpec {
                name: "read_session".to_owned(),
                description: "Read an earlier session's conversation: the prompts, the answers, \
                              and the tools it used, by name. Long sessions are cut in the middle."
                    .to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "sessionID": {
                            "type": "string",
                            "description": "The session's ID, from past_sessions.",
                        },
                    },
                    "required": ["sessionID"],
                }),
            },
        ]
    }

    fn call(&self, name: String, input: Value, session: Option<String>) -> ToolRun {
        let client = self.client.clone();
        Box::pin(async move {
            match name.as_str() {
                "past_sessions" => past_sessions(&client, &input, session.as_deref()).await,
                "read_session" => read_session(&client, &input).await,
                other => Err(format!("No tool named {other}.")),
            }
        })
    }
}

/// Lists the sessions whose titles match `input`'s query, best first,
/// leaving out the session that asked.
async fn past_sessions(client: &Client, input: &Value, asking: Option<&str>) -> ToolOutcome {
    let query = input["query"].as_str().unwrap_or_default().trim();
    if query.is_empty() {
        return Err("Give a query: words to look for in session titles.".to_owned());
    }
    let limit = input["limit"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);

    // The whole query first, then each longer word, ranked by how many of
    // them a title holds.
    let mut searches = vec![query.to_owned()];
    let words: Vec<&str> = query
        .split_whitespace()
        .filter(|w| w.chars().count() >= MIN_WORD)
        .collect();
    if words.len() > 1 {
        searches.extend(words.iter().map(|&w| w.to_owned()));
    }
    let mut found: HashMap<String, (usize, Session)> = HashMap::new();
    for (index, search) in searches.iter().enumerate() {
        let sessions = client
            .search_sessions(search, MAX_LIMIT)
            .await
            .map_err(|e| format!("Couldn't search the sessions: {e}"))?;
        // A match on the whole query counts as every word.
        let weight = if index == 0 { words.len().max(1) } else { 1 };
        for session in sessions {
            if Some(session.id.as_str()) == asking {
                continue;
            }
            found
                .entry(session.id.clone())
                .and_modify(|(score, _)| *score += weight)
                .or_insert((weight, session));
        }
    }
    let mut list: Vec<(usize, Session)> = found.into_values().collect();
    list.sort_by(|(a, x), (b, y)| b.cmp(a).then(y.updated.total_cmp(&x.updated)));
    list.truncate(limit);
    if list.is_empty() {
        return Ok(format!("No session titles match \"{query}\"."));
    }
    let mut out = String::new();
    for (_, session) in &list {
        let _ = writeln!(
            out,
            "- {} — id {}, folder {}, last active {}",
            session.title.as_deref().unwrap_or("Untitled"),
            session.id,
            session.directory,
            day(session.updated),
        );
    }
    Ok(out)
}

/// The session `input` names, as text.
async fn read_session(client: &Client, input: &Value) -> ToolOutcome {
    let id = input["sessionID"].as_str().unwrap_or_default().trim();
    if id.is_empty() {
        return Err("Give the session's ID, from past_sessions.".to_owned());
    }
    let session = client
        .session(id)
        .await
        .map_err(|e| format!("Couldn't open session {id}: {e}"))?;
    let messages = client
        .messages(id)
        .await
        .map_err(|e| format!("Couldn't read session {id}: {e}"))?;
    let mut out = format!(
        "# {}\nFolder {}, last active {}\n",
        session.title.as_deref().unwrap_or("Untitled"),
        session.directory,
        day(session.updated),
    );
    out.push_str(&transcript(&messages));
    Ok(shorten(&out, MAX_TEXT))
}

/// The prompts, answers and tool names of `messages`, in order.
#[must_use]
pub fn transcript(messages: &[Message]) -> String {
    let mut out = String::new();
    for message in messages {
        match message {
            Message::User { text, .. } => {
                let _ = write!(out, "\n## User\n{}\n", text.trim());
            }
            Message::Assistant { parts, .. } => {
                let mut tools: Vec<&str> = Vec::new();
                let mut said = String::new();
                for part in parts {
                    match part {
                        Part::Text(text) if !text.trim().is_empty() => {
                            said.push_str(text.trim());
                            said.push('\n');
                        }
                        Part::Tool(call) => tools.push(&call.name),
                        Part::Text(_) | Part::Reasoning(_) => {}
                    }
                }
                if said.is_empty() && tools.is_empty() {
                    continue;
                }
                out.push_str("\n## Assistant\n");
                if !tools.is_empty() {
                    let _ = writeln!(out, "(used {})", tools.join(", "));
                }
                out.push_str(&said);
            }
            _ => {}
        }
    }
    out
}

/// `text`, or its start and end with the middle left out when it's longer
/// than `max` characters.
#[must_use]
pub fn shorten(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_owned();
    }
    let head: String = text.chars().take(max / 4).collect();
    let tail: String = text.chars().skip(count - (max - max / 4)).collect();
    format!(
        "{head}\n\n[… {} characters left out …]\n\n{tail}",
        count - max
    )
}

/// The day `millis` (Unix milliseconds) fell on, as `2026-11-22`.
fn day(millis: f64) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "milliseconds since 1970 fit an i64 for any real date"
    )]
    let millis = millis as i64;
    Timestamp::from_millisecond(millis)
        .map(|t| t.strftime("%Y-%m-%d").to_string())
        .unwrap_or_default()
}
