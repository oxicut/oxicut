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

#[cfg(test)]
mod tests {
    use super::*;

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
