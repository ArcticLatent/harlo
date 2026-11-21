//! Widgets iced 0.14 doesn't provide.

// Rust guideline compliant 2026-10-09

mod breathe;
mod ellipsis;
mod place;
mod reveal;
mod snug;

#[doc(inline)]
pub use breathe::{Breathe, breathe};
#[doc(inline)]
pub use ellipsis::{Ellipsis, ellipsis};
#[doc(inline)]
pub use place::{Place, place};
#[doc(inline)]
pub use reveal::{Edge, Reveal, reveal};
#[doc(inline)]
pub use snug::{Snug, snug};
