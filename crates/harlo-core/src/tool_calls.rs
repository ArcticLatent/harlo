//! The rules for harlo's own tools as an agent calls them.
//!
//! What a session is offered and told, who may use the wiki, and how a
//! call's arguments read. The app holds the sessions and acts on the answer;
//! the decisions are here.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::orchestration;
use crate::wiki;

/// The session calling harlo's tools, as harlo knows it.
#[derive(Debug, Clone, Default)]
pub struct Caller {
    /// Its agent ("pi", …; "" for one harlo doesn't know).
    pub harness: String,
    /// Started by harlo for a job (a workflow's step), to report back.
    pub helper: bool,
    /// A wiki session harlo started: it writes the wiki.
    pub writing: bool,
    /// Its project's settings; None outside a harlo project.
    pub project: Option<Project>,
}

/// A caller's project, as its settings have it.
#[derive(Debug, Clone)]
pub struct Project {
    /// The caller's folder.
    pub cwd: PathBuf,
    /// Its agents use the project's wiki.
    pub wiki: bool,
    /// Its agents are given the wiki's map with the note on it
    /// ([`wiki::reading_note`]): the project's Wiki map setting, else the
    /// one in Settings → Wiki.
    pub map: bool,
}

/// The tools a session is offered.
///
/// harlo's own, with a job's report and questions for a session harlo
/// started for one, then the wiki's check for a wiki session. (Other
/// sessions read the wiki with their own tools: [`reads_wiki`].)
pub fn offered(caller: &Caller) -> Vec<Value> {
    let list = |v: Value| v.as_array().cloned().unwrap_or_default();
    let mut tools = list(orchestration::tools());
    if caller.helper {
        tools.extend(list(orchestration::helper_tools()));
    }
    if caller.project.is_some() && caller.writing {
        tools.push(wiki::check_tool());
    }
    tools
}

/// What a session is told of its tools.
///
/// harlo's instructions. (The wiki's note follows when [`reads_wiki`].)
pub fn instructions() -> String {
    format!("{}\n\n{}", orchestration::BASE_INSTRUCTIONS, orchestration::DIAGRAMS)
}

/// The folder whose wiki the caller is told of ([`wiki::reading_note`]):
/// its project has the wiki on and one (`wiki_found`), and it isn't writing it.
pub fn reads_wiki(caller: &Caller, wiki_found: bool) -> Option<&Path> {
    caller.project.as_ref().filter(|p| p.wiki && !caller.writing && wiki_found).map(|p| p.cwd.as_path())
}

/// The folder a wiki tool works in, when the caller may use `tool`.
///
/// Only a wiki session has one: [`wiki::CHECK`], for what it wrote.
///
/// # Errors
///
/// When the caller isn't in a harlo project, or `tool` isn't one it may use.
pub fn wiki_folder<'a>(tool: &str, caller: &'a Caller) -> anyhow::Result<&'a Path> {
    let Some(p) = &caller.project else {
        anyhow::bail!("The wiki is for sessions in a harlo project.");
    };
    if tool != wiki::CHECK {
        anyhow::bail!("harlo's wiki has no tool {tool}: read the pages in .okf/ with your own tools.");
    }
    if !caller.writing {
        anyhow::bail!("Only a wiki session harlo starts (its Wiki panel) checks the wiki.");
    }
    Ok(&p.cwd)
}

/// A session's title as the tools list it: its name, else its first
/// message (80 characters at most).
pub fn session_title(name: &str, first_message: &str) -> String {
    if name.is_empty() { first_message.chars().take(80).collect() } else { name.to_owned() }
}

/// A session's status as the tools say it.
pub fn status_word(needs_input: bool, status: &str) -> &str {
    if needs_input { "waiting for input" } else { status }
}

/// The answers a `request_approval` offers: those given, else Approve and
/// Reject.
pub fn approval_options(options: &Value) -> Vec<String> {
    let given: Vec<String> = options.as_array().into_iter().flatten().filter_map(|o| o.as_str().map(str::trim)).filter(|o| !o.is_empty()).map(str::to_owned).collect();
    if given.is_empty() { vec!["Approve".into(), "Reject".into()] } else { given }
}

/// A deferred answer: done off the app's thread with what it says.
pub fn deferred(fields: Value) -> Value {
    let mut v = json!({ "_defer": true });
    if let (Some(o), Value::Object(f)) = (v.as_object_mut(), fields) {
        o.extend(f);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caller(harness: &str, project: Option<Project>) -> Caller {
        Caller { harness: harness.into(), project, ..Caller::default() }
    }

    fn project(wiki: bool) -> Project {
        Project { cwd: "/p".into(), wiki, map: true }
    }

    fn names(tools: &[Value]) -> Vec<&str> {
        tools.iter().filter_map(|t| t["name"].as_str()).collect()
    }

    #[test]
    fn each_session_gets_the_tools_its_project_allows() {
        let base = names(&offered(&caller("pi", None))).len();
        assert!(names(&offered(&caller("pi", None))).contains(&"request_approval"));
        // No wiki tools to read with (agents read .okf/ themselves).
        let with = offered(&caller("pi", Some(project(true))));
        assert_eq!(names(&with).len(), base);
        assert!(!names(&with).iter().any(|n| n.contains("wiki") || n.starts_with("plan_")));
        // A wiki session checks; a job's session reports.
        let writer = Caller { writing: true, helper: true, ..caller("pi", Some(project(false))) };
        let w = offered(&writer);
        assert!(names(&w).contains(&"wiki_check") && names(&w).contains(&"report_done"));
    }

    #[test]
    fn instructions_follow_the_same_settings() {
        assert!(instructions().starts_with(orchestration::BASE_INSTRUCTIONS));
        // The wiki's note: its project's wiki on and found, and not its writer.
        let reader = caller("pi", Some(project(true)));
        assert_eq!(reads_wiki(&reader, true), Some(Path::new("/p")));
        assert_eq!(reads_wiki(&reader, false), None);
        assert_eq!(reads_wiki(&caller("pi", Some(project(false))), true), None);
        assert_eq!(reads_wiki(&Caller { writing: true, ..reader }, true), None);
    }

    #[test]
    fn only_a_wiki_session_checks_the_wiki() {
        let refused = wiki_folder("wiki_check", &caller("pi", Some(project(true)))).unwrap_err().to_string();
        assert!(refused.starts_with("Only a wiki session"), "{refused}");
        wiki_folder("wiki_check", &caller("pi", None)).unwrap_err();
        let writer = Caller { writing: true, ..caller("pi", Some(project(false))) };
        assert_eq!(wiki_folder("wiki_check", &writer).unwrap(), Path::new("/p"));
        let gone = wiki_folder("wiki_read", &writer).unwrap_err().to_string();
        assert!(gone.contains(".okf/"), "{gone}");
    }

    #[test]
    fn arguments_read_as_agents_write_them() {
        assert_eq!(session_title("", &"a".repeat(100)).len(), 80);
        assert_eq!(status_word(true, "idle"), "waiting for input");
        assert_eq!(approval_options(&json!([" Yes ", "", "No"])), ["Yes", "No"]);
        assert_eq!(approval_options(&Value::Null), ["Approve", "Reject"]);
        assert_eq!(deferred(json!({ "cwd": "/p" })), json!({ "_defer": true, "cwd": "/p" }));
    }
}
