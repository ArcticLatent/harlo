//! The prompts of pi's permission package (`@gotgenes/pi-permission-system`)
//! as harlo's approvals.
//!
//! Outside its terminal UI the package asks with a `select`: a "Permission
//! Required" title with a `field : value` line each, and Yes, No and maybe
//! session-long answers. harlo shows it as the approval every other agent's
//! uses (on the tool's row), or answers it itself where the session's shield
//! lets the action through. A subagent's prompt reaches its parent the same
//! way. What the package denies outright never gets here.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::ask_bridge::Action;

#[derive(Default)]
pub(crate) struct PermissionBridge {
    /// The session's shield (`always-ask`, `write`, `yolo`; empty: ask).
    shield: String,
    /// Shown prompts, by id: their session-long answer, if they have one.
    held: HashMap<String, Option<String>>,
}

/// A prompt's title as (field, value). Its fields line up on their colons;
/// other lines carry on a wrapped value (at a space, unless the line was full).
fn fields(title: &str) -> Vec<(String, String)> {
    const WIDTH: usize = 79;
    let colon = |line: &str| line.char_indices().find(|(_, c)| *c == ':').map(|(i, _)| i);
    let mut lines = title.lines().skip(1).peekable();
    let Some(column) = lines.peek().and_then(|l| colon(l)) else { return Vec::new() };
    let mut out: Vec<(String, String)> = Vec::new();
    let mut full = false;
    for line in lines {
        let starts = line.is_char_boundary(column) && line[column..].starts_with(':') && line[..column].ends_with(' ');
        if starts {
            out.push((line[..column].trim().to_owned(), line[column + 1..].trim().to_owned()));
        } else if let Some(last) = out.last_mut() {
            if !full && !last.1.is_empty() {
                last.1.push(' ');
            }
            last.1.push_str(line.trim());
        }
        full = line.chars().count() >= WIDTH;
    }
    out
}

impl PermissionBridge {
    pub(crate) fn set_shield(&mut self, mode: &str) {
        self.shield = mode.to_owned();
    }

    pub(crate) fn on_request(&mut self, frame: &Value) -> Action {
        let title = frame["title"].as_str().unwrap_or_default();
        let options: Vec<&str> = frame["options"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        if frame["method"] != "select" || !title.starts_with("Permission Required") || !options.contains(&"Yes") || !options.contains(&"No") {
            return Action::Pass;
        }
        let fields = fields(title);
        let field = |key: &str| fields.iter().find(|(k, _)| k == key).map_or("", |(_, v)| v.as_str());
        let tool = field("tool");
        // "write": editing in the project; somewhere outside it still asks.
        let allowed = match self.shield.as_str() {
            "yolo" => true,
            "write" => matches!(tool, "edit" | "write") && !field("surface").starts_with("external_directory"),
            _ => false,
        };
        if allowed {
            return Action::Answer(json!({ "type": "extension_ui_response", "id": frame["id"], "value": "Yes" }));
        }
        let whole = !field("full command").is_empty();
        let details: Vec<String> = fields
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "tool" | "rule" | "surface" | "why" | "working directory") && !(whole && k == "command"))
            .map(|(k, v)| match k.as_str() {
                // (Who's asking, without its session's id.)
                "subagent" => format!("subagent: {}", v.split(" · ").next().unwrap_or(v)),
                "command" | "full command" | "input" => v.clone(),
                _ => format!("{k}: {v}"),
            })
            .collect();
        let session = options.iter().find(|o| o.starts_with("Yes, ")).map(|o| (*o).to_owned());
        let mut choices = vec!["Approve", "Deny"];
        if session.is_some() {
            choices.push("Always allow");
        }
        let mut card = json!({
            "type": "extension_ui_request", "id": frame["id"], "method": "select",
            "title": format!("Allow tool: {tool}\n{}", details.join("\n")),
            "options": choices,
        });
        if let Some(s) = &session {
            card["alwaysHint"] = json!(s.trim_start_matches("Yes, "));
        }
        self.held.insert(frame["id"].as_str().unwrap_or_default().to_owned(), session);
        Action::Show(card)
    }

    /// harlo's answer to a prompt this shows: the response in the package's
    /// words, or None when it isn't one.
    pub(crate) fn answer(&mut self, id: &str, answer: &Value) -> Option<Value> {
        let session = self.held.remove(id)?;
        let mut response = json!({ "type": "extension_ui_response", "id": id });
        match answer["value"].as_str() {
            Some("Approve") => response["value"] = json!("Yes"),
            Some("Always allow") => response["value"] = json!(session.unwrap_or_else(|| "Yes".to_owned())),
            Some("Deny") => response["value"] = json!("No"),
            _ => response["cancelled"] = json!(true),
        }
        Some(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(title: &str, options: &[&str]) -> Value {
        json!({ "type": "extension_ui_request", "id": "p1", "method": "select", "title": title, "options": options })
    }

    fn bash() -> Value {
        prompt("Permission Required\ntool    : bash\nrule    : *\ncommand : echo hi", &["Yes", "Yes, allow bash \"echo *\" for this session", "No", "No, provide reason"])
    }

    #[test]
    fn shows_a_prompt_as_an_approval_and_answers_in_its_words() {
        let mut b = PermissionBridge::default();
        let Action::Show(card) = b.on_request(&bash()) else { panic!("not shown") };
        assert_eq!(card["title"], "Allow tool: bash\necho hi");
        assert_eq!(card["options"], json!(["Approve", "Deny", "Always allow"]));
        assert_eq!(card["alwaysHint"], "allow bash \"echo *\" for this session");
        assert_eq!(b.answer("p1", &json!({ "value": "Approve" })).unwrap()["value"], "Yes");

        b.on_request(&bash());
        assert_eq!(b.answer("p1", &json!({ "value": "Always allow" })).unwrap()["value"], "Yes, allow bash \"echo *\" for this session");
        b.on_request(&bash());
        assert_eq!(b.answer("p1", &json!({ "value": "Deny" })).unwrap()["value"], "No");
        b.on_request(&bash());
        assert_eq!(b.answer("p1", &json!({ "cancelled": true })).unwrap()["cancelled"], true);
        // Not one of its prompts.
        assert!(b.answer("other", &json!({ "value": "Approve" })).is_none());
    }

    #[test]
    fn the_shield_answers_what_it_lets_through() {
        let write = prompt("Permission Required\ntool  : write\ninput : for 'b.txt' (1 line, 1 character)", &["Yes", "No"]);
        let outside = prompt(
            "Permission Required\ntool              : write\nsurface           : external_directory_write\nrule              : *\npath              : /etc/x\nworking directory :\n/home/u/\nproject",
            &["Yes", "Yes, allow writes to \"/etc/*\" for this session", "No"],
        );
        let yes = |frame: &Value| json!({ "type": "extension_ui_response", "id": frame["id"], "value": "Yes" });
        let mut b = PermissionBridge::default();
        b.set_shield("always-ask");
        assert!(matches!(b.on_request(&write), Action::Show(_)));

        b.set_shield("write");
        assert_eq!(b.on_request(&write), Action::Answer(yes(&write)));
        assert!(matches!(b.on_request(&bash()), Action::Show(_)), "commands still ask");
        let Action::Show(card) = b.on_request(&outside) else { panic!("outside the project asks") };
        assert_eq!(card["title"], "Allow tool: write\npath: /etc/x");

        b.set_shield("yolo");
        assert_eq!(b.on_request(&bash()), Action::Answer(yes(&bash())));
        assert_eq!(b.on_request(&outside), Action::Answer(yes(&outside)));
    }

    #[test]
    fn a_subagents_prompt_says_who_asks() {
        let mut b = PermissionBridge::default();
        let frame = prompt("Permission Required (Subagent)\nsubagent : worker · session 01a198\ntool     : bash\nrule     : *\ncommand  : echo child", &["Yes", "Yes, for this session", "No", "No, provide reason"]);
        let Action::Show(card) = b.on_request(&frame) else { panic!("not shown") };
        assert_eq!(card["title"], "Allow tool: bash\nsubagent: worker\necho child");
        assert_eq!(card["alwaysHint"], "for this session");
    }

    #[test]
    fn reads_the_whole_command() {
        let mut b = PermissionBridge::default();
        let title = |frame: Value, b: &mut PermissionBridge| match b.on_request(&frame) {
            Action::Show(card) => card["title"].as_str().unwrap().to_owned(),
            _ => panic!("not shown"),
        };
        // The part that asks, and all of it: all of it shows.
        let part = prompt("Permission Required\ntool         : bash\nrule         : *\ncommand      : touch x\nfull command : ls && touch x", &["Yes", "No"]);
        assert_eq!(title(part, &mut b), "Allow tool: bash\nls && touch x");
        // Wrapped at a space.
        let words = prompt("Permission Required\ntool    : bash\nrule    : *\ncommand : echo a b\nc d", &["Yes", "No"]);
        assert_eq!(title(words, &mut b), "Allow tool: bash\necho a b c d");
        // Broken inside a long word: back together.
        let broken = format!("Permission Required\ntool    : bash\nrule    : *\ncommand : printf %s\n{}\nZ", "X".repeat(80));
        assert_eq!(title(prompt(&broken, &["Yes", "No"]), &mut b), format!("Allow tool: bash\nprintf %s {}Z", "X".repeat(80)));
        // Why it asks is the package's own language; where it reaches isn't.
        let outside = prompt(
            "Permission Required\ntool              : bash\nsurface           : external_directory\nrule              : *\ncommand           : sudo x /\nwhy               : /: unproven; declare the command\nin commandEffects\nworking directory :\n/home/u\nexternal path     : /",
            &["Yes", "No"],
        );
        assert_eq!(title(outside, &mut b), "Allow tool: bash\nsudo x /\nexternal path: /");
    }

    #[test]
    fn leaves_other_dialogs_alone() {
        let mut b = PermissionBridge::default();
        b.set_shield("yolo");
        assert_eq!(b.on_request(&prompt("[Q] Pick one?", &["1. A", "2. B"])), Action::Pass);
        assert_eq!(b.on_request(&prompt("Permission Required\ntool : x", &["Allow", "Block"])), Action::Pass);
        assert_eq!(b.on_request(&json!({ "method": "input", "title": "Permission Required" })), Action::Pass);
    }
}
