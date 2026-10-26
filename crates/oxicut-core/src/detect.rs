//! The v3 detector end to end: a port of `detect_photos_v3`.
//!
//! Model-free and background-aware. It models the paper/platen colour in Lab,
//! finds dense non-background regions, splits touching prints at narrow
//! gutters, and snaps each box to the long physical edges around it. It works
//! on a copy scaled to at most 1800 px and maps the boxes back to the scan.

use image::{GrayImage, RgbImage};

use crate::background::background_colors;
use crate::candidates::candidate_rects;
use crate::color::{rgb_to_gray, rgb_to_lab};
use crate::foreground::foreground_capped;
use crate::geometry::RotatedRect;
use crate::mask::region_mask;
use crate::merge::{deduplicate, merge_fragments};
use crate::refine::refine_rect_to_edges;
use crate::resize::{DETECTION_MAX_DIMENSION, for_detection};
use crate::snap::snap_angle_to_long_edges;
use crate::split::split_on_gutter;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectOptions {
    /// Smallest accepted item, as a share of the scan's area.
    pub min_area_ratio: f64,
    /// Largest accepted item, as a share of the scan's area.
    pub max_area_ratio: f64,
    /// Pixels added around every box.
    pub padding: i32,
    /// Pixels removed from every side (v3's default trims the soft print edge).
    pub inset: i32,
    /// Long side of the working copy.
    pub detection_max_dimension: u32,
}

impl Default for DetectOptions {
    fn default() -> Self {
        Self {
            min_area_ratio: 0.02,
            max_area_ratio: 0.80,
            padding: 0,
            inset: 10,
            detection_max_dimension: DETECTION_MAX_DIMENSION,
        }
    }
}

/// One detected print, in source pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    /// Centre, size and angle after padding/inset; `w >= h`.
    pub rect: RotatedRect,
    /// Area of the unpadded box in source pixels.
    pub area: f64,
    /// `area` over the scan's area.
    pub area_ratio: f64,
    /// Padded axis-aligned bounding box `(x, y, width, height)`, clipped to the scan.
    pub bbox: (i64, i64, i64, i64),
}

/// Every intermediate result, in detection-image pixels, for debugging and
/// for checking the port against ScanSplitter stage by stage.
pub struct Stages {
    pub detection_size: (u32, u32),
    /// Maps detection pixels to source pixels: `source = detection / scale`.
    pub scale: f64,
    pub backgrounds_lab: Vec<[f32; 3]>,
    pub otsu: u8,
    pub foreground_mask: GrayImage,
    pub region_mask: GrayImage,
    pub candidates: Vec<RotatedRect>,
    pub merged: Vec<RotatedRect>,
    pub deduplicated: Vec<RotatedRect>,
    pub split: Vec<RotatedRect>,
    pub snapped: Vec<RotatedRect>,
    pub refined: Vec<RotatedRect>,
    /// After v7's inner-edge tightening (empty for plain v3).
    pub tightened: Vec<RotatedRect>,
}

pub struct Detection {
    pub regions: Vec<Region>,
    pub stages: Stages,
}

/// Turn rectangles in full-resolution pixels into regions: drop those outside
/// the area limits, apply the trim/padding, keep `w >= h`, and order them as
/// one reads the page (rows of about 100 px, left to right). Shared by every
/// detector; this is `_v5_regions_from_rectangles` and the end of `detect_photos_v3`.
pub fn regions_from_rects(
    rects: &[RotatedRect],
    original_width: u32,
    original_height: u32,
    options: &DetectOptions,
) -> Vec<Region> {
    let original_area = original_width as f64 * original_height as f64;
    let net_adjust = (options.padding - options.inset) as i64;
    let mut regions: Vec<Region> = rects
        .iter()
        .filter_map(|rect| {
            let rect = *rect;
            let area = rect.w * rect.h;
            let area_ratio = area / original_area;
            if !(options.min_area_ratio..=options.max_area_ratio).contains(&area_ratio) {
                return None;
            }

            let (bx, by, box_width, box_height) = rect.bounding_rect();
            let x = (bx - net_adjust).max(0);
            let y = (by - net_adjust).max(0);
            let bbox_width = (original_width as i64 - x)
                .min(box_width + 2 * net_adjust)
                .max(1);
            let bbox_height = (original_height as i64 - y)
                .min(box_height + 2 * net_adjust)
                .max(1);

            let mut padded_width = (rect.w + 2.0 * net_adjust as f64).max(1.0);
            let mut padded_height = (rect.h + 2.0 * net_adjust as f64).max(1.0);
            let mut angle = rect.angle;
            if rect.w < rect.h {
                std::mem::swap(&mut padded_width, &mut padded_height);
                angle += 90.0;
            }
            Some(Region {
                rect: RotatedRect::new(rect.cx, rect.cy, padded_width, padded_height, angle),
                area,
                area_ratio,
                bbox: (x, y, bbox_width, bbox_height),
            })
        })
        .collect();
    regions.sort_by_key(|r| (r.bbox.1.div_euclid(100), r.bbox.0));
    regions
}
pub fn detect_v3(source: &RgbImage, options: &DetectOptions) -> Detection {
    detect_v3_capped(source, options, None)
}

/// [`detect_v3`] with an optional cap on the colour threshold (v7's second proposal set).
pub fn detect_v3_capped(
    source: &RgbImage,
    options: &DetectOptions,
    color_cap: Option<f32>,
) -> Detection {
    let (original_width, original_height) = source.dimensions();
    let working = for_detection(source, options.detection_max_dimension);
    let scale = working.scale;
    let small = &working.image;
    let (width, height) = small.dimensions();
    let total_area = width as f64 * height as f64;

    let lab = rgb_to_lab(small);
    let backgrounds = background_colors(&lab);
    let fg = foreground_capped(&lab, &backgrounds, color_cap);
    let mask = region_mask(&fg);

    let minimum_area = options.min_area_ratio * total_area;
    let maximum_area = options.max_area_ratio * total_area;
    let candidates = candidate_rects(&mask, options.min_area_ratio, options.max_area_ratio);
    let merged = merge_fragments(&candidates, maximum_area);
    let deduplicated = deduplicate(&merged);
    let split: Vec<RotatedRect> = deduplicated
        .iter()
        .flat_map(|rect| split_on_gutter(rect, &fg, minimum_area))
        .collect();

    let gray = rgb_to_gray(small);
    let (gw, gh) = (gray.width() as usize, gray.height() as usize);
    let snapped: Vec<RotatedRect> = split
        .iter()
        .map(|rect| snap_angle_to_long_edges(gray.as_raw(), gw, gh, rect))
        .collect();
    let refined: Vec<RotatedRect> = snapped
        .iter()
        .map(|rect| refine_rect_to_edges(gray.as_raw(), gw, gh, rect, 0.12))
        .collect();

    let full_size: Vec<RotatedRect> = refined.iter().map(|r| r.scaled(1.0 / scale)).collect();
    let regions = regions_from_rects(&full_size, original_width, original_height, options);
    Detection {
        regions,
        stages: Stages {
            detection_size: (width, height),
            scale,
            backgrounds_lab: backgrounds,
            otsu: fg.otsu,
            foreground_mask: GrayImage::from_raw(
                fg.width as u32,
                fg.height as u32,
                fg.mask.iter().map(|&b| if b { 255 } else { 0 }).collect(),
            )
            .expect("same dimensions"),
            region_mask: mask,
            candidates,
            merged,
            deduplicated,
            split,
            snapped,
            refined,
            tightened: Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// A white page with filled, axis-aligned coloured prints `(x, y, w, h, colour)`.
    fn page(width: u32, height: u32, prints: &[(u32, u32, u32, u32, [u8; 3])]) -> RgbImage {
        let mut image = RgbImage::from_pixel(width, height, Rgb([248, 248, 246]));
        for &(x0, y0, w, h, colour) in prints {
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    // A little texture so edges are not perfectly flat.
                    let noise = ((x * 7 + y * 13) % 11) as u8;
                    image.put_pixel(
                        x,
                        y,
                        Rgb([colour[0].saturating_add(noise), colour[1], colour[2]]),
                    );
                }
            }
        }
        image
    }

    #[test]
    fn finds_two_separate_prints() {
        let image = page(
            1000,
            700,
            &[
                (100, 120, 360, 460, [70, 90, 130]),
                (560, 150, 340, 400, [150, 60, 50]),
            ],
        );
        let detection = detect_v3(
            &image,
            &DetectOptions {
                inset: 0,
                ..Default::default()
            },
        );
        assert_eq!(detection.regions.len(), 2, "{:?}", detection.regions);
        let (first, second) = (&detection.regions[0].rect, &detection.regions[1].rect);
        assert!(
            (first.cx - 280.0).abs() < 3.0 && (first.cy - 350.0).abs() < 3.0,
            "{first:?}"
        );
        assert!(
            (second.cx - 730.0).abs() < 3.0 && (second.cy - 350.0).abs() < 3.0,
            "{second:?}"
        );
        // Width >= height after normalisation: the first print is portrait.
        assert!(
            (first.w - 460.0).abs() < 4.0 && (first.h - 360.0).abs() < 4.0,
            "{first:?}"
        );
    }

    #[test]
    fn inset_shrinks_and_padding_grows_each_side() {
        let image = page(1000, 700, &[(200, 150, 500, 400, [60, 80, 120])]);
        let plain = detect_v3(
            &image,
            &DetectOptions {
                inset: 0,
                ..Default::default()
            },
        );
        let trimmed = detect_v3(
            &image,
            &DetectOptions {
                inset: 10,
                ..Default::default()
            },
        );
        let padded = detect_v3(
            &image,
            &DetectOptions {
                inset: 0,
                padding: 5,
                ..Default::default()
            },
        );
        let (a, b, c) = (
            plain.regions[0].rect,
            trimmed.regions[0].rect,
            padded.regions[0].rect,
        );
        assert!((a.w - b.w - 20.0).abs() < 1e-9 && (a.h - b.h - 20.0).abs() < 1e-9);
        assert!((c.w - a.w - 10.0).abs() < 1e-9 && (c.h - a.h - 10.0).abs() < 1e-9);
    }

    #[test]
    fn blank_page_has_no_regions() {
        let image = RgbImage::from_pixel(800, 600, Rgb([250, 250, 250]));
        assert!(
            detect_v3(&image, &DetectOptions::default())
                .regions
                .is_empty()
        );
    }

    #[test]
    fn tiny_marks_below_the_minimum_area_are_ignored() {
        let image = page(1000, 700, &[(400, 300, 30, 30, [20, 20, 20])]);
        assert!(
            detect_v3(&image, &DetectOptions::default())
                .regions
                .is_empty()
        );
    }

    #[test]
    fn regions_are_listed_in_reading_order() {
        let image = page(
            1200,
            900,
            &[
                (700, 60, 400, 330, [60, 90, 120]),
                (80, 70, 420, 320, [120, 80, 60]),
                (300, 500, 500, 330, [80, 120, 70]),
            ],
        );
        let detection = detect_v3(
            &image,
            &DetectOptions {
                inset: 0,
                ..Default::default()
            },
        );
        assert_eq!(detection.regions.len(), 3);
        let xs: Vec<f64> = detection.regions.iter().map(|r| r.rect.cx).collect();
        assert!(xs[0] < xs[1], "top row left to right: {xs:?}");
        assert!(detection.regions[2].rect.cy > detection.regions[1].rect.cy);
    }

    #[test]
    fn large_scans_are_detected_at_working_size_and_mapped_back() {
        let image = page(3600, 2400, &[(600, 400, 1500, 1200, [60, 90, 130])]);
        let detection = detect_v3(
            &image,
            &DetectOptions {
                inset: 0,
                ..Default::default()
            },
        );
        assert!((detection.stages.scale - 0.5).abs() < 1e-12);
        let region = detection.regions[0].rect;
        assert!(
            (region.cx - 1350.0).abs() < 6.0 && (region.cy - 1000.0).abs() < 6.0,
            "{region:?}"
        );
        assert!(
            (region.w - 1500.0).abs() < 8.0 && (region.h - 1200.0).abs() < 8.0,
            "{region:?}"
        );
    }
}
