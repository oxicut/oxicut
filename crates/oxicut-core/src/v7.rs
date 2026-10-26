//! The model-free part of ScanSplitter's v7 detector, "v7-lite": v3's proposals,
//! each moved onto the photograph's own edge by inner-edge tightening.
//!
//! Full v7 also starts from MobileSAM-based v5 anchors, weighs alternative
//! proposals against each other and drops prints seen through glassine sleeves;
//! those parts arrive later. This is the step that stops boxes running onto
//! white borders and drop shadows.

use image::RgbImage;

use crate::detect::{DetectOptions, Detection, detect_v3, regions_from_rects};
use crate::geometry::RotatedRect;
use crate::pil::working_gray;
use crate::tighten::{GrayF32, tighten_rect};

/// Long side of the grey working image tightening reads (`_WORK_MAX`).
pub const WORK_MAX: u32 = 2000;
/// Tightening passes (`passes=2`).
const PASSES: usize = 2;

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
