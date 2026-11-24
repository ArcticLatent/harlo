//! Harlo's agent engine: the loop that sends a session's history to a
//! model, runs the tools it calls, and goes on until it answers.
//!
//! The engine is a library, so the daemon (`harlo-server`) and tests run
//! the same code. It owns no storage and no wire format: a turn reads the
//! history it is given, reports what happens through a [`Sink`], asks a
//! [`Gate`] before any tool that needs permission, and stops when its
//! [`CancellationToken`](tokio_util::sync::CancellationToken) is cancelled.
//!
//! Messages follow OpenCode 2's shape (one assistant message per model
//! step, tool results inside their tool part), so a server can store them
//! as they are and clients that read OpenCode's sessions read these too.

// Rust guideline compliant 2026-10-09

pub mod agents;
pub mod attachment;
pub mod catalog;
mod chat;
pub mod codemode;
pub mod commands;
pub mod compaction;
mod config;
mod error;
pub mod generate;
mod id;
pub mod instructions;
pub mod jobs;
pub mod memory;
mod message;
pub mod question;
mod rules;
pub mod skills;
pub mod snapshot;
pub mod stale;
pub mod subagent;
pub mod todo;
pub mod tools;
mod turn;

pub use config::{
    Config, ModelRef, ModelSpec, Price, ProviderSpec, VariantSpec, merge_config,
    project_config_files, read_config_document,
};
pub use error::Error;
pub use id::{ascending_id, descending_id, just_before};
pub use message::{
    Assistant, Cache, Created, ErrorInfo, Message, Part, ShellOutput, ShellRun, ShellTime,
    Snapshot, StepTime, Tokens, ToolState, ToolTime,
};
pub use rules::{Decision, Effect, Rule, agent_rules, decide};
pub use turn::{Answer, Ask, Event, Gate, Outcome, Sink, Steering, Turn, now, run};
