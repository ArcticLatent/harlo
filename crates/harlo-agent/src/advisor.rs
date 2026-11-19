//! The advisor: a second model that reviews a turn before it ends.
//!
//! When the model would end its turn, the advisor (`--advisor
//! provider/model`, or `HARLO_AGENT_ADVISOR`) reads the user's request,
//! the turn's steps in brief, the answer and the diff, and replies `OK` or
//! a short note on what's wrong or missing. A note goes to the model as a
//! hidden message and the turn goes on, once a turn. It costs a request
//! of its own each turn, so it's off unless set.

// Rust guideline compliant 2026-10-09

use serde_json::Value;
use std::fmt::Write as _;
use std::path::Path;

/// Of the turn, what the advisor reads at most (characters).
const TRANSCRIPT_CHARS: usize = 24_000;
/// Of the diff, new files included, at most.
const DIFF_CHARS: usize = 16_000;
/// Of a call's arguments or a result, shown.
const STEP_CHARS: usize = 300;
/// Of the request and of each of the model's texts, shown.
const TEXT_CHARS: usize = 4_000;

/// The advisor's instructions.
pub const INSTRUCTIONS: &str = "You review a coding agent's work before it reports back. You get the user's request, the agent's steps in brief, its final answer, and the diff of what it changed. \
    If it did what was asked, correctly and completely, reply exactly OK. Otherwise reply in one to three short sentences what is wrong or missing, concretely (file, function, behaviour), so the agent can fix it. \
    Judge only what makes the result wrong or incomplete for what was asked: no style, no rare cases the request didn't raise, no extra tests or features. \
    Long parts may be left out by harlo (marked \"left out for length\"): never ask for them, and don't judge what you can't see.";

/// How the note reaches the model.
pub fn note(advice: &str) -> String {
    format!("[harlo] A reviewer read your work and says: {advice}\nCheck this against the code; fix what's right about it, then finish. If it's mistaken, say why in your answer.")
}

/// The advisor's model, from the environment (None: off).
pub fn from_env() -> Option<String> {
    std::env::var("HARLO_AGENT_ADVISOR").ok().map(|m| m.trim().to_owned()).filter(|m| !m.is_empty() && m != "off").map(|m| crate::providers::full(&m))
}

/// The advisor's note from its reply (None: it found nothing wrong).
pub fn advice(reply: &str) -> Option<String> {
    let t = reply.trim();
    let first = t.split_whitespace().next().unwrap_or_default().trim_matches(|c: char| !c.is_alphanumeric());
    (!t.is_empty() && !first.eq_ignore_ascii_case("ok")).then(|| t.chars().take(1_200).collect())
}

/// What the advisor reads: the turn's messages (`turn`, from the user's
/// request on) in brief, and the diff in `cwd`.
pub fn transcript(turn: &[Value], cwd: &Path) -> String {
    let cut = |s: &str, n: usize| -> String {
        let one = s.trim();
        if one.chars().count() > n { format!("{}…", one.chars().take(n).collect::<String>()) } else { one.to_owned() }
    };
    let text_of = |m: &Value| -> String {
        match &m["content"] {
            Value::String(s) => s.clone(),
            Value::Array(blocks) => blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
            _ => String::new(),
        }
    };
    let mut steps = String::new();
    let mut request = String::new();
    for m in turn {
        match m["role"].as_str() {
            Some("user") if request.is_empty() => request = cut(&text_of(m), TEXT_CHARS),
            Some("assistant") => {
                for b in m["content"].as_array().into_iter().flatten() {
                    match b["type"].as_str() {
                        Some("text") => {
                            let _ = writeln!(steps, "agent: {}", cut(b["text"].as_str().unwrap_or_default(), TEXT_CHARS));
                        }
                        Some("toolCall") => {
                            let _ = writeln!(steps, "→ {}({})", b["name"].as_str().unwrap_or_default(), cut(&b["arguments"].to_string().replace('\n', " "), STEP_CHARS));
                        }
                        _ => {}
                    }
                }
            }
            Some("toolResult") => {
                let failed = if m["isError"] == true { " [failed]" } else { "" };
                let _ = writeln!(steps, "←{failed} {}", cut(&text_of(m).replace('\n', " ⏎ "), STEP_CHARS));
            }
            _ => {}
        }
    }
    // (The start and the end of a long turn: the answer is at its end.)
    let n = steps.chars().count();
    if n > TRANSCRIPT_CHARS {
        let head: String = steps.chars().take(TRANSCRIPT_CHARS / 3).collect();
        let tail: String = steps.chars().skip(n - TRANSCRIPT_CHARS * 2 / 3).collect();
        steps = format!("{head}\n[… {} characters of steps left out for length …]\n{tail}", n - TRANSCRIPT_CHARS);
    }
    let mut out = format!("The user's request:\n{request}\n\nThe agent's steps (→ a call, ← its result) and its answer:\n{steps}");
    if let Some(diff) = diff(cwd) {
        let _ = write!(out, "\nThe diff of the working tree:\n{diff}");
    }
    out
}

/// The working tree's diff (new files included), cut short; None outside
/// git or with nothing changed.
fn diff(cwd: &Path) -> Option<String> {
    let run = |args: &[&str]| std::process::Command::new("git").args(args).current_dir(cwd).stdin(std::process::Stdio::null()).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
    let mut d = run(&["diff", "--no-color", "--no-ext-diff", "HEAD"])?;
    // (New files whole, as far as the room goes: an answer is often one.)
    for new in run(&["ls-files", "--others", "--exclude-standard"]).unwrap_or_default().lines().take(20) {
        if let Ok(text) = std::fs::read_to_string(cwd.join(new)) {
            let _ = write!(d, "\nnew file {new}:\n{text}");
        }
    }
    if d.trim().is_empty() {
        return None;
    }
    let n = d.chars().count();
    Some(if n > DIFF_CHARS { format!("{}\n[… {} more characters left out for length …]", d.chars().take(DIFF_CHARS).collect::<String>(), n - DIFF_CHARS) } else { d })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_ok_is_no_advice() {
        assert_eq!(advice("OK"), None);
        assert_eq!(advice(" ok."), None);
        assert_eq!(advice("**OK**"), None);
        assert_eq!(advice("The test for empty input is missing."), Some("The test for empty input is missing.".to_owned()));
        assert_eq!(advice(""), None);
    }

    #[test]
    fn the_transcript_is_the_turn_in_brief() {
        let turn = vec![
            json!({ "role": "user", "content": "Fix the parser." }),
            json!({ "role": "assistant", "content": [{ "type": "toolCall", "name": "read", "arguments": { "path": "p.py" } }] }),
            json!({ "role": "toolResult", "content": [{ "type": "text", "text": "x".repeat(1000) }], "isError": false }),
            json!({ "role": "assistant", "content": [{ "type": "text", "text": "Fixed it." }] }),
        ];
        let t = transcript(&turn, Path::new("/"));
        assert!(t.starts_with("The user's request:\nFix the parser.\n"), "{t}");
        assert!(t.contains("→ read({\"path\":\"p.py\"})") && t.contains("agent: Fixed it."), "{t}");
        assert!(t.contains(&format!("← {}…", "x".repeat(STEP_CHARS))), "{t}");
    }
}
