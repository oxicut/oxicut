//! `cv2.Canny(image, low, high)` for 8-bit input: 3x3 Sobel, L1 gradient
//! magnitude, non-maximum suppression in fixed point, then hysteresis.

use crate::filter::sobel3_replicate;

const CANNY_SHIFT: u32 = 15;
/// `tan(22.5 deg)` in 15-bit fixed point.
const TG22: i32 = 13573;

// Map states while tracing.
const WEAK: u8 = 0;
const NOT_EDGE: u8 = 1;
const EDGE: u8 = 2;

pub fn canny(src: &[u8], width: usize, height: usize, low: i32, high: i32) -> Vec<u8> {
    let (dx, dy) = sobel3_replicate(src, width, height);
    let magnitude: Vec<i32> = dx
        .iter()
        .zip(&dy)
        .map(|(&x, &y)| (x as i32).abs() + (y as i32).abs())
        .collect();

    // One-pixel border of NOT_EDGE around the image.
    let stride = width + 2;
    let mut map = vec![NOT_EDGE; stride * (height + 2)];
    let mut stack: Vec<usize> = Vec::new();

    let mag_at = |x: isize, y: isize| -> i32 {
        if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
            0
        } else {
            magnitude[y as usize * width + x as usize]
        }
    };

    for y in 0..height {
        let mut previous_flag = false;
        for x in 0..width {
            let index = y * width + x;
            let slot = (y + 1) * stride + x + 1;
            let m = magnitude[index];
            let (xi, yi) = (x as isize, y as isize);

            let mut keep = false;
            if m > low {
                let xs = dx[index] as i32;
                let ys = dy[index] as i32;
                let ax = xs.abs();
                let ay = ys.abs() << CANNY_SHIFT;
                let tg22x = ax * TG22;
                if ay < tg22x {
                    keep = m > mag_at(xi - 1, yi) && m >= mag_at(xi + 1, yi);
                } else {
                    let tg67x = tg22x + (ax << (CANNY_SHIFT + 1));
                    if ay > tg67x {
                        keep = m > mag_at(xi, yi - 1) && m >= mag_at(xi, yi + 1);
                    } else {
                        let s: isize = if (xs ^ ys) < 0 { -1 } else { 1 };
                        keep = m > mag_at(xi - s, yi - 1) && m > mag_at(xi + s, yi + 1);
                    }
                }
            }

            if !keep {
                previous_flag = false;
                map[slot] = NOT_EDGE;
                continue;
            }
            if !previous_flag && m > high && map[slot - stride] != EDGE {
                map[slot] = EDGE;
                stack.push(slot);
                previous_flag = true;
            } else {
                map[slot] = WEAK;
            }
        }
    }

    // Hysteresis: grow strong edges into weak neighbours (8-connected).
    let neighbours: [isize; 8] = [
        -(stride as isize) - 1,
        -(stride as isize),
        -(stride as isize) + 1,
        -1,
        1,
        stride as isize - 1,
        stride as isize,
        stride as isize + 1,
    ];
    while let Some(slot) = stack.pop() {
        for offset in neighbours {
            let next = (slot as isize + offset) as usize;
            if map[next] == WEAK {
                map[next] = EDGE;
                stack.push(next);
            }
        }
    }

    let mut out = vec![0u8; width * height];
    for y in 0..height {
        for x in 0..width {
            if map[(y + 1) * stride + x + 1] == EDGE {
                out[y * width + x] = 255;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: usize, height: usize, f: impl Fn(usize, usize) -> u8) -> Vec<u8> {
        (0..width * height)
            .map(|i| f(i % width, i / width))
            .collect()
    }

    #[test]
    fn flat_image_has_no_edges() {
        let flat = vec![120u8; 20 * 20];
        assert!(canny(&flat, 20, 20, 35, 110).iter().all(|&v| v == 0));
    }

    #[test]
    fn a_strong_step_gives_a_thin_line() {
        let step = image(20, 20, |x, _| if x < 10 { 20 } else { 220 });
        let edges = canny(&step, 20, 20, 35, 110);
        for y in 2..18 {
            let row: Vec<usize> = (0..20).filter(|&x| edges[y * 20 + x] != 0).collect();
            assert_eq!(row.len(), 1, "row {y}: {row:?}");
            assert!((9..=10).contains(&row[0]), "row {y}: {row:?}");
        }
    }

    #[test]
    fn weak_edges_survive_only_when_connected_to_strong_ones() {
        // Sobel responds with 4x the step height. A step of 40 gives 160 (above
        // `high` = 110) at the top and fades to 17 -> 68 (between `low` = 35 and
        // `high`) lower down: the weak part is kept because it touches strong rows.
        let fading = image(24, 24, |x, y| if x < 12 { 50 } else { 50 + (40 - y) as u8 });
        let edges = canny(&fading, 24, 24, 35, 110);
        let lit: Vec<usize> = (0..24)
            .filter(|&y| (0..24).any(|x| edges[y * 24 + x] != 0))
            .collect();
        assert!(lit.len() >= 22, "only rows {lit:?} have edges");

        // A step of 12 gives 48: above `low` but with nothing strong to start from.
        let weak_only = image(24, 24, |x, _| if x < 12 { 50 } else { 62 });
        assert!(canny(&weak_only, 24, 24, 35, 110).iter().all(|&v| v == 0));
    }

    #[test]
    fn diagonal_edges_are_found() {
        let diagonal = image(30, 30, |x, y| if x > y { 230 } else { 20 });
        let edges = canny(&diagonal, 30, 30, 35, 110);
        assert!(edges.iter().filter(|&&v| v != 0).count() >= 20);
    }
}
