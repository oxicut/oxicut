//! v3 angle snap: correct a mask-derived rectangle angle using the long straight
//! edges near it. Port of `_v3_snap_angle_to_long_edges`.

use std::f64::consts::PI;

use crate::canny::canny;
use crate::filter::gaussian_blur_5x5;
use crate::geometry::RotatedRect;
use crate::hough::hough_lines_p;

/// Only edges within this many degrees of the current angle (mod 90) count.
const MAX_DELTA: f64 = 18.0;
/// Histogram bins across `-MAX_DELTA..MAX_DELTA` (half a degree each).
const BINS: usize = 72;
/// Observations within this many degrees of the histogram peak are averaged.
const PEAK_WINDOW: f64 = 2.0;

fn py_round(x: f64) -> i32 {
    x.round_ties_even() as i32
}

/// Pixel region `[x1, y1, x2, y2)` searched for edges: the rectangle's integer
/// bounding box plus a margin, clipped to the image. `None` when empty.
pub fn patch_bounds(rect: &RotatedRect, width: usize, height: usize) -> Option<[i64; 4]> {
    // Single-precision corners: a corner on an exact pixel boundary must floor
    // the way OpenCV's `boundingRect(boxPoints(rect))` does.
    let corners = rect.corners_f32();
    let min_x = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
    let max_x = corners
        .iter()
        .map(|c| c.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min);
    let max_y = corners
        .iter()
        .map(|c| c.1)
        .fold(f32::NEG_INFINITY, f32::max);
    let (x, y) = (min_x.floor() as i64, min_y.floor() as i64);
    let (box_w, box_h) = (max_x.floor() as i64 - x + 1, max_y.floor() as i64 - y + 1);
    let margin = py_round(rect.w.min(rect.h) * 0.15).max(8) as i64;
    let x1 = (x - margin).max(0);
    let y1 = (y - margin).max(0);
    let x2 = (x + box_w + margin).min(width as i64);
    let y2 = (y + box_h + margin).min(height as i64);
    (x2 > x1 && y2 > y1).then_some([x1, y1, x2, y2])
}

/// Shortest line (in x or y extent) the Hough step may report, in pixels.
pub fn minimum_line_length(short_side: f64) -> i32 {
    py_round(short_side * 0.32).max(20)
}

/// The correction in degrees implied by detected line segments, or `None` when
/// they do not agree on one orientation.
pub fn angle_from_lines(lines: &[[i32; 4]], raw_angle: f64, minimum: i32) -> Option<f64> {
    // Angle of each line relative to the rectangle, folded to +-45 degrees
    // because a rectangle repeats every 90.
    let mut observations: Vec<(f64, f64)> = Vec::new();
    for &[lx1, ly1, lx2, ly2] in lines {
        let (dx, dy) = ((lx2 - lx1) as f64, (ly2 - ly1) as f64);
        let length = dx.hypot(dy);
        let angle = dy.atan2(dx).to_degrees();
        let delta = (angle - raw_angle + 45.0).rem_euclid(90.0) - 45.0;
        if delta.abs() <= MAX_DELTA {
            observations.push((delta, length * length));
        }
    }
    if observations.len() < 2 {
        return None;
    }

    // Length-weighted histogram of deltas; edges are -18 + 0.5 * i.
    let bin_width = 2.0 * MAX_DELTA / BINS as f64;
    let mut histogram = [0f64; BINS];
    for &(delta, weight) in &observations {
        // np.searchsorted(bins, delta) - 1, clipped: the number of edges < delta, minus one.
        let edges_below = ((delta + MAX_DELTA) / bin_width).ceil().max(0.0) as usize;
        let index = edges_below.saturating_sub(1).min(BINS - 1);
        histogram[index] += weight;
    }
    let mut peak_index = 0;
    for (i, &value) in histogram.iter().enumerate() {
        if value > histogram[peak_index] {
            peak_index = i;
        }
    }
    let peak = (-MAX_DELTA
        + peak_index as f64 * bin_width
        + (-MAX_DELTA + (peak_index + 1) as f64 * bin_width))
        / 2.0;

    let nearby: Vec<_> = observations
        .iter()
        .filter(|(d, _)| (d - peak).abs() <= PEAK_WINDOW)
        .collect();
    let total_weight: f64 = nearby.iter().map(|(_, w)| w).sum();
    if total_weight < (minimum as f64) * (minimum as f64) {
        return None;
    }
    Some(nearby.iter().map(|(d, w)| d * w).sum::<f64>() / total_weight)
}

pub fn snap_angle_to_long_edges(
    gray: &[u8],
    width: usize,
    height: usize,
    rect: &RotatedRect,
) -> RotatedRect {
    let short_side = rect.w.min(rect.h);
    if short_side < 10.0 {
        return *rect;
    }
    let Some([x1, y1, x2, y2]) = patch_bounds(rect, width, height) else {
        return *rect;
    };
    let (patch_w, patch_h) = ((x2 - x1) as usize, (y2 - y1) as usize);
    let mut patch = Vec::with_capacity(patch_w * patch_h);
    for row in y1..y2 {
        let start = row as usize * width + x1 as usize;
        patch.extend_from_slice(&gray[start..start + patch_w]);
    }

    let edges = canny(
        &gaussian_blur_5x5(&patch, patch_w, patch_h),
        patch_w,
        patch_h,
        35,
        110,
    );
    let minimum = minimum_line_length(short_side);
    let lines = hough_lines_p(
        &edges,
        patch_w,
        patch_h,
        PI / 360.0,
        py_round(minimum as f64 * 0.25).max(18),
        minimum,
        py_round(minimum as f64 * 0.12).max(8),
    );
    match angle_from_lines(&lines, rect.angle, minimum) {
        Some(delta) => RotatedRect::new(rect.cx, rect.cy, rect.w, rect.h, rect.angle + delta),
        None => *rect,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// A grey page with a dark rectangle rotated by `degrees` about its centre.
    fn page_with_print(width: usize, height: usize, rect: &RotatedRect) -> Vec<u8> {
        let theta = rect.angle.to_radians();
        let (cos, sin) = (theta.cos(), theta.sin());
        (0..width * height)
            .map(|i| {
                let (px, py) = ((i % width) as f64 - rect.cx, (i / width) as f64 - rect.cy);
                let (u, v) = (px * cos + py * sin, -px * sin + py * cos);
                if u.abs() <= rect.w / 2.0 && v.abs() <= rect.h / 2.0 {
                    40
                } else {
                    220
                }
            })
            .collect()
    }

    #[test]
    fn corrects_a_slightly_wrong_angle_towards_the_edges() {
        let truth = RotatedRect::new(150.0, 120.0, 160.0, 110.0, 4.0);
        let gray = page_with_print(300, 240, &truth);
        // The mask-derived guess is off by 2 degrees.
        let guess = RotatedRect::new(150.0, 120.0, 160.0, 110.0, 2.0);
        let snapped = snap_angle_to_long_edges(&gray, 300, 240, &guess);
        assert!(
            (snapped.angle - 4.0).abs() < 0.6,
            "snapped to {}",
            snapped.angle
        );
        assert_eq!(
            (snapped.cx, snapped.cy, snapped.w, snapped.h),
            (150.0, 120.0, 160.0, 110.0)
        );
    }

    #[test]
    fn leaves_the_angle_alone_when_there_are_no_edges() {
        let rect = RotatedRect::new(100.0, 100.0, 80.0, 60.0, 3.0);
        let flat = vec![200u8; 200 * 200];
        assert_eq!(snap_angle_to_long_edges(&flat, 200, 200, &rect), rect);
    }

    #[test]
    fn ignores_tiny_rectangles() {
        let rect = RotatedRect::new(20.0, 20.0, 50.0, 8.0, 3.0);
        let gray = vec![100u8; 100 * 100];
        assert_eq!(snap_angle_to_long_edges(&gray, 100, 100, &rect), rect);
    }
}
