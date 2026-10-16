//! The `--config` overlay harlo passes to each omp process it starts. omp
//! watches the file, so harlo rewrites it as the session's model changes.
//!
//! - Goals continue between turns over RPC (omp's default is the terminal
//!   UI only), unless turned off.
//! - Subagents fall back to the session's model: omp picks a subagent's
//!   model from a role (`smol` for scouts, `task`, `tiny`), with no fallback
//!   when that model is unavailable. Each of those roles becomes "your
//!   configured model, else the session's" (omp takes the first available
//!   in a comma list). The overlay replaces the whole `modelRoles` map, so
//!   your other roles are copied in unchanged.
//! - The session's approval mode (`tools.approvalMode`), when chosen in
//!   harlo.
//! - Plan mode: edits are denied and commands, eval and subagents need
//!   approval (`tools.approval`, with your own policies merged in; a deny of
//!   yours stays).

use std::collections::BTreeMap;
use std::path::PathBuf;

use yaml_rust::{Yaml, YamlLoader};

/// Roles omp's subagents use.
pub const SUBAGENT_ROLES: [&str; 3] = ["smol", "task", "tiny"];

/// omp's global config file.
pub fn user_config_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".omp/agent/config.yml"))
}

/// omp's approval modes, strictest first.
pub const APPROVAL_MODES: [&str; 3] = ["always-ask", "write", "yolo"];

/// Tool policies in plan mode.
pub const PLAN_POLICIES: [(&str, &str); 6] =
    [("edit", "deny"), ("write", "deny"), ("ast_edit", "deny"), ("bash", "prompt"), ("eval", "prompt"), ("task", "prompt")];

/// The config's root document (Null if unreadable).
fn root(config_yaml: &str) -> Yaml {
    YamlLoader::load_from_str(config_yaml).ok().and_then(|d| d.into_iter().next()).unwrap_or(Yaml::Null)
}

/// A map of strings in the config (empty if none).
fn string_map(node: &Yaml) -> BTreeMap<String, String> {
    let Yaml::Hash(map) = node else { return BTreeMap::new() };
    map.iter()
        .filter_map(|(k, v)| Some((k.as_str()?.to_owned(), v.as_str()?.to_owned())))
        .collect()
}

/// `modelRoles` from an omp config file's text (empty if none or unreadable).
pub fn model_roles(config_yaml: &str) -> BTreeMap<String, String> {
    string_map(&root(config_yaml)["modelRoles"])
}

/// `tools.approval`: per-tool policies (empty if none).
pub fn tool_policies(config_yaml: &str) -> BTreeMap<String, String> {
    string_map(&root(config_yaml)["tools"]["approval"])
}

/// `tools.approvalMode`, or omp's default (`yolo`).
pub fn approval_mode(config_yaml: &str) -> String {
    root(config_yaml)["tools"]["approvalMode"]
        .as_str()
        .filter(|m| APPROVAL_MODES.contains(m))
        .unwrap_or("yolo")
        .to_owned()
}

pub struct Overlay<'a> {
    pub goal_continue: bool,
    /// The user's roles and the session's model (`provider/id`), when
    /// subagents should fall back to it.
    pub subagent_fallback: Option<(&'a BTreeMap<String, String>, &'a str)>,
    /// `tools.approvalMode` for this session.
    pub approval_mode: Option<&'a str>,
    /// Plan mode, with the user's tool policies.
    pub plan: Option<&'a BTreeMap<String, String>>,
}

impl Overlay<'_> {
    pub fn render(&self) -> String {
        let mut out = String::from("# Written by harlo for one omp session (Settings → Harnesses → omp).\n");
        if self.goal_continue {
            out.push_str("goal:\n  continuationModes:\n    - interactive\n    - rpc\n");
        }
        if let Some((user, model)) = self.subagent_fallback.filter(|(_, m)| !m.trim().is_empty()) {
            let mut roles = user.clone();
            let default = user.get("default").cloned().unwrap_or_default();
            for role in SUBAGENT_ROLES {
                let preferred = user.get(role).cloned().unwrap_or_else(|| default.clone());
                let list: Vec<&str> = preferred.split(',').map(str::trim).filter(|p| !p.is_empty() && *p != model).chain([model]).collect();
                roles.insert(role.to_owned(), list.join(","));
            }
            out.push_str("modelRoles:\n");
            for (role, value) in &roles {
                out.push_str(&format!("  {}: {}\n", quote(role), quote(value)));
            }
        }
        if self.approval_mode.is_some() || self.plan.is_some() {
            out.push_str("tools:\n");
            if let Some(mode) = self.approval_mode {
                out.push_str(&format!("  approvalMode: {}\n", quote(mode)));
            }
            if let Some(user) = self.plan {
                let mut policies = user.clone();
                for (tool, policy) in PLAN_POLICIES {
                    let denied = policies.get(tool).is_some_and(|p| p.trim().eq_ignore_ascii_case("deny"));
                    if !denied {
                        policies.insert(tool.to_owned(), policy.to_owned());
                    }
                }
                out.push_str("  approval:\n");
                for (tool, policy) in &policies {
                    out.push_str(&format!("    {}: {}\n", quote(tool), quote(policy)));
                }
            }
        }
        out
    }
}

/// A YAML double-quoted scalar.
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_model_roles() {
        let cfg = "setupVersion: 2\nmodelRoles:\n  default: llama.cpp/qwen:medium\n  advisor: openai-codex/gpt-6-sol\ntheme:\n  dark: x\n";
        let roles = model_roles(cfg);
        assert_eq!(roles.len(), 2);
        assert_eq!(roles["default"], "llama.cpp/qwen:medium");
        assert!(model_roles("modelRoles: {default: a/b}")["default"] == "a/b");
        assert!(model_roles("theme: x").is_empty());
        assert!(model_roles(": : not yaml [").is_empty());
    }

    #[test]
    fn renders_goal_and_role_fallbacks() {
        let mut user = BTreeMap::new();
        user.insert("default".to_owned(), "llama.cpp/qwen:medium".to_owned());
        user.insert("smol".to_owned(), "local/tiny, openai-codex/gpt-5.6-luna".to_owned());
        user.insert("advisor".to_owned(), "openai-codex/gpt-6-sol".to_owned());
        let text = Overlay { goal_continue: true, subagent_fallback: Some((&user, "openai-codex/gpt-5.6-luna")), approval_mode: None, plan: None }.render();
        let doc = &YamlLoader::load_from_str(&text).unwrap()[0];
        assert_eq!(doc["goal"]["continuationModes"][1].as_str(), Some("rpc"));
        let roles = &doc["modelRoles"];
        // Configured first, the session's model last (not twice).
        assert_eq!(roles["smol"].as_str(), Some("local/tiny,openai-codex/gpt-5.6-luna"));
        // Unset roles start from the default role.
        assert_eq!(roles["task"].as_str(), Some("llama.cpp/qwen:medium,openai-codex/gpt-5.6-luna"));
        assert_eq!(roles["tiny"].as_str(), Some("llama.cpp/qwen:medium,openai-codex/gpt-5.6-luna"));
        // Other roles are kept as they are.
        assert_eq!(roles["default"].as_str(), Some("llama.cpp/qwen:medium"));
        assert_eq!(roles["advisor"].as_str(), Some("openai-codex/gpt-6-sol"));

        let plain = Overlay { goal_continue: false, subagent_fallback: Some((&user, "")), approval_mode: None, plan: None }.render();
        assert!(!plain.contains("goal:") && !plain.contains("modelRoles") && !plain.contains("tools:"));
        let empty = BTreeMap::new();
        let only_model = Overlay { goal_continue: false, subagent_fallback: Some((&empty, "a/b")), approval_mode: None, plan: None }.render();
        assert_eq!(YamlLoader::load_from_str(&only_model).unwrap()[0]["modelRoles"]["smol"].as_str(), Some("a/b"));
    }

    #[test]
    fn reads_approval_settings() {
        let cfg = "tools:\n  approvalMode: write\n  approval:\n    bash: deny\n    read: allow\n";
        assert_eq!(approval_mode(cfg), "write");
        assert_eq!(approval_mode("theme: x"), "yolo");
        assert_eq!(approval_mode("tools:\n  approvalMode: sometimes"), "yolo");
        let policies = tool_policies(cfg);
        assert_eq!(policies["bash"], "deny");
        assert_eq!(policies["read"], "allow");
    }

    #[test]
    fn renders_approval_mode_and_plan() {
        let mut user = BTreeMap::new();
        user.insert("bash".to_owned(), "deny".to_owned());
        user.insert("read".to_owned(), "allow".to_owned());
        user.insert("edit".to_owned(), "allow".to_owned());
        let text = Overlay { goal_continue: false, subagent_fallback: None, approval_mode: Some("always-ask"), plan: Some(&user) }.render();
        let doc = &YamlLoader::load_from_str(&text).unwrap()[0];
        assert_eq!(doc["tools"]["approvalMode"].as_str(), Some("always-ask"));
        let approval = &doc["tools"]["approval"];
        // Plan denies edits, even ones the user allows…
        assert_eq!(approval["edit"].as_str(), Some("deny"));
        assert_eq!(approval["write"].as_str(), Some("deny"));
        assert_eq!(approval["task"].as_str(), Some("prompt"));
        // …keeps the user's deny and other policies.
        assert_eq!(approval["bash"].as_str(), Some("deny"));
        assert_eq!(approval["read"].as_str(), Some("allow"));

        let mode_only = Overlay { goal_continue: false, subagent_fallback: None, approval_mode: Some("yolo"), plan: None }.render();
        let doc = &YamlLoader::load_from_str(&mode_only).unwrap()[0];
        assert_eq!(doc["tools"]["approvalMode"].as_str(), Some("yolo"));
        assert!(doc["tools"]["approval"].is_badvalue());
    }
}
