//! Affine warps matching `cv2.warpAffine(..., flags=... | WARP_INVERSE_MAP)`.
//!
//! `matrix` maps destination pixels to source pixels, as OpenCV expects with
//! `WARP_INVERSE_MAP`. OpenCV evaluates it in fixed point (1/1024 pixel), and
//! that rounding decides which source pixel a destination pixel reads, so it is
//! reproduced exactly.

const AB_BITS: i32 = 10;
const AB_SCALE: f64 = (1 << AB_BITS) as f64;

/// 2x3 matrix as `[[m00, m01, m02], [m10, m11, m12]]`. OpenCV stores it as
/// `float32`; callers pass the already-rounded values.
pub type Affine = [[f64; 3]; 2];

fn round_to_int(x: f64) -> i32 {
    x.round_ties_even() as i32
}

/// Single-channel `INTER_NEAREST` with a constant border.
pub fn warp_nearest(
    src: &[u8],
    src_width: usize,
    src_height: usize,
    matrix: &Affine,
    dst_width: usize,
    dst_height: usize,
    border: u8,
) -> Vec<u8> {
    let round_delta = (AB_SCALE / 2.0) as i32;
    let adelta: Vec<i32> = (0..dst_width)
        .map(|x| round_to_int(matrix[0][0] * x as f64 * AB_SCALE))
        .collect();
    let bdelta: Vec<i32> = (0..dst_width)
        .map(|x| round_to_int(matrix[1][0] * x as f64 * AB_SCALE))
        .collect();

    let mut dst = vec![border; dst_width * dst_height];
    for y in 0..dst_height {
        let x0 = round_to_int((matrix[0][1] * y as f64 + matrix[0][2]) * AB_SCALE) + round_delta;
        let y0 = round_to_int((matrix[1][1] * y as f64 + matrix[1][2]) * AB_SCALE) + round_delta;
        for x in 0..dst_width {
            let sx = (x0 + adelta[x]) >> AB_BITS;
            let sy = (y0 + bdelta[x]) >> AB_BITS;
            if sx >= 0 && sy >= 0 && (sx as usize) < src_width && (sy as usize) < src_height {
                dst[y * dst_width + x] = src[sy as usize * src_width + sx as usize];
            }
        }
    }
    dst
}

use crate::geometry::RotatedRect;

/// The inverse-map matrix OpenCV receives: it is built in double, stored as
/// `float32`, and read back as double.
pub fn patch_matrix(
    rect: &RotatedRect,
    patch_width: usize,
    patch_height: usize,
) -> (Affine, [(f64, f64); 2]) {
    let theta = rect.angle.to_radians();
    let width_axis = (theta.cos(), theta.sin());
    let height_axis = (-theta.sin(), theta.cos());
    let (pw, ph) = (patch_width as f64, patch_height as f64);
    let f = |v: f64| v as f32 as f64;
    let matrix = [
        [
            f(width_axis.0),
            f(height_axis.0),
            f(rect.cx - width_axis.0 * pw / 2.0 - height_axis.0 * ph / 2.0),
        ],
        [
            f(width_axis.1),
            f(height_axis.1),
            f(rect.cy - width_axis.1 * pw / 2.0 - height_axis.1 * ph / 2.0),
        ],
    ];
    (matrix, [width_axis, height_axis])
}

/// Single-channel `INTER_LINEAR` with `BORDER_REPLICATE`, 8-bit.
///
/// Unlike nearest-neighbour, OpenCV 5 evaluates this in floating point: the
/// source coordinate in double (from the `float32` matrix), the blend in single
/// precision, the result rounded half to even. Probed against cv2 on rotated
/// transforms this agrees on all but about 0.016% of pixels (the residue is
/// likely fused multiply-add in OpenCV's vector code); a 1/32 or even 15-bit
/// fixed-point model disagrees on several percent.
pub fn warp_linear_replicate(
    src: &[u8],
    src_width: usize,
    src_height: usize,
    matrix: &Affine,
    dst_width: usize,
    dst_height: usize,
) -> Vec<u8> {
    let clamp_x = |x: i64| x.clamp(0, src_width as i64 - 1) as usize;
    let clamp_y = |y: i64| y.clamp(0, src_height as i64 - 1) as usize;
    let at = |y: usize, x: usize| src[y * src_width + x] as f32;

    let mut dst = vec![0u8; dst_width * dst_height];
    for y in 0..dst_height {
        for x in 0..dst_width {
            let sx = matrix[0][0] * x as f64 + matrix[0][1] * y as f64 + matrix[0][2];
            let sy = matrix[1][0] * x as f64 + matrix[1][1] * y as f64 + matrix[1][2];
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = ((sx - x0) as f32, (sy - y0) as f32);
            let (x0, y0) = (x0 as i64, y0 as i64);

            let (xa, xb) = (clamp_x(x0), clamp_x(x0 + 1));
            let (ya, yb) = (clamp_y(y0), clamp_y(y0 + 1));
            let (a, b, c, d) = (at(ya, xa), at(ya, xb), at(yb, xa), at(yb, xb));
            let value = (a * (1.0 - fx) + b * fx) * (1.0 - fy) + (c * (1.0 - fx) + d * fx) * fy;
            dst[y * dst_width + x] = value.round_ties_even().clamp(0.0, 255.0) as u8;
        }
    }
    dst
}
/// `INTER_LINEAR` with `BORDER_REPLICATE` on a single-channel `f32` image: the
/// same blend as [`warp_linear_replicate`] without the rounding to 8 bits.
pub fn warp_linear_replicate_f32(
    src: &[f32],
    src_width: usize,
    src_height: usize,
    matrix: &Affine,
    dst_width: usize,
    dst_height: usize,
) -> Vec<f32> {
    let clamp_x = |x: i64| x.clamp(0, src_width as i64 - 1) as usize;
    let clamp_y = |y: i64| y.clamp(0, src_height as i64 - 1) as usize;
    let at = |y: usize, x: usize| src[y * src_width + x];

    let mut dst = vec![0f32; dst_width * dst_height];
    for y in 0..dst_height {
        for x in 0..dst_width {
            let sx = matrix[0][0] * x as f64 + matrix[0][1] * y as f64 + matrix[0][2];
            let sy = matrix[1][0] * x as f64 + matrix[1][1] * y as f64 + matrix[1][2];
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = ((sx - x0) as f32, (sy - y0) as f32);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let (xa, xb) = (clamp_x(x0), clamp_x(x0 + 1));
            let (ya, yb) = (clamp_y(y0), clamp_y(y0 + 1));
            let (a, b, c, d) = (at(ya, xa), at(ya, xb), at(yb, xa), at(yb, xb));
            dst[y * dst_width + x] =
                (a * (1.0 - fx) + b * fx) * (1.0 - fy) + (c * (1.0 - fx) + d * fx) * fy;
        }
    }
    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_warp_interpolates_without_rounding() {
        let src: Vec<f32> = vec![0.0, 100.0, 200.0];
        let quarter = [[1.0, 0.0, 0.25], [0.0, 1.0, 0.0]];
        assert_eq!(
            warp_linear_replicate_f32(&src, 3, 1, &quarter, 3, 1),
            vec![25.0, 125.0, 200.0]
        );
    }

    #[test]
    fn linear_identity_and_integer_shift_are_exact() {
        let src: Vec<u8> = (0..20).map(|v| v * 10).collect(); // 5 x 4
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        assert_eq!(warp_linear_replicate(&src, 5, 4, &identity, 5, 4), src);
        let shift = [[1.0, 0.0, 1.0], [0.0, 1.0, 0.0]];
        let shifted = warp_linear_replicate(&src, 5, 4, &shift, 5, 4);
        // Last column reads past the edge: replicated.
        assert_eq!(&shifted[0..5], &[10, 20, 30, 40, 40]);
    }

    #[test]
    fn linear_half_pixel_averages_neighbours() {
        let src: Vec<u8> = vec![0, 100, 200];
        let half = [[1.0, 0.0, 0.5], [0.0, 1.0, 0.0]];
        let out = warp_linear_replicate(&src, 3, 1, &half, 3, 1);
        assert_eq!(out, vec![50, 150, 200]);
    }

    #[test]
    fn linear_replicates_borders_for_far_outside_samples() {
        let src: Vec<u8> = vec![7, 9];
        let far = [[1.0, 0.0, -50.0], [0.0, 1.0, 30.0]];
        assert_eq!(warp_linear_replicate(&src, 2, 1, &far, 2, 1), vec![7, 7]);
    }

    #[test]
    fn identity_copies_the_image() {
        let src: Vec<u8> = (0..12).collect();
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        assert_eq!(warp_nearest(&src, 4, 3, &identity, 4, 3, 0), src);
    }

    #[test]
    fn translation_shifts_and_fills_with_the_border() {
        let src: Vec<u8> = vec![1, 2, 3, 4, 5, 6];
        // dst(x, y) = src(x + 1, y): the last column reads outside.
        let shift = [[1.0, 0.0, 1.0], [0.0, 1.0, 0.0]];
        assert_eq!(
            warp_nearest(&src, 3, 2, &shift, 3, 2, 9),
            vec![2, 3, 9, 5, 6, 9]
        );
    }

    #[test]
    fn quarter_turn_swaps_axes() {
        // dst(x, y) = src(y, x): transpose.
        let src: Vec<u8> = vec![1, 2, 3, 4, 5, 6]; // 3 wide, 2 high
        let transpose = [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0]];
        assert_eq!(
            warp_nearest(&src, 3, 2, &transpose, 2, 3, 0),
            vec![1, 4, 2, 5, 3, 6]
        );
    }

    #[test]
    fn half_pixel_offsets_round_to_the_nearest_source_pixel() {
        let src: Vec<u8> = vec![10, 20, 30];
        // dst x reads source x + 0.4 -> pixel x (0.4 rounds down in the +512 bias).
        let slight = [[1.0, 0.0, 0.4], [0.0, 1.0, 0.0]];
        assert_eq!(warp_nearest(&src, 3, 1, &slight, 3, 1, 0), vec![10, 20, 30]);
    }
}
