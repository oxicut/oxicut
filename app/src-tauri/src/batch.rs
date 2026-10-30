//! Batch work for the app: detecting many scans in parallel and exporting the
//! approved ones. Progress goes to a callback, which the Tauri commands wire to
//! a channel, so everything here can be tested without a window.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use oxicut_core::RotatedRect;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use oxicut_core::export::{Collision, ExportSettings, OutputFormat, ScanExport};
use oxicut_core::flags::{IdentifiedBox, evaluate_scan};
use oxicut_core::io::load_image;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::engine::{BoxDto, DetectSettings, Detector, ScanAnalysis, analyze_scan, jpeg_bytes};

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "event",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DetectEvent {
    Started {
        index: usize,
        path: String,
    },
    Finished {
        index: usize,
        analysis: ScanAnalysis,
    },
    Failed {
        index: usize,
        path: String,
        error: String,
    },
    /// Not started because the batch was cancelled.
    Skipped {
        index: usize,
        path: String,
    },
}

/// Detect every scan, a few at a time (decoding big TIFFs is memory hungry).
/// `cancel` stops scans that have not started yet. One scan failing, even by
/// panicking, never stops the others.
pub fn detect_batch(
    paths: &[PathBuf],
    settings: &DetectSettings,
    cache_dir: &Path,
    cancel: &AtomicBool,
    emit: &(dyn Fn(DetectEvent) + Sync),
) -> Result<(), String> {
    // v7 runs a neural network that uses several cores itself, and every worker holds its
    // own copy of the model, so it gets fewer workers.
    let limit = if settings.mode == Detector::V7 { 2 } else { 4 };
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(1, limit);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .map_err(|e| e.to_string())?;
    pool.install(|| {
        paths.par_iter().enumerate().for_each(|(index, path)| {
            let path_text = path.to_string_lossy().into_owned();
            if cancel.load(Ordering::SeqCst) {
                emit(DetectEvent::Skipped {
                    index,
                    path: path_text,
                });
                return;
            }
            emit(DetectEvent::Started {
                index,
                path: path_text.clone(),
            });
            let result = catch_unwind(AssertUnwindSafe(|| analyze_scan(path, settings, cache_dir)))
                .unwrap_or_else(|_| Err("the detector crashed on this scan".to_string()));
            emit(match result {
                Ok(analysis) => DetectEvent::Finished { index, analysis },
                Err(error) => DetectEvent::Failed {
                    index,
                    path: path_text,
                    error,
                },
            });
        });
    });
    Ok(())
}

/// A box with the id the screen gave it.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct IdBox {
    pub id: u32,
    #[serde(flatten)]
    pub rect: BoxDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlagDto {
    pub code: String,
    pub box_id: Option<u32>,
    pub message: String,
}

pub fn flags_for(boxes: &[IdBox], width: u32, height: u32) -> Vec<FlagDto> {
    let boxes: Vec<IdentifiedBox> = boxes
        .iter()
        .map(|b| IdentifiedBox {
            id: b.id,
            rect: RotatedRect::new(b.rect.cx, b.rect.cy, b.rect.w, b.rect.h, b.rect.angle),
        })
        .collect();
    evaluate_scan(&boxes, width, height)
        .into_iter()
        .map(|f| FlagDto {
            code: f.code.as_str().to_string(),
            box_id: f.box_id,
            message: f.message,
        })
        .collect()
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportItem {
    pub path: String,
    pub boxes: Vec<BoxDto>,
    /// A file name (without extension) for each box, in the same order; `None`, or no entry, uses the
    /// naming pattern. Pairs of fronts and backs are named this way.
    #[serde(default)]
    pub names: Vec<Option<String>>,
    /// A scan whose `OxiCut output` folder to use instead of this scan's own: the backs of a pair are
    /// saved next to their fronts.
    #[serde(default)]
    pub folder_of: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    /// `same`, `jpg`, `png` or `tiff`.
    pub format: String,
    pub jpeg_quality: u8,
    pub pattern: String,
    pub keep_metadata: bool,
    pub tag_software: bool,
    /// `None`: an `OxiCut output` folder beside each scan.
    pub output_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "event",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExportEvent {
    Started {
        index: usize,
        total: usize,
        name: String,
    },
    Finished {
        index: usize,
        files: Vec<String>,
        /// Size on disk of each file, in the same order.
        sizes: Vec<u64>,
        /// How long this scan took, in milliseconds.
        millis: u64,
    },
    Failed {
        index: usize,
        name: String,
        error: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub images: usize,
    pub scans: usize,
    pub failed: usize,
    /// Folder holding the first written image, for "show in folder".
    pub folder: Option<String>,
    pub first_file: Option<String>,
}

/// Where the crops of the scan at `scan` go: the folder chosen in the settings, or an `OxiCut output`
/// folder beside the scan.
fn output_folder(options: &ExportOptions, scan: &Path) -> PathBuf {
    match &options.output_dir {
        Some(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
        _ => scan.parent().unwrap_or(Path::new(".")).join("OxiCut output"),
    }
}

pub fn parse_format(text: &str) -> Result<OutputFormat, String> {
    match text {
        "same" => Ok(OutputFormat::Same),
        "jpg" | "jpeg" => Ok(OutputFormat::Jpg),
        "png" => Ok(OutputFormat::Png),
        "tiff" | "tif" => Ok(OutputFormat::Tiff),
        other => Err(format!("unknown output format '{other}'")),
    }
}

/// Export every item, one scan at a time: each needs its full-resolution
/// original in memory, so scans are not decoded in parallel.
pub fn export_batch(
    items: &[ExportItem],
    options: &ExportOptions,
    emit: &dyn Fn(ExportEvent),
) -> Result<ExportSummary, String> {
    let format = parse_format(&options.format)?;
    let mut summary = ExportSummary {
        scans: items.len(),
        ..Default::default()
    };

    for (index, item) in items.iter().enumerate() {
        let source = PathBuf::from(&item.path);
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| item.path.clone());
        emit(ExportEvent::Started {
            index,
            total: items.len(),
            name: name.clone(),
        });
        let started = Instant::now();

        let out_dir = output_folder(options, item.folder_of.as_deref().map(Path::new).unwrap_or(&source));
        // Exports never overwrite earlier ones: the user may have kept or renamed them.
        let settings = export_settings(options, format, Collision::Rename);
        let regions: Vec<RotatedRect> = item
            .boxes
            .iter()
            .map(|b| RotatedRect::new(b.cx, b.cy, b.w, b.h, b.angle))
            .collect();

        let outcome = catch_unwind(AssertUnwindSafe(|| -> Result<Vec<String>, String> {
            let image = load_image(&source).map_err(|e| format!("Could not read {name}: {e}"))?;
            let export = ScanExport::new(&source, &out_dir, &settings).map_err(|e| e.to_string())?;
            let mut files = Vec::new();
            for (index, region) in regions.iter().enumerate() {
                let name = item.names.get(index).and_then(|n| n.as_deref());
                if let Some(done) = export
                    .write_named(&image, region, index + 1, name)
                    .map_err(|e| e.to_string())?
                {
                    files.push(done.file.path.to_string_lossy().into_owned());
                }
            }
            Ok(files)
        }))
        .unwrap_or_else(|_| Err("export crashed on this scan".to_string()));

        match outcome {
            Ok(files) => {
                summary.images += files.len();
                if summary.first_file.is_none() {
                    summary.first_file = files.first().cloned();
                    summary.folder = Some(out_dir.to_string_lossy().into_owned());
                }
                let sizes = files
                    .iter()
                    .map(|f| std::fs::metadata(f).map(|m| m.len()).unwrap_or(0))
                    .collect();
                emit(ExportEvent::Finished {
                    index,
                    files,
                    sizes,
                    millis: started.elapsed().as_millis() as u64,
                });
            }
            Err(error) => {
                summary.failed += 1;
                emit(ExportEvent::Failed { index, name, error });
            }
        }
    }
    Ok(summary)
}

/// One box cut at full resolution, as the export would write it, but into a scratch folder: for
/// looking at the real result of a crop before exporting anything.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalCrop {
    /// The file in the scratch folder.
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    /// `jpg`, `png` or `tif`.
    pub format: String,
    pub millis: u64,
    /// A picture the screen can show: the file itself for JPEG and PNG, a full-size JPEG copy of it
    /// for TIFF (which a browser cannot display).
    pub data_url: String,
    pub shown_as_copy: bool,
}

/// How long files stay in the scratch folder.
const SCRATCH_KEEP: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Delete scratch files older than `max_age`; best effort.
fn prune_scratch(dir: &Path, max_age: std::time::Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > max_age);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The settings an export with `options` uses, with the way to treat files that already exist.
fn export_settings(options: &ExportOptions, format: OutputFormat, collision: Collision) -> ExportSettings {
    ExportSettings {
        format,
        jpeg_quality: options.jpeg_quality,
        pattern: options.pattern.clone(),
        keep_metadata: options.keep_metadata,
        tag_software: options.tag_software,
        collision,
    }
}

/// Cut print number `number` of the scan at `source` for real, with the export settings, into
/// `scratch_dir` (replacing the file a former call left there).
pub fn final_crop(
    source: &Path,
    region: &BoxDto,
    number: usize,
    chosen_name: Option<&str>,
    options: &ExportOptions,
    scratch_dir: &Path,
) -> Result<FinalCrop, String> {
    let started = Instant::now();
    let format = parse_format(&options.format)?;
    prune_scratch(scratch_dir, SCRATCH_KEEP);
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.display().to_string());
    let image = load_image(source).map_err(|e| format!("Could not read {name}: {e}"))?;
    let settings = export_settings(options, format, Collision::Overwrite);
    let rect = RotatedRect::new(region.cx, region.cy, region.w, region.h, region.angle);
    let written = ScanExport::new(source, scratch_dir, &settings)
        .and_then(|export| export.write_named(&image, &rect, number, chosen_name))
        .map_err(|e| e.to_string())?
        .ok_or("This box lies outside the scan, so there is nothing to cut.")?;

    let path = written.file.path;
    let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let (data_url, shown_as_copy) = match extension.as_str() {
        "jpg" | "png" => {
            let mime = if extension == "png" { "image/png" } else { "image/jpeg" };
            let data = std::fs::read(&path).map_err(|e| format!("could not read the cut: {e}"))?;
            (format!("data:{mime};base64,{}", STANDARD.encode(data)), false)
        }
        _ => {
            let copy = jpeg_bytes(&written.crop.to_rgb8(), 92)?;
            (format!("data:image/jpeg;base64,{}", STANDARD.encode(copy)), true)
        }
    };
    Ok(FinalCrop {
        path: path.to_string_lossy().into_owned(),
        width: written.file.width,
        height: written.file.height,
        bytes,
        format: extension,
        millis: started.elapsed().as_millis() as u64,
        data_url,
        shown_as_copy,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxicut_core::export::export_regions;
    use image::{Rgb, RgbImage};
    use std::sync::{Arc, Mutex};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oxicut-batch-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_scan(path: &Path) {
        let mut scan = RgbImage::from_pixel(1000, 700, Rgb([248, 248, 246]));
        for y in 120..580 {
            for x in 100..460 {
                scan.put_pixel(
                    x,
                    y,
                    Rgb([70u8.saturating_add(((x * 7 + y * 13) % 11) as u8), 90, 130]),
                );
            }
        }
        scan.save(path).unwrap();
    }

    fn options(format: &str) -> ExportOptions {
        ExportOptions {
            format: format.into(),
            jpeg_quality: 90,
            pattern: "{source}-{n}".into(),
            keep_metadata: true,
            tag_software: true,
            output_dir: None,
        }
    }

    const PRINT: BoxDto = BoxDto { cx: 280.0, cy: 350.0, w: 360.0, h: 460.0, angle: 0.0 };

    #[test]
    fn the_real_cut_is_the_file_an_export_would_write_and_is_made_again_in_place() {
        let dir = scratch("final-png");
        let scan = dir.join("holiday.png");
        write_scan(&scan);
        let scratch_dir = dir.join("scratch");

        let cut = final_crop(&scan, &PRINT, 3, None, &options("png"), &scratch_dir).unwrap();
        // Named like the export names it (print number 3), full size, a PNG that is shown as it is.
        assert_eq!(Path::new(&cut.path), scratch_dir.join("holiday-3.png"));
        assert_eq!((cut.width, cut.height), (360, 460));
        assert_eq!(cut.format, "png");
        assert!(cut.bytes > 0 && !cut.shown_as_copy);
        let shown = STANDARD.decode(cut.data_url.trim_start_matches("data:image/png;base64,")).unwrap();
        assert_eq!(shown, std::fs::read(&cut.path).unwrap());

        // Byte for byte what the real export writes for the same box.
        let export_dir = dir.join("exported");
        let settings = export_settings(&options("png"), OutputFormat::Png, Collision::Rename);
        let image = load_image(&scan).unwrap();
        let rect = RotatedRect::new(PRINT.cx, PRINT.cy, PRINT.w, PRINT.h, PRINT.angle);
        let written = export_regions(&image, &scan, &[rect], &export_dir, &settings).unwrap();
        assert_eq!(std::fs::read(&written[0].path).unwrap(), std::fs::read(&cut.path).unwrap());

        // Cutting again replaces the file instead of piling up " (2)" copies.
        final_crop(&scan, &PRINT, 3, None, &options("png"), &scratch_dir).unwrap();
        assert_eq!(std::fs::read_dir(&scratch_dir).unwrap().count(), 1);
    }

    #[test]
    fn a_tiff_cut_is_shown_through_a_full_size_jpeg_copy() {
        let dir = scratch("final-tiff");
        let scan = dir.join("holiday.png");
        write_scan(&scan);
        let cut = final_crop(&scan, &PRINT, 1, None, &options("tiff"), &dir.join("scratch")).unwrap();
        assert_eq!(cut.format, "tif");
        assert!(cut.shown_as_copy && cut.data_url.starts_with("data:image/jpeg;base64,"));
        let copy = STANDARD.decode(cut.data_url.trim_start_matches("data:image/jpeg;base64,")).unwrap();
        let picture = image::load_from_memory(&copy).unwrap();
        assert_eq!((picture.width(), picture.height()), (360, 460));
    }

    #[test]
    fn a_box_outside_the_scan_is_explained_not_cut() {
        let dir = scratch("final-outside");
        let scan = dir.join("holiday.png");
        write_scan(&scan);
        let outside = BoxDto { cx: 9000.0, cy: 9000.0, w: 100.0, h: 100.0, angle: 0.0 };
        let error = final_crop(&scan, &outside, 1, None, &options("png"), &dir.join("scratch")).unwrap_err();
        assert!(error.contains("outside the scan"), "{error}");
    }

    #[test]
    fn old_scratch_files_are_cleaned_up_and_recent_ones_kept() {
        let dir = scratch("final-prune");
        let (old, recent) = (dir.join("old.png"), dir.join("recent.png"));
        std::fs::write(&old, b"x").unwrap();
        std::fs::write(&recent, b"x").unwrap();
        let two_days = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 24 * 60 * 60);
        std::fs::File::options().write(true).open(&old).unwrap().set_modified(two_days).unwrap();
        prune_scratch(&dir, SCRATCH_KEEP);
        assert!(!old.exists() && recent.exists());
    }

    /// A sink that records every event, shareable across the detector's threads.
    fn collect<T: Send + 'static>() -> (Arc<Mutex<Vec<T>>>, impl Fn(T) + Sync) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        (events, move |event| sink.lock().unwrap().push(event))
    }

    fn taken<T>(events: &Arc<Mutex<Vec<T>>>) -> Vec<T> {
        std::mem::take(&mut *events.lock().unwrap())
    }
    #[test]
    fn detection_reports_every_scan_and_one_failure_does_not_stop_the_rest() {
        let dir = scratch("detect");
        let (good, other) = (dir.join("good.png"), dir.join("other.png"));
        write_scan(&good);
        write_scan(&other);
        let broken = dir.join("broken.jpg");
        std::fs::write(&broken, b"not a jpeg").unwrap();
        let paths = vec![good.clone(), broken.clone(), other.clone()];

        let (events, emit) = collect::<DetectEvent>();
        detect_batch(
            &paths,
            &DetectSettings::default(),
            &dir.join("cache"),
            &AtomicBool::new(false),
            &emit,
        )
        .unwrap();
        let events = taken(&events);

        let finished: Vec<usize> = events
            .iter()
            .filter_map(|e| {
                if let DetectEvent::Finished { index, .. } = e {
                    Some(*index)
                } else {
                    None
                }
            })
            .collect();
        let failed: Vec<usize> = events
            .iter()
            .filter_map(|e| {
                if let DetectEvent::Failed { index, .. } = e {
                    Some(*index)
                } else {
                    None
                }
            })
            .collect();
        let started = events
            .iter()
            .filter(|e| matches!(e, DetectEvent::Started { .. }))
            .count();
        let mut finished_sorted = finished.clone();
        finished_sorted.sort_unstable();
        assert_eq!(finished_sorted, vec![0, 2]);
        assert_eq!(failed, vec![1]);
        assert_eq!(started, 3);
        let error = events
            .iter()
            .find_map(|e| {
                if let DetectEvent::Failed { error, .. } = e {
                    Some(error.clone())
                } else {
                    None
                }
            })
            .unwrap();
        assert!(error.contains("broken.jpg"), "{error}");
    }

    #[test]
    fn cancelling_skips_scans_that_have_not_started() {
        let dir = scratch("cancel");
        let scan = dir.join("a.png");
        write_scan(&scan);
        let (events, emit) = collect::<DetectEvent>();
        detect_batch(
            &[scan.clone(), scan],
            &DetectSettings::default(),
            &dir.join("cache"),
            &AtomicBool::new(true),
            &emit,
        )
        .unwrap();
        let events = taken(&events);
        assert_eq!(events.len(), 2);
        assert!(
            events
                .iter()
                .all(|e| matches!(e, DetectEvent::Skipped { .. }))
        );
    }

    #[test]
    fn events_serialize_with_camel_case_names_for_the_screen() {
        let json = serde_json::to_value(DetectEvent::Failed {
            index: 3,
            path: "a.png".into(),
            error: "bad".into(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"event": "failed", "index": 3, "path": "a.png", "error": "bad"})
        );
        let json = serde_json::to_value(ExportEvent::Started {
            index: 0,
            total: 2,
            name: "x".into(),
        })
        .unwrap();
        assert_eq!(json["event"], "started");
        assert_eq!(json["total"], 2);
    }

    #[test]
    fn flags_come_back_with_codes_ids_and_messages() {
        let boxes = [IdBox {
            id: 9,
            rect: BoxDto {
                cx: 500.0,
                cy: 400.0,
                w: 1000.0, // spans the whole scan, so it touches both side edges
                h: 100.0,
                angle: 0.0,
            },
        }];
        let flags = flags_for(&boxes, 1000, 800);
        let codes: Vec<_> = flags.iter().map(|f| f.code.as_str()).collect();
        assert!(
            codes.contains(&"extreme_aspect") && codes.contains(&"touches_edge"),
            "{codes:?}"
        );
        assert!(flags.iter().all(|f| f.box_id == Some(9)));
        let from_screen: IdBox =
            serde_json::from_str(r#"{"id": 4, "cx": 1, "cy": 2, "w": 3, "h": 4, "angle": 5}"#)
                .unwrap();
        assert_eq!((from_screen.id, from_screen.rect.angle), (4, 5.0));
    }

    #[test]
    fn export_writes_crops_beside_the_scan_by_default_and_never_overwrites() {
        let dir = scratch("export");
        let scan = dir.join("holiday.png");
        write_scan(&scan);
        let items = vec![ExportItem {
            path: scan.to_string_lossy().into_owned(),
            boxes: vec![BoxDto {
                cx: 280.0,
                cy: 350.0,
                w: 460.0,
                h: 360.0,
                angle: 90.0,
            }],
            ..Default::default()
        }];
        let options = ExportOptions {
            format: "png".into(),
            jpeg_quality: 92,
            pattern: "{source}-{n}".into(),
            keep_metadata: true,
            tag_software: true,
            output_dir: None,
        };

        let (events, emit) = collect::<ExportEvent>();
        let first = export_batch(&items, &options, &emit).unwrap();
        assert_eq!((first.images, first.scans, first.failed), (1, 1, 0));
        let folder = PathBuf::from(first.folder.clone().unwrap());
        assert_eq!(folder, dir.join("OxiCut output"));
        assert!(folder.join("holiday-1.png").is_file());

        let second = export_batch(&items, &options, &|_| {}).unwrap();
        assert!(second.first_file.unwrap().ends_with("holiday-1 (2).png"));
        let events = taken(&events);
        assert!(matches!(events[0], ExportEvent::Started { total: 1, .. }));
        assert!(matches!(events[1], ExportEvent::Finished { .. }));
    }

    #[test]
    fn a_back_is_named_as_asked_and_saved_beside_its_front() {
        let dir = scratch("export-pair");
        let fronts = dir.join("fronts");
        let backs = dir.join("backs");
        std::fs::create_dir_all(&fronts).unwrap();
        std::fs::create_dir_all(&backs).unwrap();
        let front = fronts.join("cards.png");
        let back = backs.join("cards-back-side.png");
        write_scan(&front);
        write_scan(&back);
        let one = vec![BoxDto {
            cx: 280.0,
            cy: 350.0,
            w: 360.0,
            h: 460.0,
            angle: 0.0,
        }];
        let items = vec![
            ExportItem {
                path: front.to_string_lossy().into_owned(),
                boxes: one.clone(),
                names: vec![Some("cards-1-front".into())],
                folder_of: None,
            },
            ExportItem {
                path: back.to_string_lossy().into_owned(),
                boxes: one,
                names: vec![Some("cards-1-back".into())],
                folder_of: Some(front.to_string_lossy().into_owned()),
            },
        ];
        let options = ExportOptions {
            format: "png".into(),
            jpeg_quality: 92,
            pattern: "{source}-{n}".into(),
            keep_metadata: true,
            tag_software: true,
            output_dir: None,
        };
        let summary = export_batch(&items, &options, &|_| {}).unwrap();
        assert_eq!((summary.images, summary.failed), (2, 0));
        let together = fronts.join("OxiCut output");
        assert!(together.join("cards-1-front.png").is_file());
        assert!(together.join("cards-1-back.png").is_file());
        assert!(!backs.join("OxiCut output").exists());
    }

    #[test]
    fn export_honours_the_chosen_folder_and_reports_failures_without_stopping() {
        let dir = scratch("export-dir");
        let scan = dir.join("a.png");
        write_scan(&scan);
        let out = dir.join("chosen");
        let boxes = vec![BoxDto {
            cx: 280.0,
            cy: 350.0,
            w: 360.0,
            h: 460.0,
            angle: 0.0,
        }];
        let items = vec![
            ExportItem {
                path: dir.join("missing.png").to_string_lossy().into_owned(),
                boxes: boxes.clone(),
                ..Default::default()
            },
            ExportItem {
                path: scan.to_string_lossy().into_owned(),
                boxes,
                ..Default::default()
            },
        ];
        let options = ExportOptions {
            format: "jpg".into(),
            jpeg_quality: 85,
            pattern: "print_{n}".into(),
            keep_metadata: true,
            tag_software: true,
            output_dir: Some(out.to_string_lossy().into_owned()),
        };
        let summary = export_batch(&items, &options, &|_| {}).unwrap();
        assert_eq!((summary.images, summary.scans, summary.failed), (1, 2, 1));
        assert!(out.join("print_1.jpg").is_file());
    }

    #[test]
    fn formats_parse_and_unknown_ones_are_rejected() {
        assert_eq!(parse_format("same").unwrap(), OutputFormat::Same);
        assert_eq!(parse_format("tiff").unwrap(), OutputFormat::Tiff);
        assert_eq!(parse_format("jpeg").unwrap(), OutputFormat::Jpg);
        assert!(parse_format("bmp").is_err());
        let options = ExportOptions {
            format: "bmp".into(),
            jpeg_quality: 90,
            pattern: "x".into(),
            keep_metadata: true,
            tag_software: true,
            output_dir: None,
        };
        assert!(export_batch(&[], &options, &|_| {}).is_err());
    }
}
