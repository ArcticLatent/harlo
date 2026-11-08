//! UI-agnostic application logic: transcript reduction, markdown rendering,
//! live session driving, the sidebar model and persistence. Nothing here
//! depends on Qt.

pub mod actions;
pub mod attach;
pub mod autostart;
pub mod commitmsg;
pub mod context;
pub mod db;
#[cfg(feature = "dictation")]
pub mod dictation;
pub mod engine_host;
pub mod agent_host;
pub mod remote_helper;
pub mod engine_setup;
pub mod engines;
pub mod export;
pub mod files;
pub mod firewall;
pub mod project_icons;
pub mod providers;
pub mod git;
pub mod git_hosting;
pub mod listdiff;
pub mod markdown;
pub mod mcp;
pub mod model_servers;
pub mod orchestration;
pub mod migrate;
pub mod mcp_probe;
pub mod notifications;
pub mod palette;
pub mod phone;
pub mod plan;
pub mod quota;
pub mod remote;
pub mod remote_sessions;
pub mod scanner;
pub mod schedules;
pub mod search;
pub mod session_ref;
pub mod shelf;
pub mod servers;
#[cfg(test)]
pub(crate) mod scratch;
pub mod session;
pub mod signin;
pub mod sidebar;
pub mod spending;
pub mod specs;
pub mod splitdiff;
pub mod starters;
pub mod subagents;
pub mod templates;
pub mod stats;
pub mod terminal;
pub mod tool_facts;
pub mod transcript;
pub mod updates;

pub use session::{Notable, SessionHandle, SessionMetaInfo, SessionSink, SessionStatus, SessionUpdate, SlashCommand, UiRequest};
pub use transcript::{Row, RowChange, RowKind, Transcript, diff_rows};
