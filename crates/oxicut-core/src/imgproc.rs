//! Small image-processing pieces v5/v7 need that the v3 port did not: Gaussian blur
//! with a sigma, grey-level dilation, and `cv2.remap`-style bilinear sampling.

use crate::filter::{reflect101, replicate};

/// `cv2.getGaussianKernel(ksize, sigma)`.
fn gaussian_kernel(ksize: usize, sigma: f64) -> Vec<f32> {
    let centre = (ksize / 2) as f64;
    let weights: Vec<f64> = (0..ksize)
        .map(|i| (-((i as f64 - centre).powi(2)) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f64 = weights.iter().sum();
    weights.iter().map(|w| (w / sum) as f32).collect()
}

/// Kernel size `GaussianBlur` picks for `ksize = 0`: `round(sigma * k + 1) | 1` with
/// `k = 6` for floating point images and `8` for 8-bit ones.
fn auto_ksize(sigma: f64, factor: f64) -> usize {
    ((sigma * factor + 1.0).round_ties_even() as usize) | 1
}

/// Separable convolution with the kernel, borders reflected without repeating the edge
/// pixel (`BORDER_REFLECT_101`, OpenCV's default).
fn convolve(src: &[f32], width: usize, height: usize, kernel: &[f32]) -> Vec<f32> {
    let r = (kernel.len() / 2) as isize;
    let mut horizontal = vec![0f32; src.len()];
    for y in 0..height {
        let row = &src[y * width..(y + 1) * width];
        for x in 0..width {
            let mut sum = 0f32;
            for (k, &weight) in kernel.iter().enumerate() {
                let sx = reflect101(x as isize + k as isize - r, width);
                sum += row[sx] * weight;
            }
            horizontal[y * width + x] = sum;
        }
    }
    let mut out = vec![0f32; src.len()];
    for y in 0..height {
        for (k, &weight) in kernel.iter().enumerate() {
            let sy = reflect101(y as isize + k as isize - r, height);
            let row = &horizontal[sy * width..(sy + 1) * width];
            for (acc, &v) in out[y * width..(y + 1) * width].iter_mut().zip(row) {
                *acc += v * weight;
            }
        }
    }
    out
}

/// `cv2.GaussianBlur(src, (0, 0), sigma)` on a float32 image.
pub fn gaussian_blur_f32(src: &[f32], width: usize, height: usize, sigma: f64) -> Vec<f32> {
    let kernel = gaussian_kernel(auto_ksize(sigma, 6.0), sigma);
    convolve(src, width, height, &kernel)
}

/// `cv2.GaussianBlur(src, (0, 0), sigma)` on an 8-bit image.
pub fn gaussian_blur_u8(src: &[u8], width: usize, height: usize, sigma: f64) -> Vec<u8> {
    let kernel = gaussian_kernel(auto_ksize(sigma, 8.0), sigma);
    let as_float: Vec<f32> = src.iter().map(|&v| v as f32).collect();
    convolve(&as_float, width, height, &kernel)
        .into_iter()
        .map(|v| v.round().clamp(0.0, 255.0) as u8)
        .collect()
}

/// Grey-level dilation by an ellipse `size` pixels across (`cv2.dilate` with
/// `MORPH_ELLIPSE`): each pixel becomes the maximum under the ellipse.
pub fn dilate_ellipse_u8(src: &[u8], width: usize, height: usize, size: usize) -> Vec<u8> {
    let r = (size / 2) as f64;
    let inverse_r2 = if r > 0.0 { 1.0 / (r * r) } else { 0.0 };
    // The ellipse as horizontal runs, as `mask::Element::ellipse` builds it.
    let runs: Vec<(isize, usize)> = (0..size)
        .map(|i| {
            let dy = i as f64 - r;
            let dx = (r * ((r * r - dy * dy) * inverse_r2).sqrt()).round_ties_even();
            (dy as isize, dx as usize)
        })
        .collect();

    // Sliding maximum of every row for each distinct half-width, computed on demand.
    let mut by_width: std::collections::BTreeMap<usize, Vec<u8>> = Default::default();
    for &(_, dx) in &runs {
        by_width
            .entry(dx)
            .or_insert_with(|| row_maximum(src, width, height, dx));
    }
    let mut out = vec![0u8; src.len()];
    for y in 0..height {
        for &(dy, dx) in &runs {
            let sy = y as isize + dy;
            if sy < 0 || sy >= height as isize {
                continue;
            }
            let row = &by_width[&dx][sy as usize * width..(sy as usize + 1) * width];
            for (acc, &v) in out[y * width..(y + 1) * width].iter_mut().zip(row) {
                *acc = (*acc).max(v);
            }
        }
    }
    out
}

/// Maximum over `x - half ..= x + half` for every pixel, per row (van Herk would be
/// faster; a monotonic deque keeps it linear).
fn row_maximum(src: &[u8], width: usize, height: usize, half: usize) -> Vec<u8> {
    let mut out = vec![0u8; src.len()];
    let mut window: std::collections::VecDeque<usize> = Default::default();
    for y in 0..height {
        let row = &src[y * width..(y + 1) * width];
        window.clear();
        let mut next = 0usize;
        for x in 0..width {
            let hi = (x + half).min(width - 1);
            while next <= hi {
                while window.back().is_some_and(|&b| row[b] <= row[next]) {
                    window.pop_back();
                }
                window.push_back(next);
                next += 1;
            }
            while window.front().is_some_and(|&f| f + half < x) {
                window.pop_front();
            }
            out[y * width + x] = row[*window.front().expect("window holds x")];
        }
    }
    out
}

/// A float32 grey image sampled like `cv2.remap(..., INTER_LINEAR, BORDER_REPLICATE)`.
pub struct Sampler<'a> {
    pub data: &'a [f32],
    pub width: usize,
    pub height: usize,
}

impl Sampler<'_> {
    /// Bilinear sample at `(x, y)`. OpenCV rounds coordinates to 1/32 pixel before
    /// interpolating; so does this.
    pub fn at(&self, x: f32, y: f32) -> f32 {
        const SCALE: f32 = 32.0;
        let fx = (x * SCALE).round_ties_even() as i64;
        let fy = (y * SCALE).round_ties_even() as i64;
        let (ix, iy) = ((fx >> 5) as isize, (fy >> 5) as isize);
        let (ax, ay) = ((fx & 31) as f32 / SCALE, (fy & 31) as f32 / SCALE);
        let px = |dx: isize, dy: isize| {
            let sx = replicate(ix + dx, self.width);
            let sy = replicate(iy + dy, self.height);
            self.data[sy * self.width + sx]
        };
        let top = px(0, 0) * (1.0 - ax) + px(1, 0) * ax;
        let bottom = px(0, 1) * (1.0 - ax) + px(1, 1) * ax;
        top * (1.0 - ay) + bottom * ay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_keeps_a_flat_image_flat_and_spreads_an_impulse() {
        let flat = vec![100.0f32; 30 * 20];
        let blurred = gaussian_blur_f32(&flat, 30, 20, 2.0);
        assert!(blurred.iter().all(|v| (v - 100.0).abs() < 1e-3));

        let mut impulse = vec![0.0f32; 21 * 21];
        impulse[10 * 21 + 10] = 1000.0;
        let out = gaussian_blur_f32(&impulse, 21, 21, 1.0);
        let total: f32 = out.iter().sum();
        assert!((total - 1000.0).abs() < 1.0, "energy kept: {total}");
        assert!(out[10 * 21 + 10] > out[10 * 21 + 12]);
    }

    #[test]
    fn dilation_takes_the_maximum_under_the_ellipse() {
        let mut src = vec![0u8; 15 * 15];
        src[7 * 15 + 7] = 200;
        let out = dilate_ellipse_u8(&src, 15, 15, 5);
        assert_eq!(out[7 * 15 + 7], 200);
        assert_eq!(out[7 * 15 + 9], 200); // two to the right is inside a 5-wide ellipse
        assert_eq!(out[7 * 15 + 10], 0);
        assert_eq!(out[9 * 15 + 9], 0); // the corner of the box is outside the ellipse
    }

    #[test]
    fn sampling_interpolates_and_replicates_the_border() {
        let data = [0.0f32, 10.0, 20.0, 30.0];
        let sampler = Sampler {
            data: &data,
            width: 2,
            height: 2,
        };
        assert_eq!(sampler.at(0.0, 0.0), 0.0);
        assert!((sampler.at(0.5, 0.0) - 5.0).abs() < 1e-5);
        assert!((sampler.at(0.5, 0.5) - 15.0).abs() < 1e-5);
        assert_eq!(sampler.at(-5.0, -5.0), 0.0);
        assert_eq!(sampler.at(9.0, 9.0), 30.0);
    }
}
