//! Projects and their worktrees: a project's own settings (name, colour,
//! icon and the command a new worktree starts with), and making and
//! removing its git worktrees.
//!
//! A project is known from its sessions; its ID is its repository's first
//! commit, shared by every worktree.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use harlo_engine::ascending_id;
use harlo_engine::tools::shell::{Ended, execute};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use crate::git;
use crate::server::{Refusal, Server};

/// How long a worktree's start command may run.
const START_TIMEOUT: Duration = Duration::from_secs(600);

/// A change to a project's settings: fields left out stay, and an empty
/// text clears one.
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct ProjectChange {
    name: Option<String>,
    icon: Option<Map<String, Value>>,
    commands: Option<Map<String, Value>>,
}

/// Sets each of `change`'s fields in `into`; an empty text removes one.
fn merge(into: &mut Map<String, Value>, change: Map<String, Value>) {
    for (key, value) in change {
        if value.as_str().is_some_and(str::is_empty) || value.is_null() {
            into.remove(&key);
        } else {
            into.insert(key, value);
        }
    }
}

/// A new worktree's name when none is given.
fn some_name() -> String {
    let id = ascending_id("wt");
    let tail: String = id.chars().rev().take(6).collect();
    format!("harlo-{}", tail.to_lowercase())
}

impl Server {
    /// Every project with a session, most recently active first.
    ///
    /// # Errors
    ///
    /// When the store fails.
    pub(crate) async fn projects(&self) -> Result<Vec<Value>, Refusal> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for s in self.store.sessions()? {
            if seen.insert(s.project_id.clone()) {
                out.push(self.project(&s.project_id).await?);
            }
        }
        Ok(out)
    }

    /// Project `id`, as clients read it.
    ///
    /// # Errors
    ///
    /// When no session belongs to it, or the store fails.
    pub(crate) async fn project(&self, id: &str) -> Result<Value, Refusal> {
        let sessions: Vec<_> = self
            .store
            .sessions()?
            .into_iter()
            .filter(|s| s.project_id == id)
            .collect();
        let (Some(newest), Some(oldest)) = (sessions.first(), sessions.last()) else {
            return Err(Refusal::NotFound(format!("There's no project {id}.")));
        };
        let canonical = git::canonical(&oldest.location.directory).await;
        let time = json!({
            "created": oldest.time.created, "updated": newest.time.updated,
            "active": newest.time.updated,
        });
        let mut project =
            json!({ "id": id, "canonical": canonical, "time": time, "sandboxes": [] });
        if git::is_repo(&canonical).await {
            project["vcs"] = json!("git");
        }
        if let Value::Object(settings) = self.store.project_settings(id)? {
            for (key, value) in settings {
                project[key] = value;
            }
        }
        Ok(project)
    }

    /// Changes project `id`'s settings and returns it.
    ///
    /// # Errors
    ///
    /// When there's no such project, or the store fails.
    pub(crate) async fn update_project(
        &self,
        id: &str,
        change: ProjectChange,
    ) -> Result<Value, Refusal> {
        self.project(id).await?;
        let mut settings = self
            .store
            .project_settings(id)?
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(name) = change.name {
            merge(
                &mut settings,
                Map::from_iter([("name".to_owned(), json!(name))]),
            );
        }
        for (key, part) in [("icon", change.icon), ("commands", change.commands)] {
            let Some(part) = part else { continue };
            let mut now = settings
                .get(key)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            merge(&mut now, part);
            if now.is_empty() {
                settings.remove(key);
            } else {
                settings.insert(key.to_owned(), Value::Object(now));
            }
        }
        self.store
            .put_project_settings(id, &Value::Object(settings))?;
        let project = self.project(id).await?;
        self.emit("project.updated", project.clone(), None, None);
        Ok(project)
    }

    /// Project `id`'s folders: its extra worktrees, then its main checkout,
    /// then any other folder its sessions are in.
    ///
    /// # Errors
    ///
    /// When the store fails.
    pub(crate) async fn worktree_list(&self, id: &str) -> Result<Vec<Value>, Refusal> {
        let Ok(project) = self.project(id).await else {
            return Ok(Vec::new());
        };
        let main = project["canonical"].as_str().unwrap_or_default().to_owned();
        let mut listed = BTreeSet::new();
        let mut out = Vec::new();
        for tree in git::worktrees(&main).await {
            listed.insert(tree.clone());
            out.push(json!({ "directory": tree, "strategy": "git" }));
        }
        listed.insert(main.clone());
        out.push(json!({ "directory": main }));
        for s in self.store.sessions()? {
            if s.project_id == id && listed.insert(s.location.directory.clone()) {
                out.push(json!({ "directory": s.location.directory }));
            }
        }
        Ok(out)
    }

    /// Makes a git worktree of project `id` named `name`, on `branch` (a
    /// new branch named after it when not given), and runs the project's
    /// start command in it. Returns its folder.
    ///
    /// # Errors
    ///
    /// When there's no such project, git refuses, or the start command
    /// fails (the worktree then stays, for a look).
    pub(crate) async fn create_worktree(
        &self,
        id: &str,
        name: Option<String>,
        branch: Option<String>,
        directory: Option<String>,
    ) -> Result<String, Refusal> {
        let project = self.project(id).await?;
        let main = project["canonical"].as_str().unwrap_or_default().to_owned();
        let name = name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(some_name);
        let folder = directory.unwrap_or_else(|| {
            let short: String = id.chars().take(6).collect();
            let data = self.snapshots.parent().unwrap_or(Path::new("."));
            data.join("worktree")
                .join(short)
                .join(&name)
                .to_string_lossy()
                .into_owned()
        });
        if let Some(parent) = Path::new(&folder).parent() {
            std::fs::create_dir_all(parent).map_err(|e| Refusal::Worktree {
                message: format!("Couldn't make {}: {e}", parent.display()),
                force: false,
            })?;
        }
        let branch = branch.unwrap_or_else(|| name.clone());
        let known = git::run(
            &main,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
        )
        .await
        .is_ok();
        let made = if known {
            git::run(&main, &["worktree", "add", &folder, &branch]).await
        } else {
            git::run(&main, &["worktree", "add", "-b", &branch, &folder]).await
        };
        made.map_err(|message| Refusal::Worktree {
            message,
            force: false,
        })?;
        self.emit("worktree.updated", json!({ "projectID": id }), None, None);
        if let Some(start) = project["commands"]["start"].as_str() {
            self.start_worktree(start, &folder).await?;
        }
        Ok(folder)
    }

    /// Runs a project's start command `command` in a new worktree.
    async fn start_worktree(&self, command: &str, folder: &str) -> Result<(), Refusal> {
        let ran = execute(
            command,
            Path::new(folder),
            None,
            Some(START_TIMEOUT),
            &CancellationToken::new(),
        )
        .await;
        let why = match ran.ended {
            Ended::Exited(Some(0)) => return Ok(()),
            Ended::Exited(code) => {
                format!("exit {}", code.map_or("?".to_owned(), |c| c.to_string()))
            }
            Ended::TimedOut => "it took too long".to_owned(),
            Ended::Stopped => "it was stopped".to_owned(),
            Ended::NotStarted(why) => why,
        };
        Err(Refusal::Worktree {
            message: format!("Command failed ({why}): {command}: {}", ran.output.trim()),
            force: false,
        })
    }

    /// Removes worktree `folder` of project `id`; one with changes only
    /// when `force`.
    ///
    /// # Errors
    ///
    /// When `folder` isn't one of the project's extra worktrees, or git
    /// refuses; `force` in the refusal when forcing would remove it.
    pub(crate) async fn remove_worktree(
        &self,
        id: &str,
        folder: &str,
        force: bool,
    ) -> Result<(), Refusal> {
        let project = self.project(id).await?;
        let main = project["canonical"].as_str().unwrap_or_default().to_owned();
        if !git::worktrees(&main).await.iter().any(|w| w == folder) {
            return Err(Refusal::Worktree {
                message: format!("Invalid worktree directory: {folder}"),
                force: false,
            });
        }
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(folder);
        git::run(&main, &args).await.map_err(|message| {
            let force = !force && message.contains("--force");
            Refusal::Worktree { message, force }
        })?;
        self.emit("worktree.updated", json!({ "projectID": id }), None, None);
        Ok(())
    }
}
