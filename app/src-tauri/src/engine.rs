//! What the app does with a scan: find the prints, make a thumbnail and a
//! preview for the screen, and expand dropped files and folders. Plain Rust,
//! no Tauri, so it can be tested directly.

use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Instant;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use image::RgbImage;
use image::codecs::jpeg::JpegEncoder;
use oxicut_core::crop::crop_region;
use oxicut_core::detect::{DetectOptions, Detection, detect_v3};
use oxicut_core::geometry::RotatedRect;
use oxicut_core::io::{load_image, to_detection_rgb};
use oxicut_core::resize::resize_area;
use oxicut_core::v7::{detect_v7, detect_v7_lite};
use serde::{Deserialize, Serialize};

/// File types the app reads.
pub const SCAN_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "tif", "tiff"];
/// Longest side of the thumbnail shown in the queue.
const THUMBNAIL_SIDE: u32 = 220;
/// Longest side of the preview the review canvas shows.
const PREVIEW_SIDE: u32 = 2400;

pub fn is_scan_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SCAN_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// "scan2" before "scan10": compare digit runs as numbers, the rest ignoring case.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let number = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        it.next();
                    }
                    digits.trim_start_matches('0').to_string()
                };
                let (na, nb) = (number(&mut a), number(&mut b));
                let order = na.len().cmp(&nb.len()).then_with(|| na.cmp(&nb));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// Dropped or chosen paths, expanded: scan files are kept, a folder becomes the
/// scans directly inside it (sorted naturally), anything else is ignored.
/// Duplicates are removed, keeping the first.
pub fn expand_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if !out.contains(&p) {
            out.push(p);
        }
    };
    for path in paths {
        if path.is_dir() {
            let Ok(entries) = std::fs::read_dir(path) else {
                continue;
            };
            let mut files: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_file() && is_scan_file(p))
                .collect();
            files.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
            files.into_iter().for_each(&mut push);
        } else if path.is_file() && is_scan_file(path) {
            push(path.clone());
        }
    }
    out
}

/// A box in source pixels: centre, size and the angle of the `w` axis in
/// degrees (clockwise on screen).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoxDto {
    pub cx: f64,
    pub cy: f64,
    pub w: f64,
    pub h: f64,
    pub angle: f64,
}

/// Which detector finds the prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Detector {
    /// ScanSplitter's v3: colour and edges only.
    #[serde(rename = "v3")]
    V3,
    /// v3 plus v7's inner-edge tightening, which keeps boxes off white borders
    /// and drop shadows.
    #[serde(rename = "v7-lite")]
    #[default]
    V7Lite,
    /// Full v7: MobileSAM anchors, several proposal sets, evidence-based arbitration.
    /// Needs the MobileSAM models.
    #[serde(rename = "v7")]
    V7,
}

/// The detection settings that matter to the detector.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DetectSettings {
    pub mode: Detector,
    pub min_area_percent: f64,
    pub max_area_percent: f64,
    pub inset_px: i32,
}

impl Default for DetectSettings {
    fn default() -> Self {
        Self {
            mode: Detector::default(),
            min_area_percent: 2.0,
            max_area_percent: 80.0,
            // ScanSplitter's v3 trims 10 px, but measured against its own
            // ground truth the untrimmed box is the accurate one.
            inset_px: 0,
        }
    }
}

impl DetectSettings {
    fn options(&self) -> DetectOptions {
        DetectOptions {
            min_area_ratio: (self.min_area_percent / 100.0).clamp(0.0, 1.0),
            max_area_ratio: (self.max_area_percent / 100.0).clamp(0.0, 1.0),
            padding: 0,
            inset: self.inset_px.clamp(0, 500),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanAnalysis {
    pub path: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub boxes: Vec<BoxDto>,
    /// `data:image/jpeg;base64,...`, at most 220 px on the long side.
    pub thumbnail: String,
    pub millis: u64,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn jpeg_bytes(image: &RgbImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut Cursor::new(&mut bytes), quality)
        .encode(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| format!("could not encode a preview: {e}"))?;
    Ok(bytes)
}

fn data_url(jpeg: &[u8]) -> String {
    format!("data:image/jpeg;base64,{}", STANDARD.encode(jpeg))
}

/// Shrink to fit `side` on the long side (never enlarges).
fn fit(image: &RgbImage, side: u32) -> RgbImage {
    let longest = image.width().max(image.height());
    if longest <= side {
        return image.clone();
    }
    let scale = side as f64 / longest as f64;
    resize_area(
        image,
        ((image.width() as f64 * scale).round() as u32).max(1),
        ((image.height() as f64 * scale).round() as u32).max(1),
    )
}

/// Where the screen preview of `scan` is cached. The key covers the path, size
/// and modification time, so an edited scan gets a fresh preview.
pub fn preview_path(cache_dir: &Path, scan: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    scan.hash(&mut hasher);
    if let Ok(meta) = std::fs::metadata(scan) {
        meta.len().hash(&mut hasher);
        if let Ok(modified) = meta.modified() {
            modified.hash(&mut hasher);
        }
    }
    cache_dir.join(format!("{:016x}.jpg", hasher.finish()))
}

/// Where the queue thumbnail of `scan` is cached: beside the previews, same key, in `thumbs`.
pub fn thumbnail_path(cache_dir: &Path, scan: &Path) -> PathBuf {
    let key = preview_path(cache_dir, scan);
    cache_dir.join("thumbs").join(key.file_name().unwrap_or_default())
}

/// The queue thumbnail of `rgb` as a data URL; also leaves it in the cache (best effort) so a
/// reopened project does not have to rebuild it.
fn make_thumbnail(rgb: &RgbImage, cache_dir: &Path, scan: &Path) -> Result<String, String> {
    let bytes = jpeg_bytes(&fit(rgb, THUMBNAIL_SIDE), 80)?;
    let target = thumbnail_path(cache_dir, scan);
    if let Some(folder) = target.parent()
        && std::fs::create_dir_all(folder).is_ok()
    {
        let part = target.with_extension("part");
        if std::fs::write(&part, &bytes).is_ok() {
            let _ = std::fs::rename(&part, &target);
        }
    }
    Ok(data_url(&bytes))
}

fn write_preview(rgb: &RgbImage, cache_dir: &Path, scan: &Path) -> Result<(), String> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| format!("could not create the preview cache: {e}"))?;
    let target = preview_path(cache_dir, scan);
    let preview = fit(rgb, PREVIEW_SIDE);
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut Cursor::new(&mut bytes), 88)
        .encode(
            preview.as_raw(),
            preview.width(),
            preview.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| format!("could not encode the preview: {e}"))?;
    // Write then rename, so a half-written preview is never read.
    let part = target.with_extension("part");
    std::fs::write(&part, bytes).map_err(|e| format!("could not write the preview: {e}"))?;
    std::fs::rename(&part, &target).map_err(|e| format!("could not store the preview: {e}"))
}

/// Full v7 on this thread's MobileSAM sessions (loaded on first use).
fn detect_full_v7(rgb: &image::RgbImage, options: &DetectOptions) -> Result<Detection, String> {
    let dir = oxicut_models::models_dir().map_err(|e| e.to_string())?;
    let encoder = oxicut_models::model_path(&dir, &oxicut_models::MOBILESAM_ENCODER);
    let decoder = oxicut_models::model_path(&dir, &oxicut_models::MOBILESAM_DECODER);
    if !encoder.is_file() || !decoder.is_file() {
        return Err(
            "The v7 detector needs the MobileSAM models: download them in the Models tab.".into(),
        );
    }
    oxicut_core::sam::with_thread_sam(&encoder, &decoder, |sam| detect_v7(rgb, options, sam))
        .and_then(|result| result)
        .map(|(detection, _)| detection)
        .map_err(|e| e.to_string())
}

/// Decode `path`, find its prints, and leave a screen preview in the cache.
pub fn analyze_scan(
    path: &Path,
    settings: &DetectSettings,
    cache_dir: &Path,
) -> Result<ScanAnalysis, String> {
    let started = Instant::now();
    let name = file_name(path);
    let image = load_image(path).map_err(|e| format!("Could not read {name}: {e}"))?;
    let (width, height) = (image.width(), image.height());
    let rgb = to_detection_rgb(&image);
    drop(image); // a 16-bit scan is large; the 8-bit copy is all that is needed from here

    let options = settings.options();
    let detection = match settings.mode {
        Detector::V3 => detect_v3(&rgb, &options),
        Detector::V7Lite => detect_v7_lite(&rgb, &options),
        Detector::V7 => detect_full_v7(&rgb, &options)?,
    };
    let boxes = detection
        .regions
        .iter()
        .map(|r| BoxDto {
            cx: r.rect.cx,
            cy: r.rect.cy,
            w: r.rect.w,
            h: r.rect.h,
            angle: r.rect.angle,
        })
        .collect();
    write_preview(&rgb, cache_dir, path)?;
    let thumbnail = make_thumbnail(&rgb, cache_dir, path)?;
    Ok(ScanAnalysis {
        path: path.to_string_lossy().into_owned(),
        name,
        width,
        height,
        boxes,
        thumbnail,
        millis: started.elapsed().as_millis() as u64,
    })
}

/// The queue thumbnail as a data URL: from the cache when it is there, else rebuilt from the cached
/// preview (made first if that is gone too). Used when a saved project is opened.
pub fn load_thumbnail(path: &Path, cache_dir: &Path) -> Result<String, String> {
    if let Ok(bytes) = std::fs::read(thumbnail_path(cache_dir, path)) {
        return Ok(data_url(&bytes));
    }
    let preview = preview_path(cache_dir, path);
    if !preview.is_file() {
        let name = file_name(path);
        let image = load_image(path).map_err(|e| format!("Could not read {name}: {e}"))?;
        write_preview(&to_detection_rgb(&image), cache_dir, path)?;
    }
    let rgb = image::open(&preview)
        .map_err(|e| format!("could not read the preview: {e}"))?
        .to_rgb8();
    make_thumbnail(&rgb, cache_dir, path)
}

/// One straightened crop for the Crops view.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CropImage {
    /// `data:image/jpeg;base64,...`; empty when the box lies outside the scan.
    pub data_url: String,
    /// Size the exported file will have, in scan pixels (the picture shown is smaller).
    pub width: u32,
    pub height: u32,
}

/// The straightened crops of `boxes`, cut from the cached screen preview with the same code export
/// uses, so what is shown is what gets written (at preview resolution). `scan_width` is the scan's
/// real width, which tells how much smaller the preview is. Pictures are at most `side` pixels long.
pub fn render_crops(
    path: &Path,
    scan_width: u32,
    boxes: &[BoxDto],
    side: u32,
    cache_dir: &Path,
) -> Result<Vec<CropImage>, String> {
    let preview = preview_path(cache_dir, path);
    if !preview.is_file() {
        let name = file_name(path);
        let image = load_image(path).map_err(|e| format!("Could not read {name}: {e}"))?;
        write_preview(&to_detection_rgb(&image), cache_dir, path)?;
    }
    let image = image::open(&preview).map_err(|e| format!("could not read the preview: {e}"))?;
    let scale = image.width() as f64 / scan_width.max(1) as f64;
    boxes
        .iter()
        .map(|b| {
            let region = RotatedRect::new(b.cx * scale, b.cy * scale, b.w * scale, b.h * scale, b.angle);
            let data_url = match crop_region(&image, &region) {
                Some(crop) => data_url(&jpeg_bytes(&fit(&crop.to_rgb8(), side.max(16)), 85)?),
                None => String::new(),
            };
            // The exported file gets the box size rounded the way `crop_region` rounds it.
            let size = |v: f64| v.round_ties_even().max(0.0) as u32;
            Ok(CropImage {
                data_url,
                width: size(b.w),
                height: size(b.h),
            })
        })
        .collect()
}

/// The screen preview as a data URL, from the cache when it is there.
pub fn load_preview(path: &Path, cache_dir: &Path) -> Result<String, String> {
    let cached = preview_path(cache_dir, path);
    if !cached.is_file() {
        let name = file_name(path);
        let image = load_image(path).map_err(|e| format!("Could not read {name}: {e}"))?;
        write_preview(&to_detection_rgb(&image), cache_dir, path)?;
    }
    let bytes = std::fs::read(&cached).map_err(|e| format!("could not read the preview: {e}"))?;
    Ok(data_url(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oxicut-engine-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Pale scan with two coloured prints.
    fn write_scan(path: &Path) {
        let mut scan = RgbImage::from_pixel(1000, 700, Rgb([248, 248, 246]));
        for &(x0, y0, w, h, c) in &[
            (100u32, 120u32, 360u32, 460u32, [70u8, 90, 130]),
            (560, 150, 340, 400, [150, 60, 50]),
        ] {
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    scan.put_pixel(
                        x,
                        y,
                        Rgb([
                            c[0].saturating_add(((x * 7 + y * 13) % 11) as u8),
                            c[1],
                            c[2],
                        ]),
                    );
                }
            }
        }
        scan.save(path).unwrap();
    }

    #[test]
    fn natural_order_puts_scan2_before_scan10() {
        let mut names = vec![
            "scan10.tif",
            "Scan2.tif",
            "scan1.tif",
            "scan02a.tif",
            "other.tif",
        ];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            names,
            vec![
                "other.tif",
                "scan1.tif",
                "Scan2.tif",
                "scan02a.tif",
                "scan10.tif"
            ]
        );
    }

    #[test]
    fn folders_expand_to_their_scans_and_duplicates_vanish() {
        let dir = scratch("expand");
        for name in ["b10.jpg", "b2.JPG", "notes.txt", "c.tiff"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("deep.png"), b"x").unwrap();

        let expanded = expand_paths(&[
            dir.clone(),
            dir.join("c.tiff"),
            dir.join("missing.png"),
            dir.join("notes.txt"),
        ]);
        let names: Vec<_> = expanded.iter().map(|p| file_name(p)).collect();
        assert_eq!(
            names,
            vec!["b2.JPG", "b10.jpg", "c.tiff"],
            "subfolders are not entered"
        );
    }

    #[test]
    fn analysis_finds_the_prints_and_leaves_a_preview() {
        let dir = scratch("analyze");
        let scan = dir.join("scan.png");
        write_scan(&scan);
        let cache = dir.join("cache");

        let analysis = analyze_scan(
            &scan,
            &DetectSettings {
                inset_px: 0,
                ..Default::default()
            },
            &cache,
        )
        .unwrap();
        assert_eq!((analysis.width, analysis.height), (1000, 700));
        assert_eq!(analysis.name, "scan.png");
        assert_eq!(analysis.boxes.len(), 2, "{:?}", analysis.boxes);
        assert!(analysis.thumbnail.starts_with("data:image/jpeg;base64,"));
        assert!(preview_path(&cache, &scan).is_file());
        assert!(!cache.join(format!("{:016x}.part", 0)).exists());

        // The thumbnail decodes and is small.
        let bytes = STANDARD
            .decode(
                analysis
                    .thumbnail
                    .trim_start_matches("data:image/jpeg;base64,"),
            )
            .unwrap();
        let thumb = image::load_from_memory(&bytes).unwrap();
        assert!(thumb.width().max(thumb.height()) <= THUMBNAIL_SIDE);
    }

    #[test]
    fn thumbnails_survive_in_the_cache_and_are_rebuilt_when_it_is_cleared() {
        let dir = scratch("thumbs");
        let scan = dir.join("scan.png");
        write_scan(&scan);
        let cache = dir.join("cache");
        let analysis = analyze_scan(&scan, &DetectSettings::default(), &cache).unwrap();

        // Analysis leaves the thumbnail beside the preview, and reopening reads it back unchanged.
        assert!(thumbnail_path(&cache, &scan).is_file());
        assert_eq!(load_thumbnail(&scan, &cache).unwrap(), analysis.thumbnail);

        // Without the thumbnail it is rebuilt from the preview; without both, from the scan.
        std::fs::remove_file(thumbnail_path(&cache, &scan)).unwrap();
        assert!(load_thumbnail(&scan, &cache).unwrap().starts_with("data:image/jpeg;base64,"));
        assert!(thumbnail_path(&cache, &scan).is_file());
        std::fs::remove_dir_all(&cache).unwrap();
        assert!(load_thumbnail(&scan, &cache).unwrap().starts_with("data:image/jpeg;base64,"));
        assert!(preview_path(&cache, &scan).is_file());
    }

    #[test]
    fn crops_are_straightened_and_turning_a_box_turns_its_crop() {
        let dir = scratch("crops");
        let scan = dir.join("scan.png");
        // A grey scan with one print (x 100..400, y 100..300) whose top-left corner is marked red.
        let mut image = RgbImage::from_pixel(600, 400, Rgb([200, 200, 200]));
        for y in 100..140 {
            for x in 100..140 {
                image.put_pixel(x, y, Rgb([220, 20, 20]));
            }
        }
        image.save(&scan).unwrap();
        let cache = dir.join("cache");
        let boxes = [
            BoxDto { cx: 250.0, cy: 200.0, w: 300.0, h: 200.0, angle: 0.0 },
            // The same print turned clockwise: the angle drops by 90 and width and height swap.
            BoxDto { cx: 250.0, cy: 200.0, w: 200.0, h: 300.0, angle: -90.0 },
            // Turned counter-clockwise: the angle rises by 90.
            BoxDto { cx: 250.0, cy: 200.0, w: 200.0, h: 300.0, angle: 90.0 },
            // Outside the scan altogether.
            BoxDto { cx: 5000.0, cy: 5000.0, w: 100.0, h: 100.0, angle: 0.0 },
        ];
        let crops = render_crops(&scan, 600, &boxes, 400, &cache).unwrap();
        assert_eq!(crops.len(), 4);
        assert_eq!((crops[0].width, crops[0].height), (300, 200));
        assert_eq!((crops[1].width, crops[1].height), (200, 300));
        assert!(crops[3].data_url.is_empty());

        let pixel = |crop: &CropImage, x: u32, y: u32| {
            let bytes = STANDARD
                .decode(crop.data_url.trim_start_matches("data:image/jpeg;base64,"))
                .unwrap();
            image::load_from_memory(&bytes).unwrap().to_rgb8().get_pixel(x, y).0
        };
        let red = |p: [u8; 3]| p[0] > 150 && p[1] < 100 && p[2] < 100;
        // Upright: red in the top-left. Clockwise: it moves to the top-right. Counter-clockwise: bottom-left.
        assert!(red(pixel(&crops[0], 10, 10)) && !red(pixel(&crops[0], 290, 10)));
        assert!(red(pixel(&crops[1], 190, 10)) && !red(pixel(&crops[1], 10, 10)));
        assert!(red(pixel(&crops[2], 10, 290)) && !red(pixel(&crops[2], 10, 10)));
    }

    #[test]
    fn crops_of_a_big_scan_come_from_the_smaller_preview_but_report_the_real_size() {
        let dir = scratch("crops-big");
        let scan = dir.join("scan.png");
        RgbImage::from_pixel(4800, 3200, Rgb([180, 180, 180])).save(&scan).unwrap();
        let cache = dir.join("cache");
        let boxes = [BoxDto { cx: 2400.0, cy: 1600.0, w: 3000.0, h: 2000.0, angle: 2.0 }];
        let crops = render_crops(&scan, 4800, &boxes, 480, &cache).unwrap();
        // The file would be 3000 x 2000; the picture is 480 wide with the same proportions.
        assert_eq!((crops[0].width, crops[0].height), (3000, 2000));
        let bytes = STANDARD
            .decode(crops[0].data_url.trim_start_matches("data:image/jpeg;base64,"))
            .unwrap();
        let picture = image::load_from_memory(&bytes).unwrap();
        assert_eq!(picture.width(), 480);
        assert!((picture.height() as i32 - 320).abs() <= 1);
        assert!(preview_path(&cache, &scan).is_file());
    }

    #[test]
    fn settings_reach_the_detector() {
        let dir = scratch("settings");
        let scan = dir.join("scan.png");
        write_scan(&scan);
        let trimmed = analyze_scan(
            &scan,
            &DetectSettings {
                inset_px: 20,
                ..Default::default()
            },
            &dir.join("c"),
        )
        .unwrap();
        let plain = analyze_scan(
            &scan,
            &DetectSettings {
                inset_px: 0,
                ..Default::default()
            },
            &dir.join("c"),
        )
        .unwrap();
        assert!((plain.boxes[0].w - trimmed.boxes[0].w - 40.0).abs() < 1e-6);
        // A minimum area above the print size finds nothing.
        let strict = analyze_scan(
            &scan,
            &DetectSettings {
                min_area_percent: 60.0,
                max_area_percent: 90.0,
                inset_px: 0,
                ..Default::default()
            },
            &dir.join("c"),
        )
        .unwrap();
        assert!(strict.boxes.is_empty());
    }

    #[test]
    fn previews_come_from_the_cache_and_follow_edits() {
        let dir = scratch("preview");
        let scan = dir.join("scan.png");
        write_scan(&scan);
        let cache = dir.join("cache");
        let first = load_preview(&scan, &cache).unwrap();
        assert!(first.starts_with("data:image/jpeg;base64,"));
        let cached = preview_path(&cache, &scan);
        assert!(cached.is_file());
        // Editing the scan (different size) changes the cache key.
        RgbImage::from_pixel(10, 10, Rgb([1, 2, 3]))
            .save(&scan)
            .unwrap();
        assert_ne!(preview_path(&cache, &scan), cached);
    }

    #[test]
    fn the_detector_choice_reaches_the_detector_and_names_parse() {
        // A print with a mid-grey border: v3 boxes the whole print, v7-lite the picture.
        let dir = scratch("detector");
        let scan = dir.join("bordered.png");
        let mut image = RgbImage::from_pixel(1000, 700, Rgb([250, 250, 250]));
        for y in 150..550 {
            for x in 200..700 {
                let inside = (216..684).contains(&x) && (166..534).contains(&y);
                image.put_pixel(
                    x,
                    y,
                    if inside {
                        Rgb([30, 40, 70])
                    } else {
                        Rgb([130, 130, 130])
                    },
                );
            }
        }
        image.save(&scan).unwrap();

        let width_with = |mode| {
            let settings = DetectSettings {
                mode,
                ..Default::default()
            };
            analyze_scan(&scan, &settings, &dir.join("c"))
                .unwrap()
                .boxes[0]
                .w
        };
        let (v3, lite) = (width_with(Detector::V3), width_with(Detector::V7Lite));
        assert!((v3 - 500.0).abs() < 6.0, "v3 {v3}");
        assert!((lite - 468.0).abs() < 6.0, "v7-lite {lite}");

        // The screen sends the names as written in the settings file.
        let parsed: DetectSettings = serde_json::from_str(r#"{"mode": "v3"}"#).unwrap();
        assert_eq!(parsed.mode, Detector::V3);
        assert_eq!(
            serde_json::from_str::<DetectSettings>(r#"{"mode": "v7"}"#)
                .unwrap()
                .mode,
            Detector::V7
        );
        assert_eq!(
            serde_json::from_str::<DetectSettings>("{}").unwrap().mode,
            Detector::V7Lite
        );
        assert!(serde_json::from_str::<DetectSettings>(r#"{"mode": "v9"}"#).is_err());
    }

    #[test]
    fn unreadable_files_give_a_message_naming_the_file() {
        let dir = scratch("bad");
        let scan = dir.join("broken.jpg");
        std::fs::write(&scan, b"this is not a jpeg").unwrap();
        let error = analyze_scan(&scan, &DetectSettings::default(), &dir.join("c")).unwrap_err();
        assert!(error.contains("broken.jpg"), "{error}");
        assert!(
            analyze_scan(
                &dir.join("missing.png"),
                &DetectSettings::default(),
                &dir.join("c")
            )
            .is_err()
        );
    }

    #[test]
    fn large_scans_get_a_bounded_preview() {
        let dir = scratch("large");
        let scan = dir.join("big.png");
        RgbImage::from_pixel(4800, 3200, Rgb([240, 240, 240]))
            .save(&scan)
            .unwrap();
        let cache = dir.join("cache");
        analyze_scan(&scan, &DetectSettings::default(), &cache).unwrap();
        let preview = image::open(preview_path(&cache, &scan)).unwrap();
        assert_eq!(preview.width().max(preview.height()), PREVIEW_SIDE);
    }
}
