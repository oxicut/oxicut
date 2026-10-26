//! Debug images: the detector's boxes drawn over the scan.

use std::path::Path;

use anyhow::Result;
use image::{GrayImage, Rgb, RgbImage};
use oxicut_core::RotatedRect;
use oxicut_core::detect::Detection;
use oxicut_core::resize::resize_area;

const GREEN: Rgb<u8> = Rgb([40, 220, 60]);
const YELLOW: Rgb<u8> = Rgb([255, 210, 0]);
const CYAN: Rgb<u8> = Rgb([0, 210, 255]);
const MAGENTA: Rgb<u8> = Rgb([255, 60, 220]);

/// Longest side of the full-scan overlay; scans are far bigger than a screen.
const OVERLAY_MAX_SIDE: u32 = 2400;

/// Draw a thick line with a square brush.
fn line(image: &mut RgbImage, from: (f64, f64), to: (f64, f64), colour: Rgb<u8>, thickness: i64) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as i64;
    let radius = thickness / 2;
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let (x, y) = (
            (from.0 + dx * t).round() as i64,
            (from.1 + dy * t).round() as i64,
        );
        for oy in -radius..=radius {
            for ox in -radius..=radius {
                let (px, py) = (x + ox, y + oy);
                if px >= 0 && py >= 0 && (px as u32) < image.width() && (py as u32) < image.height()
                {
                    image.put_pixel(px as u32, py as u32, colour);
                }
            }
        }
    }
}

/// Outline `rect`, whose coordinates are multiplied by `scale` first.
fn outline(image: &mut RgbImage, rect: &RotatedRect, scale: f64, colour: Rgb<u8>, thickness: i64) {
    let corners = rect.corners();
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        line(
            image,
            (a.0 * scale, a.1 * scale),
            (b.0 * scale, b.1 * scale),
            colour,
            thickness,
        );
    }
}

fn mask_image(mask: &GrayImage) -> RgbImage {
    RgbImage::from_fn(mask.width(), mask.height(), |x, y| {
        let v = mask.get_pixel(x, y).0[0];
        Rgb([v, v, v])
    })
}

pub fn write_debug_images(dir: &Path, source: &RgbImage, detection: &Detection) -> Result<()> {
    // Final boxes over the scan, shrunk to a viewable size.
    let scale = (OVERLAY_MAX_SIDE as f64 / source.width().max(source.height()) as f64).min(1.0);
    let (w, h) = (
        ((source.width() as f64 * scale).round() as u32).max(1),
        ((source.height() as f64 * scale).round() as u32).max(1),
    );
    let mut page = resize_area(source, w, h);
    for (index, region) in detection.regions.iter().enumerate() {
        outline(&mut page, &region.rect, scale, GREEN, 3);
        // A tick at the centre and the reading-order number's position.
        let (cx, cy) = (region.rect.cx * scale, region.rect.cy * scale);
        line(&mut page, (cx - 8.0, cy), (cx + 8.0, cy), GREEN, 3);
        line(&mut page, (cx, cy - 8.0), (cx, cy + 8.0), GREEN, 3);
        let _ = index;
    }
    page.save(dir.join("debug.png"))?;

    // Every stage over the detection-size image.
    let stages = &detection.stages;
    let (dw, dh) = stages.detection_size;
    let working = resize_area(source, dw, dh);
    let mut staged = working.clone();
    for rect in &stages.candidates {
        outline(&mut staged, rect, 1.0, YELLOW, 2);
    }
    for rect in &stages.split {
        outline(&mut staged, rect, 1.0, CYAN, 2);
    }
    for rect in &stages.snapped {
        outline(&mut staged, rect, 1.0, MAGENTA, 2);
    }
    for rect in &stages.refined {
        outline(&mut staged, rect, 1.0, GREEN, 2);
    }
    staged.save(dir.join("stages.png"))?;

    mask_image(&stages.foreground_mask).save(dir.join("foreground.png"))?;
    mask_image(&stages.region_mask).save(dir.join("region-mask.png"))?;

    let summary = serde_json::json!({
        "detection_size": [dw, dh],
        "scale": stages.scale,
        "backgrounds_lab": stages.backgrounds_lab,
        "otsu": stages.otsu,
        "candidates": rects(&stages.candidates),
        "merged": rects(&stages.merged),
        "deduplicated": rects(&stages.deduplicated),
        "split": rects(&stages.split),
        "snapped": rects(&stages.snapped),
        "refined": rects(&stages.refined),
        "legend": "stages.png: yellow candidates, cyan after split, magenta angle-snapped, green refined",
    });
    std::fs::write(
        dir.join("stages.json"),
        serde_json::to_string_pretty(&summary)?,
    )?;
    Ok(())
}

fn rects(rects: &[RotatedRect]) -> Vec<serde_json::Value> {
    rects
        .iter()
        .map(|r| serde_json::json!({"cx": r.cx, "cy": r.cy, "w": r.w, "h": r.h, "angle": r.angle}))
        .collect()
}
