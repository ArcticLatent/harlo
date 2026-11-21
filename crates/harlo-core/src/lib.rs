//! Harlo's domain model: themes, projects and sessions, with no UI code.
//!
//! The iced app (`harlo`) depends on this crate only. Everything here is plain
//! data and pure functions, so it is tested without a window.

// Rust guideline compliant 2026-10-09

pub mod sessions;
pub mod theme;

/// Harlo's default theme, in OpenCode's desktop theme format.
pub const DEFAULT_THEME: &str = include_str!("../themes/harlo.json");
