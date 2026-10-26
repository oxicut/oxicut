//! v7's chroma proposals (`_chroma_rects`): inner-photo rectangles from the b* (yellow-blue)
//! channel. Aged album paper and print borders are strongly yellow while the silver
//! emulsion of black-and-white prints is nearly neutral, so b* separates the photograph
//! even where its luminance matches the border.

use image::{GrayImage, RgbImage};

use crate::color::rgb_to_lab;
use crate::contours::{contour_info, find_external_contours};
use crate::geometry::RotatedRect;
use crate::imgproc::{dilate_ellipse_u8, gaussian_blur_u8};
use crate::mask::{Element, close, open};
use crate::resize::resize_area;
use crate::score::percentile;

fn py_round(x: f64) -> usize {
    x.round_ties_even() as usize
}

pub fn chroma_rectangles(
    rgb: &RgbImage,
    min_area_ratio: f64,
    max_area_ratio: f64,
) -> Vec<RotatedRect> {
    const MAX_DIM: f64 = 1500.0;
    let (ow, oh) = rgb.dimensions();
    let s = (MAX_DIM / ow.max(oh) as f64).min(1.0);
    let small = resize_area(
        rgb,
        py_round(ow as f64 * s) as u32,
        py_round(oh as f64 * s) as u32,
    );
    let (w, h) = (small.width() as usize, small.height() as usize);

    let lab = rgb_to_lab(&small);
    let lum: Vec<u8> = lab.data.iter().map(|p| p[0] as u8).collect();
    let b_raw: Vec<u8> = lab.data.iter().map(|p| p[2] as u8).collect();
    let b = gaussian_blur_u8(&b_raw, w, h, 2.0);

    let bright_values: Vec<f64> = lum
        .iter()
        .zip(&b)
        .filter(|(l, _)| **l > 90)
        .map(|(_, &v)| v as f64)
        .collect();
    // Only meaningful on yellowed paper; on neutral or coloured pages the b* contrast
    // says nothing about prints.
    if (bright_values.len() as f64) < 0.2 * (w * h) as f64
        || percentile(&bright_values, 75.0) < 138.0
    {
        return Vec::new();
    }

    let kk = (0.15 * w.max(h) as f64) as usize | 1;
    let page = gaussian_blur_u8(&dilate_ellipse_u8(&b, w, h, kk), w, h, kk as f64 / 6.0);
    let raw: Vec<u8> = (0..w * h)
        .map(|i| u8::from((page[i] as f32 - b[i] as f32) > 9.0 && lum[i] > 90) * 255)
        .collect();
    let mask = GrayImage::from_raw(w as u32, h as u32, raw).expect("sized");

    let k = py_round(w.min(h) as f64 * 0.01).max(5) | 1;
    let mask = open(&mask, &Element::ellipse(k));
    let mask = close(&mask, &Element::ellipse(2 * k + 1), 1);

    let total = (w * h) as f64;
    let mut out = Vec::new();
    for contour in find_external_contours(&mask) {
        let info = contour_info(&contour);
        let area = info.rect.area();
        if !(min_area_ratio * total..=max_area_ratio * total).contains(&area) {
            continue;
        }
        if info.area / area.max(1.0) < 0.75
            || info.rect.w.max(info.rect.h) / info.rect.w.min(info.rect.h).max(1.0) > 4.0
        {
            continue;
        }
        out.push(info.rect.scaled(1.0 / s));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn neutral_pages_have_no_chroma_proposals() {
        let page = RgbImage::from_pixel(600, 400, Rgb([200, 200, 200]));
        assert!(chroma_rectangles(&page, 0.02, 0.8).is_empty());
    }

    #[test]
    fn a_neutral_photo_on_cream_paper_is_found() {
        let mut image = RgbImage::from_pixel(600, 400, Rgb([232, 214, 160]));
        for y in 90..310 {
            for x in 140..460 {
                image.put_pixel(x, y, Rgb([150, 150, 150]));
            }
        }
        let found = chroma_rectangles(&image, 0.02, 0.8);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!((found[0].cx - 300.0).abs() < 8.0 && (found[0].cy - 200.0).abs() < 8.0);
    }
}
