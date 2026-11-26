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
            plugins: &folder.plugins,
        };
        tokio::task::block_in_place(|| skills::discover(&search))
    }

    /// The commands a session in `directory` has: those written as
    /// templates, and those the plugins in use run as code, which a
    /// template of the same name replaces.
    pub(crate) async fn commands_for(&self, directory: &str) -> Vec<Command> {
        let mut found = self.templates_for(directory).await;
        for program in self.plugin_programs() {
            for (name, description) in &program.commands {
                if !found.iter().any(|c| &c.name == name) {
                    found.push(Command {
                        name: name.clone(),
                        description: description.clone(),
                        template: String::new(),
                        agent: None,
                        model: None,
                    });
                }
            }
        }
        found
    }

    /// The text command `name` of a plugin's code makes of `arguments`
    /// for session `session`; none when no plugin in use runs one of
    /// that name.
    async fn coded(
        &self,
        name: &str,
        arguments: &str,
        session: &crate::store::Session,
    ) -> Option<Result<String, Refusal>> {
        let program = self
            .plugin_programs()
            .into_iter()
            .find(|program| program.commands.iter().any(|(n, _)| n == name))?;
        let within = crate::plugins::code::Within {
            session: Some((session.id.as_str(), session.location.directory.as_str())),
            tools: None,
        };
        let made = program
            .call(
                self,
                crate::plugins::code::Kind::Command,
                name,
                Value::String(arguments.to_owned()),
                within,
            )
            .await;
        Some(match made {
            Ok(Value::String(text)) if !text.trim().is_empty() => Ok(text),
            Ok(_) => Err(Refusal::Bad(format!(
                "The plugin {}'s /{name} gave no prompt to send; it must return text.",
                program.plugin
            ))),
            Err(why) => Err(Refusal::Bad(why)),
        })
    }

    /// The commands written as templates for a session in `directory`.
    async fn templates_for(&self, directory: &str) -> Vec<Command> {
        let root = git::toplevel(directory).await;
        let folder = self.folder(directory);
        let search = commands::Search {
            workspace: Path::new(directory),
            project_root: root.as_deref().map(Path::new),
            config_dir: &self.config_dir,
            configured: &folder.config.commands,
            plugins: &folder.plugins,
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
        if name == "goal" {
            self.goal_command(id, arguments, delivery)?;
            return Ok(Value::Null);
        }
        let session = self.session(id)?;
        let directory = session.location.directory.clone();
        let Some(command) = self
            .templates_for(&directory)
            .await
            .into_iter()
            .find(|c| c.name == name)
        else {
            let Some(made) = self.coded(name, arguments, &session).await else {
                return Err(Refusal::NotFound(format!("There's no command {name}.")));
            };
            let attached = self.attach(&directory, skills).await?;
            return self.prompt(id, None, &made?, delivery, attached, Vec::new());
        };
        if let Some(agent) = &command.agent {
            self.set_agent(id, agent)?;
        }
        if let Some(model) = &command.model {
            self.set_model(id, model.clone())?;
        }
        let mut text = commands::fill(&command.template, arguments);
        // A project's command runs its shell lines once the person allowed them.
        let waits = self
            .folder(&directory)
            .withheld
            .is_some_and(|held| held.commands.contains(&command.name));
        if !waits {
            text = commands::run_shell_blocks(&text, Path::new(&directory)).await;
        }
        let attached = self.attach(&directory, skills).await?;
        self.prompt(id, None, &text, delivery, attached, Vec::new())
    }
}
