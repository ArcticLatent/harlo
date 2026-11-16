//! harlo's wiki: OpenWiki-format wikis (a repository's `openwiki/` folder)
//! read, searched and written natively, with the same tools and answers as
//! OpenWiki (MIT), from which its behavior is ported. OpenWiki's licence
//! is in LICENSE-OPENWIKI; its authoring steps (`AUTHORING`) and lifecycle
//! instructions are its own words.
//!
//! Its errors ([`store::Error`]) are OpenWiki's error families, and the
//! text of each is OpenWiki's, word for word: the tools turn them into the
//! same answers. Where OpenWiki names only what failed (a path it couldn't
//! open, a file that isn't JSON), so do these, without the underlying cause.

// Rust guideline compliant 2026-10-09

#![expect(clippy::map_err_ignore, reason = "OpenWiki's answers name what failed, never the underlying error, and harlo's must match them word for word")]

/// OpenWiki's steps for writing a wiki through the tools (from its skill),
/// for a writing session's first message.
pub const AUTHORING: &str = include_str!("authoring.md");

pub mod jsre;
pub mod marked;
pub mod repo;
pub mod front;
pub mod ignore;
pub mod lang;
mod lang_data;
pub mod claims;
pub mod finish;
pub mod fsb;
pub mod run;
pub mod collate;
pub mod okf;
pub mod mcp;
pub mod retrieval;
pub mod store;
pub mod tools;
pub mod util;
pub mod workspaces;
