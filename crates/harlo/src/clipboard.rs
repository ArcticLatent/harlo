//! Images pasted from the system clipboard, as PNG bytes.
//!
//! The window's clipboard carries text only, so images come through
//! `arboard`. On Wayland it uses the data-control protocol, falling back to X11.

// Rust guideline compliant 2026-10-09

use futures_util::future::FutureExt as _;
use tokio::sync::oneshot;

/// Reads an image from the clipboard and encodes it as PNG.
///
/// Returns `None` when the clipboard holds no image, or can't be read. The
/// read runs on its own thread, since clipboard owners can be slow to answer.
pub async fn image() -> Option<Vec<u8>> {
    let (sender, receiver) = oneshot::channel();
    std::thread::Builder::new()
        .name("clipboard".to_owned())
        .spawn(move || {
            // The receiver is gone only if the paste was abandoned.
            let _ = sender.send(read());
        })
        .ok()?;
    receiver.map(|read| read.ok().flatten()).await
}

fn read() -> Option<Vec<u8>> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let image = clipboard.get_image().ok()?;
    let width = u32::try_from(image.width).ok()?;
    let height = u32::try_from(image.height).ok()?;
    encode(width, height, &image.bytes)
}

/// Encodes 8-bit RGBA pixels as a PNG.
fn encode(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(rgba).ok()?;
    writer.finish().ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn pixels_become_a_png() {
        let png = encode(2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]).expect("encodes");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(encode(2, 2, &[0; 4]), None, "too few pixels");
    }
}
