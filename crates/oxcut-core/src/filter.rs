//! 8-bit image filters matching the OpenCV calls ScanSplitter's v3 makes.

/// `BORDER_REFLECT_101` index: the edge pixel is not repeated.
pub fn reflect101(index: isize, len: usize) -> usize {
    if len == 1 {
        return 0;
    }
    let period = 2 * (len as isize - 1);
    let i = index.rem_euclid(period);
    (if i >= len as isize { period - i } else { i }) as usize
}

/// `index` clamped into `0..len` (`BORDER_REPLICATE`).
pub fn replicate(index: isize, len: usize) -> usize {
    index.clamp(0, len as isize - 1) as usize
}

/// `cv2.GaussianBlur(src, (5, 5), 0)` on 8-bit data. With sigma 0 and a kernel
/// of 5, OpenCV uses the fixed binomial kernel `[1 4 6 4 1] / 16`, applied in
/// fixed point, so the result is `(sum of 256ths + 128) >> 8`.
pub fn gaussian_blur_5x5(src: &[u8], width: usize, height: usize) -> Vec<u8> {
    const KERNEL: [u32; 5] = [1, 4, 6, 4, 1];
    let mut horizontal = vec![0u32; width * height];
    for y in 0..height {
        for x in 0..width {
            horizontal[y * width + x] = KERNEL
                .iter()
                .enumerate()
                .map(|(k, &weight)| {
                    weight * src[y * width + reflect101(x as isize + k as isize - 2, width)] as u32
                })
                .sum();
        }
    }
    let mut out = vec![0u8; width * height];
    for y in 0..height {
        for x in 0..width {
            let sum: u32 = KERNEL
                .iter()
                .enumerate()
                .map(|(k, &weight)| {
                    weight * horizontal[reflect101(y as isize + k as isize - 2, height) * width + x]
                })
                .sum();
            out[y * width + x] = ((sum + 128) >> 8) as u8;
        }
    }
    out
}

/// 3x3 Sobel derivatives of 8-bit data with `BORDER_REPLICATE`, as
/// `(d/dx, d/dy)`; each fits in `i16` like OpenCV's `CV_16S`.
pub fn sobel3_replicate(src: &[u8], width: usize, height: usize) -> (Vec<i16>, Vec<i16>) {
    let at = |x: isize, y: isize| src[replicate(y, height) * width + replicate(x, width)] as i32;
    let mut dx = vec![0i16; width * height];
    let mut dy = vec![0i16; width * height];
    for y in 0..height as isize {
        for x in 0..width as isize {
            let gx = (at(x + 1, y - 1) + 2 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2 * at(x - 1, y) + at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) + 2 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2 * at(x, y - 1) + at(x + 1, y - 1));
            dx[y as usize * width + x as usize] = gx as i16;
            dy[y as usize * width + x as usize] = gy as i16;
        }
    }
    (dx, dy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_keeps_flat_regions_and_spreads_an_impulse() {
        let flat = vec![100u8; 7 * 7];
        assert_eq!(gaussian_blur_5x5(&flat, 7, 7), flat);

        let mut impulse = vec![0u8; 9 * 9];
        impulse[4 * 9 + 4] = 255;
        let blurred = gaussian_blur_5x5(&impulse, 9, 9);
        // Centre weight is 6*6/256, a direct neighbour 6*4/256, a corner 16/256.
        assert_eq!(blurred[4 * 9 + 4], ((255 * 36 + 128) >> 8) as u8);
        assert_eq!(blurred[4 * 9 + 5], ((255 * 24 + 128) >> 8) as u8);
        assert_eq!(blurred[5 * 9 + 5], ((255 * 16 + 128) >> 8) as u8);
        assert_eq!(blurred[4 * 9 + 7], 0);
    }

    #[test]
    fn sobel_responds_to_a_vertical_step() {
        // Left half 0, right half 100: horizontal gradient only.
        let mut src = vec![0u8; 6 * 4];
        for y in 0..4 {
            for x in 3..6 {
                src[y * 6 + x] = 100;
            }
        }
        let (dx, dy) = sobel3_replicate(&src, 6, 4);
        assert_eq!(dx[6 + 2], 400); // (1 + 2 + 1) * 100 across the step
        assert_eq!(dx[6 + 3], 400);
        assert_eq!(dx[6], 0);
        assert!(dy.iter().all(|&v| v == 0));
    }

    #[test]
    fn reflect_and_replicate_borders() {
        assert_eq!(reflect101(-1, 5), 1);
        assert_eq!(reflect101(5, 5), 3);
        assert_eq!(replicate(-3, 5), 0);
        assert_eq!(replicate(9, 5), 4);
    }
}
