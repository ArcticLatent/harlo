//! harlo's code view, drawn in Rust: the text, its layout and the pixels of
//! the visible lines. No Qt here; harlo-app's CodeView shows the image this
//! draws and hands it the keys and the mouse.

pub mod doc;
pub mod fonts;
pub mod lang;
pub mod syntax;
pub mod terminal;
pub mod text;
pub mod view;

pub use view::View;
