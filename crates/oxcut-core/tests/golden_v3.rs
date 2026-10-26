//! Compare stages of the Rust v3 port with ScanSplitter's Python output.
//!
//! Golden values live in `benchmarks/golden/v3.json` (made by
//! `tools/dump_v3_reference.py`); the fixture images are in the gitignored
//! `reference/ScanSplitter` checkout. Tests skip when the checkout is missing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use oxcut_core::background::background_colors;
use oxcut_core::color::rgb_to_lab;
use oxcut_core::io::{load_image, to_detection_rgb};
use oxcut_core::resize::{DETECTION_MAX_DIMENSION, for_detection};
use serde::Deserialize;

#[derive(Deserialize)]
struct Golden {
    images: BTreeMap<String, GoldenImage>,
}

#[derive(Deserialize)]
struct GoldenImage {
    detection_size: [u32; 2],
    backgrounds_lab: Vec<[f32; 3]>,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden() -> Golden {
    let text = std::fs::read_to_string(workspace_root().join("benchmarks/golden/v3.json"))
        .expect("benchmarks/golden/v3.json is committed");
    serde_json::from_str(&text).expect("golden json parses")
}

fn reference_available() -> bool {
    let present = workspace_root()
        .join("reference/ScanSplitter/benchmarks")
        .is_dir();
    if !present {
        eprintln!("skipping: reference/ScanSplitter is not checked out");
    }
    present
}

/// Greatest per-channel gap between two sets of centres, matching each golden
/// centre to its nearest ours. `None` if the sets differ in size.
fn centre_gap(ours: &[[f32; 3]], theirs: &[[f32; 3]]) -> Option<f32> {
    if ours.len() != theirs.len() {
        return None;
    }
    let mut worst = 0f32;
    for t in theirs {
        let gap = ours
            .iter()
            .map(|o| (0..3).map(|c| (o[c] - t[c]).abs()).fold(0f32, f32::max))
            .fold(f32::INFINITY, f32::min);
        worst = worst.max(gap);
    }
    Some(worst)
}

#[test]
fn detection_size_and_background_match_python() {
    if !reference_available() {
        return;
    }
    let golden = golden();
    let mut report = Vec::new();
    for (name, expected) in &golden.images {
        // PIL-decoded pixels (see tools/dump_v3_reference.py): our JPEG decoder
        // differs from libjpeg by up to a few levels, which would blur the comparison.
        let image = load_image(
            &workspace_root()
                .join("reference/decoded")
                .join(format!("{name}.png")),
        )
        .unwrap();
        let detection = for_detection(&to_detection_rgb(&image), DETECTION_MAX_DIMENSION);
        assert_eq!(
            [detection.image.width(), detection.image.height()],
            expected.detection_size,
            "{name}: detection size"
        );

        let lab = rgb_to_lab(&detection.image);
        let ours = background_colors(&lab);
        match centre_gap(&ours, &expected.backgrounds_lab) {
            Some(gap) if gap > 3.0 => report.push((
                name.clone(),
                gap,
                Some(format!(
                    "gap {gap:.1}: ours {ours:?} vs python {:?}",
                    expected.backgrounds_lab
                )),
            )),
            Some(gap) => report.push((name.clone(), gap, None)),
            None => report.push((
                name.clone(),
                f32::NAN,
                Some(format!(
                    "ours {ours:?} vs python {:?}",
                    expected.backgrounds_lab
                )),
            )),
        }
    }

    let mismatched: Vec<_> = report.iter().filter(|r| r.2.is_some()).collect();
    let worst = report
        .iter()
        .map(|r| r.1)
        .filter(|g| g.is_finite())
        .fold(0f32, f32::max);
    let exact = report.iter().filter(|r| r.1 < 0.05).count();
    println!(
        "{} images, {exact} exact, worst centre gap {worst:.2}, {} cluster-count mismatches",
        report.len(),
        mismatched.len()
    );
    for (name, _, detail) in &mismatched {
        println!("  MISMATCH {name}: {}", detail.as_ref().unwrap());
    }
    // Exact for nearly all; the rest differ through the few dark-blue colours
    // whose 8-bit Lab is one level off OpenCV's (see docs/detector-v7-port.md).
    assert!(
        mismatched.is_empty(),
        "background cluster sets differ from Python"
    );
    assert!(worst < 2.0, "worst background centre gap {worst}");
    assert!(
        exact >= report.len() - 5,
        "only {exact} of {} exact",
        report.len()
    );
}
