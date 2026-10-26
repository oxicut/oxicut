//! v3 fragment merging and de-duplication of candidate rectangles.

use crate::geometry::{RotatedRect, iou, min_area_rect, overlap_fraction};

/// Two fragments overlapping by at least this share of the smaller one are
/// treated as pieces of the same print.
const MERGE_OVERLAP: f64 = 0.08;
/// Rectangles overlapping more than this (IoU) are duplicates.
const DUPLICATE_IOU: f64 = 0.55;

/// Merge overlapping mask fragments belonging to one physical print
/// (`_v3_merge_fragments`): the first overlapping pair is replaced by the
/// minimum-area rectangle around both, unless that exceeds `max_area`; repeat
/// until nothing changes.
pub fn merge_fragments(rectangles: &[RotatedRect], max_area: f64) -> Vec<RotatedRect> {
    let mut merged = rectangles.to_vec();
    loop {
        let mut changed = false;
        'search: for first in 0..merged.len() {
            for second in first + 1..merged.len() {
                if overlap_fraction(&merged[first], &merged[second]) < MERGE_OVERLAP {
                    continue;
                }
                let points: Vec<(f64, f64)> = merged[first]
                    .corners()
                    .into_iter()
                    .chain(merged[second].corners())
                    .collect();
                let union = min_area_rect(&points);
                if union.area() > max_area {
                    continue;
                }
                merged[first] = union;
                merged.remove(second);
                changed = true;
                break 'search;
            }
        }
        if !changed {
            return merged;
        }
    }
}

/// Largest first; a rectangle is dropped when it largely coincides with a
/// larger one already kept.
pub fn deduplicate(rectangles: &[RotatedRect]) -> Vec<RotatedRect> {
    let mut by_area = rectangles.to_vec();
    // Stable sort, like Python's `sorted(..., reverse=True)`: ties keep their order.
    by_area.sort_by(|a, b| b.area().total_cmp(&a.area()));
    let mut kept: Vec<RotatedRect> = Vec::new();
    for rect in by_area {
        if kept.iter().any(|other| iou(&rect, other) > DUPLICATE_IOU) {
            continue;
        }
        kept.push(rect);
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(cx: f64, cy: f64, w: f64, h: f64, angle: f64) -> RotatedRect {
        RotatedRect::new(cx, cy, w, h, angle)
    }

    #[test]
    fn merges_overlapping_halves_into_one_print() {
        let left = rect(50.0, 50.0, 60.0, 80.0, 0.0);
        let right = rect(100.0, 50.0, 60.0, 80.0, 0.0);
        let merged = merge_fragments(&[left, right], 1.0e6);
        assert_eq!(merged.len(), 1);
        let union = merged[0].canonical();
        assert!((union.w - 110.0).abs() < 1e-6 && (union.h - 80.0).abs() < 1e-6);
    }

    #[test]
    fn disjoint_rectangles_stay_separate() {
        let a = rect(50.0, 50.0, 40.0, 40.0, 0.0);
        let b = rect(200.0, 50.0, 40.0, 40.0, 0.0);
        assert_eq!(merge_fragments(&[a, b], 1.0e6).len(), 2);
    }

    #[test]
    fn merge_is_refused_when_the_union_is_too_big() {
        let a = rect(50.0, 50.0, 60.0, 80.0, 0.0);
        let b = rect(100.0, 50.0, 60.0, 80.0, 0.0);
        assert_eq!(merge_fragments(&[a, b], 8000.0).len(), 2);
    }

    #[test]
    fn chains_of_fragments_merge_to_a_fixpoint() {
        let parts = [
            rect(40.0, 50.0, 50.0, 60.0, 0.0),
            rect(80.0, 50.0, 50.0, 60.0, 0.0),
            rect(120.0, 50.0, 50.0, 60.0, 0.0),
        ];
        assert_eq!(merge_fragments(&parts, 1.0e6).len(), 1);
    }

    #[test]
    fn dedupe_keeps_the_larger_of_two_near_copies() {
        let big = rect(100.0, 100.0, 100.0, 80.0, 0.0);
        let near_copy = rect(102.0, 101.0, 96.0, 78.0, 0.0);
        let elsewhere = rect(400.0, 100.0, 50.0, 50.0, 0.0);
        let kept = deduplicate(&[near_copy, elsewhere, big]);
        assert_eq!(kept, vec![big, elsewhere]);
    }

    #[test]
    fn dedupe_keeps_partial_overlaps_below_the_threshold() {
        let a = rect(100.0, 100.0, 100.0, 100.0, 0.0);
        let b = rect(160.0, 100.0, 100.0, 100.0, 0.0); // IoU = 40 / 160 = 0.25
        assert_eq!(deduplicate(&[a, b]).len(), 2);
    }
}
