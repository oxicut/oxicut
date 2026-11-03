//! Compare the Rust orientation run with ScanSplitter's Python one.
//!
//! Golden values live in `benchmarks/golden/orientation.json` (made by `tools/dump_orientation_reference.py`):
//! the model's four probabilities for benchmark pictures turned 0, 90, 180 and 270 degrees clockwise. The
//! pictures are in the gitignored `reference/ScanSplitter` checkout and the model is downloaded with
//! `oxicut models download orientation`; the test skips when either is missing.

use std::path::PathBuf;

use oxicut_core::io::{load_image, to_detection_rgb};
use oxicut_core::orient::{OrientationModel, turned};
use serde::Deserialize;

#[derive(Deserialize)]
struct Golden {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    picture: String,
    clockwise: u16,
    probabilities: [f32; 4],
    answer: usize,
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn model_path() -> Option<PathBuf> {
    let dir = std::env::var_os("OXICUT_MODELS_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("oxicut").join("models")))?;
    let path = dir.join("orientation_model_v2.onnx");
    path.is_file().then_some(path)
}

#[test]
fn the_rust_run_gives_the_same_answers_as_scansplitters() {
    let root = workspace_root();
    let reference = root.join("reference/ScanSplitter");
    let Some(model) = model_path() else {
        eprintln!("skipping: the orientation model is not downloaded");
        return;
    };
    if !reference.is_dir() {
        eprintln!("skipping: reference/ScanSplitter is not checked out");
        return;
    }
    let golden: Golden =
        serde_json::from_str(&std::fs::read_to_string(root.join("benchmarks/golden/orientation.json")).unwrap()).unwrap();
    let mut model = OrientationModel::load(&model).unwrap();

    let mut worst = 0f32;
    for case in &golden.cases {
        let picture = to_detection_rgb(&load_image(&reference.join(&case.picture)).unwrap());
        // Clockwise by `clockwise` degrees, as the Python side turned it.
        let image = turned(&picture, (case.clockwise / 90) as u8);
        let probabilities = model.probabilities(&[image]).unwrap()[0];
        let answer = probabilities
            .iter()
            .enumerate()
            .fold(0, |best, (i, p)| if *p > probabilities[best] { i } else { best });
        assert_eq!(answer, case.answer, "{} turned {}: {probabilities:?} against {:?}", case.picture, case.clockwise, case.probabilities);
        for (ours, theirs) in probabilities.iter().zip(case.probabilities) {
            worst = worst.max((ours - theirs).abs());
        }
    }
    // The pictures are decoded by different JPEG libraries, so the probabilities may differ a little.
    assert!(worst < 0.05, "largest difference in a probability: {worst}");
    eprintln!("largest difference in a probability: {worst:.4} over {} cases", golden.cases.len());
}
