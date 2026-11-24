//! Subagents on the server: child sessions made for a session's
//! `subagent` calls, their background jobs, and their answers sent back to
//! the parent each time they go idle.

// Rust guideline compliant 2026-10-09

use std::sync::Arc;

use futures_util::future::BoxFuture;
use harlo_engine::subagent::{self, Request, Subagents};
use harlo_engine::{Created, Message, ascending_id, now};
use serde_json::{Value, json};

use crate::server::{Server, lock};
use crate::store::Session;

/// Where a running session's subagents start.
#[derive(Debug)]
pub(crate) struct SessionSubagents {
    /// The server.
    pub server: Arc<Server>,
    /// The parent session.
    pub session: String,
}

impl Subagents for SessionSubagents {
    fn start<'a>(
        &'a self,
        _message: &'a str,
        _call: &'a str,
        request: Request,
    ) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(std::future::ready(
            self.server.start_subagent(&self.session, &request),
        ))
    }
}

impl Server {
    /// Starts `request` in a new child of session `parent`, or sends it to
    /// the child it names; returns the child's ID.
    fn start_subagent(self: &Arc<Self>, parent: &str, request: &Request) -> Result<String, String> {
        let parent = self.session(parent).map_err(|e| e.to_string())?;
        let folder = self.folder(&parent.location.directory);
        let child = if let Some(id) = &request.session {
            let child = self
                .session(id)
                .ok()
                .filter(|c| c.parent_id.as_deref() == Some(parent.id.as_str()))
                .ok_or_else(|| format!("There's no subagent session {id} of this session."))?;
            self.prompt(
                &child.id,
                None,
                &request.prompt,
                "steer",
                Vec::new(),
                Vec::new(),
            )
            .map_err(|e| e.to_string())?;
            child
        } else {
            let Some(agent) = folder
                .agents
                .iter()
                .find(|a| a.id == request.agent && a.mode.takes_tasks() && !a.hidden)
            else {
                let known: Vec<String> = folder.subagents().into_iter().map(|(id, _)| id).collect();
                return Err(format!(
                    "There's no subagent {}. The subagents are: {}.",
                    request.agent,
                    known.join(", ")
                ));
            };
            let model = request
                .model
                .clone()
                .or_else(|| agent.model.clone())
                .or_else(|| parent.model.clone());
            if let Some(model) = model
                .as_ref()
                .filter(|m| folder.config.resolve(m).is_none())
            {
                return Err(format!(
                    "The model {}/{} isn't in the configuration.",
                    model.provider, model.model
                ));
            }
            let child = self
                .create_child(&parent, &agent.id, &request.description, model)
                .map_err(|e| e.to_string())?;
            let text = format!("{}\n{}", subagent::PREAMBLE, request.prompt);
            self.prompt(&child.id, None, &text, "steer", Vec::new(), Vec::new())
                .map_err(|e| e.to_string())?;
            child
        };
        self.job_updated(&child, "running", None);
        Ok(child.id)
    }

    /// Says how child session `child`'s job stands.
    fn job_updated(&self, child: &Session, status: &str, completed: Option<i64>) {
        let Some(parent) = &child.parent_id else {
            return;
        };
        let agent = self
            .folder(&child.location.directory)
            .agent(child.agent.as_deref())
            .name;
        let title = child.title.clone().unwrap_or_default();
        let mut time = json!({ "started": child.time.created });
        if let Some(at) = completed {
            time["completed"] = json!(at);
        }
        let info = json!({
            "kind": "subagent", "id": child.id, "sessionID": parent, "status": status,
            "title": title, "time": time, "childSessionID": child.id, "agent": agent,
            "description": title,
        });
        lock(&self.live).jobs.insert(child.id.clone(), info.clone());
        self.emit(
            "experimental.job.updated",
            json!({ "info": info }),
            Some(&child.location.directory),
            None,
        );
    }

    /// When child session `id`'s run ended as `word`, sends its last
    /// answer to its parent, which runs on it.
    pub(crate) fn report_to_parent(self: &Arc<Self>, id: &str, word: &str) {
        let Ok(child) = self.session(id) else {
            return;
        };
        let Some(parent) = child.parent_id.clone() else {
            return;
        };
        let state = match word {
            "succeeded" => "completed",
            "interrupted" => "cancelled",
            _ => "failed",
        };
        let answer = self
            .store
            .messages(id)
            .unwrap_or_default()
            .iter()
            .rev()
            .find_map(|m| match m {
                Message::Assistant(step) => {
                    let text: String = step
                        .content
                        .iter()
                        .filter_map(|p| match p {
                            harlo_engine::Part::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect();
                    (!text.trim().is_empty()).then_some(text)
                }
                _ => None,
            })
            .unwrap_or_default();
        let description = child.title.clone().unwrap_or_default();
        let agent = self
            .folder(&child.location.directory)
            .agent(child.agent.as_deref())
            .name;
        let text = subagent::report(id, state, &description, &answer);
        let metadata =
            json!({ "source": "subagent", "childID": id, "agent": agent, "state": state });
        let message = Message::Synthetic {
            id: ascending_id("msg"),
            time: Created { created: now() },
            text: text.clone(),
            description: Some(description.clone()),
            metadata: Some(metadata.clone()),
        };
        self.job_updated(&child, state, Some(now()));
        self.emit(
            "session.inbox.enqueued",
            json!({
                "inboxID": message.id(), "sessionID": parent,
                "item": { "type": "synthetic", "payload": { "text": text, "description": description, "metadata": metadata }, "delivery": "queue" },
            }),
            Some(&child.location.directory),
            Some(&parent),
        );
        self.enqueue(&parent, message, "queue");
    }

    /// The background jobs going, or ended since the server started.
    pub(crate) fn jobs(&self) -> Vec<Value> {
        lock(&self.live).jobs.values().cloned().collect()
    }
}
