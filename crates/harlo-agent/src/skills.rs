//! Skills: instructions for particular tasks, each a folder with a
//! `SKILL.md` (frontmatter `name` and `description`, then the body). The
//! instructions list them, by name, description and path, and the model
//! reads one when a task matches it. `/skill:<name> what to do` sends one
//! with the prompt. Found in harlo-agent's own folder, `~/.agents/skills`
//! and `~/.claude/skills`, then the project's `.agents/skills` and
//! `.claude/skills`, which win by name.

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

/// The skills there, sorted by name; a later folder's wins by name.
pub fn find(cwd: &Path, home: &Path, config: &Path) -> Vec<Skill> {
    let mut found: Vec<Skill> = Vec::new();
    for dir in folders(cwd, home, config) {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for e in entries {
            let file = e.join("SKILL.md");
            let Ok(text) = std::fs::read_to_string(&file) else { continue };
            let folder = e.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let (head, _) = split(&text);
            let name = field(head, "name").filter(|n| !n.is_empty()).unwrap_or(folder);
            let description = field(head, "description").unwrap_or_default();
            let by_hand = field(head, "disable-model-invocation").is_some_and(|v| v == "true");
            let skill = Skill { name, description, path: file, by_hand };
            match found.iter_mut().find(|s| s.name == skill.name) {
                Some(s) => *s = skill,
                None => found.push(skill),
            }
        }
    }
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
}
