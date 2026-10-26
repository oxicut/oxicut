//! v3 region mask: density of foreground evidence, then morphological clean-up.
//! Ports `cv2.boxFilter`, `getStructuringElement` and `morphologyEx` as used by
//! `detect_photos_v3`, with OpenCV's border handling.

use image::GrayImage;

use crate::foreground::Foreground;

/// Foreground density above which a pixel belongs to a print.
const DENSITY_THRESHOLD: f32 = 0.16;

/// Python `round()` is half-to-even.
fn py_round(x: f64) -> usize {
    x.round_ties_even() as usize
}

pub fn density_window(min_side: usize) -> usize {
    py_round(min_side as f64 * 0.010).max(11) | 1
}

pub fn open_size(min_side: usize) -> usize {
    py_round(min_side as f64 * 0.010).max(5) | 1
}

pub fn close_size(min_side: usize) -> usize {
    py_round(min_side as f64 * 0.008).max(5) | 1
}

/// `BORDER_REFLECT_101`: the border pixel is not repeated.
fn reflect101(index: isize, len: usize) -> usize {
    if len == 1 {
        return 0;
    }
    let period = 2 * (len as isize - 1);
    let i = index.rem_euclid(period);
    (if i >= len as isize { period - i } else { i }) as usize
}

/// `density > 0.16` where density is the normalised box filter of the
/// foreground over a `window` x `window` neighbourhood.
pub fn density_mask(foreground: &Foreground, window: usize) -> GrayImage {
    let (w, h) = (foreground.width, foreground.height);
    let r = window / 2;

    // Horizontal window sums, one row at a time, over a reflected padded row.
    let mut row_sums = vec![0u32; w * h];
    let mut prefix = vec![0u32; w + 2 * r + 1];
    for y in 0..h {
        let row = &foreground.mask[y * w..(y + 1) * w];
        for i in 0..w + 2 * r {
            prefix[i + 1] = prefix[i] + row[reflect101(i as isize - r as isize, w)] as u32;
        }
        for x in 0..w {
            row_sums[y * w + x] = prefix[x + window] - prefix[x];
        }
    }

    // Slide a vertical window over the reflected row order.
    let order: Vec<usize> = (0..h + 2 * r)
        .map(|i| reflect101(i as isize - r as isize, h))
        .collect();
    let mut column = vec![0u32; w];
    for &y in &order[..window] {
        for (sum, add) in column.iter_mut().zip(&row_sums[y * w..(y + 1) * w]) {
            *sum += add;
        }
    }
    // OpenCV sums in double and scales by 1/(k*k), then stores as float.
    let scale = 1.0 / (window * window) as f64;
    let mut out = GrayImage::new(w as u32, h as u32);
    for y in 0..h {
        for (pixel, &sum) in out.as_mut()[y * w..(y + 1) * w].iter_mut().zip(&column) {
            let density = (sum as f64 * scale) as f32;
            *pixel = if density > DENSITY_THRESHOLD { 255 } else { 0 };
        }
        if y + 1 < h {
            let entering = &row_sums[order[y + window] * w..][..w];
            let leaving = &row_sums[order[y] * w..][..w];
            for ((sum, add), sub) in column.iter_mut().zip(entering).zip(leaving) {
                *sum = *sum + add - sub;
            }
        }
    }
    out
}

/// A symmetric structuring element as horizontal runs: for each row offset
/// `dy`, pixels `-dx..=dx` around the anchor.
pub struct Element {
    runs: Vec<(isize, usize)>,
}

impl Element {
    /// `getStructuringElement(MORPH_RECT, (size, size))`, `size` odd.
    pub fn rect(size: usize) -> Self {
        let r = size / 2;
        Self {
            runs: (0..size).map(|i| (i as isize - r as isize, r)).collect(),
        }
    }

    /// `getStructuringElement(MORPH_ELLIPSE, (size, size))`, `size` odd.
    pub fn ellipse(size: usize) -> Self {
        let r = (size / 2) as f64;
        let inverse_r2 = if r > 0.0 { 1.0 / (r * r) } else { 0.0 };
        let runs = (0..size)
            .filter_map(|i| {
                let dy = i as f64 - r;
                if dy.abs() > r {
                    return None;
                }
                let dx = (r * ((r * r - dy * dy) * inverse_r2).sqrt()).round_ties_even();
                Some((dy as isize, dx as usize))
            })
            .collect();
        Self { runs }
    }
}

/// Prefix counts of non-zero pixels per row, so any horizontal window is
/// answered in O(1).
fn row_prefix(image: &GrayImage) -> Vec<u32> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let mut prefix = vec![0u32; (w + 1) * h];
    for y in 0..h {
        for x in 0..w {
            let set = image.as_raw()[y * w + x] != 0;
            prefix[y * (w + 1) + x + 1] = prefix[y * (w + 1) + x] + set as u32;
        }
    }
    prefix
}

/// Pixels outside the image are ignored (OpenCV's default morphology border is
/// +inf for erosion and -inf for dilation, i.e. neutral).
fn apply(image: &GrayImage, element: &Element, erode: bool) -> GrayImage {
    let (w, h) = (image.width() as usize, image.height() as usize);
    // Non-zero pixels per row prefix. Erosion keeps a pixel only if every window
    // is full; dilation sets it if any window holds a non-zero pixel.
    let counts = row_prefix(image);
    let mut out = GrayImage::new(w as u32, h as u32);
    for y in 0..h {
        for x in 0..w {
            // `hit`: erosion found a zero / dilation found a non-zero.
            let mut hit = false;
            for &(dy, dx) in &element.runs {
                let yy = y as isize + dy;
                if yy < 0 || yy >= h as isize {
                    continue;
                }
                let row = yy as usize * (w + 1);
                let lo = x.saturating_sub(dx);
                let hi = (x + dx).min(w - 1);
                let ones = counts[row + hi + 1] - counts[row + lo];
                let window = (hi - lo + 1) as u32;
                if (erode && ones != window) || (!erode && ones > 0) {
                    hit = true;
                    break;
                }
            }
            let value = if erode { !hit } else { hit };
            out.as_mut()[y * w + x] = if value { 255 } else { 0 };
        }
    }
    out
}

pub fn erode(image: &GrayImage, element: &Element) -> GrayImage {
    apply(image, element, true)
}

pub fn dilate(image: &GrayImage, element: &Element) -> GrayImage {
    apply(image, element, false)
}

/// `morphologyEx(MORPH_OPEN)`: erode then dilate.
pub fn open(image: &GrayImage, element: &Element) -> GrayImage {
    dilate(&erode(image, element), element)
}

/// `morphologyEx(MORPH_CLOSE, iterations)`: dilate then erode, each repeated.
pub fn close(image: &GrayImage, element: &Element, iterations: usize) -> GrayImage {
    let mut current = image.clone();
    for _ in 0..iterations {
        current = dilate(&current, element);
    }
    for _ in 0..iterations {
        current = erode(&current, element);
    }
    current
}

/// The whole v3 mask: density, open with an ellipse, close with a square twice.
pub fn region_mask(foreground: &Foreground) -> GrayImage {
    let min_side = foreground.width.min(foreground.height);
    let dense = density_mask(foreground, density_window(min_side));
    let opened = open(&dense, &Element::ellipse(open_size(min_side)));
    close(&opened, &Element::rect(close_size(min_side)), 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_from(rows: &[&str]) -> GrayImage {
        let (w, h) = (rows[0].len(), rows.len());
        let mut image = GrayImage::new(w as u32, h as u32);
        for (y, row) in rows.iter().enumerate() {
            for (x, c) in row.bytes().enumerate() {
                image.put_pixel(
                    x as u32,
                    y as u32,
                    image::Luma([if c == b'#' { 255 } else { 0 }]),
                );
            }
        }
        image
    }

    fn count(image: &GrayImage) -> usize {
        image.as_raw().iter().filter(|&&v| v != 0).count()
    }

    #[test]
    fn window_sizes_are_odd_and_floored() {
        assert_eq!(density_window(1350), 15); // round(13.5) = 14 -> |1
        assert_eq!(density_window(100), 11);
        assert_eq!(open_size(1350), 15);
        assert_eq!(close_size(1350), 11); // round(10.8) = 11
        assert_eq!(close_size(100), 5);
    }

    #[test]
    fn reflect101_does_not_repeat_the_edge() {
        let indices: Vec<_> = (-3..8).map(|i| reflect101(i, 5)).collect();
        assert_eq!(indices, vec![3, 2, 1, 0, 1, 2, 3, 4, 3, 2, 1]);
    }

    #[test]
    fn ellipse_5x5_matches_opencv() {
        let element = Element::ellipse(5);
        let mut shape = [[false; 5]; 5];
        for &(dy, dx) in &element.runs {
            for x in -(dx as isize)..=dx as isize {
                shape[(dy + 2) as usize][(x + 2) as usize] = true;
            }
        }
        let rows: Vec<String> = shape
            .iter()
            .map(|r| r.iter().map(|&b| if b { '1' } else { '0' }).collect())
            .collect();
        assert_eq!(rows, vec!["00100", "11111", "11111", "11111", "00100"]);
    }

    #[test]
    fn dilating_a_pixel_gives_the_element() {
        let image = image_from(&[
            ".......", ".......", ".......", "...#...", ".......", ".......", ".......",
        ]);
        assert_eq!(count(&dilate(&image, &Element::rect(3))), 9);
        assert_eq!(count(&dilate(&image, &Element::ellipse(5))), 17);
    }

    #[test]
    fn erosion_shrinks_but_leaves_the_image_border_alone() {
        let block = image_from(&["#####", "#####", "#####", "#####", "#####"]);
        // Outside the image does not count as background.
        assert_eq!(count(&erode(&block, &Element::rect(3))), 25);
        let island = image_from(&[
            ".......", ".#####.", ".#####.", ".#####.", ".#####.", ".#####.", ".......",
        ]);
        assert_eq!(count(&erode(&island, &Element::rect(3))), 9);
    }

    #[test]
    fn open_removes_specks_and_close_fills_pinholes() {
        let specks = image_from(&[
            "#......", ".......", "...###.", "...###.", "...###.", ".......", ".......",
        ]);
        let opened = open(&specks, &Element::rect(3));
        assert_eq!(count(&opened), 9);

        let pinhole = image_from(&["#######", "#######", "###.###", "#######", "#######"]);
        assert_eq!(count(&close(&pinhole, &Element::rect(3), 1)), 35);
    }

    #[test]
    fn density_keeps_blocks_and_drops_stray_pixels() {
        let (w, h) = (40, 40);
        let mut mask = vec![false; w * h];
        for y in 10..30 {
            for x in 10..30 {
                mask[y * w + x] = true;
            }
        }
        mask[2 * w + 2] = true; // lone pixel: 1/121 of the window
        let fg = Foreground {
            width: w,
            height: h,
            mask,
            otsu: 0,
        };
        let dense = density_mask(&fg, 11);
        assert_eq!(dense.get_pixel(20, 20).0[0], 255);
        assert_eq!(dense.get_pixel(2, 2).0[0], 0);
        assert_eq!(dense.get_pixel(35, 35).0[0], 0);
    }
}
