//! MobileSAM stage against ScanSplitter's output (`benchmarks/golden/sam.json`, made by
//! `tools/dump_sam_reference.py`). Skips when the reference checkout or the model files
//! are missing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use oxicut_core::geometry::RotatedRect;
use oxicut_core::io::{load_image, to_detection_rgb};
use oxicut_core::sam::{MobileSam, box_prompt};
use serde::Deserialize;

#[derive(Deserialize)]
struct Golden {
    images: BTreeMap<String, Image>,
}

#[derive(Deserialize)]
struct Image {
    embedding_shape: Vec<i64>,
    embedding_sum: f64,
    embedding_head: Vec<f32>,
    proposals: Vec<Rect>,
    masks: Vec<MaskStats>,
    refined: Vec<Rect>,
}

#[derive(Deserialize)]
struct MaskStats {
    predicted_iou: f32,
    mask_pixels: usize,
}

#[derive(Deserialize, Clone, Copy)]
struct Rect {
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    angle: f64,
}

impl From<Rect> for RotatedRect {
    fn from(r: Rect) -> Self {
        RotatedRect::new(r.cx, r.cy, r.w, r.h, r.angle)
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn model_paths() -> Option<(PathBuf, PathBuf)> {
    let dir = oxicut_models_dir()?;
    let encoder = dir.join("mobile_sam_image_encoder.onnx");
    let decoder = dir.join("sam_mask_decoder_single.onnx");
    (encoder.is_file() && decoder.is_file()).then_some((encoder, decoder))
}

fn oxicut_models_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("OXICUT_MODELS_DIR") {
        return Some(PathBuf::from(dir));
    }
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("oxicut/models"))
}

#[test]
fn mobilesam_matches_scansplitter() {
    let root = workspace_root();
    if !root.join("reference/ScanSplitter/benchmarks").is_dir() {
        eprintln!("skipping: reference/ScanSplitter is not checked out");
        return;
    }
    let Some((encoder, decoder)) = model_paths() else {
        eprintln!("skipping: MobileSAM models are not downloaded (oxicut models download)");
        return;
    };
    let golden: Golden = serde_json::from_str(
        &std::fs::read_to_string(root.join("benchmarks/golden/sam.json")).unwrap(),
    )
    .unwrap();
    let mut sam = MobileSam::load(&encoder, &decoder).unwrap();

    for (name, expected) in &golden.images {
        let decoded =
            load_image(&root.join("reference/decoded").join(format!("{name}.png"))).unwrap();
        let rgb = to_detection_rgb(&decoded);
        let embedding = sam.embed(&rgb).unwrap();

        assert_eq!(
            embedding.shape(),
            expected.embedding_shape,
            "{name}: embedding shape"
        );
        let sum: f64 = embedding.data().iter().map(|&v| v as f64).sum();
        let relative = (sum - expected.embedding_sum).abs() / expected.embedding_sum.abs().max(1.0);
        let head_diff = embedding
            .data()
            .iter()
            .zip(&expected.embedding_head)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        eprintln!("{name}: embedding sum off by {relative:.2e}, head max diff {head_diff:.2e}");
        assert!(
            relative < 1e-3,
            "{name}: embedding sum {sum} vs {}",
            expected.embedding_sum
        );

        let proposals: Vec<RotatedRect> = expected.proposals.iter().map(|&r| r.into()).collect();
        for (i, proposal) in proposals.iter().enumerate() {
            let mask = sam
                .mask(&embedding, box_prompt(&embedding, proposal, 0.04))
                .unwrap();
            let pixels = mask.inside.iter().filter(|&&b| b).count();
            let want = &expected.masks[i];
            eprintln!(
                "{name}[{i}]: iou {:.4} vs {:.4}, mask {pixels} vs {} px",
                mask.predicted_iou, want.predicted_iou, want.mask_pixels
            );
            assert!(
                (mask.predicted_iou - want.predicted_iou).abs() < 0.01,
                "{name}[{i}] iou"
            );
            let drift = pixels.abs_diff(want.mask_pixels) as f64 / want.mask_pixels.max(1) as f64;
            assert!(
                drift < 0.01,
                "{name}[{i}] mask pixels {pixels} vs {}",
                want.mask_pixels
            );
        }

        let refined = sam.refine_rectangles(&rgb, &proposals, 0.04).unwrap();
        assert_eq!(refined.len(), expected.refined.len(), "{name}: count");
        for (i, (got, want)) in refined.iter().zip(&expected.refined).enumerate() {
            let want: RotatedRect = (*want).into();
            let iou = oxicut_core::geometry::iou(got, &want);
            eprintln!("{name}[{i}]: refined IoU with Python {iou:.5}");
            assert!(iou > 0.99, "{name}[{i}]: {got:?} vs {want:?}");
        }
    }
}
