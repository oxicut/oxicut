//! Reasons a scan needs a human look. Port of ScanSplitter's `confidence.py`
//! (`evaluate_scan`), which decides which scans are approved automatically.
//!
//! These are warnings, not errors: a flagged scan still exports, it just is not
//! approved until someone has looked at it.

use crate::geometry::RotatedRect;

/// A box touches an edge when a corner is within this share of the shorter side.
const EDGE_TOLERANCE_RATIO: f64 = 0.005;
/// Longest side over shortest side beyond which a box looks wrong.
const EXTREME_ASPECT_RATIO: f64 = 3.0;
/// With at least `AREA_OUTLIER_MIN_BOXES` boxes, one smaller than this share of
/// the median area is suspect.
const AREA_OUTLIER_RATIO: f64 = 0.35;
const AREA_OUTLIER_MIN_BOXES: usize = 3;
/// Bounding rectangles overlapping by more than this (IoU) are suspect.
const OVERLAP_IOU_THRESHOLD: f64 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagCode {
    NoBoxes,
    TouchesEdge,
    ExtremeAspect,
    AreaOutlier,
    Overlap,
}

impl FlagCode {
    /// Stable machine name, as ScanSplitter spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            FlagCode::NoBoxes => "no_boxes",
            FlagCode::TouchesEdge => "touches_edge",
            FlagCode::ExtremeAspect => "extreme_aspect",
            FlagCode::AreaOutlier => "area_outlier",
            FlagCode::Overlap => "overlap",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flag {
    pub code: FlagCode,
    /// The box the warning is about; `None` for scan-wide warnings.
    pub box_id: Option<u32>,
    pub message: String,
}

/// A box with the caller's own id, so warnings can point at it.
#[derive(Debug, Clone, Copy)]
pub struct IdentifiedBox {
    pub id: u32,
    pub rect: RotatedRect,
}

/// Corners as ScanSplitter computes them: centre plus rotated half extents.
fn corners(rect: &RotatedRect) -> [(f64, f64); 4] {
    let (half_w, half_h) = (rect.w / 2.0, rect.h / 2.0);
    let theta = rect.angle.to_radians();
    let (cos, sin) = (theta.cos(), theta.sin());
    [
        (-half_w, -half_h),
        (half_w, -half_h),
        (half_w, half_h),
        (-half_w, half_h),
    ]
    .map(|(x, y)| (rect.cx + x * cos - y * sin, rect.cy + x * sin + y * cos))
}

/// Axis-aligned bounds `(left, top, right, bottom)` of the rotated box.
fn bounds(rect: &RotatedRect) -> (f64, f64, f64, f64) {
    let c = corners(rect);
    let xs = c.map(|p| p.0);
    let ys = c.map(|p| p.1);
    (
        xs.into_iter().fold(f64::INFINITY, f64::min),
        ys.into_iter().fold(f64::INFINITY, f64::min),
        xs.into_iter().fold(f64::NEG_INFINITY, f64::max),
        ys.into_iter().fold(f64::NEG_INFINITY, f64::max),
    )
}

/// IoU of the two boxes' axis-aligned bounding rectangles (that is what
/// ScanSplitter measures, not the rotated overlap).
fn bounds_iou(a: &RotatedRect, b: &RotatedRect) -> f64 {
    let (al, at, ar, ab) = bounds(a);
    let (bl, bt, br, bb) = bounds(b);
    let width = (ar.min(br) - al.max(bl)).max(0.0);
    let height = (ab.min(bb) - at.max(bt)).max(0.0);
    let intersection = width * height;
    let area_a = (ar - al).max(0.0) * (ab - at).max(0.0);
    let area_b = (br - bl).max(0.0) * (bb - bt).max(0.0);
    let union = area_a + area_b - intersection;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// Every warning for the boxes found on an `image_width` x `image_height` scan.
pub fn evaluate_scan(boxes: &[IdentifiedBox], image_width: u32, image_height: u32) -> Vec<Flag> {
    let mut flags = Vec::new();
    if boxes.is_empty() {
        flags.push(Flag {
            code: FlagCode::NoBoxes,
            box_id: None,
            message: "No photos found".into(),
        });
    }

    let (width, height) = (image_width as f64, image_height as f64);
    let edge_tolerance = width.min(height) * EDGE_TOLERANCE_RATIO;
    for b in boxes {
        let c = corners(&b.rect);
        let touched = [
            ("left", c.iter().any(|p| p.0 <= edge_tolerance)),
            ("right", c.iter().any(|p| p.0 >= width - edge_tolerance)),
            ("top", c.iter().any(|p| p.1 <= edge_tolerance)),
            ("bottom", c.iter().any(|p| p.1 >= height - edge_tolerance)),
        ];
        for (edge, hit) in touched {
            if hit {
                flags.push(Flag {
                    code: FlagCode::TouchesEdge,
                    box_id: Some(b.id),
                    message: format!("Box touches the {edge} edge"),
                });
            }
        }

        let shorter = b.rect.w.min(b.rect.h);
        let aspect = if shorter > 0.0 {
            b.rect.w.max(b.rect.h) / shorter
        } else {
            f64::INFINITY
        };
        if aspect > EXTREME_ASPECT_RATIO {
            flags.push(Flag {
                code: FlagCode::ExtremeAspect,
                box_id: Some(b.id),
                message: format!("Box has an extreme {aspect:.1}:1 aspect ratio"),
            });
        }
    }

    if boxes.len() >= AREA_OUTLIER_MIN_BOXES {
        let areas: Vec<f64> = boxes.iter().map(|b| b.rect.w * b.rect.h).collect();
        let median_area = median(&mut areas.clone());
        for (b, area) in boxes.iter().zip(&areas) {
            if *area < median_area * AREA_OUTLIER_RATIO {
                flags.push(Flag {
                    code: FlagCode::AreaOutlier,
                    box_id: Some(b.id),
                    message: "Box is much smaller than the others".into(),
                });
            }
        }
    }

    for (i, first) in boxes.iter().enumerate() {
        for second in &boxes[i + 1..] {
            let iou = bounds_iou(&first.rect, &second.rect);
            if iou > OVERLAP_IOU_THRESHOLD {
                flags.push(Flag {
                    code: FlagCode::Overlap,
                    box_id: Some(first.id),
                    message: format!(
                        "Box overlaps box {} ({:.0}% overlap)",
                        second.id,
                        iou * 100.0
                    ),
                });
            }
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(id: u32, x: f64, y: f64, w: f64, h: f64, angle: f64) -> IdentifiedBox {
        IdentifiedBox {
            id,
            rect: RotatedRect::new(x, y, w, h, angle),
        }
    }

    fn summary(flags: &[Flag]) -> Vec<(&'static str, Option<u32>)> {
        flags.iter().map(|f| (f.code.as_str(), f.box_id)).collect()
    }

    // The expectations below were produced by running ScanSplitter's
    // `evaluate_scan` on the same boxes.

    #[test]
    fn an_empty_scan_is_flagged() {
        let flags = evaluate_scan(&[], 1000, 800);
        assert_eq!(summary(&flags), vec![("no_boxes", None)]);
        assert_eq!(flags[0].message, "No photos found");
    }

    #[test]
    fn two_clean_boxes_raise_nothing() {
        let boxes = [
            boxed(1, 300.0, 300.0, 300.0, 200.0, 0.0),
            boxed(2, 700.0, 500.0, 300.0, 200.0, 5.0),
        ];
        assert!(evaluate_scan(&boxes, 1000, 800).is_empty());
    }

    #[test]
    fn edges_aspect_and_small_outliers_are_reported_in_python_order() {
        let boxes = [
            boxed(1, 150.0, 400.0, 300.0, 200.0, 0.0),
            boxed(2, 600.0, 200.0, 900.0, 100.0, 0.0),
            boxed(3, 700.0, 600.0, 400.0, 300.0, 0.0),
            boxed(4, 400.0, 700.0, 60.0, 50.0, 0.0),
            boxed(5, 850.0, 400.0, 250.0, 250.0, 0.0),
        ];
        let flags = evaluate_scan(&boxes, 1000, 800);
        assert_eq!(
            summary(&flags),
            vec![
                ("touches_edge", Some(1)),
                ("touches_edge", Some(2)),
                ("extreme_aspect", Some(2)),
                ("area_outlier", Some(4))
            ]
        );
        assert_eq!(flags[0].message, "Box touches the left edge");
        assert_eq!(flags[1].message, "Box touches the right edge");
        assert_eq!(flags[2].message, "Box has an extreme 9.0:1 aspect ratio");
    }

    #[test]
    fn overlapping_boxes_report_the_percentage() {
        let boxes = [
            boxed(1, 300.0, 300.0, 300.0, 200.0, 0.0),
            boxed(2, 400.0, 350.0, 300.0, 200.0, 0.0),
        ];
        let flags = evaluate_scan(&boxes, 1000, 800);
        assert_eq!(summary(&flags), vec![("overlap", Some(1))]);
        assert_eq!(flags[0].message, "Box overlaps box 2 (33% overlap)");
    }

    #[test]
    fn rotation_is_taken_into_account_and_just_below_the_threshold_is_quiet() {
        let rotated = [
            boxed(1, 500.0, 400.0, 400.0, 200.0, 30.0),
            boxed(2, 120.0, 100.0, 200.0, 140.0, -10.0),
        ];
        assert!(evaluate_scan(&rotated, 1000, 800).is_empty());
        let near = [
            boxed(1, 300.0, 300.0, 300.0, 200.0, 0.0),
            boxed(2, 585.0, 300.0, 300.0, 200.0, 0.0),
        ];
        assert!(evaluate_scan(&near, 1000, 800).is_empty());
    }

    #[test]
    fn a_zero_height_box_counts_as_extreme() {
        let flags = evaluate_scan(&[boxed(7, 500.0, 400.0, 100.0, 0.0, 0.0)], 1000, 800);
        assert!(
            flags
                .iter()
                .any(|f| f.code == FlagCode::ExtremeAspect && f.box_id == Some(7))
        );
    }

    #[test]
    fn outliers_need_at_least_three_boxes() {
        let two = [
            boxed(1, 300.0, 300.0, 300.0, 200.0, 0.0),
            boxed(2, 700.0, 500.0, 40.0, 30.0, 0.0),
        ];
        assert!(
            evaluate_scan(&two, 1000, 800)
                .iter()
                .all(|f| f.code != FlagCode::AreaOutlier)
        );
    }
}
