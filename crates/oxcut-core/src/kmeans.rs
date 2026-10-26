//! k-means with k-means++ seeding, a port of OpenCV's `cv2.kmeans(...,
//! KMEANS_PP_CENTERS)` including its `cv::RNG`, so ScanSplitter's background
//! model (which seeds with `cv2.setRNGSeed(0)`) reproduces exactly.
//!
//! Arithmetic is kept in `f32`/`f64` at the same points as OpenCV (float
//! distances and centre sums, double compactness and RNG draws).

pub struct KMeans {
    pub centers: Vec<[f32; 3]>,
    pub labels: Vec<usize>,
}

/// `cv::RNG`: multiply-with-carry generator.
struct CvRng(u64);

impl CvRng {
    fn new(seed: u32) -> Self {
        // `RNG(state)` maps 0 to 0xffffffff.
        Self(if seed == 0 { 0xffff_ffff } else { seed as u64 })
    }

    fn next(&mut self) -> u32 {
        self.0 = (self.0 as u32 as u64)
            .wrapping_mul(4_164_903_690)
            .wrapping_add(self.0 >> 32);
        self.0 as u32
    }

    /// `(double)rng`: uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        let high = self.next() as u64;
        let low = self.next() as u64;
        ((high << 32) | low) as f64 * 5.421_010_862_427_522e-20
    }
}

/// `SPP_TRIALS` in OpenCV: candidate centres tried per k-means++ step.
const SEEDING_TRIALS: usize = 3;

fn dist2(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    let mut d = 0f32;
    for c in 0..3 {
        let t = a[c] - b[c];
        d += t * t;
    }
    d
}

/// First minimum, like OpenCV's strict `<` scan.
fn nearest(point: &[f32; 3], centers: &[[f32; 3]]) -> (usize, f32) {
    let mut best = (0, dist2(point, &centers[0]));
    for (i, center) in centers.iter().enumerate().skip(1) {
        let d = dist2(point, center);
        if d < best.1 {
            best = (i, d);
        }
    }
    best
}

/// `generateCentersPP`.
fn seed_plus_plus(points: &[[f32; 3]], k: usize, rng: &mut CvRng) -> Vec<[f32; 3]> {
    let n = points.len();
    let mut chosen = vec![rng.next() as usize % n];
    let mut dist: Vec<f32> = points
        .iter()
        .map(|p| dist2(p, &points[chosen[0]]))
        .collect();
    let mut sum: f64 = dist.iter().map(|&d| d as f64).sum();

    for _ in 1..k {
        let mut best_sum = f64::MAX;
        let mut best_center = None;
        let mut best_dist = Vec::new();
        for _ in 0..SEEDING_TRIALS {
            let mut p = rng.unit() * sum;
            let mut index = 0;
            while index < n - 1 {
                p -= dist[index] as f64;
                if p <= 0.0 {
                    break;
                }
                index += 1;
            }
            let candidate: Vec<f32> = points
                .iter()
                .zip(&dist)
                .map(|(point, &d)| d.min(dist2(point, &points[index])))
                .collect();
            let candidate_sum: f64 = candidate.iter().map(|&d| d as f64).sum();
            if candidate_sum < best_sum {
                best_sum = candidate_sum;
                best_center = Some(index);
                best_dist = candidate;
            }
        }
        chosen.push(best_center.expect("at least one trial"));
        dist = best_dist;
        sum = best_sum;
    }
    chosen.into_iter().map(|i| points[i]).collect()
}

/// Cluster `points` into `k` groups; the best of `attempts` runs (lowest
/// compactness) wins. `eps` is the centre-shift tolerance (squared internally).
pub fn kmeans(
    points: &[[f32; 3]],
    k: usize,
    attempts: usize,
    max_iter: usize,
    eps: f32,
    seed: u32,
) -> KMeans {
    let n = points.len();
    assert!(k >= 1 && n >= k, "need at least k points");
    let mut rng = CvRng::new(seed);
    let epsilon = (eps.max(0.0) as f64).powi(2);
    let max_count = max_iter.max(2);

    let mut best: Option<(f64, Vec<[f32; 3]>, Vec<usize>)> = None;
    let mut labels = vec![0usize; n];

    for _ in 0..attempts.max(1) {
        let mut centers: Vec<[f32; 3]> = Vec::new();
        let mut max_center_shift = f64::MAX;
        let mut iteration = 0;
        let compactness = loop {
            let old_centers = std::mem::take(&mut centers);
            if iteration == 0 {
                centers = seed_plus_plus(points, k, &mut rng);
            } else {
                centers = vec![[0f32; 3]; k];
                let mut counters = vec![0usize; k];
                for (point, &label) in points.iter().zip(&labels) {
                    for c in 0..3 {
                        centers[label][c] += point[c];
                    }
                    counters[label] += 1;
                }
                max_center_shift = 0.0;

                for cluster in 0..k {
                    if counters[cluster] != 0 {
                        continue;
                    }
                    // Empty cluster: take the farthest point of the biggest one.
                    let mut biggest = 0;
                    for other in 1..k {
                        if counters[biggest] < counters[other] {
                            biggest = other;
                        }
                    }
                    let scale = 1.0f32 / counters[biggest] as f32;
                    let base = centers[biggest].map(|v| v * scale);
                    let mut max_dist = 0f64;
                    let mut farthest = None;
                    for (i, point) in points.iter().enumerate() {
                        if labels[i] != biggest {
                            continue;
                        }
                        let d = dist2(point, &base) as f64;
                        if max_dist <= d {
                            max_dist = d;
                            farthest = Some(i);
                        }
                    }
                    let Some(farthest) = farthest else { continue };
                    counters[biggest] -= 1;
                    counters[cluster] += 1;
                    labels[farthest] = cluster;
                    for c in 0..3 {
                        centers[biggest][c] -= points[farthest][c];
                        centers[cluster][c] += points[farthest][c];
                    }
                }

                for cluster in 0..k {
                    if counters[cluster] != 0 {
                        let scale = 1.0f32 / counters[cluster] as f32;
                        centers[cluster] = centers[cluster].map(|v| v * scale);
                    } else {
                        centers[cluster] = old_centers[cluster];
                    }
                    let mut shift = 0f64;
                    for c in 0..3 {
                        let t = (centers[cluster][c] - old_centers[cluster][c]) as f64;
                        shift += t * t;
                    }
                    max_center_shift = max_center_shift.max(shift);
                }
            }

            iteration += 1;
            let last = iteration == max_count || max_center_shift <= epsilon;

            let mut compactness = 0f64;
            for (point, label) in points.iter().zip(labels.iter_mut()) {
                let (nearest_label, d) = nearest(point, &centers);
                *label = nearest_label;
                compactness += d as f64;
            }
            if last {
                break compactness;
            }
        };

        if compactness < best.as_ref().map_or(f64::MAX, |b| b.0) {
            best = Some((compactness, centers, labels.clone()));
        }
    }

    let (_, centers, labels) = best.expect("at least one attempt");
    KMeans { centers, labels }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Small deterministic generator for test data only.
    fn blob(center: [f32; 3], n: usize, salt: u32) -> Vec<[f32; 3]> {
        let mut rng = CvRng::new(salt + 1);
        (0..n)
            .map(|_| {
                let mut jitter = || (rng.unit() as f32 - 0.5) * 4.0;
                [
                    center[0] + jitter(),
                    center[1] + jitter(),
                    center[2] + jitter(),
                ]
            })
            .collect()
    }

    #[test]
    fn rng_matches_opencv_first_values() {
        // cv::RNG(0): state 0xffffffff. next() = 0xffffffff * 4164903690 + 0, low 32 bits.
        let mut rng = CvRng::new(0);
        assert_eq!(
            rng.next(),
            (0xffff_ffffu64.wrapping_mul(4_164_903_690)) as u32
        );
    }

    #[test]
    fn separates_well_spaced_blobs() {
        let truth = [
            [20.0, 128.0, 128.0],
            [120.0, 150.0, 100.0],
            [250.0, 128.0, 128.0],
        ];
        let points: Vec<_> = truth
            .iter()
            .enumerate()
            .flat_map(|(i, c)| blob(*c, 200, i as u32))
            .collect();
        let result = kmeans(&points, 3, 3, 30, 0.5, 0);
        for center in truth {
            let (_, d) = nearest(&center, &result.centers);
            assert!(d < 1.0, "no centre near {center:?}: {:?}", result.centers);
        }
        assert!(result.labels[..200].iter().all(|&l| l == result.labels[0]));
    }

    #[test]
    fn is_deterministic() {
        let points: Vec<_> = (0..3)
            .flat_map(|i| blob([i as f32 * 60.0, 128.0, 128.0], 100, i))
            .collect();
        let a = kmeans(&points, 5, 3, 30, 0.5, 0);
        let b = kmeans(&points, 5, 3, 30, 0.5, 0);
        assert_eq!(a.centers, b.centers);
        assert_eq!(a.labels, b.labels);
    }

    #[test]
    fn identical_points_do_not_hang_or_panic() {
        let points = vec![[10.0, 20.0, 30.0]; 50];
        let result = kmeans(&points, 5, 3, 30, 0.5, 0);
        assert_eq!(result.centers.len(), 5);
        assert!(result.centers.iter().all(|c| *c == [10.0, 20.0, 30.0]));
    }
}
