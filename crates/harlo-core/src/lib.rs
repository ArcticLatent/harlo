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
pub mod download;
pub mod files;
pub mod finder;
pub mod fonts;
pub mod git;
pub mod hosting;
pub mod instructions;
pub mod layout;
pub mod local_models;
pub mod makers;
pub mod mcp_catalog;
pub mod meter;
pub mod model_menu;
pub mod notes;
pub mod notifications;
pub mod phone;
pub mod schedules;
pub mod server;
pub mod sessions;
pub mod ssh;
pub mod terminal;
pub mod theme;
pub mod timeline;
pub mod undo;
pub mod updates;
pub mod voice;
pub mod workspace;
