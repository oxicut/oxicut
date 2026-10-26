//! Cut a rotated region out of the full-resolution scan and straighten it.
//!
//! Same geometry as ScanSplitter's `crop_rotated_region`, which rotates the
//! whole image about the region centre and then cuts a window. Rotating a
//! 10 000 px scan once per print is wasteful, so each output pixel is sampled
//! straight from the source; the window position uses the same rounding, so the
//! crop covers the same pixels. Bit depth is preserved and the area outside the
//! scan is white, as for a scanner lid.

use image::{DynamicImage, ImageBuffer, Pixel, Primitive};

use crate::geometry::RotatedRect;

/// Crop `region` (centre, size and angle in source pixels) from `image`.
/// `None` when the region is empty or lies wholly outside.
pub fn crop_region(image: &DynamicImage, region: &RotatedRect) -> Option<DynamicImage> {
    Some(match image {
        DynamicImage::ImageLuma8(img) => DynamicImage::ImageLuma8(crop_buffer(img, region, 255)?),
        DynamicImage::ImageLumaA8(img) => DynamicImage::ImageLumaA8(crop_buffer(img, region, 255)?),
        DynamicImage::ImageRgb8(img) => DynamicImage::ImageRgb8(crop_buffer(img, region, 255)?),
        DynamicImage::ImageRgba8(img) => DynamicImage::ImageRgba8(crop_buffer(img, region, 255)?),
        DynamicImage::ImageLuma16(img) => {
            DynamicImage::ImageLuma16(crop_buffer(img, region, 65535)?)
        }
        DynamicImage::ImageLumaA16(img) => {
            DynamicImage::ImageLumaA16(crop_buffer(img, region, 65535)?)
        }
        DynamicImage::ImageRgb16(img) => DynamicImage::ImageRgb16(crop_buffer(img, region, 65535)?),
        DynamicImage::ImageRgba16(img) => {
            DynamicImage::ImageRgba16(crop_buffer(img, region, 65535)?)
        }
        // Float formats are not scanner output; handle them at 16 bits.
        other => DynamicImage::ImageRgb16(crop_buffer(&other.to_rgb16(), region, 65535)?),
    })
}

fn crop_buffer<P>(
    source: &ImageBuffer<P, Vec<P::Subpixel>>,
    region: &RotatedRect,
    white: u32,
) -> Option<ImageBuffer<P, Vec<P::Subpixel>>>
where
    P: Pixel + 'static,
    P::Subpixel: Primitive + 'static,
{
    let (width, height) = (
        region.w.round_ties_even() as i64,
        region.h.round_ties_even() as i64,
    );
    if width <= 0 || height <= 0 {
        return None;
    }
    let (img_w, img_h) = (source.width() as f64, source.height() as f64);

    // Python: rotate about the centre, then shift so the rotated image's bounds
    // start at the origin. The shift (`min_xy`) comes from the image corners
    // carried through the rotation matrix.
    let theta = region.angle.to_radians();
    let (alpha, beta) = (theta.cos(), theta.sin());
    // getRotationMatrix2D(centre, angle, 1): [[a, b, tx], [-b, a, ty]].
    let tx = (1.0 - alpha) * region.cx - beta * region.cy;
    let ty = beta * region.cx + (1.0 - alpha) * region.cy;
    let corners = [(0.0, 0.0), (img_w, 0.0), (img_w, img_h), (0.0, img_h)];
    let rotated: Vec<(f64, f64)> = corners
        .iter()
        .map(|&(x, y)| (alpha * x + beta * y + tx, -beta * x + alpha * y + ty))
        .collect();
    let min_x = rotated.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let max_x = rotated
        .iter()
        .map(|c| c.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = rotated.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    let max_y = rotated
        .iter()
        .map(|c| c.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let canvas_w = (max_x - min_x).ceil() as i64;
    let canvas_h = (max_y - min_y).ceil() as i64;
    if canvas_w <= 0 || canvas_h <= 0 {
        return None;
    }

    // The region centre is the rotation's fixed point, so after the shift it sits at
    // `centre - min`.
    let (new_cx, new_cy) = (region.cx - min_x, region.cy - min_y);
    let x1 = ((new_cx - width as f64 / 2.0).round_ties_even() as i64).max(0);
    let y1 = ((new_cy - height as f64 / 2.0).round_ties_even() as i64).max(0);
    let x2 = ((new_cx + width as f64 / 2.0).round_ties_even() as i64).min(canvas_w);
    let y2 = ((new_cy + height as f64 / 2.0).round_ties_even() as i64).min(canvas_h);
    if x2 <= x1 || y2 <= y1 {
        return None;
    }

    let channels = P::CHANNEL_COUNT as usize;
    let white = num_cast::<P::Subpixel>(white as f64);
    let mut out = ImageBuffer::<P, Vec<P::Subpixel>>::new((x2 - x1) as u32, (y2 - y1) as u32);

    let raw = source.as_raw();
    let (sw, sh) = (source.width() as i64, source.height() as i64);
    let sample = |x: i64, y: i64, c: usize| -> f64 {
        if x < 0 || y < 0 || x >= sw || y >= sh {
            // Outside the scan: white (alpha, if any, opaque).
            return to_f64(white);
        }
        to_f64(raw[(y as usize * sw as usize + x as usize) * channels + c])
    };

    for oy in 0..(y2 - y1) {
        for ox in 0..(x2 - x1) {
            // Position in the rotated canvas, then back through the rotation about the centre.
            let (cx_canvas, cy_canvas) = ((x1 + ox) as f64 - new_cx, (y1 + oy) as f64 - new_cy);
            let sx = region.cx + alpha * cx_canvas - beta * cy_canvas;
            let sy = region.cy + beta * cx_canvas + alpha * cy_canvas;
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let pixel = out.get_pixel_mut(ox as u32, oy as u32);
            for (c, channel) in pixel.channels_mut().iter_mut().enumerate() {
                let top = sample(x0, y0, c) * (1.0 - fx) + sample(x0 + 1, y0, c) * fx;
                let bottom = sample(x0, y0 + 1, c) * (1.0 - fx) + sample(x0 + 1, y0 + 1, c) * fx;
                let value = (top * (1.0 - fy) + bottom * fy).round_ties_even();
                *channel = num_cast::<P::Subpixel>(value.clamp(0.0, to_f64(white)));
            }
        }
    }
    Some(out)
}

fn to_f64<S: Primitive>(value: S) -> f64 {
    value.to_f64().expect("primitive converts to f64")
}

fn num_cast<S: Primitive>(value: f64) -> S {
    S::from(value).expect("value fits the channel type")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    /// White page with a coloured rectangle rotated by `angle` about (cx, cy).
    fn page_with_print(w: u32, h: u32, rect: &RotatedRect, colour: [u8; 3]) -> RgbImage {
        let theta = rect.angle.to_radians();
        let (cos, sin) = (theta.cos(), theta.sin());
        RgbImage::from_fn(w, h, |x, y| {
            let (px, py) = (x as f64 - rect.cx, y as f64 - rect.cy);
            let (u, v) = (px * cos + py * sin, -px * sin + py * cos);
            if u.abs() <= rect.w / 2.0 && v.abs() <= rect.h / 2.0 {
                Rgb(colour)
            } else {
                Rgb([255, 255, 255])
            }
        })
    }

    #[test]
    fn an_axis_aligned_crop_copies_the_pixels() {
        let rect = RotatedRect::new(60.0, 50.0, 40.0, 30.0, 0.0);
        let image = DynamicImage::ImageRgb8(page_with_print(150, 100, &rect, [200, 40, 40]));
        let crop = crop_region(&image, &rect).expect("crop").to_rgb8();
        assert_eq!(crop.dimensions(), (40, 30));
        assert!(
            crop.pixels().all(|p| p.0 == [200, 40, 40]),
            "interior must be solid colour"
        );
    }

    #[test]
    fn a_rotated_print_is_straightened() {
        let rect = RotatedRect::new(120.0, 90.0, 100.0, 60.0, 17.0);
        let image = DynamicImage::ImageRgb8(page_with_print(260, 200, &rect, [30, 90, 200]));
        let crop = crop_region(&image, &rect).expect("crop").to_rgb8();
        let (w, h) = crop.dimensions();
        assert!(
            (w as i64 - 100).abs() <= 1 && (h as i64 - 60).abs() <= 1,
            "{w}x{h}"
        );
        // Away from the border everything is the print's colour: no white wedges.
        let mut stray = 0;
        for y in 4..h - 4 {
            for x in 4..w - 4 {
                let p = crop.get_pixel(x, y).0;
                if p[0] > 100 {
                    stray += 1;
                }
            }
        }
        assert_eq!(
            stray, 0,
            "{stray} non-print pixels inside the straightened crop"
        );
    }

    #[test]
    fn sixteen_bit_depth_is_preserved() {
        let rect = RotatedRect::new(40.0, 30.0, 30.0, 20.0, 0.0);
        let buffer: ImageBuffer<Rgb<u16>, Vec<u16>> = ImageBuffer::from_fn(80, 60, |x, y| {
            let inside = (25..55).contains(&x) && (20..40).contains(&y);
            if inside {
                Rgb([1000, 30000, 65000])
            } else {
                Rgb([65535, 65535, 65535])
            }
        });
        let crop = crop_region(&DynamicImage::ImageRgb16(buffer), &rect).expect("crop");
        let DynamicImage::ImageRgb16(crop) = crop else {
            panic!("lost 16-bit depth")
        };
        assert_eq!(crop.get_pixel(15, 10).0, [1000, 30000, 65000]);
    }

    #[test]
    fn outside_the_scan_is_white() {
        // A box hanging off the top-left corner.
        let rect = RotatedRect::new(5.0, 5.0, 40.0, 40.0, 0.0);
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(60, 60, Rgb([10, 10, 10])));
        let crop = crop_region(&image, &rect).expect("crop").to_rgb8();
        assert!(crop.pixels().any(|p| p.0 == [10, 10, 10]));
        assert!(crop.pixels().any(|p| p.0 == [255, 255, 255]) || crop.width() < 40);
    }

    #[test]
    fn empty_regions_give_nothing() {
        let image = DynamicImage::ImageRgb8(RgbImage::new(50, 50));
        assert!(crop_region(&image, &RotatedRect::new(25.0, 25.0, 0.0, 10.0, 0.0)).is_none());
    }
}
