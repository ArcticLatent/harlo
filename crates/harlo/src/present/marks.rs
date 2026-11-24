//! A project's mark: the colour the server keeps for it, and the icon image
//! chosen for it, decoded once and kept while the same image is in use.

// Rust guideline compliant 2026-10-09

use std::cell::RefCell;
use std::collections::HashMap;

use slint::{Color, Image, Rgba8Pixel, SharedPixelBuffer};

/// The colour Harlo draws the server's project colour `name` in, one tone
/// that reads on light and dark backgrounds; `None` for a name it doesn't
/// know.
#[must_use]
pub fn project_color(name: &str) -> Option<Color> {
    let [r, g, b] = match name {
        "orange" => [0xf9, 0x73, 0x16],
        "yellow" => [0xea, 0xb3, 0x08],
        "cyan" => [0x06, 0xb6, 0xd4],
        "green" => [0x22, 0xc5, 0x5e],
        "red" => [0xef, 0x44, 0x44],
        "pink" => [0xec, 0x48, 0x99],
        "blue" => [0x3b, 0x82, 0xf6],
        "purple" => [0xa8, 0x55, 0xf7],
        "gray" => [0x8b, 0x8b, 0x94],
        _ => return None,
    };
    Some(Color::from_rgb_u8(r, g, b))
}

/// Project icons, decoded from their `data:` URLs.
#[derive(Debug, Default)]
pub struct Marks {
    /// Each URL's image; an empty image when it can't be decoded.
    decoded: RefCell<HashMap<String, Image>>,
}

impl Marks {
    /// The image at `url`, which must be a `data:` URL; others, and ones
    /// that don't decode, give an empty image, and the plain mark shows.
    pub fn image(&self, url: Option<&str>) -> Image {
        let Some(url) = url else {
            return Image::default();
        };
        let mut decoded = self.decoded.borrow_mut();
        if let Some(image) = decoded.get(url) {
            return image.clone();
        }
        // Icons change rarely: forget old ones rather than keep them all.
        if decoded.len() > 64 {
            decoded.clear();
        }
        let image = decode(url).unwrap_or_default();
        decoded.insert(url.to_owned(), image.clone());
        image
    }
}

/// Decodes a base64 `data:` URL holding an image.
fn decode(url: &str) -> Option<Image> {
    use base64::Engine as _;
    let (head, data) = url.strip_prefix("data:")?.split_once(',')?;
    if !head.ends_with(";base64") {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .ok()?;
    let picture = image::load_from_memory(&bytes).ok()?.into_rgba8();
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        picture.as_raw(),
        picture.width(),
        picture.height(),
    );
    Some(Image::from_rgba8(buffer))
}
