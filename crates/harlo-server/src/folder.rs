//! What a folder sees: the global configuration with the project's own
//! config files laid over it, the config folders, and the agents they make.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use harlo_engine::Config;
use harlo_engine::agents::{self, Agent};
use harlo_engine::{merge_config, project_config_files, read_config_document};
use serde_json::{Value, json};

use crate::server::Server;

/// The configuration as a folder sees it.
#[derive(Debug)]
pub(crate) struct Folder {
    /// The configuration, merged.
    pub config: Config,
    /// The config documents it was merged from, global first.
    pub documents: Vec<(PathBuf, Value)>,
    /// The config folders: the global one, then the project's `.opencode`
    /// folders from its root down.
    pub dirs: Vec<PathBuf>,
    /// The agents, built-in and configured.
    pub agents: Vec<Agent>,
}

impl Folder {
    /// The agent chosen, else Build, else the first.
    pub fn agent(&self, chosen: Option<&str>) -> Agent {
        chosen
            .and_then(|id| self.agents.iter().find(|a| a.id == id))
            .or_else(|| self.agents.iter().find(|a| a.id == agents::DEFAULT))
            .or_else(|| self.agents.first())
            .cloned()
            .unwrap_or_else(fallback)
    }

    /// The agents other agents can hand a task, `(id, what for)`.
    pub fn subagents(&self) -> Vec<(String, String)> {
        self.agents
            .iter()
            .filter(|a| a.mode.takes_tasks() && !a.hidden)
            .map(|a| (a.id.clone(), a.description.clone()))
            .collect()
    }

    /// The agents, as OpenCode lists them.
    pub fn agents_json(&self) -> Vec<Value> {
        self.agents
            .iter()
            .map(|a| {
                let mut agent = json!({
                    "id": a.id, "name": a.name, "description": a.description,
                    "mode": a.mode.as_str(), "hidden": a.hidden, "permissions": a.rules,
                    "request": { "settings": {}, "headers": {}, "body": {} },
                });
                if let Some(system) = &a.system {
                    agent["system"] = json!(system);
                }
                if let Some(color) = &a.color {
                    agent["color"] = json!(color);
                }
                if let Some(model) = &a.model {
                    agent["model"] = json!(model);
                }
                agent
            })
            .collect()
    }

    /// The config entries as OpenCode lists them: the global document
    /// and folder, then the project's documents and folders.
    pub fn entries(&self) -> Vec<Value> {
        let document = |(path, info): &(PathBuf, Value)| json!({ "type": "document", "path": path, "info": info });
        let directory = |dir: &PathBuf| json!({ "type": "directory", "path": dir });
        let global = usize::from(self.config.source.is_some()).min(self.documents.len());
        let mut out: Vec<Value> = self.documents[..global].iter().map(document).collect();
        out.extend(self.dirs.first().map(directory));
        out.extend(self.documents[global..].iter().map(document));
        out.extend(self.dirs.iter().skip(1).map(directory));
        out
    }
}

/// Build, as built in, for a configuration that disabled every agent.
fn fallback() -> Agent {
    agents::builtins(Path::new(""))
        .into_iter()
        .next()
        .unwrap_or_else(|| unreachable!("there are built-in agents"))
}

/// The top of the git checkout `directory` is in.
fn project_root(directory: &Path) -> Option<PathBuf> {
    directory
        .ancestors()
        .find(|d| d.join(".git").exists())
        .map(Path::to_path_buf)
}

impl Server {
    /// The configuration as `directory` sees it.
    pub(crate) fn folder(&self, directory: &str) -> Folder {
        let workspace = Path::new(directory);
        let root = project_root(workspace);
        let mut documents = Vec::new();
        let (global, global_text) = self.global();
        let mut merged = global_text
            .clone()
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
        if let (Some(path), Some(info)) = (&global.source, &global_text) {
            documents.push((path.clone(), info.clone()));
        }
        for file in project_config_files(workspace, root.as_deref()) {
            match read_config_document(&file) {
                Ok(info) => {
                    merge_config(&mut merged, &info);
                    documents.push((file, info));
                }
                Err(e) => {
                    tracing::warn!(name: "config.project.unreadable", error = %e, "skipping a project config file");
                }
            }
        }
        let mut config = if documents.len() > usize::from(global.source.is_some()) {
            Config::from_value(&merged, global.source.clone())
        } else {
            global.clone()
        };
        // Providers come from the global file, whose keys stay there.
        if config.providers.is_empty() {
            config.providers.clone_from(&global.providers);
        }
        let signed_in = self.signed_in(&config.providers);
        config.providers.extend(signed_in);
        let mut dirs = vec![self.config_dir.clone()];
        let mut project: Vec<PathBuf> = workspace
            .ancestors()
            .take_while(|d| root.as_deref().is_some_and(|r| d.starts_with(r)) || *d == workspace)
            .map(|d| d.join(".opencode"))
            .filter(|d| d.is_dir())
            .collect();
        project.reverse();
        dirs.extend(project);
        let agents = agents::configured(self.agents.clone(), &config.agents, &dirs);
        Folder {
            config,
            documents,
            dirs,
            agents,
        }
    }
}
