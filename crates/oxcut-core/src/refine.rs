//! v3 side refinement: move each side of a rectangle onto the strongest nearby
//! edge. Port of `_refine_rect_to_edges`.
//!
//! Adaptive thresholding is generous, so a contour can absorb a soft shadow
//! outside the print. This deskews a band around the rectangle and, per side,
//! looks for the peak of the gradient *averaged along the whole side*, which
//! favours the physical border over short edges inside the picture. A side
//! stays put unless its peak stands out from the local profile.

use crate::filter::{gaussian_blur_3x3, sobel3_reflect101_abs};
use crate::geometry::RotatedRect;
use crate::warp::{patch_matrix, warp_linear_replicate};

/// Peak gradient (mean |Sobel|) below which a side is left alone.
const MIN_PEAK: f64 = 8.0;
/// The peak must exceed the local median by at least this (and 4 deviations).
const MIN_PEAK_ABOVE_BASELINE: f64 = 6.0;

fn py_round(x: f64) -> i64 {
    x.round_ties_even() as i64
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Position of the edge peak within `margin` of `expected`, or `expected`
/// itself when no peak stands out.
fn strongest_edge(profile: &[f64], expected: f64, margin: i64) -> i64 {
    let expected_index = py_round(expected);
    let start = (expected_index - margin).max(0) as usize;
    let stop = (expected_index + margin + 1)
        .min(profile.len() as i64)
        .max(0) as usize;
    if stop <= start {
        return expected_index;
    }
    let candidates = &profile[start..stop];
    let mut best = 0;
    for (i, &v) in candidates.iter().enumerate() {
        if v > candidates[best] {
            best = i;
        }
    }
    let peak = candidates[best];
    let baseline = median(candidates);
    let deviations: Vec<f64> = candidates.iter().map(|v| (v - baseline).abs()).collect();
    let deviation = median(&deviations);
    if peak < MIN_PEAK || peak < baseline + MIN_PEAK_ABOVE_BASELINE.max(4.0 * deviation) {
        return expected_index;
    }
    (start + best) as i64
}

pub fn refine_rect_to_edges(
    gray: &[u8],
    width: usize,
    height: usize,
    rect: &RotatedRect,
    search_margin_ratio: f64,
) -> RotatedRect {
    if rect.w < 10.0 || rect.h < 10.0 {
        return *rect;
    }

    let margin = py_round(rect.w.min(rect.h) * search_margin_ratio).clamp(8, 140);
    let patch_width = (py_round(rect.w) + 2 * margin).max(3) as usize;
    let patch_height = (py_round(rect.h) + 2 * margin).max(3) as usize;

    let (matrix, [width_axis, height_axis]) = patch_matrix(rect, patch_width, patch_height);
    let patch = warp_linear_replicate(gray, width, height, &matrix, patch_width, patch_height);
    let patch = gaussian_blur_3x3(&patch, patch_width, patch_height);
    let (gradient_x, gradient_y) = sobel3_reflect101_abs(&patch, patch_width, patch_height);

    // Mean |gradient| across the interior of the patch, one value per column
    // (for vertical edges) and per row (for horizontal edges).
    let m = margin as usize;
    let rows = patch_height - 2 * m;
    let cols = patch_width - 2 * m;
    let x_profile: Vec<f64> = (0..patch_width)
        .map(|x| {
            (m..patch_height - m)
                .map(|y| gradient_x[y * patch_width + x] as u64)
                .sum::<u64>() as f64
                / rows as f64
        })
        .collect();
    let y_profile: Vec<f64> = (0..patch_height)
        .map(|y| {
            (m..patch_width - m)
                .map(|x| gradient_y[y * patch_width + x] as u64)
                .sum::<u64>() as f64
                / cols as f64
        })
        .collect();

    let left = strongest_edge(&x_profile, margin as f64, margin);
    let right = strongest_edge(&x_profile, margin as f64 + rect.w, margin);
    let top = strongest_edge(&y_profile, margin as f64, margin);
    let bottom = strongest_edge(&y_profile, margin as f64 + rect.h, margin);

    let refined_width = (right - left) as f64;
    let refined_height = (bottom - top) as f64;
    if refined_width <= 0.0 || refined_height <= 0.0 {
        return *rect;
    }
    let shift_x = (left + right - patch_width as i64) as f64 / 2.0;
    let shift_y = (top + bottom - patch_height as i64) as f64 / 2.0;
    RotatedRect::new(
        rect.cx + width_axis.0 * shift_x + height_axis.0 * shift_y,
        rect.cy + width_axis.1 * shift_x + height_axis.1 * shift_y,
        refined_width,
        refined_height,
        rect.angle,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grey page (200) with a dark print (50) of `w` x `h` centred at (cx, cy).
    fn page(width: usize, height: usize, cx: usize, cy: usize, w: usize, h: usize) -> Vec<u8> {
        (0..width * height)
            .map(|i| {
                let (x, y) = (i % width, i / width);
                let inside = x >= cx - w / 2 && x < cx + w / 2 && y >= cy - h / 2 && y < cy + h / 2;
                if inside { 50 } else { 200 }
            })
            .collect()
    }

    #[test]
    fn pulls_a_loose_box_onto_the_print_edges() {
        let gray = page(400, 300, 200, 150, 160, 100); // x 120..280, y 100..200
        // Right side 10 px out and top side 7 px out: both inside the +-13 px search window.
        let loose = RotatedRect::new(205.0, 146.0, 170.0, 106.0, 0.0);
        let refined = refine_rect_to_edges(&gray, 400, 300, &loose, 0.12);
        assert!((refined.w - 160.0).abs() <= 2.0, "width {}", refined.w);
        assert!((refined.h - 100.0).abs() <= 2.0, "height {}", refined.h);
        assert!((refined.cx - 200.0).abs() <= 2.0, "cx {}", refined.cx);
        assert!((refined.cy - 150.0).abs() <= 2.0, "cy {}", refined.cy);
        assert_eq!(refined.angle, 0.0);
    }

    #[test]
    fn keeps_the_box_when_there_is_no_edge_to_find() {
        let flat = vec![128u8; 300 * 300];
        let rect = RotatedRect::new(150.0, 150.0, 120.0, 90.0, 5.0);
        let refined = refine_rect_to_edges(&flat, 300, 300, &rect, 0.12);
        assert!((refined.w - 120.0).abs() < 1e-9 && (refined.h - 90.0).abs() < 1e-9);
        assert!((refined.cx - 150.0).abs() < 1e-9 && (refined.cy - 150.0).abs() < 1e-9);
    }

    #[test]
    fn ignores_tiny_rectangles() {
        let gray = vec![0u8; 100 * 100];
        let rect = RotatedRect::new(50.0, 50.0, 9.0, 60.0, 0.0);
        assert_eq!(refine_rect_to_edges(&gray, 100, 100, &rect, 0.12), rect);
    }

    #[test]
    fn median_handles_even_and_odd_counts() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
    }

    #[test]
    fn a_weak_peak_is_ignored() {
        // A 6-level step: peak (4 * 6 = 24 after Sobel) is above the floor but
        // the profile is noisy enough that a broader flat region wins nothing.
        let profile = vec![10.0; 41];
        assert_eq!(strongest_edge(&profile, 20.0, 10), 20);
    }
}
