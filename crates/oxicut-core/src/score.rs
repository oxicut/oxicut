//! v7's evidence for choosing between proposals: how strongly each side of a candidate is a
//! brightness step (`_score_rect`), how crisp those steps are (`_edge_sharpness`), whether the
//! inside looks like a print seen through a translucent sleeve (`_ghost_stats`), and the
//! arbitration that uses them (`_select`, `_drop_ghosts`).

use crate::geometry::{RotatedRect, overlap_fraction};
use crate::imgproc::{Sampler, gaussian_blur_f32};
use crate::tighten::GrayF32;

/// One proposal after tightening, with its evidence.
#[derive(Debug, Clone, Copy)]
pub struct Candidate {
    pub rect: RotatedRect,
    pub score: f64,
    pub sharp: f64,
    /// Low-frequency contrast inside the print.
    pub low_std: f64,
    /// Fine-detail energy inside the print.
    pub fine: f64,
}

fn sampler(gray: &GrayF32) -> Sampler<'_> {
    Sampler {
        data: &gray.data,
        width: gray.width,
        height: gray.height,
    }
}

/// Unit vectors of the rectangle's axes and its centre/size in working pixels.
struct Frame {
    ux: [f64; 2],
    uy: [f64; 2],
    centre: [f64; 2],
    w: f64,
    h: f64,
}

impl Frame {
    fn new(rect: &RotatedRect, scale: f64) -> Self {
        let t = rect.angle.to_radians();
        Self {
            ux: [t.cos(), t.sin()],
            uy: [-t.sin(), t.cos()],
            centre: [rect.cx * scale, rect.cy * scale],
            w: rect.w * scale,
            h: rect.h * scale,
        }
    }

    /// Normal, tangent, and half extents of the four sides, in the order v7 visits them.
    fn sides(&self) -> [([f64; 2], [f64; 2], f64, f64); 4] {
        let neg = |v: [f64; 2]| [-v[0], -v[1]];
        [
            (self.ux, self.uy, self.w / 2.0, self.h / 2.0),
            (neg(self.ux), self.uy, self.w / 2.0, self.h / 2.0),
            (self.uy, self.ux, self.h / 2.0, self.w / 2.0),
            (neg(self.uy), self.ux, self.h / 2.0, self.w / 2.0),
        ]
    }

    /// 41 sample points along a side, from -80 % to +80 % of its half length.
    fn along(
        &self,
        normal: [f64; 2],
        tangent: [f64; 2],
        half_n: f64,
        half_t: f64,
    ) -> Vec<[f64; 2]> {
        (0..41)
            .map(|i| {
                let f = -0.8 + 1.6 * i as f64 / 40.0;
                [
                    self.centre[0] + normal[0] * half_n + tangent[0] * (f * half_t),
                    self.centre[1] + normal[1] * half_n + tangent[1] * (f * half_t),
                ]
            })
            .collect()
    }
}

/// `np.percentile(values, q)` with linear interpolation.
pub fn percentile(values: &[f64], q: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let position = (sorted.len() - 1) as f64 * q / 100.0;
    let lower = position.floor() as usize;
    let upper = (lower + 1).min(sorted.len() - 1);
    sorted[lower] + (sorted[upper] - sorted[lower]) * (position - lower as f64)
}

/// `_side_contrast`: robust inside/outside brightness step along each side.
pub fn side_contrast(gray: &GrayF32, rect: &RotatedRect, scale: f64) -> [f64; 4] {
    let frame = Frame::new(rect, scale);
    let image = sampler(gray);
    const BAND: i32 = 3;
    let mut out = [0.0; 4];
    for (slot, (normal, tangent, half_n, half_t)) in out.iter_mut().zip(frame.sides()) {
        let base = frame.along(normal, tangent, half_n, half_t);
        let mut best = vec![0f32; base.len()];
        for shift in [-3.0, -1.5, 0.0, 1.5, 3.0] {
            for (best, point) in best.iter_mut().zip(&base) {
                let (mut inside, mut outside) = (0f32, 0f32);
                for k in 1..=BAND {
                    let at = |offset: f64| {
                        image.at(
                            (point[0] - normal[0] * offset) as f32,
                            (point[1] - normal[1] * offset) as f32,
                        )
                    };
                    inside += at(shift + k as f64);
                    outside += at(shift - k as f64);
                }
                *best = best.max((inside - outside).abs() / BAND as f32);
            }
        }
        let values: Vec<f64> = best.iter().map(|&v| v as f64).collect();
        *slot = percentile(&values, 30.0);
    }
    out
}

/// `_score_rect`: the mean side contrast, each side capped at 30.
pub fn score_rect(gray: &GrayF32, rect: &RotatedRect, scale: f64) -> f64 {
    side_contrast(gray, rect, scale)
        .iter()
        .map(|&c| c.min(30.0))
        .sum::<f64>()
        / 4.0
}

/// `_edge_sharpness`: the weakest side's peak gradient over its total step, a median along
/// the side. A print edge completes its step within a couple of pixels; a print behind a
/// translucent sleeve is blurred.
pub fn edge_sharpness(gray: &GrayF32, rect: &RotatedRect, scale: f64) -> f64 {
    const HALF: i32 = 12;
    let frame = Frame::new(rect, scale);
    let image = sampler(gray);
    let mut weakest = f64::INFINITY;
    for (normal, tangent, half_n, half_t) in frame.sides() {
        let mut ratios: Vec<f64> = Vec::new();
        for point in frame.along(normal, tangent, half_n, half_t) {
            let profile: Vec<f32> = (-HALF..=HALF)
                .map(|t| {
                    image.at(
                        (point[0] + t as f64 * normal[0]) as f32,
                        (point[1] + t as f64 * normal[1]) as f32,
                    )
                })
                .collect();
            let n = profile.len();
            let mean = |s: &[f32]| s.iter().sum::<f32>() / s.len() as f32;
            let step = (mean(&profile[n - 7..]) - mean(&profile[..7])).abs();
            let grad = (2..n)
                .map(|i| (profile[i] - profile[i - 2]).abs())
                .fold(0f32, f32::max)
                / 2.0;
            if step >= 6.0 {
                ratios.push((grad / step) as f64);
            }
        }
        let side = if ratios.is_empty() {
            0.0
        } else {
            percentile(&ratios, 50.0)
        };
        weakest = weakest.min(side);
    }
    weakest
}

/// The grey image's blurred versions `_ghost_stats` reads, made once per scan.
pub struct GhostMaps {
    pub width: usize,
    pub height: usize,
    low: Vec<f32>,
    detail: Vec<f32>,
}

impl GhostMaps {
    pub fn new(gray: &GrayF32) -> Self {
        let (w, h) = (gray.width, gray.height);
        let low = gaussian_blur_f32(&gray.data, w, h, 4.0);
        let fine = gaussian_blur_f32(&gray.data, w, h, 1.0);
        let coarse = gaussian_blur_f32(&gray.data, w, h, 3.0);
        Self {
            width: w,
            height: h,
            low,
            detail: fine
                .iter()
                .zip(&coarse)
                .map(|(a, b)| (a - b).abs())
                .collect(),
        }
    }

    /// `_ghost_stats`: (low-frequency spread, fine-detail energy) inside the central 80 %.
    pub fn stats(&self, rect: &RotatedRect, scale: f64) -> (f64, f64) {
        let inner = RotatedRect::new(
            rect.cx * scale,
            rect.cy * scale,
            rect.w * scale * 0.8,
            rect.h * scale * 0.8,
            rect.angle,
        );
        // fillPoly takes integer vertices: the float32 corners truncated toward zero.
        let polygon: Vec<(i64, i64)> = inner
            .corners_f32()
            .iter()
            .map(|&(x, y)| (x as i64, y as i64))
            .collect();
        let (mut count, mut sum, mut sum_sq, mut detail) = (0usize, 0f64, 0f64, 0f64);
        let y0 = polygon.iter().map(|p| p.1).min().unwrap().max(0);
        let y1 = polygon
            .iter()
            .map(|p| p.1)
            .max()
            .unwrap()
            .min(self.height as i64 - 1);
        let x0 = polygon.iter().map(|p| p.0).min().unwrap().max(0);
        let x1 = polygon
            .iter()
            .map(|p| p.0)
            .max()
            .unwrap()
            .min(self.width as i64 - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                if !inside_or_on(&polygon, (x, y)) {
                    continue;
                }
                let index = y as usize * self.width + x as usize;
                let v = self.low[index] as f64;
                count += 1;
                sum += v;
                sum_sq += v * v;
                detail += self.detail[index] as f64;
            }
        }
        if count < 50 {
            return (99.0, 99.0);
        }
        let n = count as f64;
        let mean = sum / n;
        ((sum_sq / n - mean * mean).max(0.0).sqrt(), detail / n)
    }
}

/// Whether an integer point lies inside or on a convex polygon with integer vertices.
fn inside_or_on(polygon: &[(i64, i64)], point: (i64, i64)) -> bool {
    let (mut positive, mut negative) = (false, false);
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        let cross = (b.0 - a.0) * (point.1 - a.1) - (b.1 - a.1) * (point.0 - a.0);
        positive |= cross > 0;
        negative |= cross < 0;
    }
    !(positive && negative)
}

/// `_low_detail`: a blurred, low-contrast interior (crinkled glassine over a print).
pub fn low_detail(low_std: f64, fine: f64) -> bool {
    (low_std < 22.0 && fine < 4.6) || (fine < 2.6 && low_std < 30.0)
}

/// `_select`: anchors survive unless an overlapping alternative scores clearly higher;
/// alternatives with no anchor must be strong on their own; boxes mostly contained in a
/// better accepted box are dropped.
pub fn select(anchors: &[Candidate], alternatives: &[Candidate]) -> Vec<Candidate> {
    const MARGIN: f64 = 3.0;
    const STANDALONE: f64 = 22.0;
    const CONTAINED: f64 = 0.6;

    let mut chosen: Vec<Candidate> = Vec::new();
    for item in anchors {
        let mut best = *item;
        for alt in alternatives {
            if overlap_fraction(&item.rect, &alt.rect) > 0.5 && alt.score > best.score + MARGIN {
                best = *alt;
            }
        }
        chosen.push(best);
    }
    let mut by_score: Vec<&Candidate> = alternatives.iter().collect();
    by_score.sort_by(|a, b| b.score.total_cmp(&a.score));
    for alt in by_score {
        if alt.score >= STANDALONE
            && !chosen
                .iter()
                .any(|c| overlap_fraction(&alt.rect, &c.rect) > 0.5)
        {
            chosen.push(*alt);
        }
    }
    let mut ordered = chosen;
    ordered.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut result: Vec<Candidate> = Vec::new();
    for item in ordered {
        if result
            .iter()
            .any(|o| overlap_fraction(&item.rect, &o.rect) > CONTAINED)
        {
            continue;
        }
        result.push(item);
    }
    result
}

/// `_drop_ghosts`: remove blurry prints seen through sleeves, judged against the sharp
/// prints of the same scan, so a uniformly soft scan keeps everything.
pub fn drop_ghosts(items: Vec<Candidate>) -> Vec<Candidate> {
    const Q: f64 = 75.0;
    const K: f64 = 0.8;
    if items.len() < 2 {
        return items;
    }
    let detailed: Vec<f64> = items
        .iter()
        .filter(|a| !low_detail(a.low_std, a.fine))
        .map(|a| a.sharp)
        .collect();
    let reference = if detailed.is_empty() {
        percentile(&items.iter().map(|a| a.sharp).collect::<Vec<_>>(), Q)
    } else {
        percentile(&detailed, Q)
    };
    let keep: Vec<Candidate> = items
        .iter()
        .copied()
        .filter(|a| !(low_detail(a.low_std, a.fine) && a.sharp < K * reference))
        .collect();
    if keep.is_empty() { items } else { keep }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(cx: f64, score: f64) -> Candidate {
        Candidate {
            rect: RotatedRect::new(cx, 100.0, 100.0, 80.0, 0.0),
            score,
            sharp: 0.5,
            low_std: 40.0,
            fine: 8.0,
        }
    }

    #[test]
    fn percentile_interpolates_like_numpy() {
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0], 50.0), 2.5);
        assert_eq!(percentile(&[5.0], 75.0), 5.0);
        assert_eq!(percentile(&[0.0, 10.0], 30.0), 3.0);
    }

    #[test]
    fn a_clearly_better_alternative_replaces_the_anchor() {
        let anchor = candidate(100.0, 10.0);
        let better = candidate(102.0, 20.0);
        let chosen = select(&[anchor], &[better]);
        assert_eq!(chosen.len(), 1);
        assert_eq!(chosen[0].score, 20.0);
        // Within the margin the anchor stays.
        let near = candidate(102.0, 12.0);
        assert_eq!(select(&[anchor], &[near])[0].score, 10.0);
    }

    #[test]
    fn strong_standalone_alternatives_are_added_and_weak_ones_are_not() {
        let anchor = candidate(100.0, 25.0);
        let strong = candidate(400.0, 30.0);
        let weak = candidate(700.0, 15.0);
        let chosen = select(&[anchor], &[strong, weak]);
        assert_eq!(chosen.len(), 2);
        assert_eq!(chosen[0].score, 30.0); // ordered by score
    }

    #[test]
    fn mostly_contained_duplicates_collapse() {
        let a = candidate(100.0, 25.0);
        let mut inner = candidate(100.0, 24.0);
        inner.rect = RotatedRect::new(100.0, 100.0, 60.0, 50.0, 0.0);
        let chosen = select(&[a, inner], &[]);
        assert_eq!(chosen.len(), 1);
    }

    #[test]
    fn ghosts_are_dropped_only_next_to_sharper_prints() {
        let sharp = candidate(100.0, 20.0);
        let mut ghost = candidate(300.0, 20.0);
        ghost.low_std = 10.0;
        ghost.fine = 2.0;
        ghost.sharp = 0.2;
        let kept = drop_ghosts(vec![sharp, ghost]);
        assert_eq!(kept.len(), 1);
        // Alone, or when everything is soft, nothing is dropped.
        assert_eq!(drop_ghosts(vec![ghost]).len(), 1);
        assert_eq!(drop_ghosts(vec![ghost, ghost]).len(), 2);
    }

    #[test]
    fn a_hard_edge_scores_high_and_a_flat_area_scores_zero() {
        let (w, h) = (200usize, 160usize);
        let mut data = vec![230f32; w * h];
        for y in 40..120 {
            for x in 50..150 {
                data[y * w + x] = 60.0;
            }
        }
        let gray = GrayF32 {
            width: w,
            height: h,
            data,
        };
        let on_edge = RotatedRect::new(100.0, 80.0, 100.0, 80.0, 0.0);
        assert!(score_rect(&gray, &on_edge, 1.0) > 25.0);
        assert!(edge_sharpness(&gray, &on_edge, 1.0) > 0.3);
        let in_flat = RotatedRect::new(100.0, 80.0, 40.0, 30.0, 0.0);
        assert!(score_rect(&gray, &in_flat, 1.0) < 1e-3);
    }
}
