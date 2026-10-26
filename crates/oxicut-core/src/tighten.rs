//! v7 inner-image tightening: move each side of a rectangle inward onto the
//! photograph's own edge when a clear light border lies between them.
//! Port of `_deskew_patch`, `_side_step` and `_tighten_rect` in ScanSplitter's
//! `detector_v7.py`.
//!
//! v3 locks onto the outer paper edge or its drop shadow. A print has a light,
//! uniform border and then a step down to the emulsion; this looks for exactly
//! that along each side and only moves a side when the evidence is consistent.

use image::GrayImage;

use crate::filter::reflect101;
use crate::geometry::RotatedRect;
use crate::warp::{patch_matrix, warp_linear_replicate_f32};

/// Brightness drop that counts as the step from border to picture.
const MIN_STEP: f32 = 7.0;
/// A plateau may vary by at most this many steps and still be "uniform".
const PLATEAU_RANGE: f32 = 1.6;
/// A side moves only if the border is at least this thick (working pixels).
const MIN_MOVE: f64 = 1.5;

/// A grey image as `f32`, the form v7 keeps its working copy in.
pub struct GrayF32 {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl GrayF32 {
    pub fn from_gray(image: &GrayImage) -> Self {
        Self {
            width: image.width() as usize,
            height: image.height() as usize,
            data: image.as_raw().iter().map(|&v| v as f32).collect(),
        }
    }
}

fn py_round(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// `np.median` of finite values.
fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// The rectangle's content, axis aligned: rows follow its height.
fn deskew_patch(gray: &GrayF32, rect: &RotatedRect) -> (Vec<f32>, usize, usize) {
    let width = py_round(rect.w).max(3) as usize;
    let height = py_round(rect.h).max(3) as usize;
    let (matrix, _) = patch_matrix(rect, width, height);
    let patch =
        warp_linear_replicate_f32(&gray.data, gray.width, gray.height, &matrix, width, height);
    (patch, width, height)
}

/// One side of a patch seen from the outside in: row 0 is the side itself and
/// each further row lies deeper. Only the rows `side_step` reads are kept.
struct Strip {
    /// Rows the full strip would have.
    rows: usize,
    length: usize,
    data: Vec<f32>,
}

#[derive(Clone, Copy)]
enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

impl Side {
    const ALL: [Side; 4] = [Side::Top, Side::Bottom, Side::Left, Side::Right];
}

fn strip_of(patch: &[f32], width: usize, height: usize, side: Side, keep: usize) -> Strip {
    let (rows, length) = match side {
        Side::Top | Side::Bottom => (height, width),
        Side::Left | Side::Right => (width, height),
    };
    let kept = keep.min(rows);
    let mut data = Vec::with_capacity(kept * length);
    for r in 0..kept {
        for c in 0..length {
            data.push(match side {
                Side::Top => patch[r * width + c],
                Side::Bottom => patch[(height - 1 - r) * width + c],
                Side::Left => patch[c * width + r],
                Side::Right => patch[c * width + (width - 1 - r)],
            });
        }
    }
    Strip { rows, length, data }
}

/// Depth of the inner edge of a uniform light band at row 0 of `strip`, or
/// `None` when there is no convincing one.
///
/// Sampled columns each vote for the depth where a plateau of near-maximal
/// brightness ends in a step towards darker content. The side moves only if the
/// plateau is uniform, wide enough, ends within `max_depth`, and the votes lie
/// on one straight line (a physical, possibly slightly skewed edge).
fn side_step(strip: &Strip, max_depth: usize, min_band: usize) -> Option<f64> {
    let length = strip.length;
    if strip.rows < max_depth + 3 || length < 20 {
        return None;
    }
    let lo = (length as f64 * 0.10) as usize;
    let hi = (length as f64 * 0.90) as usize;
    let k = ((length as f64 * 0.03) as usize).max(3) | 1;
    let rows = (max_depth + min_band + 3).min(strip.rows);
    let width = hi - lo;

    // cv2.blur(.., (k, 1)) then cv2.blur(.., (1, 3)): box filters, reflected borders.
    let mut horizontal = vec![0f32; rows * width];
    let half = (k / 2) as isize;
    let k_scale = 1.0 / k as f64;
    for r in 0..rows {
        let row = &strip.data[r * length + lo..r * length + hi];
        for x in 0..width {
            let sum: f64 = (0..k as isize)
                .map(|d| row[reflect101(x as isize + d - half, width)] as f64)
                .sum();
            horizontal[r * width + x] = (sum * k_scale) as f32;
        }
    }
    let mut smooth = vec![0f32; rows * width];
    for r in 0..rows {
        for x in 0..width {
            let sum: f64 = (-1..=1isize)
                .map(|d| horizontal[reflect101(r as isize + d, rows) * width + x] as f64)
                .sum();
            smooth[r * width + x] = (sum * (1.0 / 3.0)) as f32;
        }
    }

    let n = 60.min(width);
    let step_x = (width - 1) as f64 / (n - 1) as f64;
    let mut votes: Vec<(f64, f64)> = Vec::new();
    let mut column = vec![0f32; rows];
    for i in 0..n {
        let x = if i == n - 1 {
            width - 1
        } else {
            (i as f64 * step_x) as usize
        };
        for (r, value) in column.iter_mut().enumerate() {
            *value = smooth[r * width + x];
        }
        let p = &column;
        let head = (max_depth + 1).min(rows);
        let mut dm = 0;
        for r in 1..head {
            if p[r] > p[dm] {
                dm = r;
            }
        }
        let peak = p[dm];
        let threshold = peak - MIN_STEP;
        let Some(step) = (dm..rows).find(|&r| p[r] < threshold) else {
            continue;
        };
        let start = (0..=dm)
            .rev()
            .find(|&r| p[r] < threshold)
            .map_or(0, |r| r + 1);
        if step < start + min_band || step > max_depth {
            continue;
        }
        let band = &p[start..step];
        let (band_max, band_min) = band
            .iter()
            .fold((f32::NEG_INFINITY, f32::INFINITY), |(hi, lo), &v| {
                (hi.max(v), lo.min(v))
            });
        if band_max - band_min > PLATEAU_RANGE * MIN_STEP {
            continue;
        }
        let after = &p[step..(step + min_band).min(rows)];
        if after.len() < min_band {
            continue;
        }
        let mean = after.iter().sum::<f32>() / after.len() as f32;
        if mean as f64 > peak as f64 - MIN_STEP as f64 {
            continue;
        }
        votes.push((x as f64, step as f64));
    }
    if (votes.len() as f64) < 0.55 * n as f64 {
        return None;
    }

    // Theil-Sen style robust line through the votes.
    let (xs, ys): (Vec<f64>, Vec<f64>) = votes.iter().copied().unzip();
    let mut slopes = Vec::new();
    for i in (0..xs.len()).step_by(2) {
        for j in (i + 1..xs.len()).step_by(3) {
            if xs[j] != xs[i] {
                slopes.push((ys[j] - ys[i]) / (xs[j] - xs[i]));
            }
        }
    }
    let slope = if slopes.is_empty() {
        0.0
    } else {
        median(&mut slopes)
    };
    let mut offsets: Vec<f64> = xs.iter().zip(&ys).map(|(x, y)| y - slope * x).collect();
    let intercept = median(&mut offsets);
    let tolerance = (0.004 * length as f64).max(1.5);
    let inliers = xs
        .iter()
        .zip(&ys)
        .filter(|&(x, y)| (y - (intercept + slope * x)).abs() <= tolerance)
        .count() as f64
        / xs.len() as f64;
    if inliers < 0.75 || votes.len() as f64 * inliers < 0.55 * n as f64 {
        return None;
    }
    Some(intercept + slope * (width as f64 / 2.0))
}

/// Move each side of `rect` (in full-resolution pixels) inward onto the
/// emulsion edge when a clear border exists. `gray` is the working grey image
/// and `scale` maps full-resolution to working pixels.
pub fn tighten_rect(gray: &GrayF32, rect: &RotatedRect, scale: f64, passes: usize) -> RotatedRect {
    let (scx, scy) = (rect.cx * scale, rect.cy * scale);
    let (sw, sh) = (rect.w * scale, rect.h * scale);
    let angle = rect.angle;
    if sw < 40.0 || sh < 40.0 {
        return *rect;
    }
    let theta = angle.to_radians();
    let (ux, uy) = ((theta.cos(), theta.sin()), (-theta.sin(), theta.cos()));
    // How far each side has moved in, in working pixels: top, bottom, left, right.
    let mut moved = [0f64; 4];
    let index = |side: Side| match side {
        Side::Top => 0,
        Side::Bottom => 1,
        Side::Left => 2,
        Side::Right => 3,
    };
    let centre = |moved: &[f64; 4]| {
        let (dx, dy) = ((moved[2] - moved[3]) / 2.0, (moved[0] - moved[1]) / 2.0);
        (scx + ux.0 * dx + uy.0 * dy, scy + ux.1 * dx + uy.1 * dy)
    };

    for _ in 0..passes {
        let (cx, cy) = centre(&moved);
        let current = RotatedRect::new(
            cx,
            cy,
            sw - moved[2] - moved[3],
            sh - moved[0] - moved[1],
            angle,
        );
        let (patch, pw, ph) = deskew_patch(gray, &current);
        let mut any = false;
        for side in Side::ALL {
            let dim = match side {
                Side::Top | Side::Bottom => ph,
                Side::Left | Side::Right => pw,
            };
            let max_depth = ((0.10 * dim as f64).round_ties_even() as usize).max(4);
            let min_band = ((0.012 * dim as f64).round_ties_even() as usize).max(2);
            let strip = strip_of(&patch, pw, ph, side, max_depth + min_band + 3);
            if let Some(step) = side_step(&strip, max_depth, min_band)
                && step >= MIN_MOVE
            {
                moved[index(side)] += step;
                any = true;
            }
        }
        if !any {
            break;
        }
    }

    let (new_w, new_h) = (sw - moved[2] - moved[3], sh - moved[0] - moved[1]);
    if new_w < 0.5 * sw || new_h < 0.5 * sh {
        return *rect;
    }
    let (cx, cy) = centre(&moved);
    RotatedRect::new(cx / scale, cy / scale, new_w / scale, new_h / scale, angle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page (grey 200) with a dark picture (60) inside a light border (235) of
    /// `border` px, the whole print being `outer` = (x0, y0, x1, y1).
    fn print_on_page(
        width: usize,
        height: usize,
        outer: (usize, usize, usize, usize),
        border: usize,
    ) -> GrayF32 {
        let (x0, y0, x1, y1) = outer;
        let data = (0..width * height)
            .map(|i| {
                let (x, y) = (i % width, i / width);
                if x >= x0 + border && x < x1 - border && y >= y0 + border && y < y1 - border {
                    60.0
                } else if x >= x0 && x < x1 && y >= y0 && y < y1 {
                    235.0
                } else {
                    200.0
                }
            })
            .collect();
        GrayF32 {
            width,
            height,
            data,
        }
    }

    fn outer_rect(x0: f64, y0: f64, x1: f64, y1: f64) -> RotatedRect {
        RotatedRect::new((x0 + x1) / 2.0, (y0 + y1) / 2.0, x1 - x0, y1 - y0, 0.0)
    }

    #[test]
    fn a_light_border_is_trimmed_off_every_side() {
        let page = print_on_page(500, 400, (100, 80, 400, 320), 14);
        let tight = tighten_rect(&page, &outer_rect(100.0, 80.0, 400.0, 320.0), 1.0, 2);
        // The picture is x 114..386, y 94..306.
        assert!((tight.w - 272.0).abs() <= 2.0, "width {}", tight.w);
        assert!((tight.h - 212.0).abs() <= 2.0, "height {}", tight.h);
        assert!(
            (tight.cx - 250.0).abs() <= 1.0 && (tight.cy - 200.0).abs() <= 1.0,
            "{tight:?}"
        );
    }

    #[test]
    fn only_the_sides_that_have_a_border_move() {
        // Border on the right and bottom only: the picture starts at the box's top left.
        let (width, height) = (500, 400);
        let data = (0..width * height)
            .map(|i| {
                let (x, y) = (i % width, i / width);
                if (100..386).contains(&x) && (80..306).contains(&y) {
                    60.0
                } else if (100..400).contains(&x) && (80..320).contains(&y) {
                    235.0
                } else {
                    200.0
                }
            })
            .collect();
        let page = GrayF32 {
            width,
            height,
            data,
        };
        let tight = tighten_rect(&page, &outer_rect(100.0, 80.0, 400.0, 320.0), 1.0, 2);
        assert!(
            (tight.w - 286.0).abs() <= 2.0 && (tight.h - 226.0).abs() <= 2.0,
            "{tight:?}"
        );
        assert!(
            (tight.cx - 243.0).abs() <= 1.5 && (tight.cy - 193.0).abs() <= 1.5,
            "{tight:?}"
        );
    }

    #[test]
    fn a_box_already_on_the_picture_edge_does_not_move() {
        let page = print_on_page(500, 400, (100, 80, 400, 320), 14);
        let on_picture = outer_rect(114.0, 94.0, 386.0, 306.0);
        let tight = tighten_rect(&page, &on_picture, 1.0, 2);
        assert!(
            (tight.w - on_picture.w).abs() < 1.0 && (tight.h - on_picture.h).abs() < 1.0,
            "{tight:?}"
        );
    }

    #[test]
    fn flat_areas_and_small_boxes_are_left_alone() {
        let flat = GrayF32 {
            width: 300,
            height: 300,
            data: vec![150.0; 300 * 300],
        };
        let rect = outer_rect(50.0, 50.0, 250.0, 250.0);
        assert_eq!(tighten_rect(&flat, &rect, 1.0, 2), rect);
        let tiny = outer_rect(10.0, 10.0, 40.0, 40.0);
        assert_eq!(tighten_rect(&flat, &tiny, 1.0, 2), tiny);
    }

    #[test]
    fn working_scale_maps_in_and_back() {
        // The same print at half size, with the box given in full-resolution pixels.
        let page = print_on_page(250, 200, (50, 40, 200, 160), 7);
        let full = outer_rect(100.0, 80.0, 400.0, 320.0);
        let tight = tighten_rect(&page, &full, 0.5, 2);
        assert!(
            (tight.w - 272.0).abs() <= 4.0 && (tight.h - 212.0).abs() <= 4.0,
            "{tight:?}"
        );
    }

    #[test]
    fn a_tilted_print_is_tightened_along_its_own_axes() {
        // Rotate a 300x240 print with a 14 px border by 8 degrees about its centre.
        let (cx, cy, angle) = (250.0f64, 200.0f64, 8.0f64);
        let (cos, sin) = (angle.to_radians().cos(), angle.to_radians().sin());
        let (width, height) = (500usize, 400usize);
        let data = (0..width * height)
            .map(|i| {
                let (px, py) = ((i % width) as f64 - cx, (i / width) as f64 - cy);
                let (u, v) = (px * cos + py * sin, -px * sin + py * cos);
                if u.abs() < 150.0 - 14.0 && v.abs() < 120.0 - 14.0 {
                    60.0
                } else if u.abs() < 150.0 && v.abs() < 120.0 {
                    235.0
                } else {
                    200.0
                }
            })
            .collect();
        let page = GrayF32 {
            width,
            height,
            data,
        };
        let tight = tighten_rect(
            &page,
            &RotatedRect::new(cx, cy, 300.0, 240.0, angle),
            1.0,
            2,
        );
        assert!(
            (tight.w - 272.0).abs() <= 3.0 && (tight.h - 212.0).abs() <= 3.0,
            "{tight:?}"
        );
        assert_eq!(tight.angle, angle);
    }
}
