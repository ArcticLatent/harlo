//! A session's agent and model, switched by a person, and the messages
//! that steer a running turn.

// Rust guideline compliant 2026-10-09

use std::sync::Arc;

use harlo_engine::{Created, Message, ModelRef, Steering, ascending_id, now};
use serde_json::json;

use crate::server::{Refusal, Server, lock};

impl Server {
    /// Sets session `id`'s model, with its variant.
    ///
    /// # Errors
    ///
    /// When there's no such session, or the model isn't configured.
    pub(crate) fn set_model(&self, id: &str, model: ModelRef) -> Result<(), Refusal> {
        let session = self.session(id)?;
        let folder = self.folder(&session.location.directory);
        if folder.config.resolve(&model).is_none() {
            return Err(Refusal::Bad(format!(
                "The model {}/{} isn't in the configuration.",
                model.provider, model.model
            )));
        }
        let session = self.change(id, |s| {
            s.model = Some(model);
            s.time.updated = now();
        })?;
        self.emit(
            "session.model.selected",
            json!({ "sessionID": id, "model": session.model }),
            Some(&session.location.directory),
            Some(id),
        );
        Ok(())
    }

    /// Sets session `id`'s agent. Leaving an agent and entering one that
    /// tells the model so queues those notices, delivered at the next safe
    /// point.
    ///
    /// # Errors
    ///
    /// When there's no such session, or no such agent to talk to.
    pub(crate) fn set_agent(&self, id: &str, agent: &str) -> Result<(), Refusal> {
        let session = self.session(id)?;
        let folder = self.folder(&session.location.directory);
        let Some(next) = folder
            .agents
            .iter()
            .find(|a| a.id == agent && a.mode.talks_to_people())
        else {
            return Err(Refusal::Bad(format!(
                "There's no agent {agent} to switch to."
            )));
        };
        let before = folder.agent(session.agent.as_deref());
        if before.id == next.id {
            return Ok(());
        }
        let notices: Vec<String> = [before.left.clone(), next.entered.clone()]
            .into_iter()
            .flatten()
            .collect();
        self.change(id, |s| {
            s.agent = Some(next.id.clone());
            s.time.updated = now();
        })?;
        let directory = session.location.directory.clone();
        self.emit(
            "session.agent.selected",
            json!({ "sessionID": id, "agent": next.id }),
            Some(&directory),
            Some(id),
        );
        for text in notices {
            let message = Message::Synthetic {
                id: ascending_id("msg"),
                time: Created { created: now() },
                text: text.clone(),
                description: None,
                metadata: None,
            };
            self.emit(
                "session.inbox.enqueued",
                json!({
                    "inboxID": message.id(), "sessionID": id,
                    "item": { "type": "synthetic", "payload": { "text": text }, "delivery": "steer" },
                }),
                Some(&directory),
                Some(id),
            );
            lock(&self.live)
                .inbox
                .entry(id.to_owned())
                .or_default()
                .push((message, "steer".to_owned()));
        }
        Ok(())
    }

    /// Takes session `id`'s messages sent to steer it, stores them and
    /// says they were delivered.
    pub(crate) fn take_steers(&self, id: &str, directory: &str) -> Vec<Message> {
        let steers: Vec<Message> = {
            let mut live = lock(&self.live);
            let Some(queue) = live.inbox.get_mut(id) else {
                return Vec::new();
            };
            // A compaction waits for the run's end.
            let (steer, rest): (Vec<_>, Vec<_>) = queue.drain(..).partition(|(m, delivery)| {
                delivery == "steer" && !matches!(m, Message::Compaction(_))
            });
            *queue = rest;
            steer.into_iter().map(|(m, _)| m).collect()
        };
        for message in &steers {
            if let Err(e) = self.store.put_message(id, message) {
                tracing::error!(name: "message.store.failed", error = %e, "couldn't store a steer");
            }
            self.emit(
                "session.inbox.delivered",
                json!({ "sessionID": id, "inboxID": message.id() }),
                Some(directory),
                Some(id),
            );
        }
        steers
    }
}

/// A running session's steering: what people sent to steer it.
#[derive(Debug)]
pub(crate) struct SessionSteering {
    /// The server.
    pub server: Arc<Server>,
    /// The session.
    pub session: String,
    /// Its folder.
    pub directory: String,
}

impl Steering for SessionSteering {
    fn take(&self) -> Vec<Message> {
        self.server.take_steers(&self.session, &self.directory)
    }
}
