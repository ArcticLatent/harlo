//! Widgets iced 0.14 doesn't provide.

// Rust guideline compliant 2026-10-09

mod breathe;
mod diff_lines;
mod ellipsis;
mod place;
mod popover;
mod reveal;

#[doc(inline)]
pub use breathe::{Breathe, breathe};
#[doc(inline)]
pub use diff_lines::{Colors as DiffColors, DiffLines, diff_lines};
#[doc(inline)]
pub use ellipsis::{Ellipsis, ellipsis};
#[doc(inline)]
pub use place::{Place, place};
#[doc(inline)]
pub use popover::{Popover, popover};
#[doc(inline)]
pub use reveal::{Edge, Reveal, reveal};
