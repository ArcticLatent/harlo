//! Which instruction files the server gives a session's agent from the start,
//! worked out for a folder on this computer.
//!
//! The server doesn't list them, so this follows its rule (as of 2.3):
//!
//! - **Global:** `AGENTS.md` in the server's config folder, and
//!   `~/.claude/CLAUDE.md`.
//! - **Project:** when the session's folder is inside its project, each
//!   folder from the session's up to the home folder (or up to the project's
//!   root, for a project outside the home folder) gives its `AGENTS.md`, or
//!   its `CLAUDE.md` when it has no `AGENTS.md`.
//!
//! Files the agent picks up later, such as a subfolder's `AGENTS.md` once it
//! reads a file there, come from the server itself (see
//! `opencode_client::SessionContext::loaded`). Paths from a config file's
//! `instructions` setting aren't followed here.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

/// The file each folder offers first.
const AGENTS: &str = "AGENTS.md";

/// The file a folder offers when it has no [`AGENTS`].
const CLAUDE: &str = "CLAUDE.md";

/// Where an instruction file applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Every session, everywhere.
    Global,
    /// Sessions in this project.
    Project,
}

/// An instruction file the agent gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// Its absolute path.
    pub path: PathBuf,
    /// Where it applies.
    pub scope: Scope,
}

/// The folders instructions are looked for in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places {
    /// The session's folder.
    pub folder: PathBuf,
    /// The project's root folder.
    pub project: PathBuf,
    /// The user's home folder.
    pub home: PathBuf,
    /// OpenCode's config folder, such as `~/.config/opencode`.
    pub config: PathBuf,
}

impl Places {
    /// The places for a session in `folder` of the project at `project`,
    /// with this computer's home and config folders.
    ///
    /// Returns `None` when the home folder can't be found.
    #[must_use]
    pub fn here(folder: &str, project: &str) -> Option<Self> {
        let home = dirs::home_dir()?;
        let config = dirs::config_dir().unwrap_or_else(|| home.join(".config"));
        Some(Self {
            folder: PathBuf::from(folder),
            project: PathBuf::from(project),
            home,
            config: config.join("opencode"),
        })
    }
}

/// Returns the instruction files that exist for `places`, global ones first.
#[must_use]
pub fn files(places: &Places) -> Vec<File> {
    let global = [
        places.config.join(AGENTS),
        places.home.join(".claude").join(CLAUDE),
    ];
    let mut found: Vec<File> = global
        .into_iter()
        .filter(|path| path.is_file())
        .map(|path| File {
            path,
            scope: Scope::Global,
        })
        .collect();
    if !places.folder.starts_with(&places.project) {
        return found;
    }
    let stop = if places.folder.starts_with(&places.home) {
        &places.home
    } else {
        &places.project
    };
    for folder in places.folder.ancestors() {
        let agents = folder.join(AGENTS);
        let claude = folder.join(CLAUDE);
        let pick = if agents.is_file() {
            Some(agents)
        } else {
            claude.is_file().then_some(claude)
        };
        if let Some(path) = pick
            && !found.iter().any(|f| f.path == path)
        {
            found.push(File {
                path,
                scope: Scope::Project,
            });
        }
        if folder == stop {
            break;
        }
    }
    found
}

/// Returns this computer's home folder, if it can be found.
#[must_use]
pub fn home() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Shows `path` from the project's root when it's inside the project, and
/// otherwise with the home folder as `~`.
#[must_use]
pub fn display_in(path: &Path, project: &Path, home: &Path) -> String {
    match path.strip_prefix(project) {
        Ok(rest) if !project.as_os_str().is_empty() && !rest.as_os_str().is_empty() => {
            rest.display().to_string()
        }
        _ => display(path, home),
    }
}

/// Shows `path` with the home folder as `~`, as people write it.
#[must_use]
pub fn display(path: &Path, home: &Path) -> String {
    if home.as_os_str().is_empty() {
        return path.display().to_string();
    }
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}
