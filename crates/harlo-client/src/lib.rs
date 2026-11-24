//! Client for Harlo's server API.
//!
//! Harlo attaches to its server's background service (found through
//! [`read_registration`]) or to an explicit server, and uses its HTTP API for
//! sessions, prompts, approvals, questions, diffs and undo, and its SSE stream
//! for live events.
//!
//! The API's wire types are private to this crate (`wire`); its public types
//! are its own, so a change to the wire stops here.
//!
//! The client never starts, stops, restarts or reconfigures a server.

// Rust guideline compliant 2026-10-09

mod approval;
mod automation;
mod catalog;
mod client;
mod daemon;
mod endpoint;
mod error;
mod event;
mod id;
mod integration;
mod job;
mod prompt;
mod pty;
mod settings;
mod setup;
mod tool_host;
mod transfer;
mod types;
mod wire;

#[doc(inline)]
pub use approval::SavedApproval;
#[doc(inline)]
pub use automation::{
    Action as AutomationAction, Automation, NewAction, NewAutomation, Status as AutomationStatus,
    Trigger as AutomationTrigger,
};
#[doc(inline)]
pub use catalog::{Agent, Command, FileMatch, McpResource, Model, Skill};
#[doc(inline)]
pub use client::{Client, Events};
#[doc(inline)]
pub use daemon::{
    AvatarPick, Note, NoteScope, NoteSource, PhoneDevice, PhoneRelay, PhoneState, PhoneWanted,
};
#[doc(inline)]
pub use endpoint::{
    Endpoint, SharedService, opencode_registration_path, parse_registration, read_registration,
};
#[doc(inline)]
pub use error::Error;
#[doc(inline)]
pub use event::{Event, FileChange, Kind as EventKind};
#[doc(inline)]
pub use id::message_id;
#[doc(inline)]
pub use integration::{AttemptStatus, OAuthAttempt, SignInMethod};
#[doc(inline)]
pub use job::{Job, JobKind, JobStatus};
#[doc(inline)]
pub use prompt::{Attachment, Mention, Prompt};
#[doc(inline)]
pub use pty::{Pty, PtyInput, PtyOutput};
#[doc(inline)]
pub use settings::{
    Connection, Day, Integration, McpServer, McpStatus, ModelUsage, Provider, Stats, WebSearch,
};
#[doc(inline)]
pub use setup::{CommandSetup, Setup};
#[doc(inline)]
pub use tool_host::{ToolHost, ToolOutcome, ToolRun, ToolSpec};
#[doc(inline)]
pub use types::{
    Answer, Choice, Condition, Delivery, Effect, Field, FieldKind, FileDiff, Form, Goal,
    GoalStatus, InboxItem, Message, ModelRef, PROJECT_COLORS, Part, Permission, PermissionReply,
    Picture, Project, ProjectChange, Report, Revert, Rule, ScriptCall, ServerInfo, Session,
    SessionContext, ShellRun, ShellStatus, Tokens, ToolCall, ToolStatus,
};
