//! The commands and skills a session's folder offers, and running a
//! command in a session.

// Rust guideline compliant 2026-10-09

use std::path::Path;
use std::sync::Arc;

use harlo_engine::commands::{self, Command};
use harlo_engine::skills::{self, Attached, Skill};
use serde_json::Value;

use crate::git;
use crate::server::{Refusal, Server};

impl Server {
    /// The skills found for a session in `directory`.
    pub(crate) async fn skills_for(&self, directory: &str) -> Vec<Skill> {
        let root = git::toplevel(directory).await;
        let folder = self.folder(directory);
        let home = dirs::home_dir().unwrap_or_default();
        let search = skills::Search {
            workspace: Path::new(directory),
            project_root: root.as_deref().map(Path::new),
            home: &home,
            config_dir: &self.config_dir,
            configured: &folder.config.skills,
        };
        tokio::task::block_in_place(|| skills::discover(&search))
    }

    /// The commands found for a session in `directory`.
    pub(crate) async fn commands_for(&self, directory: &str) -> Vec<Command> {
        let root = git::toplevel(directory).await;
        let folder = self.folder(directory);
        let search = commands::Search {
            workspace: Path::new(directory),
            project_root: root.as_deref().map(Path::new),
            config_dir: &self.config_dir,
            configured: &folder.config.commands,
        };
        tokio::task::block_in_place(|| commands::discover(&search))
    }

    /// The skills with IDs `ids`, as loaded into a prompt.
    ///
    /// # Errors
    ///
    /// When one isn't found.
    pub(crate) async fn attach(
        &self,
        directory: &str,
        ids: &[String],
    ) -> Result<Vec<Attached>, Refusal> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let found = self.skills_for(directory).await;
        ids.iter()
            .map(|id| {
                found
                    .iter()
                    .find(|s| &s.id == id)
                    .map(Attached::from)
                    .ok_or_else(|| Refusal::Bad(format!("There's no skill {id}.")))
            })
            .collect()
    }

    /// Runs command `name` with `arguments` in session `id`: its agent and
    /// model are chosen, and its filled-in template is sent as a prompt.
    ///
    /// # Errors
    ///
    /// When there's no such session or command, or its agent or model
    /// isn't there.
    pub(crate) async fn run_command(
        self: &Arc<Self>,
        id: &str,
        name: &str,
        arguments: &str,
        delivery: &str,
        skills: &[String],
    ) -> Result<Value, Refusal> {
        let session = self.session(id)?;
        let directory = session.location.directory.clone();
        let Some(command) = self
            .commands_for(&directory)
            .await
            .into_iter()
            .find(|c| c.name == name)
        else {
            return Err(Refusal::NotFound(format!("There's no command {name}.")));
        };
        if let Some(agent) = &command.agent {
            self.set_agent(id, agent)?;
        }
        if let Some(model) = &command.model {
            self.set_model(id, model.clone())?;
        }
        let text = commands::fill(&command.template, arguments);
        let text = commands::run_shell_blocks(&text, Path::new(&directory)).await;
        let attached = self.attach(&directory, skills).await?;
        self.prompt(id, None, &text, delivery, attached, Vec::new())
    }
}
