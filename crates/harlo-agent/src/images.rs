//! Images the model is shown: a prompt's, a file read, an MCP tool's.
//!
//! Each is kept as pi's image block (`{type: "image", data, mimeType}`),
//! scaled first to what a model takes well: at most `MAX_SIDE` pixels a
//! side and about `MAX_BYTES`.

use std::io::Cursor;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use image::{DynamicImage, GenericImageView, ImageFormat, ImageReader, imageops::FilterType};
use serde_json::{Value, json};

/// The longest side sent, in pixels.
pub const MAX_SIDE: u32 = 2048;
/// About the most an image sent may be, in bytes.
pub const MAX_BYTES: usize = 1_500_000;
/// What an image costs the context, roughly, in tokens.
pub const TOKENS: u64 = 1000;
/// Files larger than this aren't opened.
pub const MAX_FILE: usize = 50 << 20;

/// An image's type, from its first bytes (None: not one of those taken).
pub fn mime_of(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        _ => None,
    }
}

/// An image as it's sent.
#[derive(Debug)]
pub struct Fitted {
    pub data: Vec<u8>,
    pub mime: &'static str,
    /// Its size, and the size it was when it was scaled (None: sent as it was).
    pub size: (u32, u32),
    pub was: Option<(u32, u32)>,
}

impl Fitted {
    /// pi's block for it.
    pub fn block(&self) -> Value {
        json!({ "type": "image", "data": STANDARD.encode(&self.data), "mimeType": self.mime })
    }

    /// `1200×800 PNG, 312 KB` (and what it was scaled from).
    pub fn describe(&self) -> String {
        let kind = self.mime.trim_start_matches("image/").to_uppercase();
        let mut s = format!("{}×{} {kind}, {}", self.size.0, self.size.1, kb(self.data.len()));
        if let Some((w, h)) = self.was {
            s.push_str(&format!(", scaled from {w}×{h}"));
        }
        s
    }
}

fn kb(n: usize) -> String {
    if n >= 1 << 20 { format!("{:.1} MB", n as f64 / (1 << 20) as f64) } else { format!("{} KB", n.div_ceil(1024)) }
}

/// [`fit`], off the async threads (decoding and scaling a large image
/// takes a while).
pub async fn fit_apart(bytes: Vec<u8>) -> Result<Fitted, String> {
    tokio::task::spawn_blocking(move || fit(&bytes)).await.unwrap_or_else(|e| Err(format!("it couldn't be read ({e})")))
}

/// `bytes` as they'll be sent: as they are when they're small enough,
/// otherwise scaled down (its first frame, for an animation) and saved
/// again (a photo as JPEG). Err: why it can't be sent.
pub fn fit(bytes: &[u8]) -> Result<Fitted, String> {
    let Some(mime) = mime_of(bytes) else { return Err("it isn't a PNG, JPEG, GIF or WebP image".into()) };
    if bytes.len() > MAX_FILE {
        return Err(format!("it's {}, more than the {} an image may be", kb(bytes.len()), kb(MAX_FILE)));
    }
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let (w, h) = reader.into_dimensions().map_err(|e| format!("it can't be read ({e})"))?;
    if w <= MAX_SIDE && h <= MAX_SIDE && bytes.len() <= MAX_BYTES {
        return Ok(Fitted { data: bytes.to_vec(), mime, size: (w, h), was: None });
    }
    let img = image::load_from_memory(bytes).map_err(|e| format!("it can't be read ({e})"))?;
    let mut side = MAX_SIDE.min(w.max(h));
    // (A photo stays a JPEG; anything else tries PNG first, which keeps
    // text and lines sharp.)
    let mut png = mime != "image/jpeg";
    for _ in 0..6 {
        let scaled = if w.max(h) > side { img.resize(side, side, FilterType::Triangle) } else { img.clone() };
        let data = if png { encode_png(&scaled)? } else { encode_jpeg(&scaled)? };
        if data.len() <= MAX_BYTES {
            let mime = if png { "image/png" } else { "image/jpeg" };
            return Ok(Fitted { data, mime, size: scaled.dimensions(), was: Some((w, h)) });
        }
        if png {
            png = false;
        } else {
            // (Smaller by the share it's over, and a little more.)
            let share = (MAX_BYTES as f64 / data.len() as f64).sqrt() * 0.9;
            side = ((side as f64 * share) as u32).max(64);
        }
    }
    Err("it's too large even scaled down".into())
}

fn encode_png(img: &DynamicImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out)
}

/// As a JPEG (quality 85), what's see-through on white.
fn encode_jpeg(img: &DynamicImage) -> Result<Vec<u8>, String> {
    let rgb = if img.color().has_alpha() {
        let mut rgba = img.to_rgba8();
        for p in rgba.pixels_mut() {
            let a = p[3] as u32;
            for c in 0..3 {
                p[c] = ((p[c] as u32 * a + 255 * (255 - a)) / 255) as u8;
            }
        }
        DynamicImage::ImageRgba8(rgba).to_rgb8()
    } else {
        img.to_rgb8()
    };
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85).encode_image(&rgb).map_err(|e| e.to_string())?;
    Ok(out)
}

/// An image block from elsewhere (harlo's prompt, an MCP server's result),
/// fitted: Err says why it can't be sent.
pub fn from_block(b: &Value) -> Result<Fitted, String> {
    let data = b["data"].as_str().ok_or("it has no data")?;
    let bytes = STANDARD.decode(data.trim()).map_err(|_| "its data isn't base64".to_owned())?;
    fit(&bytes)
}

/// Images given with a prompt, fitted: those that can't be sent become a
/// line of text saying why.
pub fn prompt_blocks(images: &[Value]) -> Vec<Value> {
    images
        .iter()
        .map(|b| match from_block(b) {
            Ok(f) => f.block(),
            Err(why) => json!({ "type": "text", "text": format!("[An image was attached but can't be sent: {why}.]") }),
        })
        .collect()
}

/// A block's data URL.
pub fn data_url(b: &Value) -> String {
    format!("data:{};base64,{}", b["mimeType"].as_str().unwrap_or("image/png"), b["data"].as_str().unwrap_or_default())
}

/// What a model that doesn't take images gets in an image's place.
pub fn not_sent(model: &str) -> String {
    format!("[image: not sent, {model} doesn't take images]")
}

/// Whether a message holds an image.
pub fn has_image(m: &Value) -> bool {
    m["content"].as_array().is_some_and(|c| c.iter().any(|b| b["type"] == "image"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, y| Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]));
        encode_png(&DynamicImage::ImageRgba8(img)).unwrap()
    }

    #[test]
    fn knows_its_types() {
        assert_eq!(mime_of(&png(2, 2)), Some("image/png"));
        assert_eq!(mime_of(b"GIF89a.."), Some("image/gif"));
        assert_eq!(mime_of(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(mime_of(b"hello"), None);
        assert!(fit(b"hello").unwrap_err().contains("isn't a PNG"));
    }

    #[test]
    fn small_ones_go_as_they_are() {
        let bytes = png(40, 30);
        let f = fit(&bytes).unwrap();
        assert_eq!((f.data, f.size, f.was), (bytes, (40, 30), None));
    }

    #[test]
    fn large_ones_are_scaled() {
        let f = fit(&png(4100, 1000)).unwrap();
        assert_eq!(f.size.0, MAX_SIDE);
        assert!(f.size.1 == 499 || f.size.1 == 500, "{:?}", f.size);
        assert_eq!(f.was, Some((4100, 1000)));
        assert!(f.data.len() <= MAX_BYTES);
        assert!(f.describe().contains("scaled from 4100×1000"));
    }

    #[test]
    fn noisy_ones_become_jpegs_small_enough() {
        // (Noise doesn't compress: as PNG it's over the limit.)
        let mut seed = 1u32;
        let img = RgbaImage::from_fn(1500, 1500, |_, _| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let b = seed.to_le_bytes();
            Rgba([b[0], b[1], b[2], 255])
        });
        let f = fit(&encode_png(&DynamicImage::ImageRgba8(img)).unwrap()).unwrap();
        assert_eq!(f.mime, "image/jpeg");
        assert!(f.data.len() <= MAX_BYTES);
    }

    #[test]
    fn blocks_round_trip() {
        let b = fit(&png(3, 3)).unwrap().block();
        assert_eq!(b["mimeType"], "image/png");
        assert_eq!(from_block(&b).unwrap().size, (3, 3));
        assert!(data_url(&b).starts_with("data:image/png;base64,iVBOR"));
        let bad = prompt_blocks(&[json!({ "type": "image", "data": "aGVsbG8=", "mimeType": "image/png" })]);
        assert_eq!(bad[0]["type"], "text");
    }
}
