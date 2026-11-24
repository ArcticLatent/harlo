//! Skills: instructions kept in Markdown files that a model loads when a
//! task calls for them, or that a person attaches to a prompt.
//!
//! They are found in `.claude/skills`, `.agents/skills` and
//! `.harlo/skills` in the project's folders, the
//! same in the home folder, the config folder's `skills`, and the folders
//! the config's `skills` entries name. A folder holding a `SKILL.md` is one
//! skill named after the folder; a Markdown file at a source's top is one
//! named after the file. Where two share an ID, the later source wins.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// Its ID: the folder's name, or the file's without `.md`.
    pub id: String,
    /// The name shown for it.
    pub name: String,
    /// What it's for; a skill without one isn't offered to the model.
    pub description: Option<String>,
    /// Its file.
    pub path: PathBuf,
    /// Its instructions, without the front matter.
    pub content: String,
    /// Whether the model is offered it; people can load it either way.
    pub autoinvoke: bool,
}

impl Skill {
    /// As the server lists it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id, "name": self.name, "description": self.description,
            "autoinvoke": self.autoinvoke, "path": self.path, "content": self.content,
        })
    }

    /// Whether the model is told about it.
    #[must_use]
    pub fn offered(&self) -> bool {
        self.autoinvoke && self.description.is_some()
    }

    /// What the model reads when the skill is loaded: its instructions,
    /// its folder, and some of the files beside it.
    #[must_use]
    pub fn loaded(&self) -> String {
        let base = self.path.parent().unwrap_or(Path::new(""));
        let mut files: Vec<String> = Vec::new();
        if self.path.file_name().is_some_and(|n| n == "SKILL.md") {
            let walk = ignore::WalkBuilder::new(base)
                .max_depth(Some(3))
                .standard_filters(true)
                .build();
            for entry in walk.flatten() {
                if files.len() >= SAMPLED_FILES {
                    break;
                }
                let path = entry.path();
                if path.is_file() && path != self.path {
                    files.push(path.display().to_string());
                }
            }
        }
        let mut text = format!(
            "<skill_content name=\"{}\">\n# Skill: {}\n\n{}\n\nThe skill's folder: {}\n\
             Paths in the skill are relative to it.",
            self.id,
            self.name,
            self.content.trim(),
            base.display()
        );
        if !files.is_empty() {
            text.push_str(" Some of the files there:\n<skill_files>\n");
            text.push_str(&files.join("\n"));
            text.push_str("\n</skill_files>");
        }
        text.push_str("\n</skill_content>");
        text
    }
}

/// A skill as loaded into a prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attached {
    /// The skill's ID.
    pub id: String,
    /// Its name.
    pub name: String,
    /// What the model read.
    pub text: String,
}

impl From<&Skill> for Attached {
    fn from(skill: &Skill) -> Self {
        Self {
            id: skill.id.clone(),
            name: skill.name.clone(),
            text: skill.loaded(),
        }
    }
}

/// Files listed beside a loaded skill, at most.
const SAMPLED_FILES: usize = 10;
/// How deep below a source skills are looked for.
const DEPTH: usize = 6;

/// Where skills are looked for.
#[derive(Debug, Clone, Copy)]
pub struct Search<'a> {
    /// The session's folder.
    pub workspace: &'a Path,
    /// The project's root (its git checkout's top), if known; project
    /// folders are searched from the workspace up to it.
    pub project_root: Option<&'a Path>,
    /// The home folder.
    pub home: &'a Path,
    /// Harlo's config folder (`$XDG_CONFIG_HOME/harlo`).
    pub config_dir: &'a Path,
    /// The config's `skills` entries: folders, relative to the workspace,
    /// from the home folder (`~/`) or absolute.
    pub configured: &'a [String],
}

/// The skills found, by ID.
#[must_use]
pub fn discover(search: &Search<'_>) -> Vec<Skill> {
    // The project's folders, outermost first, so nearer ones win.
    let mut folders = vec![search.workspace.to_path_buf()];
    if let Some(root) = search.project_root {
        let mut at = search.workspace;
        while at != root {
            match at.parent() {
                Some(up) if up.starts_with(root) => {
                    folders.push(up.to_path_buf());
                    at = up;
                }
                _ => break,
            }
        }
    }
    folders.reverse();
    let mut sources: Vec<PathBuf> = Vec::new();
    for kind in [".claude", ".agents"] {
        sources.push(search.home.join(kind).join("skills"));
        sources.extend(folders.iter().map(|f| f.join(kind).join("skills")));
    }
    sources.push(search.config_dir.join("skills"));
    sources.extend(
        folders
            .iter()
            .map(|f| f.join(crate::config::PROJECT_FOLDER).join("skills")),
    );
    for entry in search.configured {
        let path = if let Some(rest) = entry.strip_prefix("~/") {
            search.home.join(rest)
        } else {
            search.workspace.join(entry)
        };
        sources.push(path);
    }
    let mut found: BTreeMap<String, Skill> = BTreeMap::new();
    for source in sources {
        for skill in in_source(&source) {
            found.insert(skill.id.clone(), skill);
        }
    }
    found.into_values().collect()
}

/// The skills in one source folder.
fn in_source(source: &Path) -> Vec<Skill> {
    let mut skills = Vec::new();
    let Ok(top) = std::fs::read_dir(source) else {
        return skills;
    };
    let mut entries: Vec<PathBuf> = top.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            skills.extend(read(&path));
        } else if path.is_dir() {
            nested(&path, 1, &mut skills);
        }
    }
    skills
}

/// Looks for `SKILL.md` files in `folder` and below it.
fn nested(folder: &Path, depth: usize, skills: &mut Vec<Skill>) {
    let hidden = folder
        .file_name()
        .and_then(|n| n.to_str())
        .is_none_or(|n| n.starts_with('.') || n == "node_modules");
    if hidden || depth > DEPTH {
        return;
    }
    let file = folder.join("SKILL.md");
    if file.is_file() {
        skills.extend(read(&file));
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        nested(&dir, depth + 1, skills);
    }
}

#[derive(Debug, Default, Deserialize)]
struct Front {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    metadata: BTreeMap<String, Value>,
    #[serde(rename = "disable-model-invocation")]
    disable_model_invocation: Option<bool>,
}

/// Reads the skill in `file`.
fn read(file: &Path) -> Option<Skill> {
    let source = std::fs::read_to_string(file).ok()?;
    let id = if file.file_name().is_some_and(|n| n == "SKILL.md") {
        file.parent()?.file_name()?.to_str()?.to_owned()
    } else {
        file.file_stem()?.to_str()?.to_owned()
    };
    let parsed = Matter::<YAML>::new().parse::<Front>(&source).ok();
    let (front, content) = match parsed {
        Some(p) => (p.data.unwrap_or_default(), p.content),
        None => (Front::default(), source),
    };
    let autoinvoke = front
        .metadata
        .get("harlo/autoinvoke")
        .and_then(Value::as_bool)
        .or(front.disable_model_invocation.map(|off| !off))
        .unwrap_or(true);
    Some(Skill {
        name: front.name.unwrap_or_else(|| id.clone()),
        id,
        description: front.description.filter(|d| !d.trim().is_empty()),
        path: file.to_path_buf(),
        content,
        autoinvoke,
    })
}

/// The `skill` tool's definition, listing the skills offered.
#[must_use]
pub fn definition(offered: &[&Skill]) -> Value {
    let mut description = "Loads a skill's instructions into the conversation. Load one when \
        the task matches its description, or when the person names it.\n\nSkills:"
        .to_owned();
    for skill in offered {
        let _ = write!(
            description,
            "\n- {}: {}",
            skill.id,
            skill.description.as_deref().unwrap_or_default()
        );
    }
    json!({
        "name": "skill",
        "description": description,
        "parameters": { "type": "object", "properties": {
            "id": { "type": "string", "description": "The skill's ID, from the list or as the person named it." },
        }, "required": ["id"] },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearer_sources_win_and_front_matter_is_read() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().join("project");
        let home = dir.path().join("home");
        let write = |path: PathBuf, text: &str| {
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(path, text).expect("write");
        };
        write(
            home.join(".claude/skills/notes/SKILL.md"),
            "---\ndescription: From home.\n---\nHome notes.\n",
        );
        write(
            root.join(".harlo/skills/notes/SKILL.md"),
            "---\nname: Notes\ndescription: From the project.\n---\nProject notes.\n",
        );
        write(
            root.join(".agents/skills/quiet.md"),
            "---\ndescription: Hidden.\ndisable-model-invocation: true\n---\nShh.\n",
        );
        write(root.join(".harlo/skills/.hidden/x/SKILL.md"), "no\n");
        let config = dir.path().join("config");
        let skills = discover(&Search {
            workspace: &root,
            project_root: Some(&root),
            home: &home,
            config_dir: &config,
            configured: &[],
        });
        let ids: Vec<&str> = skills.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["notes", "quiet"]);
        assert_eq!(skills[0].name, "Notes");
        assert_eq!(skills[0].content.trim(), "Project notes.");
        assert!(skills[0].offered());
        assert!(!skills[1].offered(), "turned off for the model");
        assert!(skills[0].loaded().contains("Project notes."));
    }
}
