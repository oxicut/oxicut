//! v3 background model: the scan's paper/platen colour as a few Lab clusters.
//! Port of `_v3_background_colors` in ScanSplitter's `detector.py`.

use crate::color::LabImage;
use crate::kmeans::kmeans;

const CLUSTERS: usize = 5;
const SAMPLE_TARGET: f64 = 30_000.0;

fn dist(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Estimate the scan background as one or more Lab colour clusters.
///
/// Several clusters are kept because scanner falloff and shadows make one sheet
/// of paper span a wide lightness range. Returned in cluster-index order.
pub fn background_colors(lab: &LabImage) -> Vec<[f32; 3]> {
    let (width, height) = (lab.width, lab.height);
    let stride = ((width * height) as f64 / SAMPLE_TARGET).sqrt().max(1.0) as usize;

    let sample_width = width.div_ceil(stride);
    let sample_height = height.div_ceil(stride);
    let mut pixels = Vec::with_capacity(sample_width * sample_height);
    for y in (0..height).step_by(stride) {
        for x in (0..width).step_by(stride) {
            pixels.push(lab.at(x, y));
        }
    }

    if pixels.len() < 5 {
        let n = pixels.len() as f32;
        let mean = pixels.iter().fold([0f32; 3], |acc, p| {
            [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]]
        });
        return vec![[mean[0] / n, mean[1] / n, mean[2] / n]];
    }

    let clustering = kmeans(&pixels, CLUSTERS, 3, 30, 0.5, 0);
    let centers = &clustering.centers;
    let mut counts = [0usize; CLUSTERS];
    for &label in &clustering.labels {
        counts[label] += 1;
    }

    // Prefer a large, light cluster: white/cream paper rather than a big dark photo.
    let score: Vec<f32> = (0..CLUSTERS)
        .map(|i| counts[i] as f32 * (centers[i][0] / 255.0).clamp(0.15, 1.0).powi(2))
        .collect();
    let primary_index = argmax(&score);
    let primary = centers[primary_index];

    // A colour confined to a print is not paper: real background clusters recur
    // around the outer band of the scan.
    let border_depth =
        ((sample_width.min(sample_height) as f64 * 0.06).round_ties_even() as usize).max(1);
    let mut border_counts = [0usize; CLUSTERS];
    let mut border_total = 0usize;
    for y in 0..sample_height {
        for x in 0..sample_width {
            let on_border = y < border_depth
                || y >= sample_height.saturating_sub(border_depth)
                || x < border_depth
                || x >= sample_width.saturating_sub(border_depth);
            if on_border {
                border_counts[clustering.labels[y * sample_width + x]] += 1;
                border_total += 1;
            }
        }
    }

    let mut selected = [false; CLUSTERS];
    for i in 0..CLUSTERS {
        let chroma_delta =
            ((centers[i][1] - primary[1]).powi(2) + (centers[i][2] - primary[2]).powi(2)).sqrt();
        let similar = centers[i][0] >= primary[0] - 55.0 && chroma_delta <= 24.0;
        let border_share = border_counts[i] as f32 / border_total.max(1) as f32;
        selected[i] = similar && border_share >= 0.015;
    }
    selected[primary_index] = true;

    // A dark platen or album cover can surround the whole page: when at least
    // three corners agree on a cluster it is background too.
    let corners = [
        lab.at(0, 0),
        lab.at(width - 1, 0),
        lab.at(0, height - 1),
        lab.at(width - 1, height - 1),
    ];
    let mut corner_votes = [0usize; CLUSTERS];
    for corner in &corners {
        let nearest = (0..CLUSTERS)
            .min_by(|&a, &b| dist(corner, &centers[a]).total_cmp(&dist(corner, &centers[b])))
            .expect("non-empty");
        corner_votes[nearest] += 1;
    }
    for i in 0..CLUSTERS {
        if corner_votes[i] >= 3 {
            selected[i] = true;
        }
    }

    (0..CLUSTERS)
        .filter(|&i| selected[i])
        .map(|i| centers[i])
        .collect()
}

/// First index of the maximum, like `np.argmax`.
fn argmax(values: &[f32]) -> usize {
    let mut best = 0;
    for (i, &v) in values.iter().enumerate() {
        if v > values[best] {
            best = i;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::rgb_to_lab;
    use image::{Rgb, RgbImage};

    #[test]
    fn white_platen_with_a_dark_print_selects_white() {
        let mut rgb = RgbImage::from_pixel(400, 300, Rgb([250, 250, 250]));
        for y in 80..220 {
            for x in 100..300 {
                rgb.put_pixel(x, y, Rgb([40, 60, 90]));
            }
        }
        let centers = background_colors(&rgb_to_lab(&rgb));
        assert!(!centers.is_empty());
        assert!(
            centers.iter().all(|c| c[0] > 230.0),
            "dark print leaked into background: {centers:?}"
        );
    }

    #[test]
    fn dark_platen_is_background_when_corners_agree() {
        let mut rgb = RgbImage::from_pixel(400, 300, Rgb([20, 20, 20]));
        for y in 60..240 {
            for x in 60..340 {
                rgb.put_pixel(x, y, Rgb([235, 230, 220]));
            }
        }
        let centers = background_colors(&rgb_to_lab(&rgb));
        assert!(
            centers.iter().any(|c| c[0] < 40.0),
            "dark corners not selected: {centers:?}"
        );
    }

    #[test]
    fn tiny_images_fall_back_to_the_mean() {
        let rgb = RgbImage::from_pixel(2, 2, Rgb([255, 255, 255]));
        assert_eq!(
            background_colors(&rgb_to_lab(&rgb)),
            vec![[255.0, 128.0, 128.0]]
        );
    }
}
