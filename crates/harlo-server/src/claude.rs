//! Claude Agent: Claude Code, signed in with the person's own Claude
//! login, offered as the `claude-agent` provider. Its models are what
//! Claude Code lists; a session on one of them runs its turns through
//! Claude Code.

// Rust guideline compliant 2026-10-09

use std::path::PathBuf;
use std::sync::Arc;

use harlo_engine::claude::{ClaudeTurn, PROVIDER};
use harlo_engine::{Message, ModelRef, ModelSpec, Outcome, ProviderSpec, VariantSpec};
use serde_json::Map;
use tokio_util::sync::CancellationToken;

use crate::server::{Server, lock};
use crate::store::Session;

/// The `claude` program: `OPENCODE_CLAUDE_PATH`, as OpenCode reads it, or
/// `claude` on the path.
pub(crate) fn program() -> PathBuf {
    std::env::var_os("OPENCODE_CLAUDE_PATH")
        .filter(|p| !p.is_empty())
        .map_or_else(|| PathBuf::from("claude"), PathBuf::from)
}

/// The setting that keeps a session's Claude Code session.
fn resume_key(session: &str) -> String {
    format!("claude/{session}")
}

/// What to send Claude Code: what was said since its last answer, and,
/// when it has no session yet, the conversation before it.
fn prompt(history: &[Message], fresh: bool) -> String {
    let said = |m: &Message| match m {
        Message::User { text, .. } | Message::Synthetic { text, .. } => {
            Some(("Person", text.clone()))
        }
        Message::Assistant(step) => {
            let text: String = step
                .content
                .iter()
                .filter_map(|p| match p {
                    harlo_engine::Part::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            (!text.is_empty()).then_some(("Assistant", text))
        }
        _ => None,
    };
    let last_answer = history
        .iter()
        .rposition(|m| matches!(m, Message::Assistant(_)))
        .map_or(0, |i| i + 1);
    let new: Vec<String> = history[last_answer..]
        .iter()
        .filter_map(said)
        .map(|(_, text)| text)
        .collect();
    let new = new.join("\n\n");
    if !fresh || last_answer == 0 {
        return new;
    }
    let earlier: Vec<String> = history[..last_answer]
        .iter()
        .filter_map(said)
        .map(|(who, text)| format!("{who}: {text}"))
        .collect();
    format!(
        "The conversation so far, before you joined it:\n\n{}\n\nNow: {new}",
        earlier.join("\n\n")
    )
}

impl Server {
    /// Whether Claude Agent is connected.
    fn claude_connected(&self) -> bool {
        self.store
            .credentials()
            .unwrap_or_default()
            .iter()
            .any(|c| c["integrationID"] == crate::credentials::CLAUDE_AGENT)
    }

    /// The `claude-agent` provider, with the models Claude Code listed,
    /// while Claude Agent is connected.
    pub(crate) fn claude_provider(&self) -> Option<ProviderSpec> {
        if !self.claude_connected() {
            return None;
        }
        Some(ProviderSpec {
            id: PROVIDER.to_owned(),
            name: "Claude Agent".to_owned(),
            base_url: String::new(),
            api_key: None,
            models: lock(&self.claude_models).clone(),
            wire: harlo_engine::catalog::Wire::Unsupported(PROVIDER.to_owned()),
        })
    }

    /// Asks Claude Code for its models again, when Claude Agent is
    /// connected.
    pub fn refresh_claude(self: &Arc<Self>) {
        if !self.claude_connected() {
            return;
        }
        let server = Arc::clone(self);
        tokio::spawn(async move {
            let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
            match harlo_engine::claude::models(&program(), &home).await {
                Ok(listed) => {
                    let models = listed
                        .into_iter()
                        .map(|(id, name, efforts)| ModelSpec {
                            id,
                            name,
                            variants: efforts
                                .into_iter()
                                .map(|id| VariantSpec {
                                    id,
                                    body: Map::new(),
                                    headers: Vec::new(),
                                })
                                .collect(),
                            ..ModelSpec::default()
                        })
                        .collect();
                    *lock(&server.claude_models) = models;
                    server.integrations_changed();
                }
                Err(why) => {
                    tracing::warn!(name: "claude.models.failed", error = %why, "Claude Code didn't list its models");
                }
            }
        });
    }

    /// Runs session `session`'s turn through Claude Code.
    pub(crate) async fn run_claude(
        self: &Arc<Self>,
        session: &Session,
        model: ModelRef,
        agent: &str,
        rules: Vec<(String, Vec<harlo_engine::Rule>)>,
        history: &[Message],
        stop: &CancellationToken,
    ) -> (&'static str, Option<String>) {
        let key = resume_key(&session.id);
        let resume = self.store.setting(&key).ok().flatten();
        let turn = ClaudeTurn {
            program: program(),
            cwd: PathBuf::from(&session.location.directory),
            agent: agent.to_owned(),
            model,
            prompt: prompt(history, resume.is_none()),
            resume,
            rules,
        };
        let hooks = crate::server::Hooks::new(self, &session.id, &session.location.directory);
        let ran = harlo_engine::claude::run(turn, &hooks, &hooks, stop).await;
        if let Some(claude) = &ran.session {
            let _ = self.store.put_setting(&key, claude);
        }
        match ran.outcome {
            Outcome::Succeeded => ("succeeded", None),
            Outcome::Interrupted => ("interrupted", None),
            Outcome::Failed(e) => ("failed", Some(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harlo_engine::Created;

    fn user(text: &str) -> Message {
        Message::User {
            id: "msg".into(),
            time: Created { created: 1 },
            text: text.into(),
            skills: Vec::new(),
            files: Vec::new(),
        }
    }

    #[test]
    fn a_fresh_session_hears_the_conversation_so_far() {
        assert_eq!(prompt(&[user("hi")], true), "hi");
        assert_eq!(prompt(&[user("a"), user("b")], false), "a\n\nb");
    }
}
