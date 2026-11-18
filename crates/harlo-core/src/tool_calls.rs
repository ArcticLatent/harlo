//! The rules for harlo's own tools as an agent calls them.
//!
//! What a session is offered and told, who may use the wiki and the shared
//! plan, and how a call's arguments read (folders, budgets, titles). The
//! app holds the sessions and acts on the answer; the decisions are here.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use harlo_harness::HarnessKind;
use serde_json::{Value, json};

use crate::orchestration::{self, Starting};
use crate::{plan, remote, wiki};

/// The session calling harlo's tools, as harlo knows it.
#[derive(Debug, Clone, Default)]
pub struct Caller {
    /// Its agent ("harlo", "claude", …; "" for one harlo doesn't know).
    pub harness: String,
    /// Started by another session, to report back to it.
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
    /// The shared plan is on.
    pub plan: bool,
    /// harlo commits each finished step.
    pub commits: bool,
    /// Its agents may look names up in the code index (Harlo with its own
    /// tool, others with harlo's: [`lookup_tool`]).
    pub lookup: bool,
}

/// The tools a session is offered.
///
/// harlo's own, without `request_approval` for Harlo (it asks with its own
/// ask tool), with a helper's report and questions for a helper, then the
/// wiki's check for a wiki session, and the plan's when it's on. (Other
/// sessions read the wiki with their own tools: [`reads_wiki`].)
pub fn offered(starting: Starting, caller: &Caller) -> Vec<Value> {
    let list = |v: Value| v.as_array().cloned().unwrap_or_default();
    let mut tools = list(orchestration::tools(starting));
    if caller.harness == "harlo" {
        tools.retain(|t| t["name"] != "request_approval");
    }
    if caller.helper {
        tools.extend(list(orchestration::helper_tools()));
    }
    if let Some(p) = &caller.project {
        if caller.writing {
            tools.push(wiki::check_tool());
        }
        if p.plan {
            tools.extend(list(plan::tools(p.commits)));
        }
        if p.lookup && caller.harness != "harlo" {
            tools.push(lookup_tool());
        }
    }
    tools
}

/// The code index's lookup tool, as harlo serves it.
pub fn lookup_tool() -> Value {
    json!({ "name": LOOKUP, "description": harlo_okf::indexed::LOOKUP_ABOUT, "inputSchema": harlo_okf::indexed::lookup_parameters() })
}

/// The lookup tool's name.
pub const LOOKUP: &str = harlo_okf::indexed::LOOKUP_TOOL;

/// The folder the caller looks names up in, when its project has lookup on.
///
/// # Errors
///
/// When the caller isn't in a harlo project, or its project has it off.
pub fn lookup_folder(caller: &Caller) -> anyhow::Result<&Path> {
    match &caller.project {
        Some(p) if p.lookup => Ok(&p.cwd),
        Some(_) => anyhow::bail!("Code lookup is off for this project (its settings in harlo)."),
        None => anyhow::bail!("Code lookup is for sessions in a harlo project."),
    }
}

/// The lookup a call asks for (its `name`, `path` and `depth`).
pub fn lookup_query(args: &Value) -> harlo_okf::indexed::Lookup<'_> {
    harlo_okf::indexed::Lookup {
        name: args["name"].as_str().unwrap_or_default().trim(),
        path: args["path"].as_str().map(|p| p.trim().trim_start_matches("./")).filter(|p| !p.is_empty() && *p != "."),
        depth: args["depth"].as_u64().and_then(|d| usize::try_from(d).ok()).unwrap_or(1),
    }
}

/// What a session is told of its tools.
///
/// harlo's instructions and when to start sessions, then the plan's.
/// Claude Code plans in its own flow, so it isn't told to put a Plan mode
/// plan in the card. Claude Code, which sees harlo's tools by name only
/// until it looks one up, is told of `lookup` when the project has it on.
/// (The wiki's note follows when [`reads_wiki`].)
pub fn instructions(starting: Starting, caller: &Caller) -> String {
    let mut text = format!("{} {}\n\n{}", orchestration::BASE_INSTRUCTIONS, orchestration::when_to_start(starting), orchestration::DIAGRAMS);
    if let Some(p) = &caller.project
        && p.plan
    {
        text = format!("{text}\n\n{}", plan::instructions(p.commits, caller.harness != "claude"));
    }
    if caller.harness == "claude" && caller.project.as_ref().is_some_and(|p| p.lookup) {
        text = format!("{text}\n\n{LOOKUP_NOTE}");
    }
    text
}

/// What Claude Code is told of `lookup` (its description reaches it only
/// when it looks the tool up).
pub const LOOKUP_NOTE: &str = "harlo's lookup tool answers from this project's code index: where a function, method or type is declared, its signature, what it calls and what calls it. For where-is and who-calls questions it's quicker than grep and read.";

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

/// The caller's project, when it may use the shared plan's `tool`.
///
/// `handed_out`: its plan runs with helpers, so harlo hands out its steps
/// and the agent leaves them alone.
///
/// # Errors
///
/// When the caller isn't a session in a harlo project, its plan is off, or
/// `tool` changes steps of a plan that runs with helpers.
pub fn plan_project<'a>(tool: &str, caller: &'a Caller, known: bool, handed_out: bool) -> anyhow::Result<&'a Project> {
    let (true, Some(p)) = (known, &caller.project) else {
        anyhow::bail!("The shared plan is for sessions in a harlo project on this computer.");
    };
    if !p.plan {
        anyhow::bail!("The shared plan is off for this project (its settings in harlo).");
    }
    if handed_out && matches!(tool, "plan_set" | "plan_start" | "plan_done") {
        anyhow::bail!("This plan is running with helpers: harlo hands its steps to them, so leave the steps alone until the run ends (the user can stop it).");
    }
    Ok(p)
}

/// The agent a call names ("harlo", "claude"; "demo" only by that name).
///
/// # Errors
///
/// When `agent` isn't one harlo runs.
pub fn agent(agent: &str) -> anyhow::Result<HarnessKind> {
    let agent = agent.trim().to_lowercase();
    HarnessKind::parse(&agent)
        .filter(|k| *k != HarnessKind::Demo || agent == "demo")
        .ok_or_else(|| anyhow::anyhow!("Unknown agent \"{agent}\": use harlo or claude (list_agents)."))
}

/// Where a new session works: `folder` as the caller's machine reads it.
///
/// Empty: the caller's folder, else `home`. A `ssh://` path as it is; an
/// absolute one on the caller's machine; a relative one from the caller's
/// folder.
pub fn folder(folder: &str, caller_cwd: Option<&Path>, home: &Path) -> PathBuf {
    let path = Path::new(folder);
    if folder.is_empty() {
        return caller_cwd.map_or_else(|| home.to_owned(), Path::to_owned);
    }
    if remote::is_remote(path) {
        return path.to_owned();
    }
    if path.is_absolute() {
        return match caller_cwd.and_then(remote::host_of) {
            Some(h) => remote::join(&h, folder),
            None => path.to_owned(),
        };
    }
    caller_cwd.map_or_else(|| path.to_owned(), |c| c.join(path))
}

/// A token budget a call asks for: a number, or text such as "400k" or
/// "2m" (None: none asked).
///
/// # Errors
///
/// When it's given but isn't a number of tokens above 0.
pub fn budget(asked: &Value) -> anyhow::Result<Option<u64>> {
    let bad = || anyhow::Error::msg("Give the budget as a number of tokens, as 400k or 2m.");
    match asked {
        Value::Number(n) => n.as_u64().filter(|n| *n > 0).map(Some).ok_or_else(bad),
        Value::String(b) if !b.trim().is_empty() => orchestration::parse_tokens(b).map(Some).ok_or_else(bad),
        _ => Ok(None),
    }
}

/// A new session's title: the one asked for, else its prompt's first line
/// (60 characters at most).
pub fn title(asked: &str, prompt: &str) -> String {
    if asked.is_empty() { prompt.lines().next().unwrap_or_default().chars().take(60).collect() } else { asked.to_owned() }
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

    fn project(wiki: bool, plan: bool) -> Project {
        Project { cwd: "/p".into(), wiki, plan, commits: false, lookup: false }
    }

    fn names(tools: &[Value]) -> Vec<&str> {
        tools.iter().filter_map(|t| t["name"].as_str()).collect()
    }

    #[test]
    fn each_session_gets_the_tools_its_project_allows() {
        let base = names(&offered(Starting::WhenAsked, &caller("claude", None))).len();
        let harlo = offered(Starting::WhenAsked, &caller("harlo", None));
        assert!(!names(&harlo).contains(&"request_approval"), "Harlo asks with its own tool");
        assert!(names(&offered(Starting::WhenAsked, &caller("claude", None))).contains(&"request_approval"));
        // The plan's when on; no wiki tools to read with (agents read .okf/ themselves).
        let with = offered(Starting::WhenAsked, &caller("claude", Some(project(true, true))));
        assert!(names(&with).contains(&"plan_read") && !names(&with).iter().any(|n| n.contains("wiki")));
        let off = offered(Starting::WhenAsked, &caller("claude", Some(project(false, false))));
        assert_eq!(names(&off).len(), base);
        // A wiki session checks; a helper reports.
        let writer = Caller { writing: true, helper: true, ..caller("claude", Some(project(false, false))) };
        let w = offered(Starting::WhenAsked, &writer);
        assert!(names(&w).contains(&"wiki_check") && names(&w).contains(&"report_done"));
        // Code lookup: harlo's for Claude Code, only with it on (Harlo has its own).
        let looks = Project { lookup: true, ..project(false, false) };
        assert!(names(&offered(Starting::WhenAsked, &caller("claude", Some(looks.clone())))).contains(&LOOKUP));
        assert!(!names(&offered(Starting::WhenAsked, &caller("harlo", Some(looks.clone())))).contains(&LOOKUP));
        assert!(!names(&with).contains(&LOOKUP));
        assert_eq!(lookup_folder(&caller("claude", Some(looks))).unwrap(), Path::new("/p"));
        lookup_folder(&caller("claude", Some(project(true, true)))).unwrap_err();
        lookup_folder(&caller("claude", None)).unwrap_err();
        let q = json!({ "name": " Ledger::post ", "path": "./src/", "depth": 2 });
        let q = lookup_query(&q);
        assert_eq!((q.name, q.path, q.depth), ("Ledger::post", Some("src/"), 2));
    }

    #[test]
    fn instructions_follow_the_same_settings() {
        let plain = instructions(Starting::WhenAsked, &caller("claude", None));
        assert!(plain.starts_with(orchestration::BASE_INSTRUCTIONS));
        // The wiki's note: its project's wiki on and found, and not its writer.
        let reader = caller("claude", Some(project(true, false)));
        assert_eq!(reads_wiki(&reader, true), Some(Path::new("/p")));
        assert_eq!(reads_wiki(&reader, false), None);
        assert_eq!(reads_wiki(&caller("claude", Some(project(false, false))), true), None);
        assert_eq!(reads_wiki(&Caller { writing: true, ..reader }, true), None);
        // Claude Code plans in its own flow; Harlo puts its Plan mode plan in the card.
        let claude = instructions(Starting::WhenAsked, &caller("claude", Some(project(false, true))));
        let harlo = instructions(Starting::WhenAsked, &caller("harlo", Some(project(false, true))));
        assert!(claude.contains(&plan::instructions(false, false)) && harlo.contains(&plan::instructions(false, true)));
        // Claude Code is told of lookup when it's on; Harlo has the tool's own description.
        let with_lookup = |harness: &str| {
            let mut c = caller(harness, Some(project(false, false)));
            if let Some(p) = c.project.as_mut() {
                p.lookup = true;
            }
            instructions(Starting::WhenAsked, &c)
        };
        assert!(with_lookup("claude").contains(LOOKUP_NOTE) && !with_lookup("harlo").contains(LOOKUP_NOTE));
        assert!(!instructions(Starting::WhenAsked, &caller("claude", Some(project(false, false)))).contains(LOOKUP_NOTE));
    }

    #[test]
    fn only_a_wiki_session_checks_the_wiki() {
        let refused = wiki_folder("wiki_check", &caller("claude", Some(project(true, false)))).unwrap_err().to_string();
        assert!(refused.starts_with("Only a wiki session"), "{refused}");
        wiki_folder("wiki_check", &caller("claude", None)).unwrap_err();
        let writer = Caller { writing: true, ..caller("claude", Some(project(false, false))) };
        assert_eq!(wiki_folder("wiki_check", &writer).unwrap(), Path::new("/p"));
        let gone = wiki_folder("wiki_read", &writer).unwrap_err().to_string();
        assert!(gone.contains(".okf/"), "{gone}");
    }

    #[test]
    fn a_plan_run_with_helpers_keeps_its_steps() {
        let c = caller("claude", Some(project(false, true)));
        plan_project("plan_read", &c, true, true).unwrap();
        plan_project("plan_set", &c, true, false).unwrap();
        assert!(plan_project("plan_done", &c, true, true).unwrap_err().to_string().contains("running with helpers"));
        plan_project("plan_read", &c, false, false).unwrap_err();
        assert!(plan_project("plan_read", &caller("claude", Some(project(false, false))), true, false).unwrap_err().to_string().contains("off"));
    }

    #[test]
    fn a_new_sessions_folder_is_read_on_the_callers_machine() {
        let home = Path::new("/home/u");
        assert_eq!(folder("", None, home), home);
        assert_eq!(folder("", Some(Path::new("/w")), home), Path::new("/w"));
        assert_eq!(folder("sub", Some(Path::new("/w")), home), Path::new("/w/sub"));
        assert_eq!(folder("/abs", Some(Path::new("/w")), home), Path::new("/abs"));
        assert_eq!(folder("/abs", Some(Path::new("ssh://nas/w")), home), remote::join("nas", "/abs"));
        assert_eq!(folder("ssh://pi/x", Some(Path::new("/w")), home), Path::new("ssh://pi/x"));
    }

    #[test]
    fn arguments_read_as_agents_write_them() {
        assert_eq!(agent(" Claude ").unwrap(), HarnessKind::Claude);
        assert_eq!(agent("demo").unwrap(), HarnessKind::Demo);
        agent("pi").unwrap_err();
        assert_eq!(budget(&json!(400_000)).unwrap(), Some(400_000));
        assert_eq!(budget(&json!("2m")).unwrap(), Some(2_000_000));
        assert_eq!(budget(&json!("")).unwrap(), None);
        assert_eq!(budget(&Value::Null).unwrap(), None);
        budget(&json!(0)).unwrap_err();
        budget(&json!("lots")).unwrap_err();
        assert_eq!(title("", "Fix the tests\nthen more"), "Fix the tests");
        assert_eq!(title("Named", "x"), "Named");
        assert_eq!(session_title("", &"a".repeat(100)).len(), 80);
        assert_eq!(status_word(true, "idle"), "waiting for input");
        assert_eq!(approval_options(&json!([" Yes ", "", "No"])), ["Yes", "No"]);
        assert_eq!(approval_options(&Value::Null), ["Approve", "Reject"]);
        assert_eq!(deferred(json!({ "cwd": "/p" })), json!({ "_defer": true, "cwd": "/p" }));
    }
}
