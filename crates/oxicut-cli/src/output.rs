//! Writing cropped images.

use std::path::Path;

use anyhow::{Context, Result};
use clap::ValueEnum;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat};

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Keep the scan's own format.
    Same,
    Jpg,
    Png,
    Tiff,
}

impl Format {
    /// Replace `Same` by the format of `source`, falling back to PNG.
    pub fn resolve(self, source: &Path) -> Format {
        if self != Format::Same {
            return self;
        }
        match source
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("jpg" | "jpeg") => Format::Jpg,
            Some("tif" | "tiff") => Format::Tiff,
            _ => Format::Png,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Same | Format::Png => "png",
            Format::Jpg => "jpg",
            Format::Tiff => "tif",
        }
    }
}

/// Save `image` in `format`. JPEG is 8-bit only, so deeper images are reduced;
/// PNG and TIFF keep their bit depth.
pub fn save(image: &DynamicImage, path: &Path, format: Format, quality: u8) -> Result<()> {
    match format {
        Format::Jpg => {
            let file = std::fs::File::create(path)
                .with_context(|| format!("creating {}", path.display()))?;
            let mut writer = std::io::BufWriter::new(file);
            let encoder = JpegEncoder::new_with_quality(&mut writer, quality);
            let rgb8 = DynamicImage::ImageRgb8(image.to_rgb8());
            rgb8.write_with_encoder(encoder)
                .with_context(|| format!("encoding {}", path.display()))?;
        }
        Format::Png => image
            .save_with_format(path, ImageFormat::Png)
            .with_context(|| format!("writing {}", path.display()))?,
        Format::Tiff => image
            .save_with_format(path, ImageFormat::Tiff)
            .with_context(|| format!("writing {}", path.display()))?,
        Format::Same => unreachable!("resolved before saving"),
    }
    Ok(())
}
