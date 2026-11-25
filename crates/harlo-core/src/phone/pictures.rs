//! A session's pictures as a phone gets them: a small one for its row, or
//! one fitted to the screen.
//!
//! A phone refuses a message over 1 MB, and a screenshot from a large
//! display is several times that, so every picture is drawn smaller and
//! encoded again before it's sent: as JPEG, or as PNG when it has
//! transparency.

// Rust guideline compliant 2026-10-09

use std::io::Cursor;

use harlo_client::Picture;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat};
use serde_json::{Value, json};

use super::b64;

/// The hex digits in a picture's name: its whole key. The app takes a name
/// of sixteen or more hex digits for one Harlo made up, and says where
/// the picture came from instead ("You sent").
const NAME_DIGITS: usize = 16;

/// The longest side of a picture in a row, in pixels: a 72-point tile on
/// a phone's densest screen is 216 pixels.
const THUMB_SIDE: u32 = 320;

/// The longest sides tried for a picture opened on the phone, largest
/// first: the first whose encoding fits [`MOST_BYTES`] is sent.
const SCREEN_SIDES: [u32; 3] = [1600, 1100, 720];

/// The most bytes of an encoded picture: a third larger as base64, and
/// still well under the 1 MB a phone takes in one message.
const MOST_BYTES: usize = 600 * 1024;

/// How good a JPEG is, out of 100: screenshots keep their text readable.
const JPEG_QUALITY: u8 = 82;

/// The name a row gives `picture`, which the phone asks for it by: its
/// key in hex, with a picture's suffix.
pub(super) fn name(picture: &Picture) -> String {
    let suffix = match picture.mime.as_str() {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    };
    format!("{:016x}.{suffix}", picture.key)
}

/// The picture key in a name a row gave; `None` for anything else.
pub(super) fn key(name: &str) -> Option<u64> {
    let (digits, _) = name.split_once('.')?;
    (digits.len() == NAME_DIGITS)
        .then(|| u64::from_str_radix(digits, 16).ok())
        .flatten()
}

/// Returns `picture` as the phone's reply: small for a row when `thumb`,
/// otherwise fitted to a phone's screen.
///
/// # Errors
///
/// Fails when the picture can't be read, or can't be made small enough
/// to send.
pub(super) fn fitted(picture: &Picture, thumb: bool) -> Result<Value, String> {
    let unreadable = |_| "Harlo couldn't read that picture.".to_owned();
    let decoded = image::load_from_memory(&picture.data).map_err(unreadable)?;
    let sides: &[u32] = if thumb { &[THUMB_SIDE] } else { &SCREEN_SIDES };
    for &side in sides {
        // Never drawn larger than it is.
        let drawn = if decoded.width().max(decoded.height()) > side {
            decoded.resize(side, side, FilterType::Triangle)
        } else {
            decoded.clone()
        };
        let (bytes, mime) = encoded(&drawn).map_err(unreadable)?;
        if bytes.len() <= MOST_BYTES {
            return Ok(json!({ "data": b64(&bytes), "type": mime,
                              "width": drawn.width(), "height": drawn.height() }));
        }
    }
    Err("That picture is too large to send to a phone.".to_owned())
}

/// A picture's bytes and media type: PNG with transparency, else JPEG.
fn encoded(picture: &DynamicImage) -> Result<(Vec<u8>, &'static str), image::ImageError> {
    let mut bytes = Vec::new();
    if picture.color().has_alpha() {
        picture.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)?;
        return Ok((bytes, "image/png"));
    }
    let opaque = DynamicImage::ImageRgb8(picture.to_rgb8());
    opaque.write_with_encoder(JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY))?;
    Ok((bytes, "image/jpeg"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use image::{Rgb, RgbImage, Rgba, RgbaImage};

    use super::*;

    fn picture(image: &DynamicImage) -> Picture {
        let mut bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .expect("a PNG");
        Picture {
            name: "shot.png".to_owned(),
            mime: "image/png".to_owned(),
            data: Arc::from(bytes),
            key: 0x00ab_cdef,
        }
    }

    #[test]
    fn a_pictures_name_leads_back_to_its_key() {
        let p = picture(&DynamicImage::ImageRgb8(RgbImage::new(2, 2)));
        assert_eq!(name(&p), "0000000000abcdef.png");
        assert_eq!(key(&name(&p)), Some(0x00ab_cdef));
        assert_eq!(key("src/shot.png"), None);
        assert_eq!(key("abcdef.png"), None);
        assert_eq!(key("0000000000abcdeg.png"), None);
    }

    #[test]
    fn a_large_picture_is_drawn_to_fit_and_a_small_one_left_its_size() {
        let wide = RgbImage::from_fn(3000, 1500, |x, y| {
            let shade = u8::try_from((x + y) % 251).unwrap_or(0);
            Rgb([shade, 120, 200])
        });
        let wide = picture(&DynamicImage::ImageRgb8(wide));
        let thumb = fitted(&wide, true).expect("a thumbnail");
        assert_eq!(
            (thumb["width"].as_u64(), thumb["height"].as_u64()),
            (Some(320), Some(160))
        );
        assert_eq!(thumb["type"], "image/jpeg");
        let screen = fitted(&wide, false).expect("a picture");
        assert_eq!(screen["width"], 1600);
        let sent = screen["data"].as_str().unwrap_or_default().len();
        assert!(sent < 900 * 1024, "under what a phone takes: {sent}");

        let small = picture(&DynamicImage::ImageRgb8(RgbImage::new(40, 30)));
        let same = fitted(&small, false).expect("a picture");
        assert_eq!(
            (same["width"].as_u64(), same["height"].as_u64()),
            (Some(40), Some(30))
        );
    }

    #[test]
    fn transparency_is_kept_and_what_isnt_a_picture_is_refused() {
        let clear = RgbaImage::from_pixel(20, 20, Rgba([10, 20, 30, 0]));
        let sent = fitted(&picture(&DynamicImage::ImageRgba8(clear)), true).expect("a picture");
        assert_eq!(sent["type"], "image/png");

        let broken = Picture {
            name: "notes.png".to_owned(),
            mime: "image/png".to_owned(),
            data: Arc::from(b"not a picture".to_vec()),
            key: 1,
        };
        assert_eq!(
            fitted(&broken, true).expect_err("refused"),
            "Harlo couldn't read that picture."
        );
    }
}
