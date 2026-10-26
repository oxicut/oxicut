//! Pillow's grey conversion and antialiased bilinear resize, bit for bit.
//!
//! ScanSplitter's v7 builds its working grey images with
//! `image.convert("L").resize(size, Image.BILINEAR)`. Pillow resamples with a
//! triangle filter widened by the shrink factor (so it antialiases), using
//! 22-bit fixed-point coefficients, one horizontal then one vertical pass.

use image::{GrayImage, RgbImage};

const PRECISION_BITS: u32 = 32 - 8 - 2;

/// `Image.convert("L")`: ITU-R 601 luma in 16-bit fixed point.
pub fn to_gray(rgb: &RgbImage) -> GrayImage {
    let (width, height) = rgb.dimensions();
    let data = rgb
        .pixels()
        .map(|p| {
            ((19595 * p[0] as u32 + 38470 * p[1] as u32 + 7471 * p[2] as u32 + 0x8000) >> 16) as u8
        })
        .collect();
    GrayImage::from_raw(width, height, data).expect("same dimensions")
}

fn triangle(x: f64) -> f64 {
    let x = x.abs();
    if x < 1.0 { 1.0 - x } else { 0.0 }
}

/// Per output pixel: first source index and the fixed-point weights.
struct Taps {
    start: Vec<usize>,
    weights: Vec<Vec<i32>>,
}

fn taps(in_size: usize, out_size: usize) -> Taps {
    let scale = in_size as f64 / out_size as f64;
    let filter_scale = scale.max(1.0);
    let support = 1.0 * filter_scale;
    let inverse = 1.0 / filter_scale;
    let (mut start, mut weights) = (Vec::with_capacity(out_size), Vec::with_capacity(out_size));
    for xx in 0..out_size {
        let center = (xx as f64 + 0.5) * scale;
        let xmin = ((center - support + 0.5) as i64).max(0) as usize;
        let xmax =
            (((center + support + 0.5) as i64).min(in_size as i64)) as usize - xmin.min(in_size);
        let mut raw: Vec<f64> = (0..xmax)
            .map(|x| triangle((x as f64 + xmin as f64 - center + 0.5) * inverse))
            .collect();
        let total: f64 = raw.iter().sum();
        if total != 0.0 {
            raw.iter_mut().for_each(|w| *w /= total);
        }
        let fixed = raw
            .iter()
            .map(|&w| {
                if w < 0.0 {
                    (-0.5 + w * (1u64 << PRECISION_BITS) as f64) as i32
                } else {
                    (0.5 + w * (1u64 << PRECISION_BITS) as f64) as i32
                }
            })
            .collect();
        start.push(xmin);
        weights.push(fixed);
    }
    Taps { start, weights }
}

fn clip8(accumulator: i64) -> u8 {
    (accumulator >> PRECISION_BITS).clamp(0, 255) as u8
}

/// `Image.resize((width, height), Image.BILINEAR)` on an 8-bit grey image.
pub fn resize_bilinear(src: &GrayImage, width: u32, height: u32) -> GrayImage {
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    let (dw, dh) = (width as usize, height as usize);
    if (sw, sh) == (dw, dh) {
        return src.clone();
    }
    let half = 1i64 << (PRECISION_BITS - 1);

    // Horizontal pass: every source row, `dw` columns.
    let horizontal = taps(sw, dw);
    let mut temp = vec![0u8; dw * sh];
    for y in 0..sh {
        let row = &src.as_raw()[y * sw..(y + 1) * sw];
        for x in 0..dw {
            let acc = half
                + horizontal.weights[x]
                    .iter()
                    .enumerate()
                    .map(|(i, &k)| row[horizontal.start[x] + i] as i64 * k as i64)
                    .sum::<i64>();
            temp[y * dw + x] = clip8(acc);
        }
    }

    // Vertical pass over the intermediate image.
    let vertical = taps(sh, dh);
    let mut out = vec![0u8; dw * dh];
    for y in 0..dh {
        for x in 0..dw {
            let acc = half
                + vertical.weights[y]
                    .iter()
                    .enumerate()
                    .map(|(i, &k)| temp[(vertical.start[y] + i) * dw + x] as i64 * k as i64)
                    .sum::<i64>();
            out[y * dw + x] = clip8(acc);
        }
    }
    GrayImage::from_raw(width, height, out).expect("same dimensions")
}

/// Grey copy shrunk so its long side is at most `max_side`, as v7 builds its
/// working images (`convert("L")`, then a bilinear resize). Returns the image
/// and the scale applied (1.0 when it was not shrunk).
pub fn working_gray(rgb: &RgbImage, max_side: u32) -> (GrayImage, f64) {
    let gray = to_gray(rgb);
    let scale = (max_side as f64 / gray.width().max(gray.height()) as f64).min(1.0);
    let width = ((gray.width() as f64 * scale).round_ties_even() as u32).max(1);
    let height = ((gray.height() as f64 * scale).round_ties_even() as u32).max(1);
    (resize_bilinear(&gray, width, height), scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn grey_uses_pillows_weights() {
        let mut rgb = RgbImage::new(4, 1);
        for (x, c) in [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]]
            .iter()
            .enumerate()
        {
            rgb.put_pixel(x as u32, 0, Rgb(*c));
        }
        // 0.299, 0.587, 0.114 of 255, rounded.
        assert_eq!(to_gray(&rgb).as_raw(), &[76, 150, 29, 255]);
    }

    #[test]
    fn flat_images_stay_flat_at_any_size() {
        let flat = GrayImage::from_pixel(97, 61, image::Luma([173]));
        for (w, h) in [(50, 30), (33, 61), (97, 7), (200, 120)] {
            assert!(
                resize_bilinear(&flat, w, h).pixels().all(|p| p.0[0] == 173),
                "{w}x{h}"
            );
        }
    }

    #[test]
    fn halving_uses_the_widened_triangle_filter() {
        // 4 -> 2: the triangle is widened to the shrink factor. Output 0 is centred
        // on 1.0 and sees pixels 0..=2 with weights 0.75, 0.75, 0.25 (normalised
        // to 3/7, 3/7, 1/7); output 1 is centred on 3.0 and sees pixels 1..=3.
        let src = GrayImage::from_raw(4, 1, vec![0, 80, 160, 240]).unwrap();
        let out = resize_bilinear(&src, 2, 1);
        // 80 * 3/7 + 160 * 1/7 = 57.14; 80 * 1/7 + 160 * 3/7 + 240 * 3/7 = 182.86.
        assert_eq!(out.as_raw(), &[57, 183]);
    }

    #[test]
    fn a_gradient_stays_monotonic_when_shrunk_in_both_directions() {
        let src = GrayImage::from_fn(100, 80, |x, y| image::Luma([((x * 2 + y) / 2) as u8]));
        let out = resize_bilinear(&src, 37, 29);
        for y in 0..29 {
            for x in 1..37 {
                assert!(
                    out.get_pixel(x, y).0[0] >= out.get_pixel(x - 1, y).0[0],
                    "row {y} col {x}"
                );
            }
        }
    }
    #[test]
    fn working_gray_shrinks_only_large_images() {
        let small = RgbImage::from_pixel(300, 200, Rgb([10, 20, 30]));
        let (g, scale) = working_gray(&small, 2000);
        assert_eq!((g.dimensions(), scale), ((300, 200), 1.0));
        let big = RgbImage::from_pixel(4000, 3000, Rgb([10, 20, 30]));
        let (g, scale) = working_gray(&big, 2000);
        assert_eq!((g.dimensions(), scale), ((2000, 1500), 0.5));
    }
}
