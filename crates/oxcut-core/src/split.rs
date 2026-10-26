//! v3 gutter split: separate two touching prints when a narrow strip of
//! background runs across the blob. Port of `_v3_split_on_gutter`.

use crate::foreground::Foreground;
use crate::geometry::RotatedRect;
use crate::warp::{patch_matrix, warp_nearest};

/// Rectangles at least this elongated can hold two prints side by side.
const MIN_SPLIT_ASPECT: f64 = 1.8;
/// Foreground density in the gutter must fall to this or below.
const MAX_GUTTER_DENSITY: f32 = 0.08;
/// Both sides of the gutter must be at least this dense.
const MIN_FLANK_DENSITY: f64 = 0.16;

fn py_round(x: f64) -> usize {
    x.round_ties_even() as usize
}

/// `cv2.blur(row, (7, 1))` on a float32 row: normalised box filter, border
/// `REFLECT_101`, summed in double.
fn blur7(values: &[f32]) -> Vec<f32> {
    let n = values.len();
    let reflect = |i: isize| -> usize {
        if n == 1 {
            return 0;
        }
        let period = 2 * (n as isize - 1);
        let i = i.rem_euclid(period);
        (if i >= n as isize { period - i } else { i }) as usize
    };
    (0..n)
        .map(|x| {
            let sum: f64 = (-3..=3)
                .map(|d| values[reflect(x as isize + d)] as f64)
                .sum();
            (sum * (1.0 / 7.0)) as f32
        })
        .collect()
}

pub fn split_on_gutter(
    rect: &RotatedRect,
    foreground: &Foreground,
    minimum_area: f64,
) -> Vec<RotatedRect> {
    let (width, height) = (rect.w, rect.h);
    if width.min(height) <= 0.0 || width.max(height) / width.min(height) < MIN_SPLIT_ASPECT {
        return vec![*rect];
    }

    let patch_width = py_round(width).max(3);
    let patch_height = py_round(height).max(3);
    let (matrix, [width_axis, height_axis]) = patch_matrix(rect, patch_width, patch_height);
    let source: Vec<u8> = foreground.mask.iter().map(|&b| b as u8).collect();
    let patch = warp_nearest(
        &source,
        foreground.width,
        foreground.height,
        &matrix,
        patch_width,
        patch_height,
        0,
    );

    // Mean foreground along the short axis, then smoothed.
    let split_width = width >= height;
    let profile: Vec<f32> = if split_width {
        (0..patch_width)
            .map(|x| {
                let sum: u32 = (0..patch_height)
                    .map(|y| patch[y * patch_width + x] as u32)
                    .sum();
                (sum as f64 / patch_height as f64) as f32
            })
            .collect()
    } else {
        (0..patch_height)
            .map(|y| {
                let sum: u32 = patch[y * patch_width..(y + 1) * patch_width]
                    .iter()
                    .map(|&v| v as u32)
                    .sum();
                (sum as f64 / patch_width as f64) as f32
            })
            .collect()
    };
    let profile = blur7(&profile);

    let length = profile.len();
    let (start, stop) = (
        py_round(length as f64 * 0.25),
        py_round(length as f64 * 0.75),
    );
    if stop <= start {
        return vec![*rect];
    }
    let mut split = start;
    for i in start..stop {
        if profile[i] < profile[split] {
            split = i;
        }
    }
    let flank = py_round(length as f64 * 0.12).max(5);
    let mean = |slice: &[f32]| slice.iter().map(|&v| v as f64).sum::<f64>() / slice.len() as f64;
    let before_density = mean(&profile[split.saturating_sub(flank)..split]);
    let after_density = mean(&profile[split + 1..(split + flank).min(length)]);
    if profile[split] > MAX_GUTTER_DENSITY || before_density.min(after_density) < MIN_FLANK_DENSITY
    {
        return vec![*rect];
    }

    let (first_length, second_length) = (split as f64, (length - split) as f64);
    let short_side = if split_width { height } else { width };
    if first_length * short_side < minimum_area || second_length * short_side < minimum_area {
        return vec![*rect];
    }

    let axis = if split_width { width_axis } else { height_axis };
    let first_center = (
        rect.cx - axis.0 * second_length / 2.0,
        rect.cy - axis.1 * second_length / 2.0,
    );
    let second_center = (
        rect.cx + axis.0 * first_length / 2.0,
        rect.cy + axis.1 * first_length / 2.0,
    );
    if split_width {
        vec![
            RotatedRect::new(
                first_center.0,
                first_center.1,
                first_length,
                height,
                rect.angle,
            ),
            RotatedRect::new(
                second_center.0,
                second_center.1,
                second_length,
                height,
                rect.angle,
            ),
        ]
    } else {
        vec![
            RotatedRect::new(
                first_center.0,
                first_center.1,
                width,
                first_length,
                rect.angle,
            ),
            RotatedRect::new(
                second_center.0,
                second_center.1,
                width,
                second_length,
                rect.angle,
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Foreground with filled axis-aligned boxes `(x, y, w, h)`.
    fn foreground_with(
        width: usize,
        height: usize,
        boxes: &[(usize, usize, usize, usize)],
    ) -> Foreground {
        let mut mask = vec![false; width * height];
        for &(x0, y0, w, h) in boxes {
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    mask[y * width + x] = true;
                }
            }
        }
        Foreground {
            width,
            height,
            mask,
            otsu: 0,
        }
    }

    #[test]
    fn splits_two_prints_separated_by_a_gutter() {
        // Two 100x100 prints with a 12 px gap, one rect around both.
        let fg = foreground_with(400, 200, &[(50, 40, 100, 100), (162, 40, 100, 100)]);
        let both = RotatedRect::new(156.0, 90.0, 212.0, 100.0, 0.0);
        let parts = split_on_gutter(&both, &fg, 1000.0);
        assert_eq!(parts.len(), 2, "{parts:?}");
        assert!((parts[0].w + parts[1].w - 212.0).abs() < 1e-6);
        assert!(parts[0].cx < parts[1].cx);
        assert!(parts.iter().all(|p| (p.h - 100.0).abs() < 1e-9));
    }

    #[test]
    fn does_not_split_a_solid_print() {
        let fg = foreground_with(400, 200, &[(50, 40, 212, 100)]);
        let rect = RotatedRect::new(156.0, 90.0, 212.0, 100.0, 0.0);
        assert_eq!(split_on_gutter(&rect, &fg, 1000.0), vec![rect]);
    }

    #[test]
    fn does_not_split_compact_rectangles() {
        let fg = foreground_with(200, 200, &[(20, 20, 60, 60), (100, 20, 60, 60)]);
        let rect = RotatedRect::new(90.0, 50.0, 140.0, 80.0, 0.0); // aspect 1.75
        assert_eq!(split_on_gutter(&rect, &fg, 100.0).len(), 1);
    }

    #[test]
    fn splits_a_tall_pair_along_the_height_axis() {
        let fg = foreground_with(200, 400, &[(40, 50, 100, 100), (40, 162, 100, 100)]);
        let rect = RotatedRect::new(90.0, 156.0, 100.0, 212.0, 0.0);
        let parts = split_on_gutter(&rect, &fg, 1000.0);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].cy < parts[1].cy);
        assert!(parts.iter().all(|p| (p.w - 100.0).abs() < 1e-9));
    }

    #[test]
    fn refuses_halves_below_the_minimum_area() {
        let fg = foreground_with(400, 200, &[(50, 40, 100, 100), (162, 40, 100, 100)]);
        let both = RotatedRect::new(156.0, 90.0, 212.0, 100.0, 0.0);
        assert_eq!(split_on_gutter(&both, &fg, 1.0e6).len(), 1);
    }
}
