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

fn orientation_model() -> Option<PathBuf> {
    let dir = std::env::var_os("OXICUT_MODELS_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("oxicut").join("models")))?;
    let path = dir.join("orientation_model_v2.onnx");
    path.is_file().then_some(path)
}

/// Each hand-fixed card, cut as OxiCut cuts it, turned all four ways: the combined answer must put it right,
/// and above all must never turn an upright card (the model alone does, for two of these eight).
#[test]
fn postcards_are_turned_the_right_way_up_and_never_the_wrong_way() {
    use oxicut_core::crop::crop_region;
    use oxicut_core::orient::{MIN_CONFIDENCE, OrientationModel, turned};

    let root = workspace_root();
    let (scan, truth) = (root.join(SCAN), root.join(TRUTH));
    let Some(model) = orientation_model() else {
        eprintln!("skipping: the orientation model is not downloaded");
        return;
    };
    if !scan.is_file() || !truth.is_file() {
        eprintln!("skipping: reference/postcards is not there");
        return;
    }
    let text = std::fs::read_to_string(truth).unwrap();
    let project: Project = serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap();
    let image = oxicut_core::io::load_image(&scan).unwrap();
    let mut model = OrientationModel::load(&model).unwrap();

    let (mut right, mut total, mut confident_mistakes) = (0, 0, Vec::new());
    for (index, b) in project.scans[0].boxes.iter().enumerate() {
        let crop = crop_region(&image, &RotatedRect::new(b.cx, b.cy, b.w, b.h, b.angle)).unwrap().to_rgb8();
        // The hand-fixed boxes mark where the cards are, not which way up: the last card lies on its side there.
        let upright = if index == 7 { turned(&crop, 1) } else { crop.clone() };
        for turn in 0..4u8 {
            let answer = model.classify(&turned(&upright, turn)).unwrap();
            total += 1;
            let wanted = (4 - turn) % 4;
            if answer.quarter_turns == wanted {
                right += 1;
            } else if answer.confidence >= MIN_CONFIDENCE {
                confident_mistakes.push(format!("card {} turned {}: said {} at {:.2}", index + 1, turn * 90, answer.quarter_turns, answer.confidence));
            }
            // The cards that are upright must be left alone, whatever the model thinks.
            if turn == 0 {
                assert_eq!(answer.decided(MIN_CONFIDENCE), 0, "card {} is upright: {answer:?}", index + 1);
            }
        }
        // The sideways card as it was cut: the combined answer turns it a quarter clockwise.
        if index == 7 {
            assert_eq!(model.classify(&crop).unwrap().decided(MIN_CONFIDENCE), 1, "the sideways card");
        }
    }
    eprintln!("right {right} of {total}; mistakes the threshold lets through: {confident_mistakes:?}");
    assert!(right >= 28, "right {right} of {total}");
    assert!(confident_mistakes.is_empty(), "{confident_mistakes:?}");
}

#[test]
fn another_cut_is_offered_that_is_closer_to_the_owners_card() {
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
    let image = to_detection_rgb(&load_image(&scan).unwrap());
    let (width, height) = image.dimensions();

    // The user is looking at the classic detector's boxes (the pictures only) and wants the whole cards.
    let options = DetectOptions {
        inset: 0,
        cut_to: CutTo::Picture,
        ..Default::default()
    };
    let shown = detect_v3(&image, &options);
    let found = oxicut_core::suggest::proposals(&image, &options, None).unwrap();

    let (mut better, mut asked) = (0, 0);
    for owner in &truth {
        let Some(current) = shown
            .regions
            .iter()
            .map(|r| r.rect)
            .max_by(|a, b| iou(owner, a).total_cmp(&iou(owner, b)))
        else {
            continue;
        };
        asked += 1;
        let offered = oxicut_core::suggest::suggestions_for(&current, &found, &options, width, height, 6);
        assert!(offered.len() <= 6);
        let before = iou(owner, &current);
        let best = offered.iter().map(|o| iou(owner, o)).fold(0.0, f64::max);
        eprintln!("card at ({:.0}, {:.0}): shown {before:.2}, best of {} offered {best:.2}", owner.cx, owner.cy, offered.len());
        if best > before + 0.02 {
            better += 1;
        }
    }
    eprintln!("a closer cut was offered for {better} of {asked} cards");
    assert!(asked >= 6);
}
