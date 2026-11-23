//! Client for OpenCode 2's server API.
//!
//! Harlo attaches to OpenCode's shared background service (found through
//! [`read_registration`]) or to an explicit server, and uses its HTTP API for
//! sessions, prompts, approvals, questions, diffs and undo, and its SSE stream
//! for live events.
//!
//! The server's schema is pinned in `schema/openapi.json` (see
//! `OPENCODE_VERSION`), and `opencode-gen` turns it into private types. This
//! crate's public types are its own, so a schema change stops here.
//!
//! The client never starts, stops, restarts or reconfigures a server.

// Rust guideline compliant 2026-10-09

mod catalog;
mod client;
mod endpoint;
mod error;
mod event;
mod generated;
mod id;
mod prompt;
mod pty;
mod settings;
mod setup;
mod types;

#[doc(inline)]
pub use catalog::{Agent, Command, FileMatch, McpResource, Model, Skill};
#[doc(inline)]
pub use client::{Client, Events};
#[doc(inline)]
pub use endpoint::{Endpoint, SharedService, read_registration, registration_path};
#[doc(inline)]
pub use error::Error;
#[doc(inline)]
pub use event::{Event, FileChange, Kind as EventKind};
#[doc(inline)]
pub use id::message_id;
#[doc(inline)]
pub use prompt::{Attachment, Mention, Prompt};
#[doc(inline)]
pub use pty::{Pty, PtyInput, PtyOutput};
#[doc(inline)]
pub use settings::{
    Connection, Day, Integration, McpServer, McpStatus, ModelUsage, Plugin, PluginSource, Provider,
    Stats, WebSearch,
};
#[doc(inline)]
pub use setup::{CommandSetup, Setup};
#[doc(inline)]
pub use types::{
    Answer, Choice, Condition, Delivery, Effect, Field, FieldKind, FileDiff, Form, InboxItem,
    Message, ModelRef, PROJECT_COLORS, Part, Permission, PermissionReply, Project, ProjectChange,
    Report, Revert, Rule, ServerInfo, Session, SessionContext, Tokens, ToolCall, ToolStatus,
};

/// The OpenCode release whose schema this crate is built from.
pub const OPENCODE_VERSION: &str = include_str!("../OPENCODE_VERSION").trim_ascii();
