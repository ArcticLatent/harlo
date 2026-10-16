//! Session templates: a new session's set-up saved under a name (the agent,
//! its model, what runs without asking, Ask first, a goal, a new worktree)
//! with its first message. The new-session page and the phone fill their
//! draft from one; nothing starts until it's sent. Kept as JSON in the
//! "templates" setting.

use serde_json::{Value, json};

/// At most this many (the oldest go).
pub const MAX: usize = 40;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Template {
    pub id: String,
    pub name: String,
    /// "" = the project's (or the default) agent.
    pub harness: String,
    /// "" = the agent's default model.
    pub model: String,
    /// The shield's mode ("" = the one it starts in).
    pub approval: String,
    pub ask_first: bool,
    /// omp: start as a goal (the message is its objective).
    pub goal: bool,
    /// Work in a new worktree.
    pub worktree: bool,
    pub text: String,
}

impl Template {
    fn from_json(v: &Value) -> Option<Self> {
        let s = |k: &str| v[k].as_str().unwrap_or_default().trim().to_owned();
        let name = s("name");
        let id = s("id");
        if name.is_empty() || id.is_empty() {
            return None;
        }
        let approval = s("approval");
        Some(Self {
            id,
            name: name.chars().take(60).collect(),
            harness: s("harness"),
            model: s("model"),
            approval: if ["always-ask", "write", "yolo"].contains(&approval.as_str()) { approval } else { String::new() },
            ask_first: v["askFirst"].as_bool().unwrap_or(false),
            goal: v["goal"].as_bool().unwrap_or(false),
            worktree: v["worktree"].as_bool().unwrap_or(false),
            text: v["text"].as_str().unwrap_or_default().to_owned(),
        })
    }

    pub fn to_json(&self) -> Value {
        json!({ "id": self.id, "name": self.name, "harness": self.harness, "model": self.model, "approval": self.approval,
                "askFirst": self.ask_first, "goal": self.goal, "worktree": self.worktree, "text": self.text })
    }
}

/// The saved list (unreadable entries skipped).
pub fn parse(saved: &str) -> Vec<Template> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(saved) else { return Vec::new() };
    items.iter().filter_map(Template::from_json).collect()
}

pub fn to_json(list: &[Template]) -> Value {
    Value::Array(list.iter().map(Template::to_json).collect())
}

/// Adds or replaces one (by id; a new one without an id gets `new_id`, one
/// named as another replaces it): the list and its id. None when it has no
/// name.
pub fn save(list: &mut Vec<Template>, edit: &Value, new_id: &str) -> Option<String> {
    let mut v = edit.clone();
    let named = v["name"].as_str().unwrap_or_default().trim().to_lowercase();
    let id = match v["id"].as_str().filter(|id| !id.is_empty()) {
        Some(id) => id.to_owned(),
        None => list.iter().find(|t| t.name.to_lowercase() == named).map_or_else(|| new_id.to_owned(), |t| t.id.clone()),
    };
    v["id"] = json!(id);
    let t = Template::from_json(&v)?;
    match list.iter_mut().find(|x| x.id == id) {
        Some(existing) => *existing = t,
        None => {
            list.push(t);
            if list.len() > MAX {
                list.remove(0);
            }
        }
    }
    Some(id)
}

pub fn delete(list: &mut Vec<Template>, id: &str) {
    list.retain(|t| t.id != id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_replaces_and_deletes() {
        let mut list = parse("");
        assert!(list.is_empty());
        let id = save(&mut list, &json!({ "name": "Review a PR", "harness": "claude", "model": "opus", "approval": "write", "text": "Review the open PR" }), "t1").unwrap();
        assert_eq!(id, "t1");
        assert_eq!(list[0].approval, "write");
        // The same name: replaced, not added.
        assert_eq!(save(&mut list, &json!({ "name": "review a pr", "harness": "omp", "goal": true }), "t2").as_deref(), Some("t1"));
        assert_eq!(list.len(), 1);
        assert!(list[0].goal && list[0].harness == "omp");
        // By id: renamed.
        save(&mut list, &json!({ "id": "t1", "name": "PR review", "approval": "nonsense" }), "t3");
        assert_eq!((list[0].name.as_str(), list[0].approval.as_str()), ("PR review", ""), "an unknown mode is dropped");
        assert_eq!(save(&mut list, &json!({ "name": "  " }), "t4"), None, "a name is needed");
        save(&mut list, &json!({ "name": "Tests", "worktree": true, "askFirst": true }), "t5");
        let back = parse(&to_json(&list).to_string());
        assert_eq!(back, list, "round trip");
        delete(&mut list, "t1");
        assert_eq!(list.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Tests"]);
        assert!(parse("{broken").is_empty() && parse(r#"[{"name":"no id"}, 3]"#).is_empty());
    }

    #[test]
    fn keeps_at_most_max() {
        let mut list = Vec::new();
        for i in 0..MAX + 3 {
            save(&mut list, &json!({ "name": format!("t{i}") }), &format!("id{i}"));
        }
        assert_eq!(list.len(), MAX);
        assert_eq!(list[0].name, "t3", "the oldest go");
    }
}
