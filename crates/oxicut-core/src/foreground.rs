//! v3 foreground evidence: how far each pixel is from the background colours,
//! thresholded with Otsu. Port of the first half of `detect_photos_v3`.

use crate::color::LabImage;

/// Foreground never starts below this Lab distance, whatever Otsu says.
const MIN_COLOR_THRESHOLD: f32 = 18.0;
/// The floor when the Otsu threshold is capped.
const CAPPED_MIN_COLOR_THRESHOLD: f32 = 12.0;

pub struct Foreground {
    pub width: usize,
    pub height: usize,
    /// Row-major, `true` where the pixel is not background.
    pub mask: Vec<bool>,
    /// The Otsu threshold before the 18.0 floor.
    pub otsu: u8,
}

/// Distance in Lab to the nearest background colour, per pixel. Float32 with
/// numpy's evaluation order so thresholds fall on the same side as Python's.
pub fn background_distance(lab: &LabImage, backgrounds: &[[f32; 3]]) -> Vec<f32> {
    lab.data
        .iter()
        .map(|pixel| {
            let mut nearest = f32::INFINITY;
            for background in backgrounds {
                let d0 = pixel[0] - background[0];
                let d1 = pixel[1] - background[1];
                let d2 = pixel[2] - background[2];
                let distance = (d0 * d0 + d1 * d1 + d2 * d2).sqrt();
                nearest = nearest.min(distance);
            }
            nearest
        })
        .collect()
}

/// `cv2.threshold(..., THRESH_BINARY | THRESH_OTSU)`'s threshold for 8-bit data:
/// the first level maximising the between-class variance.
pub fn otsu_threshold(values: &[u8]) -> u8 {
    let mut histogram = [0u32; 256];
    for &v in values {
        histogram[v as usize] += 1;
    }
    let total = values.len() as f64;
    let scale = 1.0 / total;
    let mut mu = 0.0;
    for (i, &h) in histogram.iter().enumerate() {
        mu += i as f64 * h as f64;
    }
    mu *= scale;

    let (mut mu1, mut q1) = (0.0f64, 0.0f64);
    let (mut max_sigma, mut threshold) = (0.0f64, 0u8);
    for (i, &h) in histogram.iter().enumerate() {
        let p = h as f64 * scale;
        mu1 *= q1;
        q1 += p;
        let q2 = 1.0 - q1;
        if q1.min(q2) < f64::EPSILON || q1.max(q2) > 1.0 - f64::EPSILON {
            continue;
        }
        mu1 = (mu1 + i as f64 * p) / q1;
        let mu2 = (mu - q1 * mu1) / q2;
        let sigma = q1 * q2 * (mu1 - mu2) * (mu1 - mu2);
        if sigma > max_sigma {
            max_sigma = sigma;
            threshold = i as u8;
        }
    }
    threshold
}

pub fn foreground(lab: &LabImage, backgrounds: &[[f32; 3]]) -> Foreground {
    foreground_capped(lab, backgrounds, None)
}

/// [`foreground`] with v7's optional cap on the Otsu threshold: when large pale areas (lid,
/// bright page edge) push Otsu far above the contrast of light photographs, the cap keeps
/// them in, and the floor drops from 18 to 12.
pub fn foreground_capped(lab: &LabImage, backgrounds: &[[f32; 3]], cap: Option<f32>) -> Foreground {
    let delta = background_distance(lab, backgrounds);
    // numpy: np.clip(delta, 0, 255).astype(np.uint8) truncates toward zero.
    let quantised: Vec<u8> = delta.iter().map(|&d| d.clamp(0.0, 255.0) as u8).collect();
    let otsu = otsu_threshold(&quantised);
    let (floor, level) = match cap {
        Some(cap) => (CAPPED_MIN_COLOR_THRESHOLD, (otsu as f32).min(cap)),
        None => (MIN_COLOR_THRESHOLD, otsu as f32),
    };
    let threshold = floor.max(level);
    Foreground {
        width: lab.width,
        height: lab.height,
        mask: delta.iter().map(|&d| d > threshold).collect(),
        otsu,
    }
}

/// How much of a difference in lightness counts when looking for pale paper: shadows and scanner
/// falloff change the lightness of the lid, a cream or tan card changes its yellowness.
const PAPER_LIGHTNESS_WEIGHT: f32 = 0.15;
/// Distance (lightness weighted down) from the lid above which a smoothed pixel is paper.
const PAPER_THRESHOLD: f32 = 8.0;
/// Radius of the smoothing before the paper test, in detection pixels.
const PAPER_BLUR_RADIUS: usize = 4;

/// Mean over a square window (clamped at the edges), per channel.
fn box_blur(lab: &LabImage, radius: usize) -> LabImage {
    let (w, h) = (lab.width, lab.height);
    let pass = |src: &[[f32; 3]], horizontal: bool| -> Vec<[f32; 3]> {
        let mut out = vec![[0f32; 3]; src.len()];
        let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
        for o in 0..outer {
            let at = |i: usize| if horizontal { o * w + i } else { i * w + o };
            let mut sum = [0f64; 3];
            let (mut low, mut high) = (0usize, 0usize);
            for i in 0..inner {
                let want_high = (i + radius + 1).min(inner);
                while high < want_high {
                    for c in 0..3 {
                        sum[c] += src[at(high)][c] as f64;
                    }
                    high += 1;
                }
                let want_low = i.saturating_sub(radius);
                while low < want_low {
                    for c in 0..3 {
                        sum[c] -= src[at(low)][c] as f64;
                    }
                    low += 1;
                }
                let count = (high - low) as f64;
                for c in 0..3 {
                    out[at(i)][c] = (sum[c] / count) as f32;
                }
            }
        }
        out
    };
    let horizontal = pass(&lab.data, true);
    LabImage {
        width: w,
        height: h,
        data: pass(&horizontal, false),
    }
}

/// [`foreground`] plus pale paper: pixels that, once smoothed, are more yellow (or otherwise coloured)
/// than the lid even though their distance is below the Otsu threshold. A postcard with a cream margin
/// and strongly coloured art puts that threshold far above the paper's own difference from the lid.
/// Lightness counts little here, so the soft shadows round a card are not mistaken for paper.
pub fn paper_foreground(lab: &LabImage, backgrounds: &[[f32; 3]]) -> Foreground {
    let mut foreground = foreground(lab, backgrounds);
    let smooth = box_blur(lab, PAPER_BLUR_RADIUS);
    for (pixel, flag) in smooth.data.iter().zip(foreground.mask.iter_mut()) {
        if *flag {
            continue;
        }
        let nearest = backgrounds
            .iter()
            .map(|b| {
                let light = (pixel[0] - b[0]) * PAPER_LIGHTNESS_WEIGHT;
                (light * light + (pixel[1] - b[1]).powi(2) + (pixel[2] - b[2]).powi(2)).sqrt()
            })
            .fold(f32::INFINITY, f32::min);
        *flag = nearest > PAPER_THRESHOLD;
    }
    foreground
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pale_yellowish_paper_is_foreground_but_a_lightness_change_alone_is_not() {
        let lid = [240.0, 127.0, 131.0];
        let width = 60;
        let mut data = vec![lid; width * 20];
        for y in 0..20 {
            for x in 0..20 {
                // Cream card: only the yellowness differs (b 131 to 139).
                data[y * width + x] = [237.0, 127.0, 139.0];
            }
            for x in 40..60 {
                // A shadow: lighter or darker lid, same colour.
                data[y * width + x] = [228.0, 127.0, 131.0];
            }
        }
        let lab = LabImage { width, height: 20, data };
        let fg = paper_foreground(&lab, &[lid]);
        assert!(fg.mask[10 * width + 10], "the cream card is paper");
        assert!(!fg.mask[10 * width + 50], "a shadow is not");
        assert!(!fg.mask[10 * width + 30], "the lid is not");
        // The plain foreground sees neither: both are within the 18 floor.
        let plain = foreground(&lab, &[lid]);
        assert!(!plain.mask[10 * width + 10]);
    }

    #[test]
    fn otsu_splits_two_clusters_at_the_first_best_level() {
        let mut values = vec![10u8; 500];
        values.extend(vec![200u8; 500]);
        // Any level in 10..200 separates perfectly; OpenCV keeps the first.
        assert_eq!(otsu_threshold(&values), 10);
    }

    #[test]
    fn single_level_image_has_threshold_zero() {
        assert_eq!(otsu_threshold(&[77u8; 100]), 0);
    }

    #[test]
    fn distance_uses_the_nearest_background() {
        let lab = LabImage {
            width: 2,
            height: 1,
            data: vec![[100.0, 128.0, 128.0], [250.0, 128.0, 128.0]],
        };
        let delta = background_distance(&lab, &[[255.0, 128.0, 128.0], [100.0, 128.0, 128.0]]);
        assert_eq!(delta, vec![0.0, 5.0]);
    }

    #[test]
    fn floor_keeps_faint_differences_out_of_the_foreground() {
        // Everything within 10 levels of the background: Otsu may pick a low
        // threshold, but nothing at distance <= 18 may become foreground.
        let lab = LabImage {
            width: 4,
            height: 1,
            data: vec![
                [255.0, 128.0, 128.0],
                [250.0, 128.0, 128.0],
                [245.0, 128.0, 128.0],
                [240.0, 128.0, 128.0],
            ],
        };
        let fg = foreground(&lab, &[[255.0, 128.0, 128.0]]);
        assert!(fg.mask.iter().all(|&b| !b));
    }
}
