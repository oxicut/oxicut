//! v3 candidate rectangles: one per blob in the region mask that is plausibly a
//! print (right size, filled enough, not a sliver).

use image::GrayImage;

use crate::contours::{ContourInfo, contour_info, find_external_contours};
use crate::geometry::RotatedRect;

/// A blob must fill at least this share of its axis-aligned bounding box.
const MIN_COMPONENT_FILL: f64 = 0.32;
/// Longest side over shortest side above which a blob is a sliver, not a print.
const MAX_ASPECT: f64 = 5.0;

/// Whether `info` passes v3's size, fill and aspect rules for an image of
/// `total_area` pixels.
pub fn is_candidate(
    info: &ContourInfo,
    total_area: f64,
    min_area_ratio: f64,
    max_area_ratio: f64,
) -> bool {
    let rect_area = info.rect.area();
    if !(min_area_ratio * total_area..=max_area_ratio * total_area).contains(&rect_area) {
        return false;
    }
    let (_, _, box_width, box_height) = info.bbox;
    let fill = info.area / ((box_width as f64) * (box_height as f64)).max(1.0);
    let (long, short) = (info.rect.w.max(info.rect.h), info.rect.w.min(info.rect.h));
    let aspect = long / short.max(1.0);
    fill >= MIN_COMPONENT_FILL && aspect <= MAX_ASPECT
}

pub fn candidate_rects(
    mask: &GrayImage,
    min_area_ratio: f64,
    max_area_ratio: f64,
) -> Vec<RotatedRect> {
    let total_area = mask.width() as f64 * mask.height() as f64;
    find_external_contours(mask)
        .iter()
        .map(|points| contour_info(points))
        .filter(|info| is_candidate(info, total_area, min_area_ratio, max_area_ratio))
        .map(|info| info.rect)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    fn mask_with_rects(w: u32, h: u32, rects: &[(u32, u32, u32, u32)]) -> GrayImage {
        let mut mask = GrayImage::new(w, h);
        for &(x0, y0, rw, rh) in rects {
            for y in y0..y0 + rh {
                for x in x0..x0 + rw {
                    mask.put_pixel(x, y, Luma([255]));
                }
            }
        }
        mask
    }

    #[test]
    fn keeps_a_print_sized_blob() {
        let mask = mask_with_rects(200, 200, &[(40, 50, 80, 60)]);
        let rects = candidate_rects(&mask, 0.02, 0.80);
        assert_eq!(rects.len(), 1);
        let rect = rects[0].canonical();
        assert!((rect.w - 79.0).abs() < 1e-6 && (rect.h - 59.0).abs() < 1e-6);
    }

    #[test]
    fn drops_specks_page_sized_blobs_and_slivers() {
        let mask = mask_with_rects(
            200,
            200,
            &[
                (5, 5, 6, 6),     // < 2% of the image
                (60, 60, 140, 5), // sliver, aspect 28
            ],
        );
        assert!(candidate_rects(&mask, 0.02, 0.80).is_empty());
        let page = mask_with_rects(200, 200, &[(2, 2, 196, 196)]);
        assert!(candidate_rects(&page, 0.02, 0.80).is_empty());
    }

    #[test]
    fn drops_sparse_blobs() {
        // An L shape: bounding box mostly empty.
        let mask = mask_with_rects(200, 200, &[(20, 20, 8, 120), (20, 132, 120, 8)]);
        assert!(candidate_rects(&mask, 0.02, 0.80).is_empty());
    }
}
