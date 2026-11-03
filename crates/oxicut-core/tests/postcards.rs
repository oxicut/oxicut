//! Postcards on a white lid: a private scan (eight cream and tan cards whose margins the classic detector
//! leaves out) and the boxes its owner fixed by hand, kept in the gitignored `reference/postcards` folder.
//! The test skips when they are not there.

use std::path::PathBuf;

use oxicut_core::detect::{CutTo, DetectOptions, Detection, detect_v3};
use oxicut_core::geometry::{RotatedRect, iou};
use oxicut_core::io::{load_image, to_detection_rgb};
use oxicut_core::v7::detect_v7_lite;
use serde::Deserialize;

const SCAN: &str = "reference/postcards/postcards-8-cream-on-white-lid.jpg";
const TRUTH: &str = "reference/postcards/postcards-8-cream-on-white-lid.truth.oxicut";

#[derive(Deserialize)]
struct Project {
    scans: Vec<Scan>,
}

#[derive(Deserialize)]
struct Scan {
    boxes: Vec<Box>,
}

#[derive(Deserialize)]
struct Box {
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    angle: f64,
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Mean over the hand-fixed cards of the best overlap with any detected box, and how many boxes were found.
fn score(detection: &Detection, truth: &[RotatedRect]) -> (f64, usize) {
    let found: Vec<RotatedRect> = detection.regions.iter().map(|r| r.rect).collect();
    let total: f64 = truth
        .iter()
        .map(|t| found.iter().map(|f| iou(t, f)).fold(0.0, f64::max))
        .sum();
    (total / truth.len() as f64, found.len())
}

#[test]
fn eight_cream_cards_on_a_white_lid_are_cut_whole() {
    let root = workspace_root();
    let (scan, truth) = (root.join(SCAN), root.join(TRUTH));
    if !scan.is_file() || !truth.is_file() {
        eprintln!("skipping: reference/postcards is not there");
        return;
    }
    let text = std::fs::read_to_string(truth).unwrap();
    let project: Project = serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap();
    let truth: Vec<RotatedRect> = project.scans[0]
        .boxes
        .iter()
        .map(|b| RotatedRect::new(b.cx, b.cy, b.w, b.h, b.angle))
        .collect();
    assert_eq!(truth.len(), 8);
    let image = to_detection_rgb(&load_image(&scan).unwrap());

    let card = DetectOptions {
        inset: 0,
        cut_to: CutTo::Card,
        ..Default::default()
    };
    let picture = DetectOptions {
        cut_to: CutTo::Picture,
        ..card
    };

    // The classic detector (and ScanSplitter's) boxes the artwork: the cards are mostly missed.
    let (classic, _) = score(&detect_v3(&image, &picture), &truth);
    assert!(classic < 0.7, "the classic detector scores {classic:.2}");

    for (name, detection) in [("v3", detect_v3(&image, &card)), ("v7-lite", detect_v7_lite(&image, &card))] {
        let (overlap, count) = score(&detection, &truth);
        assert_eq!(count, 8, "{name} found {count} boxes");
        assert!(overlap >= 0.8, "{name}: mean overlap with the hand-fixed cards is {overlap:.2}");
    }
}
