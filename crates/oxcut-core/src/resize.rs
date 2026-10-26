//! Downsampling for detection, matching OpenCV's `cv2.resize(..., INTER_AREA)`.
//!
//! OpenCV has two INTER_AREA paths, both reproduced here:
//! - integer scale factors: plain block average with round-half-up
//!   (`resizeAreaFast_`);
//! - otherwise: each output pixel is the coverage-weighted mean of the source
//!   pixels its footprint overlaps (`resizeArea_`), rounded half to even like
//!   `saturate_cast<uchar>`.

use image::RgbImage;

/// Long side of the image the v3 detector works on (`detection_max_dimension`).
pub const DETECTION_MAX_DIMENSION: u32 = 1800;

/// A downsampled detection image plus the factor that maps it back to source
/// pixels (`source = detection * 1/scale`).
pub struct DetectionImage {
    pub image: RgbImage,
    pub scale: f64,
}

/// Shrink `rgb` so its long side is at most `max_dimension`. Never upscales.
pub fn for_detection(rgb: &RgbImage, max_dimension: u32) -> DetectionImage {
    let (width, height) = rgb.dimensions();
    let scale = (max_dimension as f64 / width.max(height) as f64).min(1.0);
    // Python's round() is half-to-even.
    let new_width = ((width as f64 * scale).round_ties_even() as u32).max(1);
    let new_height = ((height as f64 * scale).round_ties_even() as u32).max(1);
    DetectionImage {
        image: resize_area(rgb, new_width, new_height),
        scale,
    }
}

pub fn resize_area(src: &RgbImage, dst_width: u32, dst_height: u32) -> RgbImage {
    let (src_width, src_height) = src.dimensions();
    if (src_width, src_height) == (dst_width, dst_height) {
        return src.clone();
    }
    let scale_x = src_width as f64 / dst_width as f64;
    let scale_y = src_height as f64 / dst_height as f64;
    let int_x = scale_x.round_ties_even();
    let int_y = scale_y.round_ties_even();
    if (scale_x - int_x).abs() < f64::EPSILON
        && (scale_y - int_y).abs() < f64::EPSILON
        && int_x >= 1.0
        && int_y >= 1.0
    {
        resize_area_fast(src, dst_width, dst_height, int_x as u32, int_y as u32)
    } else {
        resize_area_weighted(src, dst_width, dst_height, scale_x, scale_y)
    }
}

fn resize_area_fast(src: &RgbImage, dst_width: u32, dst_height: u32, sx: u32, sy: u32) -> RgbImage {
    let area = sx * sy;
    let mut dst = RgbImage::new(dst_width, dst_height);
    for (x, y, pixel) in dst.enumerate_pixels_mut() {
        let mut sum = [0u32; 3];
        for yy in y * sy..(y + 1) * sy {
            for xx in x * sx..(x + 1) * sx {
                let p = src.get_pixel(xx, yy);
                for c in 0..3 {
                    sum[c] += p[c] as u32;
                }
            }
        }
        for c in 0..3 {
            pixel[c] = ((sum[c] + area / 2) / area) as u8;
        }
    }
    dst
}

/// One source index and the fraction of a destination pixel it covers.
struct Tap {
    dst: usize,
    src: usize,
    weight: f32,
}

/// Coverage table along one axis, as built by OpenCV's `computeResizeAreaTab`.
fn area_taps(src_len: u32, dst_len: u32, scale: f64) -> Vec<Tap> {
    let mut taps = Vec::new();
    for dst in 0..dst_len as usize {
        let f_start = dst as f64 * scale;
        let f_end = f_start + scale;
        let cell = scale.min(src_len as f64 - f_start);
        let mut start = f_start.ceil() as usize;
        let end = (f_end.floor() as usize).min(src_len as usize);
        if start as f64 - f_start > 1e-3 {
            taps.push(Tap {
                dst,
                src: start - 1,
                weight: ((start as f64 - f_start) / cell) as f32,
            });
        }
        while start < end {
            taps.push(Tap {
                dst,
                src: start,
                weight: (1.0 / cell) as f32,
            });
            start += 1;
        }
        if f_end - end as f64 > 1e-3 && end < src_len as usize {
            taps.push(Tap {
                dst,
                src: end,
                weight: ((f_end.min(src_len as f64) - end as f64) / cell) as f32,
            });
        }
    }
    taps
}

fn resize_area_weighted(
    src: &RgbImage,
    dst_width: u32,
    dst_height: u32,
    scale_x: f64,
    scale_y: f64,
) -> RgbImage {
    let (src_width, src_height) = src.dimensions();
    let x_taps = area_taps(src_width, dst_width, scale_x);
    let y_taps = area_taps(src_height, dst_height, scale_y);
    let row_len = dst_width as usize * 3;

    // Horizontal pass per source row into a float row buffer, then accumulate
    // rows into the destination row they contribute to.
    let mut accum = vec![0f32; row_len];
    let mut row = vec![0f32; row_len];
    let mut dst = RgbImage::new(dst_width, dst_height);
    let raw = src.as_raw();
    let mut tap_index = 0;
    while tap_index < y_taps.len() {
        let dst_y = y_taps[tap_index].dst;
        accum.fill(0.0);
        while tap_index < y_taps.len() && y_taps[tap_index].dst == dst_y {
            let y_tap = &y_taps[tap_index];
            row.fill(0.0);
            let src_row = &raw[y_tap.src * src_width as usize * 3..][..src_width as usize * 3];
            for x_tap in &x_taps {
                for c in 0..3 {
                    row[x_tap.dst * 3 + c] += src_row[x_tap.src * 3 + c] as f32 * x_tap.weight;
                }
            }
            for (a, r) in accum.iter_mut().zip(&row) {
                *a += r * y_tap.weight;
            }
            tap_index += 1;
        }
        let out = &mut dst.as_mut()[dst_y * row_len..][..row_len];
        for (o, a) in out.iter_mut().zip(&accum) {
            *o = a.round_ties_even().clamp(0.0, 255.0) as u8;
        }
    }
    dst
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn small_images_are_not_resized() {
        let rgb = RgbImage::from_pixel(100, 50, Rgb([1, 2, 3]));
        let detection = for_detection(&rgb, DETECTION_MAX_DIMENSION);
        assert_eq!(detection.scale, 1.0);
        assert_eq!(detection.image, rgb);
    }

    #[test]
    fn long_side_is_capped() {
        let rgb = RgbImage::new(3600, 2401);
        let detection = for_detection(&rgb, DETECTION_MAX_DIMENSION);
        assert_eq!(detection.scale, 0.5);
        // 2401 * 0.5 = 1200.5 rounds half to even, like Python.
        assert_eq!(detection.image.dimensions(), (1800, 1200));
    }

    #[test]
    fn integer_factor_block_average_rounds_half_up() {
        let mut rgb = RgbImage::new(2, 2);
        for (i, p) in rgb.pixels_mut().enumerate() {
            *p = Rgb([[0, 0, 1, 1][i], [10, 20, 30, 40][i], 255]);
        }
        let out = resize_area(&rgb, 1, 1);
        // (0+0+1+1)/4 = 0.5 -> 1; (10+20+30+40)/4 = 25.
        assert_eq!(out.get_pixel(0, 0), &Rgb([1, 25, 255]));
    }

    #[test]
    fn fractional_factor_weights_by_coverage() {
        // 3 -> 2 columns: dst0 = (p0 + 0.5 p1) / 1.5, dst1 = (0.5 p1 + p2) / 1.5.
        let mut rgb = RgbImage::new(3, 1);
        rgb.put_pixel(0, 0, Rgb([0, 0, 0]));
        rgb.put_pixel(1, 0, Rgb([90, 90, 90]));
        rgb.put_pixel(2, 0, Rgb([180, 180, 180]));
        let out = resize_area(&rgb, 2, 1);
        assert_eq!(out.get_pixel(0, 0), &Rgb([30, 30, 30]));
        assert_eq!(out.get_pixel(1, 0), &Rgb([150, 150, 150]));
    }

    #[test]
    fn constant_image_stays_constant() {
        let rgb = RgbImage::from_pixel(37, 23, Rgb([200, 100, 50]));
        let out = resize_area(&rgb, 11, 7);
        assert!(out.pixels().all(|p| *p == Rgb([200, 100, 50])));
    }
}
