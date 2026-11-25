//! Project instructions: the AGENTS.md files a session's model reads,
//! and where Harlo finds them.
//!
//! In order: the global `AGENTS.md` in Harlo's config folder (or, without
//! one, `~/.claude/CLAUDE.md`); then every `AGENTS.md` from the session's
//! folder upwards, toward the home folder for a folder inside it, else up to
//! the project's root (in a folder without an `AGENTS.md`, its `CLAUDE.md`);
//! then the files the config's `instructions` list names. Each keeps the
//! path it came from, so the prompt and the UI can say where a rule is from.
//!
//! A file brings in another by naming it after an `@`, as in `@docs/style.md`
//! or `@~/notes/rules.md`: the named file follows the one that names it. A
//! file inside the project brings in only files inside the project; the
//! person's own files, outside it, may name any.
//!
//! `HARLO_DISABLE_PROJECT_CONFIG` skips the project's files and
//! `HARLO_DISABLE_CLAUDE_MD` every `CLAUDE.md`. Not yet: URLs in
//! `instructions`, and
//! nested `AGENTS.md` files loaded when a file below them is read.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

/// One instruction file and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Where it was read from.
    pub source: PathBuf,
    /// Its text.
    pub text: String,
}

/// Where to look and what's switched off.
#[derive(Debug, Clone)]
pub struct Search<'a> {
    /// The session's folder.
    pub workspace: &'a Path,
    /// The project's root (its git checkout's top), if known.
    pub project_root: Option<&'a Path>,
    /// The home folder.
    pub home: &'a Path,
    /// Harlo's config folder (`$XDG_CONFIG_HOME/harlo`).
    pub config_dir: &'a Path,
    /// The config's `instructions` entries.
    pub configured: &'a [String],
    /// Skip the project's files (`HARLO_DISABLE_PROJECT_CONFIG`).
    pub no_project: bool,
    /// Skip every `CLAUDE.md` (`HARLO_DISABLE_CLAUDE_MD`).
    pub no_claude: bool,
}

impl Default for Search<'_> {
    fn default() -> Self {
        let none = Path::new("");
        Self {
            workspace: none,
            project_root: None,
            home: none,
            config_dir: none,
            configured: &[],
            no_project: false,
            no_claude: false,
        }
    }
}

impl Search<'_> {
    /// Reads the switches from the environment: set to anything but empty,
    /// `0` or `false`, a switch is on.
    #[must_use]
    pub fn with_env(mut self) -> Self {
        let on =
            |k: &str| std::env::var(k).is_ok_and(|v| !v.is_empty() && v != "0" && v != "false");
        self.no_project |= on("HARLO_DISABLE_PROJECT_CONFIG");
        self.no_claude |= on("HARLO_DISABLE_CLAUDE_MD");
        self
    }
}

/// How many files deep one file brings in another: a file, what it names,
/// what that names, and so on. Enough for any real layout, and an end to
/// files that name each other.
const DEEPEST_IMPORT: usize = 5;

/// `path` with its links followed, or as given when it can't be.
fn real(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The paths `text` names after an `@`, outside code.
///
/// A word such as `@docs/style.md`, on a line of its own or in a
/// sentence. Words in a fenced block or between backticks are examples,
/// not imports.
fn named(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut fenced = false;
    for line in text.lines() {
        let start = line.trim_start();
        if start.starts_with("```") || start.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        // Every other piece between backticks is code.
        for prose in line.split('`').step_by(2) {
            found.extend(
                prose
                    .split_whitespace()
                    .filter_map(|word| word.strip_prefix('@'))
                    .map(|path| path.trim_end_matches([',', ';', ':', ')', ']', '"', '\'']))
                    .filter(|path| !path.is_empty()),
            );
        }
    }
    found
}

/// The file `name`, named in the instruction file `from`, when there is
/// one and `from` may bring it in.
fn imported(name: &str, from: &Path, search: &Search<'_>) -> Option<PathBuf> {
    let folder = from.parent()?;
    let at = |name: &str| match name.strip_prefix("~/") {
        Some(rest) => search.home.join(rest),
        None => folder.join(name),
    };
    // (A sentence may end at the name: `See @README.md.`)
    let path = [name, name.trim_end_matches('.')]
        .into_iter()
        .map(at)
        .find(|p| p.is_file())?;
    // A project's file reaches no further than the project.
    let project = real(search.project_root.unwrap_or(search.workspace));
    if real(from).starts_with(&project) && !real(&path).starts_with(&project) {
        tracing::info!(
            name: "instructions.import.outside",
            file = %from.display(),
            "a project's instruction file named a file outside the project; it isn't read"
        );
        return None;
    }
    Some(path)
}

/// Adds the instruction file `path` to `out`, then the files it names,
/// `deeper` more files deep; a file already there isn't added again.
fn add(path: PathBuf, search: &Search<'_>, deeper: usize, out: &mut Vec<Instruction>) {
    let here = real(&path);
    if out.iter().any(|i| real(&i.source) == here) {
        return;
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }
    let names: Vec<PathBuf> = if deeper == 0 {
        Vec::new()
    } else {
        named(&text)
            .into_iter()
            .filter_map(|name| imported(name, &path, search))
            .collect()
    };
    out.push(Instruction { source: path, text });
    for name in names {
        add(name, search, deeper - 1, out);
    }
}

/// The instruction files for a session, in the order they're given to the
/// model. Files that can't be read are left out.
#[must_use]
pub fn load(search: &Search<'_>) -> Vec<Instruction> {
    let mut out: Vec<Instruction> = Vec::new();
    let add = |path: PathBuf, out: &mut Vec<Instruction>| {
        add(path, search, DEEPEST_IMPORT, out);
    };

    let global = search.config_dir.join("AGENTS.md");
    if global.is_file() {
        add(global, &mut out);
    } else if !search.no_claude {
        add(search.home.join(".claude/CLAUDE.md"), &mut out);
    }

    if !search.no_project {
        for dir in upward(search) {
            let agents = dir.join("AGENTS.md");
            if agents.is_file() {
                add(agents, &mut out);
            } else if !search.no_claude && dir.join("CLAUDE.md").is_file() {
                add(dir.join("CLAUDE.md"), &mut out);
            }
        }
    }

    for entry in search.configured {
        for path in configured(entry, search) {
            add(path, &mut out);
        }
    }
    out
}

/// The folders from the workspace upwards: toward home for a workspace
/// inside it (home itself left out), else up to the project's root.
fn upward(search: &Search<'_>) -> Vec<PathBuf> {
    let inside_home = search.workspace.starts_with(search.home) && search.workspace != search.home;
    let stop = if inside_home {
        Some(search.home)
    } else {
        search.project_root
    };
    let mut dirs = Vec::new();
    let mut dir = Some(search.workspace);
    while let Some(d) = dir {
        if inside_home && d == search.home {
            break;
        }
        dirs.push(d.to_path_buf());
        if stop.is_none_or(|s| d == s) {
            break;
        }
        dir = d.parent();
    }
    dirs
}

/// The files a config `instructions` entry names. A relative entry is
/// looked for in each folder from the workspace up to the project's root
/// (or in the config folder when the project's files are off); `~/` is
/// the home folder. The last part of a path may hold `*`. URLs aren't
/// fetched yet.
fn configured(entry: &str, search: &Search<'_>) -> Vec<PathBuf> {
    if entry.starts_with("http://") || entry.starts_with("https://") {
        tracing::info!(name: "instructions.url.skipped", entry, "instruction URLs aren't fetched yet");
        return Vec::new();
    }
    if let Some(rest) = entry.strip_prefix("~/") {
        return matching(&search.home.join(rest));
    }
    if Path::new(entry).is_absolute() {
        return matching(Path::new(entry));
    }
    let bases: Vec<PathBuf> = if search.no_project {
        vec![search.config_dir.to_path_buf()]
    } else {
        let mut bases = Vec::new();
        let mut dir = Some(search.workspace);
        while let Some(d) = dir {
            bases.push(d.to_path_buf());
            if search.project_root.is_none_or(|r| d == r) {
                break;
            }
            dir = d.parent();
        }
        bases
    };
    bases
        .iter()
        .flat_map(|b| matching(&b.join(entry)))
        .collect()
}

/// `path` if it's a file, or the files its last part's `*` pattern matches,
/// sorted.
fn matching(path: &Path) -> Vec<PathBuf> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !name.contains('*') {
        return if path.is_file() {
            vec![path.to_path_buf()]
        } else {
            Vec::new()
        };
    }
    let Some(folder) = path.parent() else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .is_some_and(|n| crate::rules::glob(&name, &n.to_string_lossy()))
        })
        .collect();
    found.sort();
    found
}

/// The instruction files in the folders between `cwd` and `file`, which
/// the agent meets as it reads files below the session's folder, with
/// their text, outermost first; files in `known` are left out.
///
/// In each folder an `AGENTS.md` is taken, or a `CLAUDE.md` when there's
/// none.
#[must_use]
pub fn on_the_way(cwd: &Path, file: &Path, known: &[PathBuf]) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    let mut folder = file.parent();
    while let Some(dir) = folder.filter(|d| *d != cwd && d.starts_with(cwd)) {
        let named = ["AGENTS.md", "CLAUDE.md"]
            .iter()
            .map(|name| dir.join(name))
            .find(|p| p.is_file());
        if let Some(path) = named.filter(|p| !known.contains(p))
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            found.push((path, text));
        }
        folder = dir.parent();
    }
    found.reverse();
    found
}

/// The instructions as the system prompt carries them, each under the
/// path it came from.
#[must_use]
pub fn render(instructions: &[Instruction]) -> String {
    instructions
        .iter()
        .map(|i| {
            format!(
                "Instructions from: {}\n{}",
                i.source.display(),
                i.text.trim_end()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, text).expect("write");
    }

    #[test]
    fn global_then_upward_then_configured_with_claude_md_as_fallback() {
        let t = tempfile::tempdir().expect("dir");
        let (home, config) = (t.path().join("home"), t.path().join("config/harlo"));
        let root = t.path().join("work/repo");
        let web = root.join("packages/web");
        write(&config.join("AGENTS.md"), "global");
        write(
            &home.join(".claude/CLAUDE.md"),
            "claude global, unused: there's a global AGENTS.md",
        );
        write(&root.join("AGENTS.md"), "root");
        write(&root.join("packages/CLAUDE.md"), "packages, as CLAUDE.md");
        write(&web.join("AGENTS.md"), "web");
        write(&web.join("CLAUDE.md"), "web's CLAUDE.md, unused");
        write(&root.join("docs/a.md"), "doc a");
        write(&root.join("docs/b.md"), "doc b");
        write(
            &t.path().join("work/AGENTS.md"),
            "above the project root: unused",
        );
        let configured = ["docs/*.md".to_owned()];
        let search = Search {
            workspace: &web,
            project_root: Some(&root),
            home: &home,
            config_dir: &config,
            configured: &configured,
            ..Search::default()
        };
        let texts: Vec<String> = load(&search).into_iter().map(|i| i.text).collect();
        assert_eq!(
            texts,
            [
                "global",
                "web",
                "packages, as CLAUDE.md",
                "root",
                "doc a",
                "doc b"
            ]
        );

        let quiet = Search {
            no_project: true,
            no_claude: true,
            ..search.clone()
        };
        assert_eq!(
            load(&quiet).into_iter().map(|i| i.text).collect::<Vec<_>>(),
            ["global"]
        );
    }

    #[test]
    fn a_file_brings_in_the_files_it_names() {
        let t = tempfile::tempdir().expect("dir");
        let (home, config) = (t.path().join("home"), t.path().join("config/harlo"));
        let root = t.path().join("work/repo");
        // The person's own file names one anywhere; the project's names
        // one beside it, one in a sentence, one that names it back, and
        // (not read) one outside the project.
        write(&home.join(".claude/CLAUDE.md"), "@~/rules/mine.md\n");
        write(&home.join("rules/mine.md"), "mine");
        write(&home.join("secret.txt"), "secret");
        write(
            &root.join("CLAUDE.md"),
            &format!(
                "@AGENTS.md\nSee @docs/style.md.\n`@docs/example.md` and\n```\n@docs/example.md\n```\n\
                 @{}\n@missing.md\n",
                home.join("secret.txt").display()
            ),
        );
        write(&root.join("AGENTS.md"), "agents @CLAUDE.md");
        write(&root.join("docs/style.md"), "style");
        write(&root.join("docs/example.md"), "an example, not read");
        let search = Search {
            workspace: &root,
            project_root: Some(&root),
            home: &home,
            config_dir: &config,
            ..Search::default()
        };
        // Each once, though the project's two name each other; the example
        // in code, the file outside the project and the missing one aren't
        // read.
        let sources: Vec<PathBuf> = load(&search).into_iter().map(|i| i.source).collect();
        assert_eq!(
            sources,
            [
                home.join(".claude/CLAUDE.md"),
                home.join("rules/mine.md"),
                root.join("AGENTS.md"),
                root.join("CLAUDE.md"),
                root.join("docs/style.md"),
            ]
        );
    }

    #[test]
    fn inside_home_the_walk_goes_up_to_home_and_claude_md_stands_in_globally() {
        let t = tempfile::tempdir().expect("dir");
        let home = t.path().join("home");
        let project = home.join("code/p");
        write(&home.join(".claude/CLAUDE.md"), "claude global");
        write(&home.join("code/AGENTS.md"), "code");
        write(&home.join("AGENTS.md"), "home itself: unused");
        let config = t.path().join("config/harlo");
        let search = Search {
            workspace: &project,
            project_root: Some(&project),
            home: &home,
            config_dir: &config,
            ..Search::default()
        };
        let found = load(&search);
        assert_eq!(
            found.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(),
            ["claude global", "code"]
        );
        assert!(render(&found).starts_with(&format!(
            "Instructions from: {}\nclaude global",
            home.join(".claude/CLAUDE.md").display()
        )));
    }
}
