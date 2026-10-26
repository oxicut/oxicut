//! ScanSplitter's v5 cues: rectangles from sustained edge texture, closed frames around
//! pale prints, expansion of stacked prints seen through a textured sleeve, and
//! MobileSAM-verified recovery at the scan boundary. [`rectangles`] assembles them the
//! way `detector_v7._v5_rectangles` does; v7 uses the result as its anchors.

use image::{GrayImage, RgbImage};

use crate::canny::canny;
use crate::color::rgb_to_gray;
use crate::contours::{contour_area, contour_info, find_all_contours, find_external_contours};
use crate::filter::gaussian_blur_3x3;
use crate::foreground::Foreground;
use crate::geometry::{RotatedRect, iou, overlap_fraction};
use crate::mask::{Element, close, density_mask_above, open};
use crate::resize::for_detection;
use crate::sam::{MobileSam, SamError};

/// Long side of the working copy the texture and frame passes read.
const WORK_SIDE: u32 = 1800;

fn py_round(x: f64) -> usize {
    x.round_ties_even() as usize
}

/// A texture island and whether it comes within 1 % of the scan boundary.
pub type Texture = (RotatedRect, bool);

/// `_v5_touches_image_edge`.
pub fn touches_image_edge(rect: &RotatedRect, width: u32, height: u32) -> bool {
    const MARGIN: f32 = 0.01;
    let (w, h) = (width as f32, height as f32);
    rect.corners_f32().iter().any(|&(x, y)| {
        x < w * MARGIN || x > w * (1.0 - MARGIN) || y < h * MARGIN || y > h * (1.0 - MARGIN)
    })
}

/// `_v5_texture_rectangles`: photo-shaped islands of dense Canny edges.
pub fn texture_rectangles(
    rgb: &RgbImage,
    min_area_ratio: f64,
    max_area_ratio: f64,
) -> Vec<Texture> {
    let (original_width, original_height) = rgb.dimensions();
    let working = for_detection(rgb, WORK_SIDE);
    let (width, height) = working.image.dimensions();
    let (w, h) = (width as usize, height as usize);
    let gray = rgb_to_gray(&working.image);
    let edges = canny(gray.as_raw(), w, h, 25, 80);

    let min_side = w.min(h);
    let evidence = Foreground {
        width: w,
        height: h,
        mask: edges.iter().map(|&e| e > 0).collect(),
        otsu: 0,
    };
    let window = py_round(min_side as f64 * 0.035).max(7) | 1;
    let dense = density_mask_above(&evidence, window, 0.20);
    let open_size = py_round(min_side as f64 * 0.004).max(3) | 1;
    let close_size = py_round(min_side as f64 * 0.012).max(3) | 1;
    let opened = open(&dense, &Element::ellipse(open_size));
    let mask = close(&opened, &Element::rect(close_size), 2);

    let total = (w * h) as f64;
    let mut rectangles = Vec::new();
    for contour in find_external_contours(&mask) {
        let info = contour_info(&contour);
        let area = info.rect.area();
        if !(min_area_ratio * total..=max_area_ratio * total).contains(&area) {
            continue;
        }
        let (_, _, box_width, box_height) = info.bbox;
        let fill = info.area / ((box_width * box_height) as f64).max(1.0);
        let aspect = info.rect.w.max(info.rect.h) / info.rect.w.min(info.rect.h).max(1.0);
        if fill < 0.25 || aspect > 2.5 {
            continue;
        }
        let rect = info.rect.scaled(1.0 / working.scale);
        let touches = touches_image_edge(&rect, original_width, original_height);
        rectangles.push((rect, touches));
    }
    rectangles
}

/// `_v5_frame_rectangles`: closed, nearly rectangular edge outlines.
pub fn frame_rectangles(
    rgb: &RgbImage,
    min_area_ratio: f64,
    max_area_ratio: f64,
) -> Vec<RotatedRect> {
    let working = for_detection(rgb, WORK_SIDE);
    let (width, height) = working.image.dimensions();
    let (w, h) = (width as usize, height as usize);
    let gray = rgb_to_gray(&working.image);
    let blurred = gaussian_blur_3x3(gray.as_raw(), w, h);
    let edges = canny(&blurred, w, h, 15, 50);
    let edges = GrayImage::from_raw(width, height, edges).expect("sized");
    let edges = close(&edges, &Element::rect(3), 1);

    let total = (w * h) as f64;
    let mut candidates: Vec<(f64, RotatedRect)> = Vec::new();
    for contour in find_all_contours(&edges).into_iter().rev() {
        let area = contour_area(&contour);
        if !(min_area_ratio * total..=max_area_ratio.min(0.30) * total).contains(&area) {
            continue;
        }
        let rect = contour_info(&contour).rect;
        let rect_area = rect.area();
        if rect_area <= 0.0 {
            continue;
        }
        let rectangularity = area / rect_area;
        let aspect = rect.w.max(rect.h) / rect.w.min(rect.h).max(1.0);
        if rectangularity < 0.82 || aspect > 2.5 {
            continue;
        }
        candidates.push((rectangularity, rect.scaled(1.0 / working.scale)));
    }

    // Python's sort is stable, descending by rectangularity.
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    let near = 15.0 / working.scale;
    let mut rectangles: Vec<RotatedRect> = Vec::new();
    for (_, rect) in candidates {
        let duplicate = rectangles.iter().any(|existing| {
            iou(&rect, existing) > 0.75
                || ((rect.cx - existing.cx).abs() < near && (rect.cy - existing.cy).abs() < near)
        });
        if !duplicate {
            rectangles.push(rect);
        }
    }
    rectangles
}

/// `_v5_angle_difference`: the gap between two angles, for shapes with 90-degree symmetry.
fn angle_difference(first: f64, second: f64) -> f64 {
    (((first - second + 45.0).rem_euclid(90.0)) - 45.0).abs()
}

/// `_v5_combine_rectangles`: settle each conservative base region against the texture
/// islands that overlap it.
pub fn combine(base: &[RotatedRect], textures: &[Texture]) -> Vec<RotatedRect> {
    let mut assignments: Vec<Vec<Texture>> = vec![Vec::new(); base.len()];
    let mut unmatched: Vec<Texture> = Vec::new();
    for &(texture, touches) in textures {
        let overlaps: Vec<f64> = base.iter().map(|b| overlap_fraction(&texture, b)).collect();
        // numpy's argmax takes the first maximum.
        let best = overlaps
            .iter()
            .enumerate()
            .fold(None, |best: Option<(usize, f64)>, (i, &v)| match best {
                Some((_, top)) if top >= v => best,
                _ => Some((i, v)),
            });
        match best {
            Some((index, value)) if value >= 0.60 => assignments[index].push((texture, touches)),
            _ => unmatched.push((texture, touches)),
        }
    }

    let mut combined: Vec<RotatedRect> = Vec::new();
    for (base_rect, matches) in base.iter().zip(&assignments) {
        let interior: Vec<RotatedRect> = matches
            .iter()
            .filter(|(_, edge)| !edge)
            .map(|(t, _)| *t)
            .collect();
        if interior.len() >= 2 {
            // One colour-based region spanning several detailed islands is a merged
            // album page: split it at the islands.
            combined.extend(interior);
            continue;
        }
        if let [(texture, touches)] = matches.as_slice() {
            let ratio = texture.area() / base_rect.area().max(1.0);
            if !touches
                && (0.25..=1.30).contains(&ratio)
                && angle_difference(texture.angle, base_rect.angle) <= 3.0
            {
                combined.push(*texture);
                continue;
            }
        }
        combined.push(*base_rect);
    }

    for (texture, touches) in unmatched {
        if touches {
            continue;
        }
        if combined
            .iter()
            .any(|existing| overlap_fraction(&texture, existing) > 0.65)
        {
            continue;
        }
        combined.push(texture);
    }
    combined
}

/// `cv2.pointPolygonTest(..., False) >= 0` for a convex quadrilateral.
fn inside_or_on(corners: &[(f32, f32); 4], point: (f64, f64)) -> bool {
    let mut positive = false;
    let mut negative = false;
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        let cross = (b.0 as f64 - a.0 as f64) * (point.1 - a.1 as f64)
            - (b.1 as f64 - a.1 as f64) * (point.0 - a.0 as f64);
        positive |= cross > 0.0;
        negative |= cross < 0.0;
    }
    !(positive && negative)
}

/// `_v5_expand_stacked_sleeve_rectangles`: stacked prints under one textured sleeve share
/// the sleeve's width.
pub fn expand_stacked_sleeves(
    rectangles: &[RotatedRect],
    textures: &[Texture],
    image_size: (u32, u32),
) -> Vec<RotatedRect> {
    let image_area = image_size.0 as f64 * image_size.1 as f64;
    let mut result: Vec<RotatedRect> = rectangles.to_vec();
    for &(sleeve, _) in textures {
        let (sleeve_width, sleeve_height) = (sleeve.w, sleeve.h);
        let sleeve_area = sleeve_width * sleeve_height;
        let long_size = sleeve_width.max(sleeve_height);
        let short_size = sleeve_width.min(sleeve_height);
        if sleeve_area < image_area * 0.14 || long_size / short_size.max(1.0) < 1.5 {
            continue;
        }

        let angle = sleeve.angle.to_radians() as f32 as f64;
        let width_axis = [angle.cos() as f32, angle.sin() as f32];
        let height_axis = [-(angle.sin() as f32), angle.cos() as f32];
        let (long_axis, _short_axis) = if sleeve_width >= sleeve_height {
            (width_axis, height_axis)
        } else {
            (height_axis, width_axis)
        };
        let short_axis = if sleeve_width >= sleeve_height {
            height_axis
        } else {
            width_axis
        };
        let centre = [sleeve.cx as f32, sleeve.cy as f32];
        let polygon = sleeve.corners_f32();
        let dot = |v: [f32; 2], axis: [f32; 2]| v[0] * axis[0] + v[1] * axis[1];

        // (index, offset along the stack, offset across it)
        let mut anchors: Vec<(usize, f32, f32)> = Vec::new();
        for (index, rect) in result.iter().enumerate() {
            if !inside_or_on(&polygon, (rect.cx, rect.cy)) {
                continue;
            }
            if rect.area() > sleeve_area * 0.40 {
                continue;
            }
            if angle_difference(rect.angle, sleeve.angle) > 12.0 {
                continue;
            }
            let offset = [rect.cx as f32 - centre[0], rect.cy as f32 - centre[1]];
            anchors.push((index, dot(offset, long_axis), dot(offset, short_axis)));
        }
        anchors.sort_by(|a, b| a.1.total_cmp(&b.1));
        if anchors.len() < 2 {
            continue;
        }
        let across_max = anchors.iter().map(|a| a.2).fold(f32::MIN, f32::max);
        let across_min = anchors.iter().map(|a| a.2).fold(f32::MAX, f32::min);
        if (across_max - across_min) as f64 > short_size * 0.35 {
            continue;
        }
        if anchors
            .windows(2)
            .any(|pair| ((pair[1].1 - pair[0].1) as f64) < short_size * 0.45)
        {
            continue;
        }

        let replaced: Vec<usize> = anchors.iter().map(|a| a.0).collect();
        let lengths: Vec<f64> = anchors
            .iter()
            .map(|&(index, _, _)| {
                let projected: Vec<f32> = result[index]
                    .corners_f32()
                    .iter()
                    .map(|&(x, y)| dot([x - centre[0], y - centre[1]], long_axis))
                    .collect();
                let max = projected.iter().cloned().fold(f32::MIN, f32::max);
                let min = projected.iter().cloned().fold(f32::MAX, f32::min);
                (max - min) as f64
            })
            .collect();
        let common = lengths.iter().cloned().fold(f64::MIN, f64::max);
        let shortest = lengths.iter().cloned().fold(f64::MAX, f64::min);
        let use_common_rows = shortest >= common * 0.55;
        let offsets: Vec<f64> = if use_common_rows {
            let first = -long_size / 2.0 + common / 2.0;
            let last = long_size / 2.0 - common / 2.0;
            let n = anchors.len();
            (0..n)
                .map(|i| first + (last - first) * i as f64 / (n - 1) as f64)
                .collect()
        } else {
            anchors.iter().map(|a| a.1 as f64).collect()
        };

        let mut replacements: Vec<RotatedRect> = Vec::new();
        for (length, offset) in lengths.iter().zip(&offsets) {
            let segment = if use_common_rows { common } else { *length };
            let cx = centre[0] as f64 + long_axis[0] as f64 * offset;
            let cy = centre[1] as f64 + long_axis[1] as f64 * offset;
            let (w, h) = if sleeve_width >= sleeve_height {
                (segment, short_size * 0.92)
            } else {
                (short_size * 0.92, segment)
            };
            replacements.push(RotatedRect::new(cx, cy, w, h, sleeve.angle));
        }
        result = result
            .into_iter()
            .enumerate()
            .filter(|(index, _)| !replaced.contains(index))
            .map(|(_, rect)| rect)
            .collect();
        result.extend(replacements);
    }
    result
}

/// `_v5_recover_boundary_rectangles`: texture touching the scan edge counts only when a
/// tightly prompted MobileSAM mask finds a different, interior rectangle.
pub fn recover_boundary(
    sam: &mut MobileSam,
    rgb: &RgbImage,
    textures: &[Texture],
    existing: &[RotatedRect],
    min_area_ratio: f64,
    max_area_ratio: f64,
) -> Result<Vec<RotatedRect>, SamError> {
    let proposals: Vec<RotatedRect> = textures
        .iter()
        .filter(|(_, touches)| *touches)
        .map(|(rect, _)| *rect)
        .collect();
    if proposals.is_empty() {
        return Ok(Vec::new());
    }
    let refined = sam.refine_rectangles(rgb, &proposals, 0.0)?;
    let (width, height) = rgb.dimensions();
    let image_area = width as f64 * height as f64;
    let mut recovered: Vec<RotatedRect> = Vec::new();
    for (proposal, rect) in proposals.iter().zip(&refined) {
        let area = rect.area();
        let aspect = rect.w.max(rect.h) / rect.w.min(rect.h).max(1.0);
        if iou(proposal, rect) >= 0.90 {
            continue;
        }
        if !(min_area_ratio * image_area..=max_area_ratio * image_area).contains(&area) {
            continue;
        }
        if aspect > 2.5 || touches_image_edge(rect, width, height) {
            continue;
        }
        if existing
            .iter()
            .chain(&recovered)
            .any(|other| overlap_fraction(rect, other) > 0.65)
        {
            continue;
        }
        recovered.push(*rect);
    }
    Ok(recovered)
}

/// `detector_v7._v5_rectangles`: the rectangle set v5 would turn into regions, given v3's
/// proposals in full-resolution pixels.
pub fn rectangles(
    sam: &mut MobileSam,
    rgb: &RgbImage,
    proposals: &[RotatedRect],
    min_area_ratio: f64,
    max_area_ratio: f64,
) -> Result<Vec<RotatedRect>, SamError> {
    let (width, height) = rgb.dimensions();
    let image_area = width as f64 * height as f64;
    let refined = sam.refine_rectangles(rgb, proposals, 0.08)?;
    let base: Vec<RotatedRect> = proposals
        .iter()
        .zip(&refined)
        .map(|(proposal, rect)| {
            let rect = if (min_area_ratio..=max_area_ratio).contains(&(rect.area() / image_area)) {
                *rect
            } else {
                *proposal
            };
            // v5 shrinks the SAM base by its fixed 20 px inset on every side.
            RotatedRect::new(
                rect.cx,
                rect.cy,
                (rect.w - 40.0).max(1.0),
                (rect.h - 40.0).max(1.0),
                rect.angle,
            )
        })
        .collect();

    let textures = texture_rectangles(rgb, min_area_ratio, max_area_ratio);
    let combined = combine(&base, &textures);
    let mut rects = expand_stacked_sleeves(&combined, &textures, (width, height));
    for frame in frame_rectangles(rgb, min_area_ratio, max_area_ratio) {
        if frame.area() > image_area * 0.18 {
            continue;
        }
        if !rects
            .iter()
            .any(|existing| overlap_fraction(&frame, existing) > 0.65)
        {
            rects.push(frame);
        }
    }
    let recovered = recover_boundary(sam, rgb, &textures, &rects, min_area_ratio, max_area_ratio)?;
    rects.extend(recovered);
    Ok(rects)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(cx: f64, cy: f64, w: f64, h: f64) -> RotatedRect {
        RotatedRect::new(cx, cy, w, h, 0.0)
    }

    #[test]
    fn angle_difference_wraps_at_ninety_degrees() {
        assert!((angle_difference(-90.0, 0.0)).abs() < 1e-9);
        assert!((angle_difference(-1.0, -89.0) - 2.0).abs() < 1e-9);
        assert!((angle_difference(10.0, 0.0) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn combine_prefers_the_matching_texture_and_splits_merged_regions() {
        // One base with a single similar interior texture: the texture replaces it.
        let base = [rect(100.0, 100.0, 100.0, 80.0)];
        let texture = rect(101.0, 100.0, 90.0, 70.0);
        let combined = combine(&base, &[(texture, false)]);
        assert_eq!(combined, vec![texture]);

        // A base covering two interior textures is split into them.
        let wide = [rect(200.0, 100.0, 400.0, 100.0)];
        let left = rect(100.0, 100.0, 150.0, 90.0);
        let right = rect(300.0, 100.0, 150.0, 90.0);
        let combined = combine(&wide, &[(left, false), (right, false)]);
        assert_eq!(combined, vec![left, right]);

        // A boundary-touching texture with no base never creates a region.
        assert!(combine(&[], &[(rect(10.0, 10.0, 50.0, 50.0), true)]).is_empty());
        assert_eq!(
            combine(&[], &[(rect(10.0, 10.0, 50.0, 50.0), false)]).len(),
            1
        );
    }

    #[test]
    fn point_in_quad_includes_the_edge() {
        let quad = rect(50.0, 50.0, 40.0, 20.0).corners_f32();
        assert!(inside_or_on(&quad, (50.0, 50.0)));
        assert!(!inside_or_on(&quad, (80.0, 50.0)));
    }

    #[test]
    fn texture_island_is_found_on_a_blank_page() {
        // A noisy photo-like block on white paper.
        let mut image = RgbImage::from_pixel(800, 600, image::Rgb([250, 250, 250]));
        for y in 150..450u32 {
            for x in 200..600u32 {
                let v = (((x * 37 + y * 91) ^ (x * y)) % 200) as u8 + 20;
                image.put_pixel(x, y, image::Rgb([v, v, v]));
            }
        }
        let textures = texture_rectangles(&image, 0.02, 0.8);
        assert_eq!(textures.len(), 1, "{textures:?}");
        let (r, touches) = textures[0];
        assert!(!touches);
        assert!(
            (r.cx - 400.0).abs() < 20.0 && (r.cy - 300.0).abs() < 20.0,
            "{r:?}"
        );
    }
}
