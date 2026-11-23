//! Harlo's domain model: themes, projects and sessions, with no UI code.
//!
//! The app (`harlo`) depends on this crate only. Everything here is plain
//! data and pure functions, so it is tested without a window.

// Rust guideline compliant 2026-10-09

pub mod approvals;
pub mod autostart;
pub mod claude;
pub mod composer;
pub mod config;
pub mod diff;
pub mod files;
pub mod git;
pub mod hosting;
pub mod instructions;
pub mod layout;
pub mod mcp_catalog;
pub mod notes;
pub mod notifications;
pub mod own_tools;
pub mod schedules;
pub mod server;
pub mod sessions;
pub mod terminal;
pub mod theme;
pub mod timeline;
pub mod undo;
pub mod voice;
pub mod workspace;

/// The OpenCode release Harlo is built and tested against.
pub const OPENCODE_VERSION: &str = opencode_client::OPENCODE_VERSION;
