//! `cv2.HoughLinesP`: OpenCV's progressive probabilistic Hough transform.
//!
//! It picks edge pixels in a random order, but its generator is seeded with a
//! constant, so the result is deterministic and reproduced exactly here.

use std::f64::consts::PI;

/// `cv::RNG((uint64)-1)`.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = (self.0 as u32 as u64)
            .wrapping_mul(4_164_903_690)
            .wrapping_add(self.0 >> 32);
        self.0 as u32
    }

    /// `rng.uniform(0, n)`.
    fn below(&mut self, n: usize) -> usize {
        self.next() as usize % n
    }
}

fn round_to_int(x: f32) -> i32 {
    x.round_ties_even() as i32
}

/// Line segments `[x1, y1, x2, y2]` found in a binary edge image.
///
/// `rho` is fixed at 1 pixel (what ScanSplitter uses) and `theta` is the angular
/// step in radians; `threshold` is the minimum vote count, `min_length` the
/// shortest accepted segment (in x or y extent) and `max_gap` the largest hole
/// bridged along a line.
pub fn hough_lines_p(
    edges: &[u8],
    width: usize,
    height: usize,
    theta: f64,
    threshold: i32,
    min_length: i32,
    max_gap: i32,
) -> Vec<[i32; 4]> {
    let theta = theta as f32;
    let inverse_rho = 1.0f32; // rho = 1
    let mut rng = Rng(u64::MAX);
    let num_angle = (PI / theta as f64).round_ties_even() as usize;
    let num_rho = (((width + height) * 2 + 1) as f32).round_ties_even() as usize;

    let trig: Vec<(f32, f32)> = (0..num_angle)
        .map(|n| {
            let angle = n as f64 * theta as f64;
            (
                (angle.cos() * inverse_rho as f64) as f32,
                (angle.sin() * inverse_rho as f64) as f32,
            )
        })
        .collect();

    let mut accumulator = vec![0i32; num_angle * num_rho];
    let mut mask: Vec<u8> = edges.iter().map(|&v| (v != 0) as u8).collect();
    let mut points: Vec<(i32, i32)> = Vec::new();
    for y in 0..height {
        for x in 0..width {
            if edges[y * width + x] != 0 {
                points.push((x as i32, y as i32));
            }
        }
    }

    let rho_of = |x: i32, y: i32, n: usize| -> usize {
        let r =
            round_to_int(x as f32 * trig[n].0 + y as f32 * trig[n].1) + ((num_rho as i32 - 1) / 2);
        r as usize
    };
    const SHIFT: i32 = 16;
    let mut lines = Vec::new();

    let mut count = points.len();
    while count > 0 {
        let index = rng.below(count);
        let point = points[index];
        // Remove it by overwriting with the last remaining point.
        points[index] = points[count - 1];
        count -= 1;

        let (j, i) = point;
        if mask[i as usize * width + j as usize] == 0 {
            continue; // already part of an extracted line
        }

        // Vote, and remember the strongest angle.
        let mut max_val = threshold - 1;
        let mut max_n = 0usize;
        for n in 0..num_angle {
            let cell = &mut accumulator[n * num_rho + rho_of(j, i, n)];
            *cell += 1;
            if max_val < *cell {
                max_val = *cell;
                max_n = n;
            }
        }
        if max_val < threshold {
            continue;
        }

        // Walk both ways along the line direction in fixed point.
        let a = -trig[max_n].1;
        let b = trig[max_n].0;
        let (mut x0, mut y0) = (j, i);
        let (dx0, dy0, x_major);
        if a.abs() > b.abs() {
            x_major = true;
            dx0 = if a > 0.0 { 1 } else { -1 };
            dy0 = round_to_int(b * (1 << SHIFT) as f32 / a.abs());
            y0 = (y0 << SHIFT) + (1 << (SHIFT - 1));
        } else {
            x_major = false;
            dy0 = if b > 0.0 { 1 } else { -1 };
            dx0 = round_to_int(a * (1 << SHIFT) as f32 / b.abs());
            x0 = (x0 << SHIFT) + (1 << (SHIFT - 1));
        }
        let locate = |x: i32, y: i32| -> (i32, i32) {
            if x_major {
                (x, y >> SHIFT)
            } else {
                (x >> SHIFT, y)
            }
        };

        let mut line_end = [(0i32, 0i32); 2];
        for (k, end) in line_end.iter_mut().enumerate() {
            let (mut gap, mut x, mut y) = (0, x0, y0);
            let (dx, dy) = if k == 0 { (dx0, dy0) } else { (-dx0, -dy0) };
            loop {
                let (j1, i1) = locate(x, y);
                if j1 < 0 || j1 >= width as i32 || i1 < 0 || i1 >= height as i32 {
                    break;
                }
                if mask[i1 as usize * width + j1 as usize] != 0 {
                    gap = 0;
                    *end = (j1, i1);
                } else {
                    gap += 1;
                    if gap > max_gap {
                        break;
                    }
                }
                x += dx;
                y += dy;
            }
        }

        let good_line = (line_end[1].0 - line_end[0].0).abs() >= min_length
            || (line_end[1].1 - line_end[0].1).abs() >= min_length;

        // Consume the pixels along the segment (and take back their votes).
        for (k, end) in line_end.iter().enumerate() {
            let (mut x, mut y) = (x0, y0);
            let (dx, dy) = if k == 0 { (dx0, dy0) } else { (-dx0, -dy0) };
            loop {
                let (j1, i1) = locate(x, y);
                let cell = i1 as usize * width + j1 as usize;
                if mask[cell] != 0 {
                    if good_line {
                        for n in 0..num_angle {
                            accumulator[n * num_rho + rho_of(j1, i1, n)] -= 1;
                        }
                    }
                    mask[cell] = 0;
                }
                if (j1, i1) == *end {
                    break;
                }
                x += dx;
                y += dy;
            }
        }

        if good_line {
            lines.push([line_end[0].0, line_end[0].1, line_end[1].0, line_end[1].1]);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plot(width: usize, height: usize, pixels: &[(usize, usize)]) -> Vec<u8> {
        let mut image = vec![0u8; width * height];
        for &(x, y) in pixels {
            image[y * width + x] = 255;
        }
        image
    }

    #[test]
    fn finds_a_horizontal_segment() {
        let pixels: Vec<_> = (10..90).map(|x| (x, 40)).collect();
        let edges = plot(100, 80, &pixels);
        let lines = hough_lines_p(&edges, 100, 80, PI / 360.0, 18, 30, 5);
        assert_eq!(lines.len(), 1, "{lines:?}");
        let [x1, y1, x2, y2] = lines[0];
        assert_eq!((y1, y2), (40, 40));
        assert_eq!((x1.min(x2), x1.max(x2)), (10, 89));
    }

    #[test]
    fn finds_a_vertical_segment() {
        let pixels: Vec<_> = (5..70).map(|y| (60, y)).collect();
        let lines = hough_lines_p(&plot(100, 80, &pixels), 100, 80, PI / 360.0, 18, 30, 5);
        assert_eq!(lines.len(), 1);
        let [x1, y1, x2, y2] = lines[0];
        assert_eq!((x1, x2), (60, 60));
        assert_eq!((y1.min(y2), y1.max(y2)), (5, 69));
    }

    #[test]
    fn bridges_small_gaps_but_not_large_ones() {
        let mut pixels: Vec<_> = (10..40).map(|x| (x, 30)).collect();
        pixels.extend((44..80).map(|x| (x, 30))); // gap of 4
        let lines = hough_lines_p(&plot(100, 60, &pixels), 100, 60, PI / 360.0, 18, 30, 5);
        assert_eq!(lines.len(), 1, "{lines:?}");

        // Two 40-pixel pieces 25 pixels apart: each is a line, never one.
        let mut wide: Vec<_> = (5..45).map(|x| (x, 30)).collect();
        wide.extend((70..110).map(|x| (x, 30)));
        let lines = hough_lines_p(&plot(120, 60, &wide), 120, 60, PI / 360.0, 18, 30, 5);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines.iter().all(|l| (l[0] - l[2]).abs() < 45));
    }

    #[test]
    fn short_segments_and_noise_are_ignored() {
        let pixels: Vec<_> = (10..22).map(|x| (x, 30)).collect(); // 12 px
        assert!(hough_lines_p(&plot(100, 60, &pixels), 100, 60, PI / 360.0, 18, 30, 5).is_empty());
        assert!(hough_lines_p(&vec![0u8; 100 * 60], 100, 60, PI / 360.0, 18, 30, 5).is_empty());
    }

    #[test]
    fn is_deterministic() {
        let mut pixels: Vec<_> = (10..90).map(|x| (x, 20)).collect();
        pixels.extend((10..70).map(|y| (50, y)));
        let edges = plot(100, 80, &pixels);
        let a = hough_lines_p(&edges, 100, 80, PI / 360.0, 18, 30, 5);
        let b = hough_lines_p(&edges, 100, 80, PI / 360.0, 18, 30, 5);
        assert_eq!(a, b);
        assert_eq!(a.len(), 2);
    }
}
