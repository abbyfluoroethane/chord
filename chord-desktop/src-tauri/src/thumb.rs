//! Shrink a big image before it goes to the webview.
//!
//! The webview decodes an image at its full pixel size, even if the page shows it in a
//! 32 pixel circle. A 4000 by 3000 photo takes 48 MB of RAM as decoded pixels. This module
//! decodes a PNG or a JPEG once, in Rust, and sends a smaller picture.
//!
//! Only PNG and JPEG are changed. A GIF or a WebP can have many frames, and a re-encode
//! would drop the animation, so those types pass through. An image that is small enough
//! also passes through, with no re-encode. The size check reads only the header.
//! A decode that fails or breaks a limit returns `None`, and the caller keeps the
//! original bytes.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Limits};

/// The JPEG quality of a shrunk photo.
const JPEG_QUALITY: u8 = 85;

/// The most memory that a decode may take. It stops an image that claims a huge size.
const MAX_DECODE_BYTES: u64 = 128 * 1024 * 1024;

/// A smaller copy of an image.
#[derive(Debug)]
pub struct Shrunk {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    limits
}

/// The pixel size of a PNG or a JPEG, from the header only.
fn dimensions(bytes: &[u8], format: ImageFormat) -> Option<(u32, u32)> {
    ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .ok()
}

/// A copy of `bytes` with the long side at most `max_side` pixels. Returns `None` if the
/// original is fine: it is small, it is not a PNG or a JPEG, it does not decode, or the
/// new copy is not smaller in bytes.
pub fn shrink(bytes: &[u8], max_side: u32) -> Option<Shrunk> {
    let format = image::guess_format(bytes).ok()?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg) {
        return None;
    }
    let (width, height) = dimensions(bytes, format)?;
    if width.max(height) <= max_side {
        return None;
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits());
    let mut decoder = reader.into_decoder().ok()?;
    // The webview turns a photo upright with its EXIF tag. A re-encode drops the tag, so
    // the turn happens here.
    let orientation = decoder.orientation().ok()?;
    let mut picture = DynamicImage::from_decoder(decoder).ok()?;
    picture.apply_orientation(orientation);
    let small = picture.thumbnail(max_side, max_side);
    drop(picture);
    let mut out = Vec::new();
    let mime = if format == ImageFormat::Png {
        // Keep the alpha channel of a PNG.
        let rgba = small.to_rgba8();
        PngEncoder::new(&mut out)
            .write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .ok()?;
        "image/png"
    } else {
        let rgb = small.to_rgb8();
        JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
            .write_image(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .ok()?;
        "image/jpeg"
    };
    (out.len() < bytes.len()).then_some(Shrunk { bytes: out, mime })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A noisy picture, so the encoded copy is not tiny.
    fn picture(width: u32, height: u32) -> DynamicImage {
        let mut seed = 1u32;
        DynamicImage::ImageRgba8(image::RgbaImage::from_fn(width, height, |_, _| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let b = seed.to_be_bytes();
            image::Rgba([b[0], b[1], b[2], 255])
        }))
    }

    fn encode(picture: &DynamicImage, format: ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        picture.write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    fn size_of(bytes: &[u8]) -> (u32, u32) {
        let format = image::guess_format(bytes).unwrap();
        dimensions(bytes, format).unwrap()
    }

    #[test]
    fn a_big_png_gets_smaller() {
        let big = encode(&picture(1000, 500), ImageFormat::Png);
        let small = shrink(&big, 256).unwrap();
        assert_eq!(small.mime, "image/png");
        assert_eq!(size_of(&small.bytes), (256, 128));
        // The webview decodes 4 bytes for each pixel: 2 MB before, 131 KB after.
        assert_eq!(1000 * 500 * 4 / (256 * 128 * 4), 15);
        assert!(small.bytes.len() < big.len());
    }

    #[test]
    fn a_big_jpeg_stays_a_jpeg() {
        let rgb = DynamicImage::ImageRgb8(picture(600, 900).to_rgb8());
        let big = encode(&rgb, ImageFormat::Jpeg);
        let small = shrink(&big, 300).unwrap();
        assert_eq!(small.mime, "image/jpeg");
        assert_eq!(size_of(&small.bytes), (200, 300));
    }

    #[test]
    fn a_small_image_passes_through() {
        let small = encode(&picture(64, 64), ImageFormat::Png);
        assert!(shrink(&small, 256).is_none());
        assert!(shrink(&small, 64).is_none());
    }

    #[test]
    fn other_types_and_bad_data_pass_through() {
        let gif = b"GIF89a\x58\x02\x58\x02 more bytes";
        assert!(shrink(gif, 100).is_none());
        assert!(shrink(b"not an image", 100).is_none());
        assert!(shrink(&[], 100).is_none());
        let mut cut = encode(&picture(600, 600), ImageFormat::Png);
        cut.truncate(cut.len() / 2);
        assert!(shrink(&cut, 100).is_none());
    }
}
