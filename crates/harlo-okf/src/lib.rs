//! harlo's project wiki: an Open Knowledge Format (OKF v0.2) bundle in a
//! repository's `.okf/` folder, kept current with the code it describes.
//!
//! Pages are OKF concepts: Markdown with YAML front matter, each claim
//! footnoted with the file and the declaration it comes from. The engine
//! here finds those declarations in the code ([`units`]), so it can tell,
//! with no model, which claims the code has moved past and which new code
//! no page covers yet.

// Rust guideline compliant 2026-10-09

pub mod bundle;
pub mod check;
pub mod cite;
pub mod ledger;
pub mod lex;
pub mod page;
pub mod scope;
pub mod split;
pub mod stamp;
pub mod units;
pub mod view;
pub mod work;
