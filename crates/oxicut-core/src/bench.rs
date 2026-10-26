//! Detector benchmark scoring: a port of ScanSplitter's `benchmark_metrics.py`
//! and the aggregation in `benchmarks/evaluate.py`, so results are comparable
//! with the numbers ScanSplitter publishes.
//!
//! Detected boxes are matched one-to-one to ground-truth boxes (Hungarian
//! assignment on rotated IoU). *Box quality* is an IoU-weighted F1: matched
//! boxes count for their overlap, missing and extra boxes for zero, so loose
//! boxes cannot score perfectly.

use crate::geometry::{RotatedRect, intersection_area};

/// IoU at which a match counts as a detection (`F1@0.50`).
pub const DETECTION_THRESHOLD: f64 = 0.5;
/// Stricter IoU for `F1@0.85`.
pub const STRICT_THRESHOLD: f64 = 0.85;

/// Intersection area, bounded by both areas (`_intersection_area`).
fn bounded_intersection(a: &RotatedRect, b: &RotatedRect) -> f64 {
    intersection_area(a, b).min(a.area()).min(b.area())
}

pub fn rotated_iou(a: &RotatedRect, b: &RotatedRect) -> f64 {
    let intersection = bounded_intersection(a, b);
    if intersection == 0.0 {
        return 0.0;
    }
    intersection / (a.area() + b.area() - intersection).max(1.0)
}

/// Hungarian algorithm for an `n x m` cost matrix with `n <= m`; returns the
/// column assigned to each row.
fn assign_min_cost(cost: &[Vec<f64>]) -> Vec<usize> {
    let (n, m) = (cost.len(), cost[0].len());
    let inf = f64::INFINITY;
    let mut u = vec![0.0; n + 1];
    let mut v = vec![0.0; m + 1];
    let mut owner = vec![0usize; m + 1]; // owner[j] = row (1-based) assigned to column j
    let mut way = vec![0usize; m + 1];
    for row in 1..=n {
        owner[0] = row;
        let mut j0 = 0;
        let mut minv = vec![inf; m + 1];
        let mut used = vec![false; m + 1];
        loop {
            used[j0] = true;
            let (i0, mut delta, mut j1) = (owner[j0], inf, 0);
            for j in 1..=m {
                if used[j] {
                    continue;
                }
                let current = cost[i0 - 1][j - 1] - u[i0] - v[j];
                if current < minv[j] {
                    minv[j] = current;
                    way[j] = j0;
                }
                if minv[j] < delta {
                    delta = minv[j];
                    j1 = j;
                }
            }
            for j in 0..=m {
                if used[j] {
                    u[owner[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if owner[j0] == 0 {
                break;
            }
        }
        while j0 != 0 {
            let j1 = way[j0];
            owner[j0] = owner[j1];
            j0 = j1;
        }
    }
    let mut result = vec![0; n];
    for j in 1..=m {
        if owner[j] != 0 {
            result[owner[j] - 1] = j - 1;
        }
    }
    result
}

/// The one-to-one assignment with the largest total IoU, as
/// `(expected index, actual index, iou)`.
pub fn best_pairs(expected: &[RotatedRect], actual: &[RotatedRect]) -> Vec<(usize, usize, f64)> {
    if expected.is_empty() || actual.is_empty() {
        return Vec::new();
    }
    let iou: Vec<Vec<f64>> = expected
        .iter()
        .map(|e| actual.iter().map(|a| rotated_iou(e, a)).collect())
        .collect();
    if expected.len() <= actual.len() {
        let cost: Vec<Vec<f64>> = iou
            .iter()
            .map(|row| row.iter().map(|v| -v).collect())
            .collect();
        assign_min_cost(&cost)
            .into_iter()
            .enumerate()
            .map(|(i, j)| (i, j, iou[i][j]))
            .collect()
    } else {
        let cost: Vec<Vec<f64>> = (0..actual.len())
            .map(|j| (0..expected.len()).map(|i| -iou[i][j]).collect())
            .collect();
        assign_min_cost(&cost)
            .into_iter()
            .enumerate()
            .map(|(j, i)| (i, j, iou[i][j]))
            .collect()
    }
}

/// How well one scan's boxes match its ground truth.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseScore {
    pub expected: usize,
    pub detected: usize,
    pub true_positive: usize,
    pub false_positive: usize,
    pub false_negative: usize,
    pub strict_true_positive: usize,
    pub strict_false_positive: usize,
    pub strict_false_negative: usize,
    pub iou_sum: f64,
    pub intersection_area_sum: f64,
    pub expected_area_sum: f64,
    pub actual_area_sum: f64,
    pub assigned_count: usize,
    pub count_correct: bool,
    pub runtime_seconds: f64,
}

impl CaseScore {
    /// IoU-weighted F1 for this one scan.
    pub fn box_quality(&self) -> f64 {
        let denominator = (self.expected + self.detected) as f64;
        if denominator == 0.0 {
            1.0
        } else {
            2.0 * self.iou_sum / denominator
        }
    }

    fn f1(tp: usize, fp: usize, fneg: usize, expected: usize, detected: usize) -> f64 {
        let precision = if detected > 0 {
            tp as f64 / detected as f64
        } else if expected == 0 {
            1.0
        } else {
            0.0
        };
        let recall = if expected > 0 {
            tp as f64 / expected as f64
        } else if detected == 0 {
            1.0
        } else {
            0.0
        };
        let _ = (fp, fneg);
        if precision + recall > 0.0 {
            2.0 * precision * recall / (precision + recall)
        } else {
            0.0
        }
    }

    pub fn f1_at_detection(&self) -> f64 {
        Self::f1(
            self.true_positive,
            self.false_positive,
            self.false_negative,
            self.expected,
            self.detected,
        )
    }

    pub fn f1_at_strict(&self) -> f64 {
        Self::f1(
            self.strict_true_positive,
            self.strict_false_positive,
            self.strict_false_negative,
            self.expected,
            self.detected,
        )
    }

    /// Share of this scan's crop that is wanted photograph.
    pub fn tightness(&self) -> f64 {
        if self.actual_area_sum > 0.0 {
            self.intersection_area_sum / self.actual_area_sum
        } else if self.expected_area_sum == 0.0 {
            1.0
        } else {
            0.0
        }
    }

    /// Share of the wanted photograph the crops keep.
    pub fn coverage(&self) -> f64 {
        if self.expected_area_sum > 0.0 {
            self.intersection_area_sum / self.expected_area_sum
        } else if self.actual_area_sum == 0.0 {
            1.0
        } else {
            0.0
        }
    }
}

/// Score `actual` boxes against the `expected` ones (`score_rectangles`).
pub fn score_rectangles(expected: &[RotatedRect], actual: &[RotatedRect]) -> CaseScore {
    let assigned = best_pairs(expected, actual);
    let tp = assigned
        .iter()
        .filter(|p| p.2 >= DETECTION_THRESHOLD)
        .count();
    let strict_tp = assigned.iter().filter(|p| p.2 >= STRICT_THRESHOLD).count();
    CaseScore {
        expected: expected.len(),
        detected: actual.len(),
        true_positive: tp,
        false_positive: actual.len() - tp,
        false_negative: expected.len() - tp,
        strict_true_positive: strict_tp,
        strict_false_positive: actual.len() - strict_tp,
        strict_false_negative: expected.len() - strict_tp,
        iou_sum: assigned.iter().map(|p| p.2).sum(),
        intersection_area_sum: assigned
            .iter()
            .map(|&(e, a, _)| bounded_intersection(&expected[e], &actual[a]))
            .sum(),
        expected_area_sum: expected.iter().map(RotatedRect::area).sum(),
        actual_area_sum: actual.iter().map(RotatedRect::area).sum(),
        assigned_count: assigned.len(),
        count_correct: expected.len() == actual.len(),
        runtime_seconds: 0.0,
    }
}

/// Aggregate over many scans (`summarize`).
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub cases: usize,
    pub expected_regions: usize,
    pub detected_regions: usize,
    pub box_quality: f64,
    pub crop_tightness: f64,
    pub content_coverage: f64,
    pub f1: f64,
    pub strict_f1: f64,
    pub mean_iou: f64,
    pub count_accuracy: f64,
    pub runtime_seconds: f64,
}

pub fn summarize(results: &[CaseScore]) -> Summary {
    let sum = |f: &dyn Fn(&CaseScore) -> usize| results.iter().map(f).sum::<usize>();
    let sumf = |f: &dyn Fn(&CaseScore) -> f64| results.iter().map(f).sum::<f64>();
    let pooled_f1 = |tp: usize, fp: usize, fneg: usize| {
        let precision = tp as f64 / (tp + fp).max(1) as f64;
        let recall = tp as f64 / (tp + fneg).max(1) as f64;
        if precision + recall > 0.0 {
            2.0 * precision * recall / (precision + recall)
        } else {
            0.0
        }
    };
    let expected = sum(&|c| c.expected);
    let detected = sum(&|c| c.detected);
    let (iou_sum, inter, expected_area, actual_area) = (
        sumf(&|c| c.iou_sum),
        sumf(&|c| c.intersection_area_sum),
        sumf(&|c| c.expected_area_sum),
        sumf(&|c| c.actual_area_sum),
    );
    let assigned = sum(&|c| c.assigned_count);
    Summary {
        cases: results.len(),
        expected_regions: expected,
        detected_regions: detected,
        box_quality: if expected + detected > 0 {
            2.0 * iou_sum / (expected + detected) as f64
        } else {
            1.0
        },
        crop_tightness: if actual_area > 0.0 {
            inter / actual_area
        } else if expected_area == 0.0 {
            1.0
        } else {
            0.0
        },
        content_coverage: if expected_area > 0.0 {
            inter / expected_area
        } else if actual_area == 0.0 {
            1.0
        } else {
            0.0
        },
        f1: pooled_f1(
            sum(&|c| c.true_positive),
            sum(&|c| c.false_positive),
            sum(&|c| c.false_negative),
        ),
        strict_f1: pooled_f1(
            sum(&|c| c.strict_true_positive),
            sum(&|c| c.strict_false_positive),
            sum(&|c| c.strict_false_negative),
        ),
        mean_iou: if assigned > 0 {
            iou_sum / assigned as f64
        } else {
            0.0
        },
        count_accuracy: results.iter().filter(|c| c.count_correct).count() as f64
            / results.len().max(1) as f64,
        runtime_seconds: sumf(&|c| c.runtime_seconds),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(cx: f64, cy: f64, w: f64, h: f64, angle: f64) -> RotatedRect {
        RotatedRect::new(cx, cy, w, h, angle)
    }

    #[test]
    fn iou_of_identical_shifted_and_disjoint_boxes() {
        let a = rect(100.0, 100.0, 100.0, 50.0, 0.0);
        assert!((rotated_iou(&a, &a) - 1.0).abs() < 1e-9);
        // Shifted by half the width: intersection 2500, union 7500.
        assert!((rotated_iou(&a, &rect(150.0, 100.0, 100.0, 50.0, 0.0)) - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(rotated_iou(&a, &rect(900.0, 900.0, 10.0, 10.0, 0.0)), 0.0);
        // The same box written with swapped sides and a quarter turn.
        assert!((rotated_iou(&a, &rect(100.0, 100.0, 50.0, 100.0, 90.0)) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn hungarian_finds_the_best_total_not_the_greedy_pairing() {
        // Greedy takes (0,0)=0.9 and leaves row 1 with 0.0; the optimum is 0.8 + 0.8.
        let cost = vec![vec![-0.9, -0.8], vec![-0.8, 0.0]];
        assert_eq!(assign_min_cost(&cost), vec![1, 0]);
        let wide = vec![vec![5.0, 1.0, 9.0], vec![1.0, 7.0, 3.0]];
        assert_eq!(assign_min_cost(&wide), vec![1, 0]);
    }

    #[test]
    fn matching_works_when_there_are_more_or_fewer_detections() {
        let truth = [
            rect(100.0, 100.0, 80.0, 60.0, 0.0),
            rect(400.0, 100.0, 80.0, 60.0, 0.0),
        ];
        let found = [
            rect(402.0, 101.0, 80.0, 60.0, 0.0),
            rect(98.0, 99.0, 80.0, 60.0, 0.0),
            rect(700.0, 700.0, 50.0, 50.0, 0.0),
        ];
        let pairs = best_pairs(&truth, &found);
        assert_eq!(
            pairs.iter().map(|p| (p.0, p.1)).collect::<Vec<_>>(),
            vec![(0, 1), (1, 0)]
        );
        // Fewer detections than truth: the transposed problem.
        let fewer = best_pairs(&truth, &found[..1]);
        assert_eq!(fewer.len(), 1);
        assert_eq!((fewer[0].0, fewer[0].1), (1, 0));
        assert!(best_pairs(&truth, &[]).is_empty());
    }

    #[test]
    fn perfect_missing_and_extra_boxes_score_as_python_does() {
        let truth = [
            rect(100.0, 100.0, 80.0, 60.0, 0.0),
            rect(400.0, 100.0, 80.0, 60.0, 0.0),
        ];
        let perfect = score_rectangles(&truth, &truth);
        assert!((perfect.box_quality() - 1.0).abs() < 1e-9);
        assert!(
            perfect.count_correct && perfect.false_positive == 0 && perfect.false_negative == 0
        );
        assert!((perfect.f1_at_strict() - 1.0).abs() < 1e-9);

        // One missed: quality = 2 * 1.0 / (2 + 1).
        let missed = score_rectangles(&truth, &truth[..1]);
        assert!((missed.box_quality() - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!((missed.false_negative, missed.false_positive), (1, 0));
        assert!(!missed.count_correct);

        // One extra: quality = 2 * 2.0 / (2 + 3).
        let mut extra = truth.to_vec();
        extra.push(rect(700.0, 700.0, 40.0, 40.0, 0.0));
        let extra = score_rectangles(&truth, &extra);
        assert!((extra.box_quality() - 0.8).abs() < 1e-9);
        assert_eq!(extra.false_positive, 1);
    }

    #[test]
    fn loose_boxes_cannot_score_perfectly_and_coverage_tightness_explain_why() {
        let truth = [rect(100.0, 100.0, 100.0, 100.0, 0.0)];
        let loose = [rect(100.0, 100.0, 120.0, 120.0, 0.0)]; // contains the truth
        let score = score_rectangles(&truth, &loose);
        assert!(score.box_quality() < 0.75);
        assert!(
            (score.coverage() - 1.0).abs() < 1e-9,
            "nothing wanted was cut off"
        );
        assert!(
            (score.tightness() - 10_000.0 / 14_400.0).abs() < 1e-9,
            "but a fifth of the crop is not wanted"
        );
        let tight = score_rectangles(&truth, &[rect(100.0, 100.0, 80.0, 80.0, 0.0)]);
        assert!((tight.tightness() - 1.0).abs() < 1e-9);
        assert!((tight.coverage() - 0.64).abs() < 1e-9);
    }

    #[test]
    fn an_empty_scan_with_nothing_expected_is_a_perfect_case() {
        let score = score_rectangles(&[], &[]);
        assert_eq!(score.box_quality(), 1.0);
        assert_eq!(score.f1_at_detection(), 1.0);
        assert!(score.count_correct);
    }

    #[test]
    fn summaries_pool_counts_across_scans() {
        let truth = [rect(100.0, 100.0, 80.0, 60.0, 0.0)];
        let a = score_rectangles(&truth, &truth);
        let b = score_rectangles(&truth, &[]);
        let summary = summarize(&[a, b]);
        assert_eq!(
            (
                summary.cases,
                summary.expected_regions,
                summary.detected_regions
            ),
            (2, 2, 1)
        );
        assert!((summary.box_quality - 2.0 / 3.0).abs() < 1e-9);
        assert!((summary.count_accuracy - 0.5).abs() < 1e-9);
        assert!((summary.f1 - 2.0 * 1.0 * 0.5 / 1.5).abs() < 1e-9);
    }
}
