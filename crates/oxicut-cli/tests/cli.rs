//! End-to-end checks of the `oxicut` binary on a generated scan.

use std::path::{Path, PathBuf};
use std::process::Command;

use image::{Rgb, RgbImage};

fn oxicut() -> Command {
    Command::new(env!("CARGO_BIN_EXE_oxicut"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oxicut-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A pale scan with two coloured prints on it.
fn write_scan(path: &Path) {
    let mut scan = RgbImage::from_pixel(1000, 700, Rgb([248, 248, 246]));
    for &(x0, y0, w, h, c) in &[
        (100u32, 120u32, 360u32, 460u32, [70u8, 90, 130]),
        (560, 150, 340, 400, [150, 60, 50]),
    ] {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                scan.put_pixel(
                    x,
                    y,
                    Rgb([
                        c[0].saturating_add(((x * 7 + y * 13) % 11) as u8),
                        c[1],
                        c[2],
                    ]),
                );
            }
        }
    }
    scan.save(path).unwrap();
}

#[test]
fn detect_prints_boxes_json_to_stdout() {
    let dir = scratch("stdout");
    let scan = dir.join("scan.png");
    write_scan(&scan);

    let result = oxicut()
        .args(["detect"])
        .arg(&scan)
        .args(["--inset", "0"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["width"], 1000);
    assert_eq!(json["height"], 700);
    let regions = json["regions"].as_array().unwrap();
    assert_eq!(regions.len(), 2, "{json}");
    // Reading order: left print first.
    assert!(regions[0]["cx"].as_f64().unwrap() < regions[1]["cx"].as_f64().unwrap());
}

#[test]
fn debug_flag_writes_the_overlays_and_stage_data() {
    let dir = scratch("debug");
    let scan = dir.join("scan.png");
    write_scan(&scan);
    let debug = dir.join("debug");

    let result = oxicut()
        .arg("detect")
        .arg(&scan)
        .arg("--debug")
        .arg(&debug)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for file in [
        "boxes.json",
        "debug.png",
        "stages.png",
        "foreground.png",
        "region-mask.png",
        "stages.json",
    ] {
        assert!(debug.join(file).is_file(), "{file} missing");
    }
    // With --debug and no --output, stdout stays empty.
    assert!(result.stdout.is_empty());
}

#[test]
fn crop_writes_one_straightened_image_per_box_and_leaves_the_scan_alone() {
    let dir = scratch("crop");
    let scan = dir.join("scan.png");
    write_scan(&scan);
    let before = std::fs::read(&scan).unwrap();
    let boxes = dir.join("boxes.json");

    assert!(
        oxicut()
            .arg("detect")
            .arg(&scan)
            .args(["--inset", "0", "-o"])
            .arg(&boxes)
            .status()
            .unwrap()
            .success()
    );
    let out = dir.join("crops");
    let result = oxicut()
        .arg("crop")
        .arg(&scan)
        .arg("--boxes")
        .arg(&boxes)
        .arg("-o")
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let first = image::open(out.join("scan-1.png")).unwrap().to_rgb8();
    let second = image::open(out.join("scan-2.png")).unwrap().to_rgb8();
    // The prints are 360x460 and 340x400 (normalised so width >= height).
    assert!(
        (first.width() as i64 - 460).abs() <= 3 && (first.height() as i64 - 360).abs() <= 3,
        "{:?}",
        first.dimensions()
    );
    assert!(
        (second.width() as i64 - 400).abs() <= 3 && (second.height() as i64 - 340).abs() <= 3,
        "{:?}",
        second.dimensions()
    );
    assert_eq!(
        std::fs::read(&scan).unwrap(),
        before,
        "the scan must not change"
    );
}

#[test]
fn crop_refuses_boxes_from_a_different_sized_scan() {
    let dir = scratch("mismatch");
    let scan = dir.join("scan.png");
    write_scan(&scan);
    let boxes = dir.join("boxes.json");
    assert!(
        oxicut()
            .arg("detect")
            .arg(&scan)
            .arg("-o")
            .arg(&boxes)
            .status()
            .unwrap()
            .success()
    );

    let other = dir.join("other.png");
    RgbImage::from_pixel(500, 500, Rgb([255, 255, 255]))
        .save(&other)
        .unwrap();
    let result = oxicut()
        .arg("crop")
        .arg(&other)
        .arg("--boxes")
        .arg(&boxes)
        .arg("-o")
        .arg(dir.join("out"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("boxes were detected in a 1000x700"));
}

#[test]
fn jpeg_and_tiff_output_formats_work() {
    let dir = scratch("formats");
    let scan = dir.join("scan.png");
    write_scan(&scan);
    let boxes = dir.join("boxes.json");
    assert!(
        oxicut()
            .arg("detect")
            .arg(&scan)
            .args(["--inset", "0", "-o"])
            .arg(&boxes)
            .status()
            .unwrap()
            .success()
    );

    for (format, extension) in [("jpg", "jpg"), ("tiff", "tif"), ("png", "png")] {
        let out = dir.join(format!("out-{format}"));
        let status = oxicut()
            .arg("crop")
            .arg(&scan)
            .arg("--boxes")
            .arg(&boxes)
            .arg("-o")
            .arg(&out)
            .args(["--format", format])
            .status()
            .unwrap();
        assert!(status.success(), "{format}");
        assert!(
            image::open(out.join(format!("scan-1.{extension}"))).is_ok(),
            "{format} output unreadable"
        );
    }
}

#[test]
fn output_folders_are_created_when_missing() {
    let dir = scratch("mkdir");
    let scan = dir.join("scan.png");
    write_scan(&scan);
    let boxes = dir.join("not").join("yet").join("there").join("boxes.json");
    let status = oxicut()
        .arg("detect")
        .arg(&scan)
        .arg("-o")
        .arg(&boxes)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(boxes.is_file());
}

#[test]
fn bad_arguments_fail_with_a_message() {
    let result = oxicut()
        .args(["detect", "no-such-file.png"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("reading no-such-file.png"));
    let result = oxicut()
        .args(["detect", "x.png", "--min-area", "150"])
        .output()
        .unwrap();
    assert!(!result.status.success());
}
