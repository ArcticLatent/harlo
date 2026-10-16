//! UI-agnostic application logic: transcript reduction, markdown rendering,
//! live session driving, the sidebar model and persistence. Nothing here
//! depends on Qt.

pub mod actions;
pub mod attach;
pub mod autostart;
pub mod commitmsg;
pub mod context;
pub mod db;
pub mod dictation;
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
pub mod orchestration;
pub mod migrate;
pub mod mcp_probe;
pub mod notifications;
pub mod omp_overlay;
pub mod palette;
pub mod phone;
pub mod quota;
pub mod remote;
pub mod remote_sessions;
pub mod scanner;
pub mod schedules;
pub mod search;
pub mod session;
pub mod signin;
pub mod sidebar;
pub mod spending;
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
