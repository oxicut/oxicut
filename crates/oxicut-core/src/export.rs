//! Writing cropped prints to disk: formats, file names, and what to do when a
//! name is already taken. Shared by the command line and the desktop app.

use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat};

use crate::crop::crop_region;
use crate::geometry::RotatedRect;

/// Output file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Keep the scan's own format (falling back to PNG for anything unusual).
    Same,
    Jpg,
    Png,
    Tiff,
}

impl OutputFormat {
    /// Replace `Same` by the format of `source`.
    pub fn resolve(self, source: &Path) -> OutputFormat {
        if self != OutputFormat::Same {
            return self;
        }
        match source
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("jpg" | "jpeg") => OutputFormat::Jpg,
            Some("tif" | "tiff") => OutputFormat::Tiff,
            _ => OutputFormat::Png,
        }
    }

    /// File extension (without the dot). `Same` is only meaningful after [`resolve`](Self::resolve).
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Same | OutputFormat::Png => "png",
            OutputFormat::Jpg => "jpg",
            OutputFormat::Tiff => "tif",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("could not write {}: {message}", path.display())]
    Write { path: PathBuf, message: String },
    #[error("refusing to overwrite the source scan {}", .0.display())]
    WouldOverwriteSource(PathBuf),
}

/// Save `image` in `format`. JPEG is 8-bit only, so deeper images are reduced;
/// PNG and TIFF keep their bit depth. `format` must not be `Same`.
pub fn save_image(
    image: &DynamicImage,
    path: &Path,
    format: OutputFormat,
    jpeg_quality: u8,
) -> Result<(), ExportError> {
    let fail = |message: String| ExportError::Write {
        path: path.to_path_buf(),
        message,
    };
    match format {
        OutputFormat::Jpg => {
            let file = std::fs::File::create(path).map_err(|e| fail(e.to_string()))?;
            let mut writer = std::io::BufWriter::new(file);
            let encoder = JpegEncoder::new_with_quality(&mut writer, jpeg_quality.clamp(1, 100));
            DynamicImage::ImageRgb8(image.to_rgb8())
                .write_with_encoder(encoder)
                .map_err(|e| fail(e.to_string()))
        }
        OutputFormat::Png => image
            .save_with_format(path, ImageFormat::Png)
            .map_err(|e| fail(e.to_string())),
        OutputFormat::Tiff => image
            .save_with_format(path, ImageFormat::Tiff)
            .map_err(|e| fail(e.to_string())),
        OutputFormat::Same => unreachable!("resolve the format before saving"),
    }
}

/// What to do when the target file already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collision {
    /// Replace it (re-running a command line export).
    Overwrite,
    /// Keep it and write `name (2)`, `name (3)`, ...
    Rename,
}

#[derive(Debug, Clone)]
pub struct ExportSettings {
    pub format: OutputFormat,
    pub jpeg_quality: u8,
    /// File name without extension. `{source}` is the scan's name and `{n}` the
    /// print's number (1, 2, ...). Without `{n}`, `-{n}` is appended so prints
    /// from one scan cannot collide.
    pub pattern: String,
    pub collision: Collision,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            format: OutputFormat::Same,
            jpeg_quality: 92,
            pattern: "{source}-{n}".into(),
            collision: Collision::Rename,
        }
    }
}

/// Characters Windows forbids in file names (also awkward elsewhere).
const FORBIDDEN: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A string that is safe to use as one file name component.
pub fn sanitize_file_name(name: &str) -> String {
    let mut cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || FORBIDDEN.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    // Windows silently drops trailing dots and spaces.
    cleaned = cleaned
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .to_string();
    if cleaned.chars().count() > 150 {
        cleaned = cleaned.chars().take(150).collect();
    }
    let stem = cleaned.split('.').next().unwrap_or("").to_ascii_uppercase();
    if cleaned.is_empty() || RESERVED.contains(&stem.as_str()) {
        cleaned = format!("_{cleaned}");
    }
    if cleaned == "_" {
        "scan".into()
    } else {
        cleaned
    }
}

/// Expand `pattern` for print number `n` of the scan named `source_stem`.
pub fn expand_pattern(pattern: &str, source_stem: &str, n: usize) -> String {
    let mut pattern = pattern.trim().to_string();
    if pattern.is_empty() {
        pattern = "{source}-{n}".into();
    }
    if !pattern.contains("{n}") {
        pattern.push_str("-{n}");
    }
    sanitize_file_name(
        &pattern
            .replace("{source}", source_stem)
            .replace("{n}", &n.to_string()),
    )
}

/// `dir/name.ext`, or with ` (2)`, ` (3)`... when the file exists and `collision` is `Rename`.
pub fn target_path(dir: &Path, name: &str, extension: &str, collision: Collision) -> PathBuf {
    let first = dir.join(format!("{name}.{extension}"));
    if collision == Collision::Overwrite || !first.exists() {
        return first;
    }
    (2..)
        .map(|i| dir.join(format!("{name} ({i}).{extension}")))
        .find(|p| !p.exists())
        .expect("an unused name exists")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedFile {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}

/// Crop every region out of `image` (the full-resolution scan read from
/// `source_path`) and write them to `out_dir`. Regions that fall outside the
/// scan are skipped. The source scan is never touched.
pub fn export_regions(
    image: &DynamicImage,
    source_path: &Path,
    regions: &[RotatedRect],
    out_dir: &Path,
    settings: &ExportSettings,
) -> Result<Vec<ExportedFile>, ExportError> {
    std::fs::create_dir_all(out_dir).map_err(|e| ExportError::Write {
        path: out_dir.to_path_buf(),
        message: e.to_string(),
    })?;
    let stem = source_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "scan".into());
    let format = settings.format.resolve(source_path);
    let source_canonical = source_path.canonicalize().ok();

    let mut written = Vec::new();
    for (index, region) in regions.iter().enumerate() {
        let Some(crop) = crop_region(image, region) else {
            continue;
        };
        let name = expand_pattern(&settings.pattern, &stem, index + 1);
        let path = target_path(out_dir, &name, format.extension(), settings.collision);
        if let (Some(a), Ok(b)) = (&source_canonical, path.canonicalize())
            && *a == b
        {
            return Err(ExportError::WouldOverwriteSource(source_path.to_path_buf()));
        }
        save_image(&crop, &path, format, settings.jpeg_quality)?;
        written.push(ExportedFile {
            path,
            width: crop.width(),
            height: crop.height(),
        });
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb, RgbImage};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oxicut-export-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn same_follows_the_source_extension() {
        assert_eq!(
            OutputFormat::Same.resolve(Path::new("a/b/scan.JPEG")),
            OutputFormat::Jpg
        );
        assert_eq!(
            OutputFormat::Same.resolve(Path::new("scan.tif")),
            OutputFormat::Tiff
        );
        assert_eq!(
            OutputFormat::Same.resolve(Path::new("scan.bmp")),
            OutputFormat::Png
        );
        assert_eq!(
            OutputFormat::Png.resolve(Path::new("scan.jpg")),
            OutputFormat::Png
        );
        assert_eq!(OutputFormat::Tiff.extension(), "tif");
    }

    #[test]
    fn file_names_are_made_safe_for_windows() {
        assert_eq!(
            sanitize_file_name("a<b>c:d\"e/f\\g|h?i*j"),
            "a_b_c_d_e_f_g_h_i_j"
        );
        assert_eq!(sanitize_file_name("name. "), "name");
        assert_eq!(sanitize_file_name("CON"), "_CON");
        assert_eq!(sanitize_file_name("nul.txt"), "_nul.txt");
        assert_eq!(sanitize_file_name(""), "scan");
        assert_eq!(sanitize_file_name("..."), "scan");
        assert_eq!(sanitize_file_name(&"x".repeat(400)).chars().count(), 150);
    }

    #[test]
    fn patterns_expand_and_always_get_a_counter() {
        assert_eq!(expand_pattern("{source}-{n}", "holiday", 3), "holiday-3");
        assert_eq!(
            expand_pattern("print_{n}_of_{source}", "a b", 1),
            "print_1_of_a b"
        );
        assert_eq!(expand_pattern("{source}", "holiday", 2), "holiday-2");
        assert_eq!(expand_pattern("   ", "holiday", 1), "holiday-1");
        // Path separators cannot smuggle a file out of the output folder.
        assert_eq!(expand_pattern("../{source}-{n}", "x", 1), "_x-1");
    }

    #[test]
    fn taken_names_get_a_number_unless_overwriting() {
        let dir = scratch("names");
        std::fs::write(dir.join("a-1.png"), b"x").unwrap();
        assert_eq!(
            target_path(&dir, "a-1", "png", Collision::Rename),
            dir.join("a-1 (2).png")
        );
        std::fs::write(dir.join("a-1 (2).png"), b"x").unwrap();
        assert_eq!(
            target_path(&dir, "a-1", "png", Collision::Rename),
            dir.join("a-1 (3).png")
        );
        assert_eq!(
            target_path(&dir, "a-1", "png", Collision::Overwrite),
            dir.join("a-1.png")
        );
        assert_eq!(
            target_path(&dir, "fresh", "png", Collision::Rename),
            dir.join("fresh.png")
        );
    }

    fn scan() -> DynamicImage {
        let mut image = RgbImage::from_pixel(300, 200, Rgb([255, 255, 255]));
        for y in 50..150 {
            for x in 40..140 {
                image.put_pixel(x, y, Rgb([200, 30, 30]));
            }
            for x in 170..270 {
                image.put_pixel(x, y, Rgb([30, 30, 200]));
            }
        }
        DynamicImage::ImageRgb8(image)
    }

    fn regions() -> Vec<RotatedRect> {
        vec![
            RotatedRect::new(90.0, 100.0, 100.0, 100.0, 0.0),
            RotatedRect::new(220.0, 100.0, 100.0, 100.0, 0.0),
        ]
    }

    #[test]
    fn exports_every_region_and_never_overwrites_by_default() {
        let dir = scratch("export");
        let source = dir.join("my scan.png");
        let settings = ExportSettings {
            format: OutputFormat::Png,
            ..Default::default()
        };
        let first =
            export_regions(&scan(), &source, &regions(), &dir.join("out"), &settings).unwrap();
        assert_eq!(
            first
                .iter()
                .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["my scan-1.png", "my scan-2.png"]
        );
        assert!(first.iter().all(|f| (f.width, f.height) == (100, 100)));

        // Again: the earlier files stay and new ones get a number.
        let second =
            export_regions(&scan(), &source, &regions(), &dir.join("out"), &settings).unwrap();
        assert!(second[0].path.ends_with("my scan-1 (2).png"));
        assert_eq!(std::fs::read_dir(dir.join("out")).unwrap().count(), 4);
    }

    #[test]
    fn overwrite_mode_replaces_files() {
        let dir = scratch("overwrite");
        let source = dir.join("s.png");
        let settings = ExportSettings {
            format: OutputFormat::Png,
            collision: Collision::Overwrite,
            ..Default::default()
        };
        export_regions(&scan(), &source, &regions(), &dir, &settings).unwrap();
        export_regions(&scan(), &source, &regions(), &dir, &settings).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    }

    #[test]
    fn refuses_to_replace_the_source_scan() {
        let dir = scratch("source");
        let source = dir.join("scan-1.png");
        scan().save(&source).unwrap();
        // A pattern that reproduces the scan's own file name.
        let settings = ExportSettings {
            format: OutputFormat::Png,
            pattern: "scan-{n}".into(),
            collision: Collision::Overwrite,
            ..Default::default()
        };
        let error = export_regions(&scan(), &source, &regions(), &dir, &settings).unwrap_err();
        assert!(
            matches!(error, ExportError::WouldOverwriteSource(_)),
            "{error}"
        );
    }

    #[test]
    fn jpeg_and_tiff_outputs_are_readable_and_tiff_keeps_sixteen_bits() {
        let dir = scratch("formats");
        let source = dir.join("s.png");
        let wide: ImageBuffer<Rgb<u16>, Vec<u16>> = ImageBuffer::from_fn(300, 200, |x, y| {
            if (40..140).contains(&x) && (50..150).contains(&y) {
                Rgb([1000, 30000, 65000])
            } else {
                Rgb([65535; 3])
            }
        });
        let image = DynamicImage::ImageRgb16(wide);
        let one = vec![RotatedRect::new(90.0, 100.0, 100.0, 100.0, 0.0)];

        let jpg = ExportSettings {
            format: OutputFormat::Jpg,
            jpeg_quality: 80,
            ..Default::default()
        };
        let written = export_regions(&image, &source, &one, &dir, &jpg).unwrap();
        assert!(image::open(&written[0].path).is_ok());

        let tiff = ExportSettings {
            format: OutputFormat::Tiff,
            ..Default::default()
        };
        let written = export_regions(&image, &source, &one, &dir, &tiff).unwrap();
        let reopened = image::open(&written[0].path).unwrap();
        assert!(
            matches!(reopened, DynamicImage::ImageRgb16(_)),
            "lost bit depth: {:?}",
            reopened.color()
        );
    }

    #[test]
    fn regions_outside_the_scan_are_skipped_not_fatal() {
        let dir = scratch("outside");
        let source = dir.join("s.png");
        let settings = ExportSettings {
            format: OutputFormat::Png,
            ..Default::default()
        };
        let mixed = vec![
            RotatedRect::new(9000.0, 9000.0, 100.0, 100.0, 0.0),
            RotatedRect::new(90.0, 100.0, 100.0, 100.0, 0.0),
        ];
        let written = export_regions(&scan(), &source, &mixed, &dir, &settings).unwrap();
        assert_eq!(written.len(), 1);
        // The surviving print keeps its own number, so names line up with the on-screen boxes.
        assert!(written[0].path.ends_with("s-2.png"));
    }
}
