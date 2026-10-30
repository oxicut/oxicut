//! Writing cropped prints to disk: formats, file names, and what to do when a
//! name is already taken. Shared by the command line and the desktop app.

use std::path::{Path, PathBuf};

use image::codecs::jpeg::{JpegEncoder, PixelDensity, PixelDensityUnit};
use image::{DynamicImage, ImageFormat};
use img_parts::{ImageEXIF, ImageICC};

use crate::crop::crop_region;
use crate::geometry::RotatedRect;
use crate::metadata::Metadata;

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
///
/// `metadata` (the scan's resolution, colour profile and EXIF; empty when the
/// user does not want them kept) is written into the file. Every file also
/// names OxiCut in its `Software` field.
pub fn save_image(
    image: &DynamicImage,
    path: &Path,
    format: OutputFormat,
    jpeg_quality: u8,
    metadata: &Metadata,
) -> Result<(), ExportError> {
    let fail = |message: String| ExportError::Write {
        path: path.to_path_buf(),
        message,
    };
    match format {
        OutputFormat::Jpg => {
            let mut bytes = Vec::new();
            let mut encoder = JpegEncoder::new_with_quality(&mut bytes, jpeg_quality.clamp(1, 100));
            if let Some(dpi) = metadata.dpi {
                let density = |v: f32| v.round().clamp(1.0, 65535.0) as u16;
                encoder.set_pixel_density(PixelDensity {
                    density: (density(dpi.x), density(dpi.y)),
                    unit: PixelDensityUnit::Inches,
                });
            }
            DynamicImage::ImageRgb8(image.to_rgb8())
                .write_with_encoder(encoder)
                .map_err(|e| fail(e.to_string()))?;
            std::fs::write(path, with_jpeg_metadata(bytes, metadata)).map_err(|e| fail(e.to_string()))
        }
        OutputFormat::Png => {
            let mut bytes = Vec::new();
            image
                .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
                .map_err(|e| fail(e.to_string()))?;
            std::fs::write(path, with_png_metadata(bytes, metadata)).map_err(|e| fail(e.to_string()))
        }
        OutputFormat::Tiff => write_tiff(image, path, metadata).map_err(fail),
        OutputFormat::Same => unreachable!("resolve the format before saving"),
    }
}

/// Add the colour profile and EXIF to an encoded JPEG. If it cannot be
/// parsed (it was just written by the encoder, so it should) it is left as is.
fn with_jpeg_metadata(bytes: Vec<u8>, metadata: &Metadata) -> Vec<u8> {
    let Ok(mut jpeg) = img_parts::jpeg::Jpeg::from_bytes(bytes.clone().into()) else {
        return bytes;
    };
    jpeg.set_icc_profile(metadata.icc.clone().map(Into::into));
    jpeg.set_exif(metadata.crop_exif().map(Into::into));
    jpeg.encoder().bytes().to_vec()
}

/// Add resolution (`pHYs`), colour profile, EXIF and a `Software` text chunk to an encoded PNG.
fn with_png_metadata(bytes: Vec<u8>, metadata: &Metadata) -> Vec<u8> {
    use img_parts::png::{Png, PngChunk};
    let Ok(mut png) = Png::from_bytes(bytes.clone().into()) else {
        return bytes;
    };
    png.set_icc_profile(metadata.icc.clone().map(Into::into));
    png.set_exif(metadata.crop_exif().map(Into::into));
    // Ancillary chunks go right after the header, before the pixel data.
    if let Some(software) = metadata.software_tag() {
        let mut text = b"Software\0".to_vec();
        text.extend_from_slice(software.as_bytes());
        png.chunks_mut().insert(1, PngChunk::new(*b"tEXt", text.into()));
    }
    if let Some(dpi) = metadata.dpi {
        let per_metre = |v: f32| ((v as f64 / 0.0254).round() as u32).to_be_bytes();
        let mut phys = Vec::with_capacity(9);
        phys.extend_from_slice(&per_metre(dpi.x));
        phys.extend_from_slice(&per_metre(dpi.y));
        phys.push(1); // unit: metre
        png.chunks_mut().insert(1, PngChunk::new(*b"pHYs", phys.into()));
    }
    png.encoder().bytes().to_vec()
}

/// TIFF through the `tiff` crate directly, which `image` does not let us add tags to.
fn write_tiff(image: &DynamicImage, path: &Path, metadata: &Metadata) -> Result<(), String> {
    use tiff::encoder::{TiffEncoder, colortype};
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut writer = std::io::BufWriter::new(file);
    let mut encoder = TiffEncoder::new(&mut writer).map_err(|e| e.to_string())?;
    let result = match image {
        DynamicImage::ImageLuma8(b) => {
            tiff_frame::<colortype::Gray8, _>(&mut encoder, b.width(), b.height(), b, metadata)
        }
        DynamicImage::ImageLuma16(b) => {
            tiff_frame::<colortype::Gray16, _>(&mut encoder, b.width(), b.height(), b, metadata)
        }
        DynamicImage::ImageRgb8(b) => {
            tiff_frame::<colortype::RGB8, _>(&mut encoder, b.width(), b.height(), b, metadata)
        }
        DynamicImage::ImageRgb16(b) => {
            tiff_frame::<colortype::RGB16, _>(&mut encoder, b.width(), b.height(), b, metadata)
        }
        DynamicImage::ImageRgba8(b) => {
            tiff_frame::<colortype::RGBA8, _>(&mut encoder, b.width(), b.height(), b, metadata)
        }
        DynamicImage::ImageRgba16(b) => {
            tiff_frame::<colortype::RGBA16, _>(&mut encoder, b.width(), b.height(), b, metadata)
        }
        // Grey with alpha and floating point: widen to the nearest layout TIFF viewers all read.
        DynamicImage::ImageLumaA16(_) | DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_) => {
            let b = image.to_rgba16();
            tiff_frame::<colortype::RGBA16, _>(&mut encoder, b.width(), b.height(), &b, metadata)
        }
        _ => {
            let b = image.to_rgba8();
            tiff_frame::<colortype::RGBA8, _>(&mut encoder, b.width(), b.height(), &b, metadata)
        }
    };
    result.map_err(|e| e.to_string())
}

fn tiff_frame<C, W>(
    encoder: &mut tiff::encoder::TiffEncoder<W>,
    width: u32,
    height: u32,
    pixels: &[C::Inner],
    metadata: &Metadata,
) -> tiff::TiffResult<()>
where
    C: tiff::encoder::colortype::ColorType,
    W: std::io::Write + std::io::Seek,
    [C::Inner]: tiff::encoder::TiffValue,
{
    use tiff::encoder::Rational;
    use tiff::tags::{ResolutionUnit, Tag};
    let mut frame = encoder.new_image::<C>(width, height)?;
    if let Some(dpi) = metadata.dpi {
        let ratio = |v: f32| Rational {
            n: (v * 100.0).round() as u32,
            d: 100,
        };
        frame.x_resolution(ratio(dpi.x));
        frame.y_resolution(ratio(dpi.y));
        frame.resolution_unit(ResolutionUnit::Inch);
    }
    if let Some(software) = metadata.software_tag() {
        frame.encoder().write_tag(Tag::Software, software.as_str())?;
    }
    if let Some(icc) = &metadata.icc {
        frame.encoder().write_tag(Tag::IccProfile, &icc[..])?;
    }
    frame.write_data(pixels)
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
    /// Copy the scan's resolution, colour profile and EXIF into the crops.
    /// Off, the crops still say OxiCut made them.
    pub keep_metadata: bool,
    /// Write OxiCut into each crop's `Software` field. Independent of `keep_metadata`.
    pub tag_software: bool,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            format: OutputFormat::Same,
            jpeg_quality: 92,
            pattern: "{source}-{n}".into(),
            collision: Collision::Rename,
            keep_metadata: true,
            tag_software: true,
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

/// What stays the same for every crop of one scan: where it is written and how. Built once, so the
/// scan's metadata is read once, and used by the real export and by the single "real cut" preview alike.
pub struct ScanExport<'a> {
    source_path: &'a Path,
    out_dir: &'a Path,
    settings: &'a ExportSettings,
    stem: String,
    format: OutputFormat,
    source_canonical: Option<PathBuf>,
    metadata: Metadata,
}

/// A crop that was written, with the picture that went into the file.
pub struct WrittenCrop {
    pub file: ExportedFile,
    pub crop: DynamicImage,
}

impl<'a> ScanExport<'a> {
    /// Prepare to write crops of `source_path` into `out_dir` (made if missing).
    pub fn new(
        source_path: &'a Path,
        out_dir: &'a Path,
        settings: &'a ExportSettings,
    ) -> Result<Self, ExportError> {
        std::fs::create_dir_all(out_dir).map_err(|e| ExportError::Write {
            path: out_dir.to_path_buf(),
            message: e.to_string(),
        })?;
        let stem = source_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "scan".into());
        let metadata = Metadata {
            tag_software: settings.tag_software,
            ..if settings.keep_metadata {
                Metadata::read(source_path)
            } else {
                Metadata::default()
            }
        };
        Ok(Self {
            source_path,
            out_dir,
            settings,
            stem,
            format: settings.format.resolve(source_path),
            source_canonical: source_path.canonicalize().ok(),
            metadata,
        })
    }

    /// Cut `region` out of `image` and write it as print number `number` (counting from 1, which is
    /// what the file name gets). `None` when the region lies outside the scan.
    pub fn write(
        &self,
        image: &DynamicImage,
        region: &RotatedRect,
        number: usize,
    ) -> Result<Option<WrittenCrop>, ExportError> {
        self.write_named(image, region, number, None)
    }

    /// Like `write`, but with a file name (without extension) chosen by the caller, for example the
    /// `-front` and `-back` names of a pair. It is cleaned up like any name, and still never replaces
    /// a file that exists (unless the settings say so). `None` uses the naming pattern.
    pub fn write_named(
        &self,
        image: &DynamicImage,
        region: &RotatedRect,
        number: usize,
        name: Option<&str>,
    ) -> Result<Option<WrittenCrop>, ExportError> {
        let Some(crop) = crop_region(image, region) else {
            return Ok(None);
        };
        let name = match name {
            Some(chosen) => sanitize_file_name(chosen),
            None => expand_pattern(&self.settings.pattern, &self.stem, number),
        };
        let path = target_path(
            self.out_dir,
            &name,
            self.format.extension(),
            self.settings.collision,
        );
        if let (Some(a), Ok(b)) = (&self.source_canonical, path.canonicalize())
            && *a == b
        {
            return Err(ExportError::WouldOverwriteSource(self.source_path.to_path_buf()));
        }
        save_image(&crop, &path, self.format, self.settings.jpeg_quality, &self.metadata)?;
        let file = ExportedFile {
            path,
            width: crop.width(),
            height: crop.height(),
        };
        Ok(Some(WrittenCrop { file, crop }))
    }
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
    let export = ScanExport::new(source_path, out_dir, settings)?;
    let mut written = Vec::new();
    for (index, region) in regions.iter().enumerate() {
        if let Some(done) = export.write(image, region, index + 1)? {
            written.push(done.file);
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, ImageBuffer, Rgb, RgbImage};

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
    fn a_chosen_name_replaces_the_pattern_and_is_never_overwritten() {
        let dir = scratch("chosen-name");
        let source = dir.join("s.png");
        let settings = ExportSettings {
            format: OutputFormat::Png,
            ..Default::default()
        };
        let out = dir.join("out");
        let export = ScanExport::new(&source, &out, &settings).unwrap();
        let region = &regions()[0];
        let first = export
            .write_named(&scan(), region, 1, Some("card-1-front"))
            .unwrap()
            .unwrap();
        assert!(first.file.path.ends_with("card-1-front.png"));
        // A name that tries to leave the folder is cleaned like any other.
        let odd = export
            .write_named(&scan(), region, 1, Some("../card"))
            .unwrap()
            .unwrap();
        assert!(odd.file.path.starts_with(&out));
        // The same name again gets a number instead of replacing the file.
        let again = export
            .write_named(&scan(), region, 1, Some("card-1-front"))
            .unwrap()
            .unwrap();
        assert!(again.file.path.ends_with("card-1-front (2).png"));
        // No name: the pattern decides, as before.
        let plain = export.write(&scan(), region, 1).unwrap().unwrap();
        assert!(plain.file.path.ends_with("s-1.png"));
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

    fn rich_metadata() -> Metadata {
        use exif::experimental::Writer;
        use exif::{Field, In, Tag, Value};
        let fields = [
            Field {
                tag: Tag::DateTimeOriginal,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![b"2024:05:01 10:00:00".to_vec()]),
            },
            Field {
                tag: Tag::Make,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![b"EPSON".to_vec()]),
            },
        ];
        let mut writer = Writer::new();
        for f in &fields {
            writer.push_field(f);
        }
        let mut exif = std::io::Cursor::new(Vec::new());
        writer.write(&mut exif, true).unwrap();
        Metadata {
            dpi: Some(crate::metadata::Dpi { x: 600.0, y: 600.0 }),
            icc: Some((0..300u32).map(|i| (i % 251) as u8).collect()),
            exif: Some(exif.into_inner()),
            tag_software: true,
        }
    }

    fn exif_text(raw: &[u8], tag: exif::Tag) -> Option<String> {
        let exif = exif::Reader::new().read_raw(raw.to_vec()).ok()?;
        match &exif.get_field(tag, exif::In::PRIMARY)?.value {
            exif::Value::Ascii(v) => String::from_utf8(v.first()?.clone()).ok(),
            _ => None,
        }
    }

    #[test]
    fn jpeg_and_png_keep_resolution_profile_and_exif_and_name_oxicut() {
        let dir = scratch("meta-jpg-png");
        let meta = rich_metadata();
        let crop = DynamicImage::ImageRgb8(RgbImage::from_pixel(40, 30, Rgb([90, 120, 150])));
        for (format, name) in [(OutputFormat::Jpg, "c.jpg"), (OutputFormat::Png, "c.png")] {
            let path = dir.join(name);
            save_image(&crop, &path, format, 90, &meta).unwrap();

            let back = Metadata::read(&path);
            let dpi = back.dpi.unwrap_or_else(|| panic!("{name}: no resolution"));
            assert!((dpi.x - 600.0).abs() < 0.5 && (dpi.y - 600.0).abs() < 0.5, "{name}: {dpi:?}");
            assert_eq!(back.icc, meta.icc, "{name}: colour profile");
            let exif = back.exif.unwrap_or_else(|| panic!("{name}: no EXIF"));
            assert_eq!(exif_text(&exif, exif::Tag::Make).as_deref(), Some("EPSON"));
            assert_eq!(
                exif_text(&exif, exif::Tag::DateTimeOriginal).as_deref(),
                Some("2024:05:01 10:00:00")
            );
            assert_eq!(
                exif_text(&exif, exif::Tag::Software).as_deref(),
                Some(crate::metadata::software().as_str())
            );
            // Still a picture the usual decoder reads.
            assert_eq!(image::open(&path).unwrap().dimensions(), (40, 30));
        }
    }

    #[test]
    fn tiff_keeps_resolution_and_profile_and_stays_sixteen_bit() {
        let dir = scratch("meta-tiff");
        let path = dir.join("c.tif");
        let meta = rich_metadata();
        let crop: ImageBuffer<Rgb<u16>, Vec<u16>> =
            ImageBuffer::from_fn(8, 6, |x, y| Rgb([x as u16 * 8000, y as u16 * 9000, 65535]));
        save_image(&DynamicImage::ImageRgb16(crop.clone()), &path, OutputFormat::Tiff, 90, &meta).unwrap();

        let back = Metadata::read(&path);
        let dpi = back.dpi.expect("no resolution");
        assert!((dpi.x - 600.0).abs() < 0.5, "{dpi:?}");
        assert_eq!(back.icc, meta.icc);
        let reopened = image::open(&path).unwrap();
        assert_eq!(reopened.dimensions(), (8, 6));
        assert_eq!(reopened.to_rgb16(), crop, "pixels changed");
    }

    #[test]
    fn tiff_names_oxicut_in_the_software_tag() {
        use tiff::decoder::{Decoder, ifd::Value};
        let dir = scratch("meta-tiff-software");
        let path = dir.join("c.tif");
        let crop = DynamicImage::ImageRgb8(RgbImage::from_pixel(4, 4, Rgb([1, 2, 3])));
        save_image(&crop, &path, OutputFormat::Tiff, 90, &stamp_only()).unwrap();
        let mut decoder = Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap())).unwrap();
        let Value::Ascii(text) = decoder.get_tag(tiff::tags::Tag::Software).unwrap() else {
            panic!("Software is not text")
        };
        assert_eq!(text, crate::metadata::software());
    }

    /// No scan metadata, only the OxiCut stamp.
    fn stamp_only() -> Metadata {
        Metadata {
            tag_software: true,
            ..Default::default()
        }
    }

    #[test]
    fn the_software_stamp_works_without_the_scans_metadata() {
        let dir = scratch("meta-stamp-only");
        let crop = DynamicImage::ImageRgb8(RgbImage::from_pixel(20, 20, Rgb([9, 9, 9])));
        for (format, name) in [(OutputFormat::Jpg, "c.jpg"), (OutputFormat::Png, "c.png")] {
            let path = dir.join(name);
            save_image(&crop, &path, format, 90, &stamp_only()).unwrap();
            let back = Metadata::read(&path);
            assert!(back.icc.is_none() && back.dpi.is_none());
            let exif = back.exif.expect("Software needs an EXIF block");
            assert_eq!(
                exif_text(&exif, exif::Tag::Software).as_deref(),
                Some(crate::metadata::software().as_str())
            );
            assert!(exif_text(&exif, exif::Tag::Make).is_none());
        }
    }

    #[test]
    fn without_the_stamp_the_scanners_own_software_stays_and_nothing_is_invented() {
        let dir = scratch("meta-no-stamp");
        let crop = DynamicImage::ImageRgb8(RgbImage::from_pixel(20, 20, Rgb([9, 9, 9])));

        // Scan metadata kept, stamp off: the scan's Software value survives.
        let mut kept = rich_metadata();
        kept.tag_software = false;
        kept.exif = Some({
            use exif::{Field, In, Tag, Value};
            let software = Field {
                tag: Tag::Software,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![b"EPSON Scan".to_vec()]),
            };
            let mut writer = exif::experimental::Writer::new();
            writer.push_field(&software);
            let mut out = std::io::Cursor::new(Vec::new());
            writer.write(&mut out, true).unwrap();
            out.into_inner()
        });
        let path = dir.join("kept.jpg");
        save_image(&crop, &path, OutputFormat::Jpg, 90, &kept).unwrap();
        let exif = Metadata::read(&path).exif.unwrap();
        assert_eq!(exif_text(&exif, exif::Tag::Software).as_deref(), Some("EPSON Scan"));

        // Everything off: no EXIF block, no resolution, no text chunk, no TIFF Software tag.
        let nothing = Metadata::default();
        for (format, name) in [
            (OutputFormat::Jpg, "n.jpg"),
            (OutputFormat::Png, "n.png"),
            (OutputFormat::Tiff, "n.tif"),
        ] {
            let path = dir.join(name);
            save_image(&crop, &path, format, 90, &nothing).unwrap();
            assert!(Metadata::read(&path).is_empty(), "{name} has metadata");
        }
        assert!(
            !std::fs::read(dir.join("n.png"))
                .unwrap()
                .windows(8)
                .any(|w| w == b"Software"),
            "PNG still has a Software text chunk"
        );
        let mut tiff = tiff::decoder::Decoder::new(std::io::BufReader::new(
            std::fs::File::open(dir.join("n.tif")).unwrap(),
        ))
        .unwrap();
        assert!(tiff.find_tag(tiff::tags::Tag::Software).unwrap().is_none());
    }

    #[test]
    fn a_whole_export_carries_the_scans_metadata_unless_told_not_to() {
        let dir = scratch("meta-export");
        let source = dir.join("s.jpg");
        let meta = rich_metadata();
        save_image(
            &DynamicImage::ImageRgb8(RgbImage::from_pixel(300, 200, Rgb([200, 200, 200]))),
            &source,
            OutputFormat::Jpg,
            95,
            &meta,
        )
        .unwrap();
        let scan = image::open(&source).unwrap();
        let one = [RotatedRect::new(150.0, 100.0, 100.0, 80.0, 0.0)];

        let kept = ExportSettings {
            format: OutputFormat::Png,
            pattern: "kept".into(),
            ..Default::default()
        };
        let path = export_regions(&scan, &source, &one, &dir, &kept).unwrap().remove(0).path;
        assert!(Metadata::read(&path).dpi.is_some());
        assert_eq!(Metadata::read(&path).icc, meta.icc);

        let dropped = ExportSettings {
            keep_metadata: false,
            pattern: "dropped".into(),
            ..kept
        };
        let path = export_regions(&scan, &source, &one, &dir, &dropped).unwrap().remove(0).path;
        let back = Metadata::read(&path);
        assert!(back.dpi.is_none() && back.icc.is_none());
        // The stamp is a separate choice: still there with the scan's metadata left out.
        assert!(back.exif.is_some());

        let bare = ExportSettings {
            tag_software: false,
            pattern: "bare".into(),
            ..dropped
        };
        let path = export_regions(&scan, &source, &one, &dir, &bare).unwrap().remove(0).path;
        assert!(Metadata::read(&path).is_empty());
    }
}
