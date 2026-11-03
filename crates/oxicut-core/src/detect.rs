//! The v3 detector end to end: a port of `detect_photos_v3`.
//!
//! Model-free and background-aware. It models the paper/platen colour in Lab,
//! finds dense non-background regions, splits touching prints at narrow
//! gutters, and snaps each box to the long physical edges around it. It works
//! on a copy scaled to at most 1800 px and maps the boxes back to the scan.

use image::{GrayImage, RgbImage};

use crate::background::{LIGHT_LID_CHROMA_TOLERANCE, background_colors, background_colors_with};
use crate::candidates::candidate_rects;
use crate::color::{rgb_to_gray, rgb_to_lab};
use crate::foreground::{foreground_capped, paper_foreground};
use crate::geometry::RotatedRect;
use crate::mask::region_mask;
use crate::merge::{deduplicate, merge_fragments};
use crate::refine::refine_rect_to_edges;
use crate::resize::{DETECTION_MAX_DIMENSION, for_detection};
use crate::snap::snap_angle_to_long_edges;
use crate::split::{CARD_SPLIT_ASPECT, split_on_gutter, split_on_gutter_with};

/// What a box is cut around.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CutTo {
    /// The whole card or print, paper margins included. On a light lid with separate cards the
    /// detector first looks for the paper itself (see [`detect_cards`]); when the scan is not like
    /// that it falls back to the classic detector.
    Card,
    /// The picture: ScanSplitter's behaviour, which is what its benchmark measures.
    #[default]
    Picture,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectOptions {
    /// Whether a box follows the whole card or print, margins and all, or only the picture.
    pub cut_to: CutTo,
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
            cut_to: CutTo::Picture,
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
    /// This result comes from the card pass (see [`detect_cards`]), whose boxes follow the paper.
    pub card_pass: bool,
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
    if options.cut_to == CutTo::Card
        && color_cap.is_none()
        && let Some(detection) = detect_cards(source, options)
    {
        return detection;
    }
    run_v3(source, options, color_cap, false)
}

/// Find whole cards on a light, neutral scanner lid: the lid's colour is kept apart from cream and tan
/// paper that happens to touch the edge of the scan, pale paper counts as foreground, and stacked cards
/// are separated at their gutter. `None` when the result does not look like that (an album page, a dark
/// platen, one big blob), so the caller can use the classic detector instead.
pub fn detect_cards(source: &RgbImage, options: &DetectOptions) -> Option<Detection> {
    let detection = run_v3(source, options, None, true);
    looks_like_cards(&detection).then_some(detection)
}

/// How far (detection pixels, each side) the paper mask runs past a card's real edge.
const CARD_EDGE_SHRINK: f64 = 2.0;
/// How much of a card's rectangle must be foreground for the card pass to be believed.
const CARD_MIN_MEAN_FILL: f64 = 0.75;
const CARD_MIN_FILL: f64 = 0.35;
/// More of the scan than this being "not lid" means the lid is not what it seems (eight postcards
/// side by side cover about 70%).
const CARD_MAX_FOREGROUND_SHARE: f64 = 0.85;

/// Does a card-pass result look like separate cards on a light lid?
fn looks_like_cards(detection: &Detection) -> bool {
    let stages = &detection.stages;
    // A light, neutral lid (OpenCV 8-bit Lab: lightness 0 to 255, a and b around 128).
    let light_lid = stages
        .backgrounds_lab
        .iter()
        .any(|c| c[0] >= 215.0 && (c[1] - 128.0).abs() <= 10.0 && (c[2] - 128.0).abs() <= 12.0);
    if !light_lid || detection.regions.is_empty() || stages.split.is_empty() {
        return false;
    }
    let mask = &stages.foreground_mask;
    let lit = mask.pixels().filter(|p| p.0[0] > 0).count() as f64;
    if lit / (mask.width() as f64 * mask.height() as f64) > CARD_MAX_FOREGROUND_SHARE {
        return false;
    }
    let fills: Vec<f64> = stages.split.iter().map(|r| mask_fill(mask, r)).collect();
    let mean = fills.iter().sum::<f64>() / fills.len() as f64;
    mean >= CARD_MIN_MEAN_FILL && fills.iter().all(|&f| f >= CARD_MIN_FILL)
}

/// A rectangle shrunk to the foreground inside it: along each side, up to the first and last row or column
/// that holds at least a quarter as much foreground as the fullest one.
fn fit_to_mask(rect: &RotatedRect, foreground: &crate::foreground::Foreground) -> RotatedRect {
    let corners = rect.corners();
    let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for (x, y) in corners {
        x0 = x0.min(x);
        x1 = x1.max(x);
        y0 = y0.min(y);
        y1 = y1.max(y);
    }
    let (width, height) = (foreground.width, foreground.height);
    let (x0, x1) = (x0.floor().max(0.0) as usize, (x1.ceil().min(width as f64 - 1.0)).max(0.0) as usize);
    let (y0, y1) = (y0.floor().max(0.0) as usize, (y1.ceil().min(height as f64 - 1.0)).max(0.0) as usize);
    let theta = rect.angle.to_radians();
    let (c, s) = (theta.cos(), theta.sin());
    let (columns, rows) = (rect.w.ceil() as usize + 1, rect.h.ceil() as usize + 1);
    let (mut across, mut down) = (vec![0u32; columns], vec![0u32; rows]);
    for y in y0..=y1 {
        for x in x0..=x1 {
            if !foreground.mask[y * width + x] {
                continue;
            }
            let (dx, dy) = (x as f64 + 0.5 - rect.cx, y as f64 + 0.5 - rect.cy);
            let (u, v) = (dx * c + dy * s + rect.w / 2.0, -dx * s + dy * c + rect.h / 2.0);
            if u >= 0.0 && v >= 0.0 && (u as usize) < columns && (v as usize) < rows {
                across[u as usize] += 1;
                down[v as usize] += 1;
            }
        }
    }
    let extent = |bins: &[u32]| -> Option<(usize, usize)> {
        let most = *bins.iter().max()?;
        if most == 0 {
            return None;
        }
        let least = most.div_ceil(4);
        let first = bins.iter().position(|&n| n >= least)?;
        let last = bins.iter().rposition(|&n| n >= least)?;
        Some((first, last + 1))
    };
    let (Some((u0, u1)), Some((v0, v1))) = (extent(&across), extent(&down)) else {
        return *rect;
    };
    let (new_w, new_h) = ((u1 - u0) as f64, (v1 - v0) as f64);
    if new_w < 1.0 || new_h < 1.0 {
        return *rect;
    }
    // The new centre, back in scan coordinates.
    let (du, dv) = ((u0 + u1) as f64 / 2.0 - rect.w / 2.0, (v0 + v1) as f64 / 2.0 - rect.h / 2.0);
    RotatedRect::new(rect.cx + du * c - dv * s, rect.cy + du * s + dv * c, new_w, new_h, rect.angle)
}

/// The share of `rect` (in the mask's pixels) that is foreground.
fn mask_fill(mask: &GrayImage, rect: &RotatedRect) -> f64 {
    let corners = rect.corners();
    let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for (x, y) in corners {
        x0 = x0.min(x);
        x1 = x1.max(x);
        y0 = y0.min(y);
        y1 = y1.max(y);
    }
    let (width, height) = mask.dimensions();
    let (x0, x1) = (x0.floor().max(0.0) as u32, (x1.ceil().min(width as f64 - 1.0)).max(0.0) as u32);
    let (y0, y1) = (y0.floor().max(0.0) as u32, (y1.ceil().min(height as f64 - 1.0)).max(0.0) as u32);
    let theta = rect.angle.to_radians();
    let (c, s) = (theta.cos(), theta.sin());
    let (mut inside, mut lit) = (0u64, 0u64);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (dx, dy) = (x as f64 + 0.5 - rect.cx, y as f64 + 0.5 - rect.cy);
            let (u, v) = (dx * c + dy * s, -dx * s + dy * c);
            if u.abs() <= rect.w / 2.0 && v.abs() <= rect.h / 2.0 {
                inside += 1;
                if mask.get_pixel(x, y).0[0] > 0 {
                    lit += 1;
                }
            }
        }
    }
    if inside == 0 { 0.0 } else { lit as f64 / inside as f64 }
}

/// The v3 pipeline. `cards` switches on the card pass: the stricter lid colour, pale paper as foreground
/// and gutters looked for in blobs that are hardly elongated.
fn run_v3(source: &RgbImage, options: &DetectOptions, color_cap: Option<f32>, cards: bool) -> Detection {
    let (original_width, original_height) = source.dimensions();
    let working = for_detection(source, options.detection_max_dimension);
    let scale = working.scale;
    let small = &working.image;
    let (width, height) = small.dimensions();
    let total_area = width as f64 * height as f64;

    let lab = rgb_to_lab(small);
    let backgrounds = if cards {
        background_colors_with(&lab, LIGHT_LID_CHROMA_TOLERANCE)
    } else {
        background_colors(&lab)
    };
    let fg = if cards {
        paper_foreground(&lab, &backgrounds)
    } else {
        foreground_capped(&lab, &backgrounds, color_cap)
    };
    let mask = region_mask(&fg);

    let minimum_area = options.min_area_ratio * total_area;
    let maximum_area = options.max_area_ratio * total_area;
    let candidates = candidate_rects(&mask, options.min_area_ratio, options.max_area_ratio);
    let merged = merge_fragments(&candidates, maximum_area);
    let deduplicated = deduplicate(&merged);
    let split: Vec<RotatedRect> = deduplicated
        .iter()
        .flat_map(|rect| {
            if cards {
                split_on_gutter_with(rect, &fg, minimum_area, CARD_SPLIT_ASPECT)
            } else {
                split_on_gutter(rect, &fg, minimum_area)
            }
        })
        .collect();

    // A blob split at its gutter keeps the width of the widest card in it: the card pass fits each
    // piece to the paper that is really inside it.
    let split = if cards {
        split.iter().map(|rect| fit_to_mask(rect, &fg)).collect()
    } else {
        split
    };

    let gray = rgb_to_gray(small);
    let (gw, gh) = (gray.width() as usize, gray.height() as usize);
    let snapped: Vec<RotatedRect> = split
        .iter()
        .map(|rect| snap_angle_to_long_edges(gray.as_raw(), gw, gh, rect))
        .collect();
    // The card pass trusts the paper mask: its edge is the card's, where the strongest edge nearby may be
    // the printed picture inside it. The mask runs a little past the paper (smoothing, density), which is
    // taken back.
    let refined: Vec<RotatedRect> = if cards {
        snapped
            .iter()
            .map(|r| RotatedRect::new(r.cx, r.cy, (r.w - 2.0 * CARD_EDGE_SHRINK).max(1.0), (r.h - 2.0 * CARD_EDGE_SHRINK).max(1.0), r.angle))
            .collect()
    } else {
        snapped
            .iter()
            .map(|rect| refine_rect_to_edges(gray.as_raw(), gw, gh, rect, 0.12))
            .collect()
    };

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
            card_pass: cards,
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

    /// A white lid with six postcards, two columns of three, cream or tan paper, each with a coloured picture
    /// that leaves a margin of bare paper on its right. The left column runs close to the edge of the scan.
    fn postcards_on_a_white_lid() -> (RgbImage, Vec<RotatedRect>) {
        let (width, height) = (1240u32, 1240u32);
        let mut image = RgbImage::from_fn(width, height, |x, y| {
            let noise = ((x * 3 + y * 5) % 4) as u8;
            Rgb([243 - noise, 247 - noise, 248])
        });
        let papers = [[240u8, 232, 210], [232, 212, 176], [238, 226, 198]];
        let mut cards = Vec::new();
        for (column, x0) in [(0u32, 12u32), (1, 660)] {
            for row in 0..3u32 {
                let y0 = 40 + row * 400;
                let paper = papers[((column + row) % 3) as usize];
                for y in y0..y0 + 300 {
                    for x in x0..x0 + 520 {
                        let noise = ((x * 7 + y * 13) % 5) as u8;
                        image.put_pixel(x, y, Rgb([paper[0] - noise, paper[1] - noise, paper[2]]));
                    }
                }
                // The picture: strong colour, the card's right 140 px stay bare paper.
                for y in y0 + 20..y0 + 280 {
                    for x in x0 + 20..x0 + 380 {
                        let noise = ((x * 11 + y * 3) % 9) as u8;
                        image.put_pixel(x, y, Rgb([50 + noise, 90, 150 - noise]));
                    }
                }
                cards.push(RotatedRect::new(x0 as f64 + 260.0, y0 as f64 + 150.0, 520.0, 300.0, 0.0));
            }
        }
        (image, cards)
    }

    fn best_overlap(card: &RotatedRect, detection: &Detection) -> f64 {
        detection
            .regions
            .iter()
            .map(|r| crate::geometry::iou(card, &r.rect))
            .fold(0.0, f64::max)
    }

    #[test]
    fn cards_with_paper_margins_are_cut_whole_when_asked() {
        let (image, cards) = postcards_on_a_white_lid();
        let options = DetectOptions {
            inset: 0,
            cut_to: CutTo::Card,
            ..Default::default()
        };
        let detection = detect_v3(&image, &options);
        assert_eq!(detection.regions.len(), 6, "{:?}", detection.regions);
        for card in &cards {
            let overlap = best_overlap(card, &detection);
            assert!(overlap > 0.9, "card at {:?} overlaps only {overlap:.2}; found {:?}", (card.cx, card.cy), detection.regions.iter().map(|r| (r.rect.cx.round(), r.rect.cy.round(), r.rect.w.round(), r.rect.h.round(), r.rect.angle.round())).collect::<Vec<_>>());
        }
    }

    #[test]
    fn cutting_to_the_picture_leaves_the_margins_out() {
        let (image, cards) = postcards_on_a_white_lid();
        let detection = detect_v3(
            &image,
            &DetectOptions {
                inset: 0,
                cut_to: CutTo::Picture,
                ..Default::default()
            },
        );
        // ScanSplitter's behaviour: the coloured picture, not the card (140 of 520 px are bare paper).
        assert!(detection.regions.len() >= 3);
        for card in &cards {
            assert!(best_overlap(card, &detection) < 0.85);
        }
    }

    #[test]
    fn a_page_that_is_not_a_light_lid_gets_the_classic_detector_whatever_the_setting() {
        // An album page: cream all over, three photographs on it.
        let mut image = RgbImage::from_pixel(1000, 700, Rgb([226, 208, 172]));
        for &(x0, y0, w, h) in &[(80u32, 90u32, 360u32, 240u32), (520, 110, 360, 240), (300, 400, 360, 240)] {
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    let noise = ((x * 7 + y * 13) % 11) as u8;
                    image.put_pixel(x, y, Rgb([60 + noise, 85, 120]));
                }
            }
        }
        assert!(detect_cards(&image, &DetectOptions::default()).is_none());
        let card = detect_v3(&image, &DetectOptions { cut_to: CutTo::Card, ..Default::default() });
        let picture = detect_v3(&image, &DetectOptions { cut_to: CutTo::Picture, ..Default::default() });
        let boxes = |d: &Detection| d.regions.iter().map(|r| r.rect).collect::<Vec<_>>();
        assert_eq!(boxes(&card), boxes(&picture));
        assert_eq!(card.regions.len(), 3);
    }

    #[test]
    fn the_fill_of_a_rectangle_counts_only_its_own_pixels() {
        let mut mask = GrayImage::new(100, 60);
        for y in 10..50 {
            for x in 20..60 {
                mask.put_pixel(x, y, image::Luma([255]));
            }
        }
        // Exactly on the lit block, then twice as wide, then nowhere near it.
        assert!((mask_fill(&mask, &RotatedRect::new(40.0, 30.0, 40.0, 40.0, 0.0)) - 1.0).abs() < 1e-9);
        assert!((mask_fill(&mask, &RotatedRect::new(60.0, 30.0, 80.0, 40.0, 0.0)) - 0.5).abs() < 0.03);
        assert_eq!(mask_fill(&mask, &RotatedRect::new(90.0, 30.0, 10.0, 10.0, 0.0)), 0.0);
        // Turned a quarter, a square block is the same.
        assert!((mask_fill(&mask, &RotatedRect::new(40.0, 30.0, 40.0, 40.0, 90.0)) - 1.0).abs() < 0.02);
    }
}
