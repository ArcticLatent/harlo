//! Skills: instructions for particular tasks, each a folder with a
//! `SKILL.md` (frontmatter `name` and `description`, then the body). The
//! instructions list them, by name, description and path, and the model
//! reads one when a task matches it. `/skill:<name> what to do` sends one
//! with the prompt. Found in harlo-agent's own folder, `~/.agents/skills`
//! and `~/.claude/skills`, then the project's `.agents/skills` and
//! `.claude/skills`, which win by name; and in Claude Code's installed
//! plugins (not switched off in its settings), named `plugin:skill` as
//! there, unless one of the others is the same skill.

use std::path::{Path, PathBuf};

/// Listed in the instructions, at most (the rest by `/skill:`).
const LISTED: usize = 60;
/// Of a description, what's listed.
const DESCRIBED: usize = 400;
/// Of a skill sent with a prompt, at most.
const SENT: usize = 40_000;

#[derive(Clone, Debug, PartialEq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// Its SKILL.md.
    pub path: PathBuf,
    /// `disable-model-invocation: true`: only when the user sends it.
    pub by_hand: bool,
}

/// Where they're looked for: the user's, then the project's (from `cwd`
/// up to the repository's top).
pub fn folders(cwd: &Path, home: &Path, config: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![config.join("skills"), home.join(".agents/skills"), home.join(".claude/skills")];
    let top = top_of(cwd);
    let mut project = Vec::new();
    for d in cwd.ancestors() {
        // (The home folder's are the user's, already there.)
        if d == home {
            break;
        }
        project.push(d.join(".claude/skills"));
        project.push(d.join(".agents/skills"));
        // (Outside a repository: the working directory's only.)
        if top.as_deref().is_none_or(|t| d == t) {
            break;
        }
    }
    // (Nearer the working directory wins: last.)
    project.reverse();
    dirs.extend(project);
    dirs
}

/// The repository `cwd` is in, by its `.git`.
fn top_of(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors().find(|d| d.join(".git").exists()).map(Path::to_path_buf)
}

/// Claude Code's installed plugins' skill folders, each with its
/// plugin's name: those installed for everyone, or for a project `cwd` is
/// in, and not switched off (`enabledPlugins` false in its settings).
pub fn plugin_folders(cwd: &Path, home: &Path) -> Vec<(PathBuf, String)> {
    let claude = home.join(".claude");
    let Some(installed) = json_file(&claude.join("plugins/installed_plugins.json")) else { return Vec::new() };
    // (The user's settings, then the project's, which win.)
    let project = top_of(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let mut on = std::collections::HashMap::<String, bool>::new();
    for f in [claude.join("settings.json"), project.join(".claude/settings.json"), project.join(".claude/settings.local.json")] {
        for (id, v) in json_file(&f).and_then(|v| v["enabledPlugins"].as_object().cloned()).into_iter().flatten() {
            if let Some(b) = v.as_bool() {
                on.insert(id, b);
            }
        }
    }
    let mut out = Vec::new();
    for (id, installs) in installed["plugins"].as_object().into_iter().flatten() {
        if on.get(id) == Some(&false) {
            continue;
        }
        for i in installs.as_array().into_iter().flatten() {
            if i["projectPath"].as_str().is_some_and(|p| !cwd.starts_with(p)) {
                continue;
            }
            let Some(at) = i["installPath"].as_str().map(PathBuf::from) else { continue };
            let meta = json_file(&at.join(".claude-plugin/plugin.json")).unwrap_or_default();
            let name = meta["name"].as_str().map(str::to_owned).unwrap_or_else(|| id.split('@').next().unwrap_or(id).to_owned());
            // (`skills/`, and the folders its plugin.json adds.)
            let mut dirs = vec![at.join("skills")];
            let extra: Vec<&str> = match &meta["skills"] {
                serde_json::Value::String(s) => vec![s.as_str()],
                serde_json::Value::Array(a) => a.iter().filter_map(|v| v.as_str()).collect(),
                _ => Vec::new(),
            };
            for e in extra {
                let d = at.join(e.trim_start_matches("./").trim_end_matches('/'));
                if !dirs.contains(&d) {
                    dirs.push(d);
                }
            }
            out.extend(dirs.into_iter().map(|d| (d, name.clone())));
        }
    }
    out
}

fn json_file(p: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

/// The skills there, sorted by name; a later folder's wins by name.
pub fn find(cwd: &Path, home: &Path, config: &Path) -> Vec<Skill> {
    let mut found: Vec<Skill> = Vec::new();
    let plugins = plugin_folders(cwd, home);
    let dirs = plugins.into_iter().map(|(d, p)| (d, Some(p))).chain(folders(cwd, home, config).into_iter().map(|d| (d, None)));
    for (dir, plugin) in dirs {
        // (A folder that is itself one skill, or one of skills.)
        let mut entries: Vec<PathBuf> = if dir.join("SKILL.md").is_file() {
            vec![dir.clone()]
        } else {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            entries.flatten().map(|e| e.path()).collect()
        };
        entries.sort();
        for e in entries {
            let file = e.join("SKILL.md");
            let Ok(text) = std::fs::read_to_string(&file) else { continue };
            let folder = e.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let (head, _) = split(&text);
            let name = field(head, "name").filter(|n| !n.is_empty()).unwrap_or(folder);
            let name = match &plugin {
                Some(p) => format!("{p}:{name}"),
                None => name,
            };
            let description = field(head, "description").unwrap_or_default();
            let by_hand = field(head, "disable-model-invocation").is_some_and(|v| v == "true");
            let skill = Skill { name, description, path: file, by_hand };
            match found.iter_mut().find(|s| s.name == skill.name) {
                Some(s) => *s = skill,
                None => found.push(skill),
            }
        }
    }
    // A plugin's skill that's also in a skills folder (the same name and
    // description): listed once, as that one.
    let plain: Vec<(String, String)> = found.iter().filter(|s| !s.name.contains(':')).map(|s| (s.name.clone(), s.description.clone())).collect();
    found.retain(|s| match s.name.split_once(':') {
        Some((_, bare)) => !plain.iter().any(|(n, d)| n == bare && *d == s.description),
        None => true,
    });
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// The frontmatter and the body.
fn split(text: &str) -> (&str, &str) {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(rest) = t.strip_prefix("---").and_then(|r| r.strip_prefix('\n').or_else(|| r.strip_prefix("\r\n"))) else { return ("", t) };
    let end = rest.find("\n---").map(|i| (i, i + 4));
    match end {
        Some((i, j)) => {
            let body = &rest[j..];
            let body = body.split_once('\n').map_or("", |(_, b)| b);
            (&rest[..i], body)
        }
        None => ("", t),
    }
}

/// A frontmatter field's value: on its line (quotes taken off), or the
/// indented lines after `key: >` / `key: |`.
fn field(head: &str, key: &str) -> Option<String> {
    let lines: Vec<&str> = head.lines().collect();
    let i = lines.iter().position(|l| l.split_once(':').is_some_and(|(k, _)| k.trim() == key && !l.starts_with(' ')))?;
    let value = lines[i].split_once(':').map(|(_, v)| v.trim()).unwrap_or_default();
    if value.is_empty() || matches!(value, ">" | "|" | ">-" | "|-") {
        let more: Vec<&str> = lines[i + 1..].iter().take_while(|l| l.starts_with(' ') || l.starts_with('\t') || l.trim().is_empty()).map(|l| l.trim()).collect();
        let joined = more.join(if value.starts_with('|') { "\n" } else { " " });
        return Some(joined.trim().to_owned());
    }
    let unquoted = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\''))).unwrap_or(value);
    Some(unquoted.to_owned())
}

/// What the instructions say of them ("" none the model may use).
pub fn listing(skills: &[Skill]) -> String {
    let listed: Vec<&Skill> = skills.iter().filter(|s| !s.by_hand).take(LISTED).collect();
    if listed.is_empty() {
        return String::new();
    }
    let mut s = String::from(
        "\n<skills>\nSkills: instructions for particular kinds of task, each in a SKILL.md file. When a task matches a skill's description, read its file before you start and follow it (paths in it are from its folder).\n",
    );
    for k in listed {
        let d: String = k.description.chars().take(DESCRIBED).collect();
        s.push_str(&format!("- {}: {} ({})\n", k.name, d.trim(), k.path.display()));
    }
    s.push_str("</skills>\n");
    s
}

/// `/skill:<name> rest` as the prompt the model gets: the skill's body,
/// then the rest. None when it isn't one (or there's no such skill).
pub fn expand(text: &str, skills: &[Skill]) -> Option<String> {
    let rest = text.trim_start().strip_prefix("/skill:")?;
    let (name, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let skill = skills.iter().find(|s| s.name == name)?;
    let text = std::fs::read_to_string(&skill.path).ok()?;
    let whole = split(&text).1.trim();
    let mut body: String = whole.chars().take(SENT).collect();
    if body.len() < whole.len() {
        body.push_str(&format!("\n\n(Cut short: the rest is in {}.)", skill.path.display()));
    }
    let folder = skill.path.parent().unwrap_or(Path::new("/"));
    let args = args.trim();
    Some(format!(
        "<skill name=\"{}\" folder=\"{}\">\n{body}\n</skill>\n\n{}",
        skill.name,
        folder.display(),
        if args.is_empty() { format!("Use the skill {}.", skill.name) } else { args.to_owned() }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn found_listed_and_sent() {
        let root = std::env::temp_dir().join(format!("harlo-agent-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (home, config) = (root.join("home"), root.join("home/.config/harlo-agent"));
        let proj = home.join("proj");
        std::fs::create_dir_all(proj.join(".git")).unwrap();
        write(&home.join(".claude/skills/deploy/SKILL.md"), "---\nname: deploy\ndescription: \"Deploy the app (the user's way)\"\n---\nUser's deploy steps.\n");
        write(&config.join("skills/review/SKILL.md"), "---\ndescription: >\n  Review a change\n  for bugs.\n---\n# Review\nLook hard.\n");
        write(&home.join(".agents/skills/secret/SKILL.md"), "---\nname: secret\ndescription: by hand only\ndisable-model-invocation: true\n---\nShh.\n");
        write(&proj.join(".claude/skills/deploy/SKILL.md"), "---\nname: deploy\ndescription: Deploy this project\n---\nProject deploy: run ./ship.\n");
        write(&proj.join(".agents/skills/broken/README.md"), "no SKILL.md here");
        let skills = find(&proj, &home, &config);
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["deploy", "review", "secret"]);
        assert_eq!(skills[0].description, "Deploy this project");
        assert!(skills[0].path.starts_with(&proj));
        assert_eq!(skills[1].description, "Review a change for bugs.");
        let l = listing(&skills);
        assert!(l.contains("- deploy: Deploy this project (") && l.contains("- review: Review a change for bugs.") && !l.contains("secret"), "{l}");
        let sent = expand("/skill:deploy to staging", &skills).unwrap();
        assert!(sent.starts_with("<skill name=\"deploy\" folder=\"") && sent.contains("Project deploy: run ./ship.\n</skill>\n\nto staging") && !sent.contains("description:"), "{sent}");
        assert!(expand("/skill:secret", &skills).unwrap().ends_with("Use the skill secret."));
        assert!(expand("/skill:nope x", &skills).is_none() && expand("deploy", &skills).is_none());
        assert!(listing(&[]).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn claude_codes_plugins_skills() {
        let root = std::env::temp_dir().join(format!("harlo-agent-plugin-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (home, config) = (root.join("home"), root.join("home/.config/harlo-agent"));
        let proj = home.join("proj");
        let other = home.join("other");
        std::fs::create_dir_all(proj.join(".git")).unwrap();
        std::fs::create_dir_all(other.join(".git")).unwrap();
        let cache = home.join(".claude/plugins/cache");
        let (design, tools, off, local) = (cache.join("m/design/1.0"), cache.join("m/tools/2.0"), cache.join("m/off/1.0"), cache.join("m/local/1.0"));
        write(&home.join(".claude/plugins/installed_plugins.json"), &serde_json::json!({ "version": 2, "plugins": {
            "design@m": [{ "scope": "user", "installPath": design }],
            "tools@m": [{ "scope": "user", "installPath": tools }],
            "off@m": [{ "scope": "user", "installPath": off }],
            "local@m": [{ "scope": "project", "projectPath": proj, "installPath": local }],
        }}).to_string());
        write(&home.join(".claude/settings.json"), r#"{ "enabledPlugins": { "design@m": true, "off@m": true } }"#);
        write(&proj.join(".claude/settings.local.json"), r#"{ "enabledPlugins": { "off@m": false } }"#);
        write(&design.join(".claude-plugin/plugin.json"), r#"{ "name": "design", "skills": "./more/" }"#);
        write(&design.join("skills/critique/SKILL.md"), "---\nname: critique\ndescription: Review a design\n---\nLook.\n");
        write(&design.join("more/polish/SKILL.md"), "---\ndescription: Polish it\n---\nFinish.\n");
        // (The same skill in the user's folder: listed once, as that one.)
        write(&design.join("skills/same/SKILL.md"), "---\nname: same\ndescription: In both places\n---\nA.\n");
        write(&home.join(".claude/skills/same/SKILL.md"), "---\nname: same\ndescription: In both places\n---\nA.\n");
        write(&tools.join("skills/lint/SKILL.md"), "---\nname: lint\ndescription: Lint\n---\nRun it.\n");
        write(&off.join("skills/hidden/SKILL.md"), "---\nname: hidden\ndescription: Off\n---\nNo.\n");
        write(&local.join("skills/here/SKILL.md"), "---\nname: here\ndescription: This project's\n---\nYes.\n");
        let names = |cwd: &Path| find(cwd, &home, &config).into_iter().map(|s| s.name).collect::<Vec<_>>();
        assert_eq!(names(&proj), ["design:critique", "design:polish", "local:here", "same", "tools:lint"]);
        assert_eq!(names(&other), ["design:critique", "design:polish", "off:hidden", "same", "tools:lint"], "another project: its own plugins and switches");
        let skills = find(&proj, &home, &config);
        let sent = expand("/skill:design:critique the sidebar", &skills).unwrap();
        assert!(sent.starts_with("<skill name=\"design:critique\"") && sent.contains("Look.\n</skill>\n\nthe sidebar"), "{sent}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
