//! The model-free part of ScanSplitter's v7 detector, "v7-lite": v3's proposals,
//! each moved onto the photograph's own edge by inner-edge tightening.
//!
//! Full v7 also starts from MobileSAM-based v5 anchors, weighs alternative
//! proposals against each other and drops prints seen through glassine sleeves;
//! those parts arrive later. This is the step that stops boxes running onto
//! white borders and drop shadows.

use image::RgbImage;

use crate::chroma::chroma_rectangles;
use crate::detect::{DetectOptions, Detection, detect_v3, detect_v3_capped, regions_from_rects};
use crate::geometry::RotatedRect;
use crate::imgproc::gaussian_blur_f32;
use crate::pil::working_gray;
use crate::sam::{MobileSam, SamError};
use crate::score::{Candidate, GhostMaps, drop_ghosts, edge_sharpness, score_rect, select};
use crate::tighten::{GrayF32, tighten_rect};
use crate::v5;

/// Long side of the grey working image tightening reads (`_WORK_MAX`).
pub const WORK_MAX: u32 = 2000;
/// Tightening passes (`passes=2`).
const PASSES: usize = 2;
/// Cap on the colour threshold of v7's second v3 pass.
const CAPPED_COLOR_THRESHOLD: f32 = 30.0;
/// Long side of the grey copy the ghost statistics read.
const GHOST_SIDE: u32 = 1500;

/// v3's rectangles in full-resolution pixels, within the area limits
/// (`_v3_rects` with no colour cap).
fn v3_proposals(
    detection: &Detection,
    source: &RgbImage,
    options: &DetectOptions,
) -> Vec<RotatedRect> {
    let scale = detection.stages.scale;
    let area = source.width() as f64 * source.height() as f64;
    detection
        .stages
        .refined
        .iter()
        .map(|r| r.scaled(1.0 / scale))
        .filter(|r| (options.min_area_ratio..=options.max_area_ratio).contains(&(r.w * r.h / area)))
        .collect()
}

pub fn detect_v7_lite(source: &RgbImage, options: &DetectOptions) -> Detection {
    let mut detection = detect_v3(source, options);
    let proposals = v3_proposals(&detection, source, options);

    let (gray, scale) = working_gray(source, WORK_MAX);
    let gray = GrayF32::from_gray(&gray);
    let tightened: Vec<RotatedRect> = proposals
        .iter()
        .map(|r| tighten_rect(&gray, r, scale, PASSES))
        .collect();

    detection.regions = regions_from_rects(&tightened, source.width(), source.height(), options);
    // Stage overlays are in detection-image pixels.
    let detection_scale = detection.stages.scale;
    detection.stages.tightened = tightened
        .iter()
        .map(|r| r.scaled(detection_scale))
        .collect();
    detection
}

/// Everything `detect_v7` works out before arbitration, in full-resolution pixels. Kept
/// separate so the stages can be compared with ScanSplitter's one by one.
pub struct V7Stages {
    pub v3: Vec<RotatedRect>,
    pub v3_capped: Vec<RotatedRect>,
    pub chroma: Vec<RotatedRect>,
    pub v5: Vec<RotatedRect>,
    pub anchors: Vec<Candidate>,
    pub alternatives: Vec<Candidate>,
    pub chosen: Vec<Candidate>,
    pub final_candidates: Vec<Candidate>,
}

/// The full v7 detector: v5's MobileSAM-anchored rectangles, v3, colour-capped v3 and
/// chroma proposals, all moved onto the photograph's own edge and scored by the
/// brightness step along their sides. A better-fitting alternative replaces an anchor,
/// strong stand-alone alternatives are added, mostly-contained duplicates are removed,
/// and blurry prints seen through album sleeves are dropped.
pub fn detect_v7(
    source: &RgbImage,
    options: &DetectOptions,
    sam: &mut MobileSam,
) -> Result<(Detection, V7Stages), SamError> {
    let mut detection = detect_v3(source, options);
    let v3 = v3_proposals(&detection, source, options);
    let capped = detect_v3_capped(source, options, Some(CAPPED_COLOR_THRESHOLD));
    let v3_capped = v3_proposals(&capped, source, options);
    let chroma = chroma_rectangles(source, options.min_area_ratio, options.max_area_ratio);
    let v5 = v5::rectangles(
        sam,
        source,
        &v3,
        options.min_area_ratio,
        options.max_area_ratio,
    )?;

    let (work, scale) = working_gray(source, WORK_MAX);
    let work = GrayF32::from_gray(&work);
    let smooth = blurred(&work, 1.0);
    let sharp = blurred(&work, 0.7);
    let (ghost_gray, ghost_scale) = working_gray(source, GHOST_SIDE);
    let ghosts = GhostMaps::new(&GrayF32::from_gray(&ghost_gray));

    let prepare = |rects: &[RotatedRect]| -> Vec<Candidate> {
        rects
            .iter()
            .map(|rect| {
                let tight = tighten_rect(&work, rect, scale, PASSES);
                let (low_std, fine) = ghosts.stats(&tight, ghost_scale);
                Candidate {
                    rect: tight,
                    score: score_rect(&smooth, &tight, scale),
                    sharp: edge_sharpness(&sharp, &tight, scale),
                    low_std,
                    fine,
                }
            })
            .collect()
    };
    let anchors = prepare(&v5);
    let mut alternatives = prepare(&v3);
    alternatives.extend(prepare(&v3_capped));
    alternatives.extend(prepare(&chroma));

    let chosen = select(&anchors, &alternatives);
    let area = source.width() as f64 * source.height() as f64;
    let in_range: Vec<Candidate> = chosen
        .iter()
        .copied()
        .filter(|c| {
            (options.min_area_ratio..=options.max_area_ratio).contains(&(c.rect.area() / area))
        })
        .collect();
    let final_candidates = drop_ghosts(in_range);

    let rects: Vec<RotatedRect> = final_candidates.iter().map(|c| c.rect).collect();
    detection.regions = regions_from_rects(&rects, source.width(), source.height(), options);
    let detection_scale = detection.stages.scale;
    detection.stages.tightened = rects.iter().map(|r| r.scaled(detection_scale)).collect();
    Ok((
        detection,
        V7Stages {
            v3,
            v3_capped,
            chroma,
            v5,
            anchors,
            alternatives,
            chosen,
            final_candidates,
        },
    ))
}

/// `cv2.GaussianBlur(gray, (0, 0), sigma)` on the working image.
fn blurred(gray: &GrayF32, sigma: f64) -> GrayF32 {
    GrayF32 {
        width: gray.width,
        height: gray.height,
        data: gaussian_blur_f32(&gray.data, gray.width, gray.height, sigma),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// White page with a print: a mid-grey border (130, clearly not page) `border` px thick around a darker picture.
    fn page_with_bordered_print(border: u32) -> RgbImage {
        let mut image = RgbImage::from_pixel(1000, 700, Rgb([250, 250, 250]));
        for y in 150..550 {
            for x in 200..700 {
                let inside =
                    x >= 200 + border && x < 700 - border && y >= 150 + border && y < 550 - border;
                let noise = ((x * 7 + y * 13) % 9) as u8;
                image.put_pixel(
                    x,
                    y,
                    if inside {
                        Rgb([30 + noise, 40, 70])
                    } else {
                        Rgb([130, 130, 130])
                    },
                );
            }
        }
        image
    }

    fn options() -> DetectOptions {
        DetectOptions {
            inset: 0,
            ..Default::default()
        }
    }

    #[test]
    fn v3_takes_in_the_border_and_v7_lite_trims_to_the_picture() {
        let image = page_with_bordered_print(16);
        let v3 = detect_v3(&image, &options());
        let lite = detect_v7_lite(&image, &options());
        assert_eq!((v3.regions.len(), lite.regions.len()), (1, 1));
        let (a, b) = (v3.regions[0].rect, lite.regions[0].rect);
        // v3 covers the whole 500 x 400 print; the picture is 468 x 368.
        assert!((a.w - 500.0).abs() < 6.0, "v3 width {}", a.w);
        assert!(
            (b.w - 468.0).abs() < 6.0 && (b.h - 368.0).abs() < 6.0,
            "v7-lite {b:?}"
        );
        assert!(
            (b.cx - 450.0).abs() < 3.0 && (b.cy - 350.0).abs() < 3.0,
            "{b:?}"
        );
        assert_eq!(lite.stages.tightened.len(), 1);
    }

    #[test]
    fn prints_without_a_border_are_left_as_v3_found_them() {
        let image = page_with_bordered_print(0);
        let v3 = detect_v3(&image, &options()).regions[0].rect;
        let lite = detect_v7_lite(&image, &options()).regions[0].rect;
        assert!(
            (v3.w - lite.w).abs() < 2.0 && (v3.h - lite.h).abs() < 2.0,
            "{v3:?} vs {lite:?}"
        );
    }

    #[test]
    fn trim_and_padding_still_apply_after_tightening() {
        let image = page_with_bordered_print(16);
        let plain = detect_v7_lite(&image, &options()).regions[0].rect;
        let trimmed = detect_v7_lite(
            &image,
            &DetectOptions {
                inset: 10,
                ..Default::default()
            },
        )
        .regions[0]
            .rect;
        assert!((plain.w - trimmed.w - 20.0).abs() < 1e-6);
    }

    #[test]
    fn a_blank_page_still_has_nothing() {
        let blank = RgbImage::from_pixel(800, 600, Rgb([250, 250, 250]));
        assert!(
            detect_v7_lite(&blank, &DetectOptions::default())
                .regions
                .is_empty()
        );
    }
}
