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

#[cfg(test)]
mod tests {
    use super::*;

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
