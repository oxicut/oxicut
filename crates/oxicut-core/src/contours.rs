//! Outer contours of a binary mask, like `cv2.findContours(RETR_EXTERNAL)`.
//!
//! Each 8-connected blob that is not enclosed by another blob's hole yields its
//! outer border as a closed pixel path. Points are not compressed (OpenCV's
//! `CHAIN_APPROX_SIMPLE` only drops collinear ones, which changes neither the
//! area, bounding box nor hull).

use std::collections::VecDeque;

use image::GrayImage;

use crate::geometry::{RotatedRect, min_area_rect};

pub type Point = (i32, i32);

/// Neighbour offsets in clockwise screen order (y points down), starting east.
const DIRS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

struct Grid<'a> {
    width: i32,
    height: i32,
    data: &'a [u8],
}

impl Grid<'_> {
    fn set(&self, x: i32, y: i32) -> bool {
        x >= 0
            && y >= 0
            && x < self.width
            && y < self.height
            && self.data[(y * self.width + x) as usize] != 0
    }
}

/// Zero pixels reachable from outside the image through 4-connected zeros.
fn outer_background(grid: &Grid) -> Vec<bool> {
    let (w, h) = (grid.width, grid.height);
    let mut reached = vec![false; (w * h) as usize];
    let mut queue = VecDeque::new();
    let push = |x: i32, y: i32, reached: &mut Vec<bool>, queue: &mut VecDeque<(i32, i32)>| {
        if !grid.set(x, y) && !reached[(y * w + x) as usize] {
            reached[(y * w + x) as usize] = true;
            queue.push_back((x, y));
        }
    };
    for x in 0..w {
        push(x, 0, &mut reached, &mut queue);
        push(x, h - 1, &mut reached, &mut queue);
    }
    for y in 0..h {
        push(0, y, &mut reached, &mut queue);
        push(w - 1, y, &mut reached, &mut queue);
    }
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx >= 0 && ny >= 0 && nx < w && ny < h {
                push(nx, ny, &mut reached, &mut queue);
            }
        }
    }
    reached
}

/// Moore-neighbour trace of the outer border starting at the blob's first
/// pixel in raster order, stopping when the first move repeats (Jacob's rule).
fn trace(grid: &Grid, start: Point) -> Vec<Point> {
    let mut points = vec![start];
    let mut current = start;
    // The backtrack pixel is the (background) pixel west of the start.
    let mut backtrack = 4usize;
    let mut first_move = None;
    loop {
        let found = (1..=8)
            .map(|k| (backtrack + k) % 8)
            .find(|&d| grid.set(current.0 + DIRS[d].0, current.1 + DIRS[d].1));
        let Some(direction) = found else {
            return points; // isolated pixel
        };
        if current == start {
            match first_move {
                None => first_move = Some(direction),
                Some(first) if first == direction => {
                    return points;
                }
                Some(_) => {}
            }
        }
        // The pixel examined just before the hit is background; it becomes the
        // new backtrack, seen from the new position.
        let before = (direction + 7) % 8;
        let background = (current.0 + DIRS[before].0, current.1 + DIRS[before].1);
        current = (current.0 + DIRS[direction].0, current.1 + DIRS[direction].1);
        let offset = (background.0 - current.0, background.1 - current.1);
        backtrack = DIRS
            .iter()
            .position(|&d| d == offset)
            .expect("adjacent pixel");
        points.push(current);
    }
}

/// Outer contours. OpenCV lists them in *reverse* raster order of each blob's
/// first pixel (bottom blobs first), and later stages depend on the order.
pub fn find_external_contours(mask: &GrayImage) -> Vec<Vec<Point>> {
    let grid = Grid {
        width: mask.width() as i32,
        height: mask.height() as i32,
        data: mask.as_raw(),
    };
    let (w, h) = (grid.width, grid.height);
    let outside = outer_background(&grid);
    let mut seen = vec![false; (w * h) as usize];
    let mut contours = Vec::new();

    for y in 0..h {
        for x in 0..w {
            let index = (y * w + x) as usize;
            if !grid.set(x, y) || seen[index] {
                continue;
            }
            // First pixel of an unseen blob: mark the whole blob (8-connected).
            let mut stack = vec![(x, y)];
            seen[index] = true;
            while let Some((cx, cy)) = stack.pop() {
                for (dx, dy) in DIRS {
                    let (nx, ny) = (cx + dx, cy + dy);
                    if grid.set(nx, ny) && !seen[(ny * w + nx) as usize] {
                        seen[(ny * w + nx) as usize] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            // A blob nested in another's hole starts next to enclosed background.
            let left_is_outside = x == 0 || outside[(y * w + x - 1) as usize];
            if left_is_outside {
                contours.push(trace(&grid, (x, y)));
            }
        }
    }
    contours.reverse();
    contours
}

/// Every border, outer and hole (`cv2.findContours(RETR_LIST)`): Suzuki and Abe's
/// border following with 8-connected foreground. Used where enclosed regions matter,
/// since the border of a hole is the outline of the empty space inside a closed edge.
pub fn find_all_contours(mask: &GrayImage) -> Vec<Vec<Point>> {
    let (w, h) = (mask.width() as usize, mask.height() as usize);
    // One pixel of zeros all round, so neighbours never need bounds checks.
    let stride = w + 2;
    let mut labels = vec![0i32; stride * (h + 2)];
    for y in 0..h {
        for x in 0..w {
            labels[(y + 1) * stride + x + 1] = i32::from(mask.as_raw()[y * w + x] != 0);
        }
    }
    let offset = |d: usize| DIRS[d].1 as isize * stride as isize + DIRS[d].0 as isize;
    let direction_to = |from: usize, to: usize| {
        let delta = (
            (to % stride) as isize - (from % stride) as isize,
            (to / stride) as isize - (from / stride) as isize,
        );
        DIRS.iter()
            .position(|&(dx, dy)| (dx as isize, dy as isize) == delta)
            .expect("neighbouring pixels")
    };

    let mut contours = Vec::new();
    let mut border = 1;
    for y in 1..=h {
        for x in 1..=w {
            let start = y * stride + x;
            let value = labels[start];
            if value == 0 {
                continue;
            }
            let first_direction = if value == 1 && labels[start - 1] == 0 {
                4 // outer border: begin at the empty pixel to the west
            } else if value > 0 && labels[start + 1] == 0 {
                0 // hole border: begin at the empty pixel to the east
            } else {
                continue;
            };
            border += 1;

            // 3.1: first foreground neighbour, clockwise from the start direction.
            let Some(first) = (0..8)
                .map(|k| (first_direction + k) % 8)
                .find(|&d| labels[(start as isize + offset(d)) as usize] != 0)
            else {
                labels[start] = -border;
                contours.push(vec![(x as i32 - 1, y as i32 - 1)]);
                continue;
            };
            let first_pixel = (start as isize + offset(first)) as usize;

            let mut points = Vec::new();
            let (mut previous, mut current) = (first_pixel, start);
            loop {
                points.push(((current % stride) as i32 - 1, (current / stride) as i32 - 1));
                // 3.3: counter-clockwise from just after `previous`.
                let from = direction_to(current, previous);
                let mut east_empty = false;
                let mut next = None;
                for k in 1..=8 {
                    let d = (from + 8 - k) % 8;
                    let pixel = (current as isize + offset(d)) as usize;
                    if labels[pixel] != 0 {
                        next = Some(pixel);
                        break;
                    }
                    if d == 0 {
                        east_empty = true;
                    }
                }
                let next = next.expect("the previous pixel is foreground");
                // 3.4: mark the pixel, noting whether its east side is open.
                if east_empty {
                    labels[current] = -border;
                } else if labels[current] == 1 {
                    labels[current] = border;
                }
                // 3.5
                if next == start && current == first_pixel {
                    break;
                }
                previous = current;
                current = next;
            }
            contours.push(points);
        }
    }
    contours
}
/// Area of the polygon through the points (`cv2.contourArea`).
pub fn contour_area(points: &[Point]) -> f64 {
    let n = points.len();
    let mut twice = 0i64;
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        twice += a.0 as i64 * b.1 as i64 - b.0 as i64 * a.1 as i64;
    }
    twice.abs() as f64 / 2.0
}

/// `(x, y, width, height)` of the tight axis-aligned box (`cv2.boundingRect`).
pub fn bounding_rect(points: &[Point]) -> (i32, i32, i32, i32) {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for &(x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    (min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
}

pub struct ContourInfo {
    pub area: f64,
    pub bbox: (i32, i32, i32, i32),
    pub rect: RotatedRect,
}

pub fn contour_info(points: &[Point]) -> ContourInfo {
    let as_f64: Vec<(f64, f64)> = points.iter().map(|&(x, y)| (x as f64, y as f64)).collect();
    ContourInfo {
        area: contour_area(points),
        bbox: bounding_rect(points),
        rect: min_area_rect(&as_f64),
    }
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

    #[test]
    fn filled_rectangle_has_one_contour_with_opencv_area() {
        let image = image_from(&[
            "..........",
            "..######..",
            "..######..",
            "..######..",
            "..######..",
            "..........",
        ]);
        let contours = find_external_contours(&image);
        assert_eq!(contours.len(), 1);
        let info = contour_info(&contours[0]);
        // OpenCV: polygon through pixel centres, so (w-1) * (h-1).
        assert_eq!(info.area, 5.0 * 3.0);
        assert_eq!(info.bbox, (2, 1, 6, 4));
    }

    #[test]
    fn separate_blobs_are_separate_contours() {
        let image = image_from(&["##....##", "##....##", "........", "..####..", "..####.."]);
        assert_eq!(find_external_contours(&image).len(), 3);
    }

    #[test]
    fn diagonal_pixels_are_one_blob() {
        let image = image_from(&["#...", ".#..", "..#.", "...#"]);
        let contours = find_external_contours(&image);
        assert_eq!(contours.len(), 1);
        assert_eq!(bounding_rect(&contours[0]), (0, 0, 4, 4));
    }

    #[test]
    fn isolated_pixel_is_a_single_point() {
        let contours = find_external_contours(&image_from(&["...", ".#.", "..."]));
        assert_eq!(contours, vec![vec![(1, 1)]]);
        assert_eq!(contour_area(&contours[0]), 0.0);
    }

    #[test]
    fn a_hole_is_not_a_contour_and_neither_is_an_island_inside_it() {
        let image = image_from(&[
            "#########",
            "#.......#",
            "#.#.....#",
            "#.......#",
            "#########",
        ]);
        let contours = find_external_contours(&image);
        assert_eq!(contours.len(), 1, "ring only: {contours:?}");
        assert_eq!(bounding_rect(&contours[0]), (0, 0, 9, 5));
    }

    #[test]
    fn blob_touching_the_image_edge_is_found() {
        let image = image_from(&["####", "####", "...."]);
        let contours = find_external_contours(&image);
        assert_eq!(contours.len(), 1);
        assert_eq!(bounding_rect(&contours[0]), (0, 0, 4, 2));
    }

    #[test]
    fn thin_line_contour_visits_both_sides() {
        let image = image_from(&["......", ".####.", "......"]);
        let contours = find_external_contours(&image);
        assert_eq!(contours.len(), 1);
        assert_eq!(contour_area(&contours[0]), 0.0);
        assert_eq!(bounding_rect(&contours[0]), (1, 1, 4, 1));
    }

    #[test]
    fn empty_mask_has_no_contours() {
        assert!(find_external_contours(&GrayImage::new(8, 8)).is_empty());
    }
}

#[cfg(test)]
mod all_contour_tests {
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

    #[test]
    fn a_ring_has_an_outer_and_a_hole_border() {
        let image = image_from(&[
            "#########",
            "#.......#",
            "#.......#",
            "#.......#",
            "#########",
        ]);
        let mut areas: Vec<f64> = find_all_contours(&image)
            .iter()
            .map(|c| contour_area(c))
            .collect();
        areas.sort_by(f64::total_cmp);
        // The hole border only visits pixels that touch the hole edge-on, so it cuts the
        // four corner pixels off the 8 x 4 outline.
        assert_eq!(areas, vec![30.0, 32.0]);
    }

    #[test]
    fn solid_shapes_agree_with_the_external_finder() {
        let image = image_from(&[
            "..........",
            ".####..##.",
            ".####..##.",
            ".#.###....",
            "..........",
            "#.........",
        ]);
        let mut all: Vec<f64> = find_all_contours(&image)
            .iter()
            .map(|c| contour_area(c))
            .collect();
        let mut outer: Vec<f64> = find_external_contours(&image)
            .iter()
            .map(|c| contour_area(c))
            .collect();
        all.sort_by(f64::total_cmp);
        outer.sort_by(f64::total_cmp);
        assert_eq!(all, outer);
    }

    #[test]
    fn isolated_pixel_and_diagonal_line() {
        assert_eq!(
            find_all_contours(&image_from(&["...", ".#.", "..."])),
            vec![vec![(1, 1)]]
        );
        let diagonal = find_all_contours(&image_from(&["#...", ".#..", "..#.", "...#"]));
        assert_eq!(diagonal.len(), 1);
        assert_eq!(bounding_rect(&diagonal[0]), (0, 0, 4, 4));
    }
}
