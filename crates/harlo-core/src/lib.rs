//! Harlo's domain model: themes, projects and sessions, with no UI code.
//!
//! The iced app (`harlo`) depends on this crate only. Everything here is plain
//! data and pure functions, so it is tested without a window.

// Rust guideline compliant 2026-10-09

pub mod approvals;
pub mod composer;
pub mod diff;
pub mod git;
pub mod layout;
pub mod server;
pub mod sessions;
pub mod theme;
pub mod timeline;
pub mod undo;
pub mod workspace;

/// The OpenCode release Harlo is built and tested against.
pub const OPENCODE_VERSION: &str = opencode_client::OPENCODE_VERSION;

/// Harlo's default theme, in OpenCode's desktop theme format.
pub const DEFAULT_THEME: &str = include_str!("../themes/harlo.json");
