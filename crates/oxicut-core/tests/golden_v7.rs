//! Compare stages of the Rust v7 port with ScanSplitter's Python output.
//!
//! Golden values are in `benchmarks/golden/v7.json` (made by
//! `tools/dump_v7_reference.py`); pixels come from `reference/decoded/*.png`,
//! Pillow's own decode of each fixture, so differences in JPEG decoders cannot
//! blur the comparison. Tests skip when the reference checkout is missing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use oxicut_core::RotatedRect;
use oxicut_core::io::{load_image, to_detection_rgb};
use oxicut_core::pil::working_gray;
use oxicut_core::tighten::{GrayF32, tighten_rect};
use serde::Deserialize;

/// Long side of v7's working grey image (`_WORK_MAX`).
const WORK_MAX: u32 = 2000;

#[derive(Deserialize)]
struct Golden {
    images: BTreeMap<String, GoldenImage>,
}

#[derive(Deserialize)]
struct GoldenImage {
    work_scale: f64,
    v3: Vec<GoldenRect>,
    tight: Vec<GoldenRect>,
}

#[derive(Deserialize, Clone, Copy)]
struct GoldenRect {
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    angle: f64,
}

impl GoldenRect {
    fn rect(self) -> RotatedRect {
        RotatedRect::new(self.cx, self.cy, self.w, self.h, self.angle)
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden() -> Golden {
    let text = std::fs::read_to_string(workspace_root().join("benchmarks/golden/v7.json"))
        .expect("benchmarks/golden/v7.json is committed");
    serde_json::from_str(&text).expect("golden json parses")
}

fn reference_available() -> bool {
    let present = workspace_root()
        .join("reference/decoded/benchmarks")
        .is_dir();
    if !present {
        eprintln!("skipping: reference/decoded is missing (run tools/dump_v3_reference.py)");
    }
    present
}

fn decoded(name: &str) -> image::RgbImage {
    let path = workspace_root()
        .join("reference/decoded")
        .join(format!("{name}.png"));
    to_detection_rgb(&load_image(&path).unwrap())
}

/// Largest difference in centre, size, or angle between two rectangles.
fn gap(a: &RotatedRect, b: &RotatedRect) -> f64 {
    [
        (a.cx - b.cx).abs(),
        (a.cy - b.cy).abs(),
        (a.w - b.w).abs(),
        (a.h - b.h).abs(),
        (a.angle - b.angle).abs(),
    ]
    .into_iter()
    .fold(0.0, f64::max)
}

#[test]
fn the_working_image_matches_pythons_scale() {
    if !reference_available() {
        return;
    }
    for (name, expected) in golden().images.iter().take(12) {
        let (_, scale) = working_gray(&decoded(name), WORK_MAX);
        assert_eq!(scale, expected.work_scale, "{name}");
    }
}

#[test]
fn tightening_matches_python() {
    if !reference_available() {
        return;
    }
    let golden = golden();
    let (mut rects, mut exact, mut close, mut moved, mut worst) = (0, 0, 0, 0, 0f64);
    let mut problems = Vec::new();
    for (name, expected) in &golden.images {
        let (gray, scale) = working_gray(&decoded(name), WORK_MAX);
        let gray = GrayF32::from_gray(&gray);
        for (index, (input, want)) in expected.v3.iter().zip(&expected.tight).enumerate() {
            let ours = tighten_rect(&gray, &input.rect(), scale, 2);
            let difference = gap(&ours, &want.rect());
            rects += 1;
            moved += (gap(&input.rect(), &want.rect()) > 0.01) as usize;
            exact += (difference < 0.01) as usize;
            close += (difference < 1.0) as usize;
            worst = worst.max(difference);
            if difference >= 1.0 {
                problems.push(format!(
                    "{name}#{index}: ours ({:.1},{:.1}) {:.1}x{:.1}  python ({:.1},{:.1}) {:.1}x{:.1}  input {:.1}x{:.1}",
                    ours.cx, ours.cy, ours.w, ours.h, want.cx, want.cy, want.w, want.h, input.w, input.h
                ));
            }
        }
    }
    println!(
        "{rects} rectangles ({moved} moved by Python): within 0.01 px {exact}, within 1 px {close}, worst {worst:.3} px"
    );
    for problem in problems.iter().take(20) {
        println!("  {problem}");
    }
    assert_eq!(exact, rects, "tightening differs from Python");
}
