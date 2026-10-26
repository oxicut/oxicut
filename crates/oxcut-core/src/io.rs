//! Loading scans from disk.
//!
//! The decoded image keeps its native bit depth (8-bit JPEG, 8/16-bit PNG and
//! TIFF) so crops can be written back without loss. Detection works on an 8-bit
//! RGB copy, see [`to_detection_rgb`].

use std::path::Path;

use image::{DynamicImage, ImageReader, RgbImage};

pub fn load_image(path: &Path) -> image::ImageResult<DynamicImage> {
    ImageReader::open(path)?.with_guessed_format()?.decode()
}

/// 8-bit RGB view for detection, matching ScanSplitter's `image.convert("RGB")`.
/// Alpha is dropped. 16-bit channels keep their high byte (`v >> 8`) like PIL;
/// the `image` crate would round instead (1000 -> 4, PIL -> 3).
pub fn to_detection_rgb(image: &DynamicImage) -> RgbImage {
    match image {
        DynamicImage::ImageRgb16(_)
        | DynamicImage::ImageRgba16(_)
        | DynamicImage::ImageLuma16(_)
        | DynamicImage::ImageLumaA16(_) => {
            let wide = image.to_rgb16();
            let (width, height) = wide.dimensions();
            let bytes = wide.as_raw().iter().map(|v| (v >> 8) as u8).collect();
            RgbImage::from_raw(width, height, bytes).expect("same dimensions")
        }
        _ => image.to_rgb8(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("oxcut-io-{}-{name}", std::process::id()))
    }

    #[test]
    fn sixteen_bit_tiff_round_trips_at_sixteen_bits() {
        let path = temp_path("16bit.tiff");
        let source: ImageBuffer<Rgb<u16>, Vec<u16>> =
            ImageBuffer::from_fn(4, 3, |x, y| Rgb([x as u16 * 1000, y as u16 * 2000, 65535]));
        source.save(&path).unwrap();

        let loaded = load_image(&path).unwrap();
        std::fs::remove_file(&path).ok();

        let DynamicImage::ImageRgb16(loaded) = loaded else {
            panic!("expected 16-bit RGB, got {:?}", loaded.color());
        };
        assert_eq!(loaded, source);
    }

    #[test]
    fn detection_rgb_truncates_sixteen_bit_like_pil() {
        // Values taken from PIL: [0, 257, 32896] -> [0, 1, 128], [65535, 128, 1000] -> [255, 0, 3].
        let mut buffer = ImageBuffer::new(2, 1);
        buffer.put_pixel(0, 0, Rgb([0u16, 257, 32896]));
        buffer.put_pixel(1, 0, Rgb([65535, 128, 1000]));
        let rgb = to_detection_rgb(&DynamicImage::ImageRgb16(buffer));
        assert_eq!(rgb.get_pixel(0, 0), &Rgb([0, 1, 128]));
        assert_eq!(rgb.get_pixel(1, 0), &Rgb([255, 0, 3]));
    }

    #[test]
    fn format_is_detected_from_content_not_extension() {
        let path = temp_path("really-a-png.jpg");
        let source = RgbImage::from_pixel(2, 2, Rgb([10, 20, 30]));
        source
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        let loaded = load_image(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(loaded.to_rgb8(), source);
    }
}
